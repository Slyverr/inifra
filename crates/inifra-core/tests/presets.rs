use inifra_core::{catalog::Catalog, presets};
use std::{fs, path::Path};

fn bundled_catalog() -> (tempfile::TempDir, Catalog) {
    let directory = tempfile::tempdir().unwrap();
    inifra_core::catalog::extract_bundled(directory.path()).unwrap();
    let catalog = Catalog::load(directory.path()).unwrap();
    (directory, catalog)
}

fn write_preset(root: &Path, id: &str, manifest: &str) {
    let directory = root.join(id);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("preset.toml"), manifest).unwrap();
}

#[test]
fn missing_store_is_an_empty_preset_list() {
    let (_bundled, catalog) = bundled_catalog();
    let root = tempfile::tempdir().unwrap();

    let presets = presets::load_all(&root.path().join("missing"), &catalog).unwrap();

    assert!(presets.is_empty());
}

#[test]
fn loads_the_documented_reference_fixtures() {
    let (_bundled, catalog) = bundled_catalog();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets");

    let loaded = presets::load_all(&fixtures, &catalog).unwrap();

    assert_eq!(
        loaded
            .iter()
            .map(|preset| preset.id.as_str())
            .collect::<Vec<_>>(),
        ["documentation", "rust-cli"]
    );
    assert_eq!(loaded[1].templates.len(), 6);
    assert_eq!(loaded[1].values["project_name"], "example-cli");
}

#[test]
fn rejects_unknown_templates_and_unused_saved_values() {
    let (_bundled, catalog) = bundled_catalog();
    let root = tempfile::tempdir().unwrap();
    write_preset(
        root.path(),
        "unknown-template",
        "schema_version = 1\ndescription = 'Unknown'\ntemplates = ['not-in-catalog']\n",
    );

    let error = presets::load_all(root.path(), &catalog).unwrap_err();
    assert!(error.to_string().contains("unknown template"));

    fs::remove_dir_all(root.path().join("unknown-template")).unwrap();
    write_preset(
        root.path(),
        "unused-value",
        "schema_version = 1\ndescription = 'Unused'\ntemplates = ['editorconfig']\n[values]\nproject_name = 'Example'\n",
    );
    let error = presets::load_all(root.path(), &catalog).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("not used by the selected templates")
    );
}

#[test]
fn rejects_duplicate_templates_and_unsupported_schema_versions() {
    let (_bundled, catalog) = bundled_catalog();
    let root = tempfile::tempdir().unwrap();
    write_preset(
        root.path(),
        "duplicate",
        "schema_version = 1\ndescription = 'Duplicate'\ntemplates = ['readme', 'readme']\n",
    );
    let error = presets::load_all(root.path(), &catalog).unwrap_err();
    assert!(error.to_string().contains("repeated template ID"));

    fs::remove_dir_all(root.path().join("duplicate")).unwrap();
    write_preset(
        root.path(),
        "future-schema",
        "schema_version = 2\ndescription = 'Future'\ntemplates = ['readme']\n",
    );
    let error = presets::load_all(root.path(), &catalog).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsupported preset schema version 2")
    );
}
