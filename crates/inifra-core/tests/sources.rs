use inifra_core::sources::discover;
use std::fs;

#[test]
fn discovers_nested_rendered_and_static_sources() {
    let root = std::env::temp_dir().join(format!("inifra-sources-{}", std::process::id()));
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("template.toml"), "manifest").unwrap();
    fs::write(root.join(".editorconfig"), "static").unwrap();
    fs::write(root.join("docs/guide.md.j2"), "template").unwrap();
    let sources = discover(&root).unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[1].output, std::path::Path::new("docs/guide.md"));
    assert!(sources[1].render);
    assert!(!sources[0].render);
    fs::remove_dir_all(root).unwrap();
}
