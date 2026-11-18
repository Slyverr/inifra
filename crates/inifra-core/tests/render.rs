use inifra_core::{render::render, sources::discover};
use std::{collections::BTreeMap, fs};

#[test]
fn renders_templates_and_preserves_static_bytes() {
    let root = std::env::temp_dir().join(format!("inifra-render-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("README.md.j2"), "# {{ name }}\n").unwrap();
    fs::write(root.join("static.bin"), [0, 255, 10]).unwrap();
    let sources = discover(&root).unwrap();
    assert!(render(&root, &sources, &BTreeMap::new()).is_err());
    let result = render(
        &root,
        &sources,
        &BTreeMap::from([("name".into(), "Inifra".into())]),
    )
    .unwrap();
    assert_eq!(result[0].contents, b"# Inifra\n");
    assert_eq!(result[1].contents, [0, 255, 10]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_source_paths_outside_pack() {
    use inifra_core::sources::Source;
    let root = std::env::temp_dir();
    let source = Source {
        source: "../secret".into(),
        output: "output".into(),
        render: false,
    };
    assert!(render(&root, &[source], &BTreeMap::new()).is_err());
}

#[cfg(unix)]
#[test]
fn rejects_source_replaced_by_symlink_after_discovery() {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!("inifra-render-link-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.txt");
    fs::write(&source, "original").unwrap();
    let discovered = discover(&root).unwrap();
    fs::remove_file(&source).unwrap();
    fs::write(root.join("outside"), "other content").unwrap();
    symlink(root.join("outside"), &source).unwrap();
    assert!(render(&root, &discovered, &BTreeMap::new()).is_err());
    fs::remove_dir_all(root).unwrap();
}
