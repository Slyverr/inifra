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
