use inifra_core::{
    plan::{Action, build},
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
