use inifra_core::{catalog, user_templates};
use std::{fs, path::Path};

#[test]
fn create_pack_writes_a_manifest_and_rejects_overwrite() {
    let root = tempfile::tempdir().unwrap();
    let source = Path::new("notes/README.md.j2");
    user_templates::create_pack(root.path(), "personal", "notes", "Personal notes", source)
        .unwrap();

    let catalog = catalog::Catalog::load(root.path()).unwrap();
    assert_eq!(catalog.packs[0].id, "notes");
    assert!(catalog.packs[0].directory.join("template.toml").is_file());
    assert!(catalog.packs[0].directory.join(source).is_file());
    assert!(
        user_templates::create_pack(root.path(), "personal", "notes", "Again", source).is_err()
    );
}

#[test]
fn create_pack_rejects_unsafe_source_paths_without_creating_a_pack() {
    let root = tempfile::tempdir().unwrap();

    assert!(
        user_templates::create_pack(
            root.path(),
            "personal",
            "notes",
            "Personal notes",
            Path::new("../outside.md.j2")
        )
        .is_err()
    );
    assert!(!root.path().join("personal/notes").exists());
}

#[test]
fn copy_and_rename_preserve_nested_sources() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir_all(source.join("docs")).unwrap();
    fs::write(
        source.join("template.toml"),
        "schema_version = 2\ndescription = 'x'\n",
    )
    .unwrap();
    fs::write(source.join("docs/guide.md.j2"), "# {{ project_name }}\n").unwrap();
    let destination = root.path().join("templates/personal/notes");

    user_templates::copy_pack(&source, &destination).unwrap();
    user_templates::rename_pack(
        root.path().join("templates").as_path(),
        "personal",
        "notes",
        "guide",
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(
            root.path()
                .join("templates/personal/guide/docs/guide.md.j2")
        )
        .unwrap(),
        "# {{ project_name }}\n"
    );
}

#[cfg(unix)]
#[test]
fn copy_rejects_symlink_sources_without_leaving_partial_files() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("regular.md"), "copied first").unwrap();
    symlink("regular.md", source.join("linked.md")).unwrap();

    assert!(user_templates::copy_pack(&source, &destination).is_err());
    assert!(!destination.exists());
}

#[test]
fn rename_references_updates_user_manifests_for_the_new_catalog_id() {
    let root = tempfile::tempdir().unwrap();
    let target = user_templates::create_pack(
        root.path(),
        "personal",
        "target",
        "Target pack",
        Path::new("GUIDE.md"),
    )
    .unwrap();
    fs::write(target.join("GUIDE.md"), "guide\n").unwrap();
    let consumer = user_templates::create_pack(
        root.path(),
        "personal",
        "consumer",
        "Consumer pack",
        Path::new("CONSUMER.md"),
    )
    .unwrap();
    fs::write(
        consumer.join("template.toml"),
        "schema_version = 2\ndescription = 'Consumer pack'\n\n[references.guide]\ntemplate = 'target'\nsource = 'GUIDE.md'\n",
    )
    .unwrap();
    catalog::Catalog::load(root.path()).unwrap();

    user_templates::rename_pack(root.path(), "personal", "target", "renamed").unwrap();
    assert_eq!(
        user_templates::rename_references(root.path(), "target", "renamed").unwrap(),
        1
    );

    let catalog = catalog::Catalog::load(root.path()).unwrap();
    assert_eq!(catalog.packs[0].id, "consumer");
    assert_eq!(catalog.packs[0].references["guide"].template, "renamed");
}
