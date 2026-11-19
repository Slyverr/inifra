use inifra_core::inputs::{InputDefinition, resolve};
use std::collections::BTreeMap;

#[cfg(unix)]
use std::fs;

#[test]
fn resolves_defaults_and_explicit_values() {
    let definitions = BTreeMap::from([(
        "name".into(),
        InputDefinition {
            prompt: "Name".into(),
            required: true,
            default: Some("default".into()),
        },
    )]);
    assert_eq!(
        resolve(&definitions, &BTreeMap::new()).unwrap()["name"],
        "default"
    );
    assert_eq!(
        resolve(
            &definitions,
            &BTreeMap::from([("name".into(), "chosen".into())])
        )
        .unwrap()["name"],
        "chosen"
    );
    assert!(
        resolve(
            &definitions,
            &BTreeMap::from([("unknown".into(), "x".into())])
        )
        .is_err()
    );
    assert!(
        resolve(
            &definitions,
            &BTreeMap::from([("name".into(), "  ".into())])
        )
        .is_err()
    );
}

#[test]
fn reserves_references_as_a_template_context_namespace() {
    let path = std::env::temp_dir().join(format!("inifra-reserved-input-{}", std::process::id()));
    std::fs::write(
        &path,
        "schema_version = 1\n[inputs.references]\nprompt = 'Reference value'\n",
    )
    .unwrap();
    let error = inifra_core::inputs::load(&path).unwrap_err().to_string();
    assert!(
        error.contains("invalid definition \"references\""),
        "{error}"
    );
    std::fs::remove_file(path).unwrap();
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_input_definitions() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("inifra-inputs-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let target = root.join("outside.toml");
    fs::write(&target, "schema_version = 1\n").unwrap();
    let path = root.join("inputs.toml");
    symlink(&target, &path).unwrap();

    let error = inifra_core::inputs::load(&path).unwrap_err().to_string();
    assert!(error.contains("regular file"), "{error}");

    fs::remove_dir_all(root).unwrap();
}
