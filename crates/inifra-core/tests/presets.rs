use inifra_core::{catalog::Catalog, presets};
use std::collections::BTreeMap;
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

#[test]
fn saves_renames_and_removes_a_validated_preset() {
    let (_bundled, catalog) = bundled_catalog();
    let root = tempfile::tempdir().unwrap();
    let preset = presets::Preset {
        id: "docs-workflow".into(),
        description: "Project documentation files".into(),
        templates: vec!["agents".into(), "readme".into()],
        values: BTreeMap::from([("project_name".into(), "example".into())]),
    };

    presets::save(root.path(), &preset, &catalog, false).unwrap();
    let saved = presets::load_all(root.path(), &catalog).unwrap();
    assert_eq!(saved, [preset]);
    assert!(presets::save(root.path(), &saved[0], &catalog, false).is_err());
    let mut replacement = saved[0].clone();
    replacement.description = "Updated documentation files".into();
    presets::save(root.path(), &replacement, &catalog, true).unwrap();
    assert_eq!(
        presets::load_all(root.path(), &catalog).unwrap()[0].description,
        "Updated documentation files"
    );

    presets::rename(root.path(), "docs-workflow", "docs-team", &catalog).unwrap();
    assert_eq!(
        presets::load_all(root.path(), &catalog).unwrap()[0].id,
        "docs-team"
    );
    presets::remove(root.path(), "docs-team").unwrap();
    assert!(presets::load_all(root.path(), &catalog).unwrap().is_empty());
}

#[test]
fn renaming_a_template_updates_saved_preset_manifests() {
    let (_bundled, catalog) = bundled_catalog();
    let root = tempfile::tempdir().unwrap();
    let preset = presets::Preset {
        id: "docs-workflow".into(),
        description: "Project documentation files".into(),
        templates: vec!["readme".into()],
        values: BTreeMap::from([("project_name".into(), "example".into())]),
    };
    presets::save(root.path(), &preset, &catalog, false).unwrap();

    assert_eq!(
        presets::rename_template_references(root.path(), "readme", "project-readme").unwrap(),
        1
    );
    let manifest = std::fs::read_to_string(root.path().join("docs-workflow/preset.toml")).unwrap();
    assert!(manifest.contains("project-readme"));
}

#[test]
fn remove_preserves_unexpected_files_in_a_preset_directory() {
    let (_bundled, catalog) = bundled_catalog();
    let root = tempfile::tempdir().unwrap();
    let preset = presets::Preset {
        id: "docs-workflow".into(),
        description: "Project documentation files".into(),
        templates: vec!["agents".into()],
        values: BTreeMap::new(),
    };
    presets::save(root.path(), &preset, &catalog, false).unwrap();
    let extra = root.path().join("docs-workflow/notes.txt");
    std::fs::write(&extra, "keep").unwrap();

    let error = presets::remove(root.path(), "docs-workflow").unwrap_err();

    assert!(error.to_string().contains("unexpected preset data"));
    assert_eq!(std::fs::read_to_string(extra).unwrap(), "keep");
}

#[test]
fn malformed_preset_can_be_replaced_or_removed_without_touching_extra_files() {
    let (_bundled, catalog) = bundled_catalog();
    let root = tempfile::tempdir().unwrap();
    let preset = presets::Preset {
        id: "docs-workflow".into(),
        description: "Project documentation files".into(),
        templates: vec!["agents".into()],
        values: BTreeMap::new(),
    };
    presets::save(root.path(), &preset, &catalog, false).unwrap();
    let manifest = root.path().join("docs-workflow/preset.toml");
    std::fs::write(&manifest, "not = [valid TOML").unwrap();

    presets::save(root.path(), &preset, &catalog, true).unwrap();
    let loaded = presets::load_all(root.path(), &catalog).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0], preset);

    std::fs::write(&manifest, "not = [valid TOML").unwrap();
    presets::remove(root.path(), "docs-workflow").unwrap();
    assert!(!root.path().join("docs-workflow").exists());
}
