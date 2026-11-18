use inifra_core::catalog::Catalog;
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

    fn write(&self, relative: &str, contents: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
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
    let catalog = Catalog::load(&root.0).unwrap();
    assert_eq!(
        catalog
            .packs
            .iter()
            .map(|pack| pack.id.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(catalog.packs[0].inputs, ["name"]);
}

#[test]
fn rejects_duplicate_ids_and_invalid_manifests() {
    let root = Fixture::new();
    root.pack("one", "same", "schema_version = 2\ndescription = 'One'\n");
    root.pack("two", "same", "schema_version = 2\ndescription = 'Two'\n");
    assert!(
        Catalog::load(&root.0)
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    fs::remove_dir_all(root.0.join("two")).unwrap();
    root.pack("one", "bad", "schema_version = 99\ndescription = 'Bad'\n");
    assert!(
        Catalog::load(&root.0)
            .unwrap_err()
            .to_string()
            .contains("schema version")
    );
}

#[test]
fn initial_catalog_packs_load_and_have_sources() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates");
    let catalog = Catalog::load(&root).unwrap();
    assert_eq!(
        catalog
            .packs
            .iter()
            .map(|pack| pack.id.as_str())
            .collect::<Vec<_>>(),
        ["agents", "changelog", "editorconfig", "readme"]
    );
    for pack in &catalog.packs {
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
    let error = Catalog::load(&root.0).unwrap_err().to_string();
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
        Catalog::load(&root.0)
            .unwrap_err()
            .to_string()
            .contains("duplicate input")
    );
}

#[test]
fn loads_category_names_and_input_scope_precedence() {
    let root = Fixture::new();
    root.write(
        "inputs.toml",
        "schema_version = 1\n[inputs.shared]\nprompt = 'Root shared'\n[inputs.root_only]\nprompt = 'Root only'\n",
    );
    root.write(
        "project-docs/inputs.toml",
        "schema_version = 1\n[inputs.shared]\nprompt = 'Category shared'\ndefault = 'category-default'\n",
    );
    root.write(
        "categories.toml",
        "schema_version = 1\n[categories.project-docs]\nname = 'Project docs'\n",
    );
    root.pack(
        "project-docs",
        "architecture",
        "schema_version = 2\ndescription = 'Architecture'\ninputs = ['shared', 'root_only']\n[local_inputs.local_only]\nprompt = 'Pack local'\n",
    );
    root.pack(
        "project-docs",
        "custom",
        "schema_version = 2\ndescription = 'Custom'\n[local_inputs.shared]\nprompt = 'Pack override'\n",
    );

    let catalog = Catalog::load(&root.0).unwrap();
    assert_eq!(catalog.categories["project-docs"].name, "Project docs");
    let architecture = catalog
        .packs
        .iter()
        .find(|pack| pack.id == "architecture")
        .unwrap();
    let definitions = catalog.input_definitions_for(architecture).unwrap();
    assert_eq!(definitions["shared"].prompt, "Category shared");
    assert_eq!(
        definitions["shared"].default.as_deref(),
        Some("category-default")
    );
    assert_eq!(definitions["root_only"].prompt, "Root only");
    assert_eq!(definitions["local_only"].prompt, "Pack local");

    let custom = catalog
        .packs
        .iter()
        .find(|pack| pack.id == "custom")
        .unwrap();
    assert_eq!(
        catalog.input_definitions_for(custom).unwrap()["shared"].prompt,
        "Pack override"
    );
}

#[test]
fn rejects_local_inputs_repeated_in_shared_input_list() {
    let root = Fixture::new();
    root.pack(
        "project",
        "example",
        "schema_version = 2\ndescription = 'Example'\ninputs = ['name']\n[local_inputs.name]\nprompt = 'Name'\n",
    );
    let error = Catalog::load(&root.0).unwrap_err().to_string();
    assert!(error.contains("both shared and pack-local"), "{error}");
}

#[test]
fn rejects_category_display_names_without_a_category_directory() {
    let root = Fixture::new();
    root.write(
        "categories.toml",
        "schema_version = 1\n[categories.missing]\nname = 'Missing'\n",
    );
    let error = Catalog::load(&root.0).unwrap_err().to_string();
    assert!(error.contains("unknown category"), "{error}");
}

#[test]
fn rejects_category_display_names_with_control_characters() {
    let root = Fixture::new();
    root.write(
        "categories.toml",
        r#"schema_version = 1
[categories.project]
name = "\u001b[31mProject\u001b[0m"
"#,
    );
    root.pack(
        "project",
        "example",
        "schema_version = 2\ndescription = 'Example'\n",
    );

    let error = Catalog::load(&root.0).unwrap_err().to_string();
    assert!(error.contains("invalid category definition"), "{error}");
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_category_metadata() {
    use std::os::unix::fs::symlink;

    let root = Fixture::new();
    root.write(
        "target.toml",
        "schema_version = 1\n[categories.project]\nname = 'Project'\n",
    );
    symlink(root.0.join("target.toml"), root.0.join("categories.toml")).unwrap();

    let error = Catalog::load(&root.0).unwrap_err().to_string();
    assert!(error.contains("regular file"), "{error}");
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
    let catalog = Catalog::load(&root.0).unwrap();
    assert!(
        catalog
            .validate()
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
        Catalog::load(&root.0)
            .unwrap_err()
            .to_string()
            .contains("regular file")
    );
}
