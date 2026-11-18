use inifra_core::{apply::apply, plan::build, render::RenderedOutput};
use std::{fs, path::PathBuf};

fn output(path: &str) -> RenderedOutput {
    RenderedOutput {
        path: PathBuf::from(path),
        contents: b"new".to_vec(),
    }
}

#[test]
fn applies_new_files_and_preserves_skipped_files() {
    let root = std::env::temp_dir().join(format!("inifra-apply-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("existing"), b"old").unwrap();
    let plan = build(&root, vec![output("existing"), output("nested/new")], false).unwrap();
    apply(&root, &plan).unwrap();
    assert_eq!(fs::read(root.join("existing")).unwrap(), b"old");
    assert_eq!(fs::read(root.join("nested/new")).unwrap(), b"new");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn applies_into_existing_nested_directories() {
    let root = std::env::temp_dir().join(format!("inifra-apply-nested-{}", std::process::id()));
    fs::create_dir_all(root.join("one/two")).unwrap();
    let plan = build(&root, vec![output("one/two/file")], false).unwrap();
    apply(&root, &plan).unwrap();
    assert_eq!(fs::read(root.join("one/two/file")).unwrap(), b"new");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn detects_changes_before_writing_any_file() {
    let root = std::env::temp_dir().join(format!("inifra-apply-changed-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let plan = build(&root, vec![output("first"), output("second")], false).unwrap();
    fs::write(root.join("second"), b"unexpected").unwrap();
    assert!(apply(&root, &plan).is_err());
    assert!(!root.join("first").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn refuses_to_replace_a_file_edited_after_review() {
    let root = std::env::temp_dir().join(format!("inifra-apply-edited-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("existing");
    fs::write(&path, b"original").unwrap();
    let plan = build(&root, vec![output("existing")], true).unwrap();
    fs::write(&path, b"edited").unwrap();
    assert!(apply(&root, &plan).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"edited");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn overwriting_a_hard_link_does_not_change_its_other_path() {
    let root = std::env::temp_dir().join(format!("inifra-apply-hardlink-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let target = root.join("target");
    let linked = root.join("linked");
    fs::write(&target, b"old").unwrap();
    fs::hard_link(&target, &linked).unwrap();

    let plan = build(&root, vec![output("target")], true).unwrap();
    apply(&root, &plan).unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert_eq!(fs::read(&linked).unwrap(), b"old");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn refuses_a_symlink_inserted_after_planning() {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!("inifra-apply-link-{}", std::process::id()));
    let outside = std::env::temp_dir().join(format!("inifra-apply-outside-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let plan = build(&root, vec![output("nested/file")], false).unwrap();
    symlink(&outside, root.join("nested")).unwrap();
    assert!(apply(&root, &plan).is_err());
    assert!(!outside.join("file").exists());
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}
