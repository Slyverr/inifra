use inifra_core::{catalog, inputs, workflow};
use std::{collections::BTreeMap, fs};

#[test]
fn prepares_a_selected_pack_without_writing_destination() {
    let root = std::env::temp_dir().join(format!("inifra-workflow-{}", std::process::id()));
    let pack = root.join("catalog/project/readme");
    fs::create_dir_all(&pack).unwrap();
    fs::write(
        pack.join("template.toml"),
        "schema_version = 2\ndescription = 'Readme'\ninputs = ['name']\n",
    )
    .unwrap();
    fs::write(pack.join("README.md.j2"), "# {{ name }}\n").unwrap();
    let definitions_path = root.join("catalog/inputs.toml");
    fs::write(
        &definitions_path,
        "schema_version = 1\n[inputs.name]\nprompt = 'Name'\nrequired = true\n",
    )
    .unwrap();
    let catalog = catalog::discover(&root.join("catalog")).unwrap();
    let definitions = inputs::load(&definitions_path).unwrap();
    let destination = root.join("target");
    let selected = vec!["readme".to_string()];
    assert!(
        workflow::prepare(
            &catalog,
            &definitions,
            &[],
            &BTreeMap::new(),
            &destination,
            false
        )
        .is_err()
    );
    assert!(
        workflow::prepare(
            &catalog,
            &definitions,
            &["unknown".into()],
            &BTreeMap::new(),
            &destination,
            false
        )
        .is_err()
    );
    assert!(
        workflow::prepare(
            &catalog,
            &definitions,
            &["readme".into(), "readme".into()],
            &BTreeMap::new(),
            &destination,
            false
        )
        .is_err()
    );
    assert!(
        workflow::prepare(
            &catalog,
            &definitions,
            &selected,
            &BTreeMap::new(),
            &destination,
            false
        )
        .is_err()
    );
    let result = workflow::prepare(
        &catalog,
        &definitions,
        &selected,
        &BTreeMap::from([("name".into(), "Example".into())]),
        &destination,
        false,
    )
    .unwrap();
    assert_eq!(result[0].contents, b"# Example\n");
    assert!(!destination.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn conflicting_selected_packs_fail_without_creating_destination() {
    let root =
        std::env::temp_dir().join(format!("inifra-conflicting-packs-{}", std::process::id()));
    for id in ["first", "second"] {
        let dir = root.join("catalog/project").join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("template.toml"),
            "schema_version = 2\ndescription = 'Example'\n",
        )
        .unwrap();
        fs::write(dir.join("README.md"), id).unwrap();
    }
    let catalog = catalog::discover(&root.join("catalog")).unwrap();
    let destination = root.join("target");
    let error = workflow::prepare(
        &catalog,
        &BTreeMap::new(),
        &["first".into(), "second".into()],
        &BTreeMap::new(),
        &destination,
        false,
    )
    .unwrap_err();
    assert!(error.to_string().contains("duplicate output"));
    assert!(!destination.exists());
    fs::remove_dir_all(root).unwrap();
}
