//! Small filesystem helpers for reading catalog metadata without following symlinks.

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};

static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn read_regular_file(path: &Path, description: &str) -> io::Result<String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "expected a file path"))?;
    let directory = Dir::open_ambient_dir(parent, ambient_authority())?;
    let metadata = directory.symlink_metadata(filename)?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{description} must be a regular file"),
        ));
    }

    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = directory.open_with(filename, &options)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

pub(crate) fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "expected a file path"))?;
    let target_exists = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => true,
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} must be a regular file", path.display()),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    };

    let (temporary, mut file) = create_temporary(parent, filename)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);

    #[cfg(windows)]
    if target_exists {
        replace_on_windows(path, &temporary)?;
        return Ok(());
    }

    #[cfg(not(windows))]
    let _ = target_exists;

    fs::rename(&temporary.0, path)?;
    Ok(())
}

fn create_temporary(parent: &Path, filename: &std::ffi::OsStr) -> io::Result<(TempPath, fs::File)> {
    for _ in 0..100 {
        let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{}.inifra-{}-{sequence}.tmp",
            filename.to_string_lossy(),
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((TempPath(temporary), file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a temporary file",
    ))
}

#[cfg(windows)]
fn replace_on_windows(path: &Path, temporary: &TempPath) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let filename = path.file_name().unwrap_or_default();
    let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
    let backup = parent.join(format!(
        ".{}.inifra-{}-{sequence}.bak",
        filename.to_string_lossy(),
        std::process::id()
    ));
    fs::rename(path, &backup)?;
    if let Err(error) = fs::rename(&temporary.0, path) {
        return match fs::rename(&backup, path) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(io::Error::other(format!(
                "{error}; restoring the previous file also failed: {rollback_error}"
            ))),
        };
    }
    if let Err(error) = fs::remove_file(&backup) {
        let restore = fs::remove_file(path).and_then(|()| fs::rename(&backup, path));
        return match restore {
            Ok(()) => Err(error),
            Err(restore_error) => Err(io::Error::other(format!(
                "{error}; restoring the previous file also failed: {restore_error}"
            ))),
        };
    }
    Ok(())
}

struct TempPath(std::path::PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
