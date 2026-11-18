use inifra_core::catalog::discover;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "inifra-catalog-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn pack(&self, category: &str, id: &str, manifest: &str) {
        let dir = self.0.join(category).join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("template.toml"), manifest).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn discovers_packs_in_deterministic_order() {
    let root = Fixture::new();
    root.pack(
        "z-category",
        "second",
        "schema_version = 2\ndescription = 'Second'\n",
    );
    root.pack(
        "a-category",
        "first",
        "schema_version = 2\ndescription = 'First'\ninputs = ['name']\n",
    );
    let packs = discover(&root.0).unwrap();
    assert_eq!(
        packs.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(packs[0].inputs, ["name"]);
}

#[test]
fn rejects_duplicate_ids_and_invalid_manifests() {
    let root = Fixture::new();
    root.pack("one", "same", "schema_version = 2\ndescription = 'One'\n");
    root.pack("two", "same", "schema_version = 2\ndescription = 'Two'\n");
    assert!(
        discover(&root.0)
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    fs::remove_dir_all(root.0.join("two")).unwrap();
    root.pack("one", "bad", "schema_version = 99\ndescription = 'Bad'\n");
    assert!(
        discover(&root.0)
            .unwrap_err()
            .to_string()
            .contains("schema version")
    );
}

#[test]
fn initial_catalog_packs_load_and_have_sources() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates");
    let packs = discover(&root).unwrap();
    assert_eq!(
        packs
            .iter()
            .map(|pack| pack.id.as_str())
            .collect::<Vec<_>>(),
        ["agents", "changelog", "editorconfig", "readme"]
    );
    for pack in packs {
        assert!(
            !inifra_core::sources::discover(&pack.directory)
                .unwrap()
                .is_empty()
        );
    }
    assert!(inifra_core::inputs::load(&root.join("inputs.toml")).is_ok());
}

#[test]
fn rejects_unsupported_manifest_fields() {
    let root = Fixture::new();
    root.pack(
        "project",
        "example",
        "schema_version = 2\ndescription = 'Example'\ndirectories = ['empty']\n",
    );
    let error = discover(&root.0).unwrap_err().to_string();
    assert!(error.contains("directories"), "{error}");
}

#[test]
fn rejects_duplicate_input_declarations() {
    let root = Fixture::new();
    root.pack(
        "project",
        "example",
        "schema_version = 2\ndescription = 'Example'\ninputs = ['name', 'name']\n",
    );
    assert!(
        discover(&root.0)
            .unwrap_err()
            .to_string()
            .contains("duplicate input")
    );
}

#[test]
fn validate_rejects_file_and_directory_output_overlap() {
    let root = Fixture::new();
    root.pack(
        "project",
        "example",
        "schema_version = 2\ndescription = 'Example'\n",
    );
    fs::write(root.0.join("inputs.toml"), "schema_version = 1\n").unwrap();
    let pack = root.0.join("project/example");
    fs::write(pack.join("docs.j2"), "hello").unwrap();
    fs::create_dir(pack.join("docs")).unwrap();
    fs::write(pack.join("docs/guide.md"), "hello").unwrap();
    let packs = discover(&root.0).unwrap();
    assert!(
        inifra_core::catalog::validate(&root.0, &packs)
            .unwrap_err()
            .to_string()
            .contains("overlapping")
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_manifest() {
    use std::os::unix::fs::symlink;
    let root = Fixture::new();
    root.pack(
        "project",
        "example",
        "schema_version = 2\ndescription = 'Example'\n",
    );
    let manifest = root.0.join("project/example/template.toml");
    let target = root.0.join("manifest.toml");
    fs::rename(&manifest, &target).unwrap();
    symlink(&target, &manifest).unwrap();
    assert!(
        discover(&root.0)
            .unwrap_err()
            .to_string()
            .contains("regular file")
    );
}
