use inifra_core::{
    plan::{Action, DirectoryAction, build, build_with_directories},
    render::RenderedOutput,
};
use std::{fs, path::PathBuf};

fn output(path: &str) -> RenderedOutput {
    RenderedOutput {
        path: PathBuf::from(path),
        contents: b"new".to_vec(),
    }
}

#[test]
fn plans_existing_and_new_files_without_writing() {
    let root = std::env::temp_dir().join(format!("inifra-plan-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("existing"), "old").unwrap();
    let plan = build(&root, vec![output("existing"), output("docs/new")], false).unwrap();
    assert_eq!(plan[0].action, Action::Skip);
    assert_eq!(plan[1].action, Action::Create);
    assert!(!root.join("docs").exists());
    assert_eq!(fs::read_to_string(root.join("existing")).unwrap(), "old");
    assert_eq!(
        build(&root, vec![output("existing")], true).unwrap()[0].action,
        Action::Replace
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_duplicate_overlapping_and_escaping_outputs() {
    let root = std::env::temp_dir();
    assert!(build(&root, vec![output("a"), output("a")], false).is_err());
    assert!(build(&root, vec![output("a"), output("a/b")], false).is_err());
    assert!(build(&root, vec![output("../escape")], false).is_err());
    assert!(build(&root, vec![output("C:\\escape")], false).is_err());
    assert!(build(&root, vec![output("NUL.txt")], false).is_err());
    assert!(build(&root, vec![output("README.md"), output("readme.md")], false).is_err());
    assert!(build(&root, vec![output("Docs"), output("docs/guide.md")], false).is_err());
}

#[test]
fn plans_empty_directories_without_creating_them() {
    let parent =
        std::env::temp_dir().join(format!("inifra-plan-directories-{}", std::process::id()));
    fs::create_dir_all(&parent).unwrap();
    let root = parent.join("target");
    let plan = build_with_directories(
        &root,
        Vec::new(),
        vec!["code".into(), "drafts".into()],
        false,
    )
    .unwrap();

    assert_eq!(plan.files.len(), 0);
    assert_eq!(plan.directories.len(), 2);
    assert!(
        plan.directories
            .iter()
            .all(|directory| directory.action == DirectoryAction::Create)
    );
    assert!(!root.exists());
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn plans_an_existing_directory_as_unchanged() {
    let root = std::env::temp_dir().join(format!(
        "inifra-plan-existing-directory-{}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("code")).unwrap();

    let plan = build_with_directories(&root, Vec::new(), vec!["code".into()], false).unwrap();

    assert_eq!(plan.directories[0].action, DirectoryAction::Existing);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_directory_outputs_occupied_by_files() {
    let root = std::env::temp_dir().join(format!(
        "inifra-plan-file-blocks-directory-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("code"), "file").unwrap();

    assert!(build_with_directories(&root, Vec::new(), vec!["code".into()], false).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_directory_outputs() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "inifra-plan-linked-directory-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    symlink(std::env::temp_dir(), root.join("code")).unwrap();

    assert!(build_with_directories(&root, Vec::new(), vec!["code".into()], false).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_file_directory_and_nested_directory_output_overlaps() {
    let root = std::env::temp_dir();
    assert!(
        build_with_directories(
            &root,
            vec![output("workspace")],
            vec!["workspace/code".into()],
            false,
        )
        .is_err()
    );
    assert!(
        build_with_directories(
            &root,
            Vec::new(),
            vec!["workspace".into(), "workspace/code".into()],
            false,
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_parent_directory() {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!("inifra-plan-link-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    symlink(std::env::temp_dir(), root.join("linked")).unwrap();
    assert!(build(&root, vec![output("linked/file")], false).is_err());
    fs::remove_dir_all(root).unwrap();
}
