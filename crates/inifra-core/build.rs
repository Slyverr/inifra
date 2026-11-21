use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").ok_or("Cargo did not provide CARGO_MANIFEST_DIR")?,
    );
    let workspace_templates = manifest_dir.join("../../templates");
    let packaged_templates = manifest_dir.join("templates");
    let source = if workspace_templates.is_dir() {
        workspace_templates
    } else if packaged_templates.is_dir() {
        packaged_templates
    } else {
        return Err("bundled templates are missing from the workspace and package".into());
    };

    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("Cargo did not provide OUT_DIR")?)
        .join("bundled_templates");
    if output.exists() {
        fs::remove_dir_all(&output)?;
    }
    copy_directory(&source, &output)?;

    println!("cargo:rerun-if-changed={}", source.display());

    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if entry.file_type()?.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else if entry.file_type()?.is_file() {
            fs::copy(source_path, destination_path)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported bundled template entry: {}",
                    source_path.display()
                ),
            ));
        }
    }

    Ok(())
}
