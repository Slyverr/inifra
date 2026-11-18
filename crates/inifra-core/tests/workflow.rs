use inifra_core::{catalog::Catalog, workflow};
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
    let catalog = Catalog::load(&root.join("catalog")).unwrap();
    let destination = root.join("target");
    let selected = vec!["readme".to_string()];
    assert!(workflow::prepare(&catalog, &[], &BTreeMap::new(), &destination, false).is_err());
    assert!(
        workflow::prepare(
            &catalog,
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
            &["readme".into(), "readme".into()],
            &BTreeMap::new(),
            &destination,
            false
        )
        .is_err()
    );
    assert!(workflow::prepare(&catalog, &selected, &BTreeMap::new(), &destination, false).is_err());
    let result = workflow::prepare(
        &catalog,
        &selected,
        &BTreeMap::from([("name".into(), "Example".into())]),
        &destination,
        false,
    )
    .unwrap();
    assert_eq!(result.files[0].contents, b"# Example\n");
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
    let catalog = Catalog::load(&root.join("catalog")).unwrap();
    let destination = root.join("target");
    let error = workflow::prepare(
        &catalog,
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

#[test]
fn co_selected_packs_render_to_their_declared_alternate_paths() {
    let root =
        std::env::temp_dir().join(format!("inifra-output-alternatives-{}", std::process::id()));
    let catalog_root = root.join("catalog");
    for (id, alternative) in [
        ("license-mit", "LICENSE-MIT"),
        ("license-apache", "LICENSE-APACHE"),
    ] {
        let pack = catalog_root.join("project-policies").join(id);
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            pack.join("template.toml"),
            format!(
                "schema_version = 2\ndescription = 'License'\n[[output_rules]]\nsource = 'LICENSE.j2'\nco_selected_output = {{ group = 'licenses', path = '{alternative}' }}\n"
            ),
        )
        .unwrap();
        fs::write(pack.join("LICENSE.j2"), id).unwrap();
    }

    let catalog = Catalog::load(&catalog_root).unwrap();
    let destination = root.join("target");
    let plan = workflow::prepare(
        &catalog,
        &["license-mit".into(), "license-apache".into()],
        &BTreeMap::new(),
        &destination,
        false,
    )
    .unwrap();
    assert_eq!(plan.files.len(), 2);
    assert_eq!(plan.files[0].path, std::path::PathBuf::from("LICENSE-MIT"));
    assert_eq!(plan.files[0].contents, b"license-mit");
    assert_eq!(
        plan.files[1].path,
        std::path::PathBuf::from("LICENSE-APACHE")
    );
    assert_eq!(plan.files[1].contents, b"license-apache");
    assert!(!destination.exists());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn co_selected_alternative_collisions_fail_before_creating_destination() {
    let root = std::env::temp_dir().join(format!(
        "inifra-output-alternative-collision-{}",
        std::process::id()
    ));
    let catalog_root = root.join("catalog");
    for id in ["first", "second"] {
        let pack = catalog_root.join("project").join(id);
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            pack.join("template.toml"),
            "schema_version = 2\ndescription = 'Example'\n[[output_rules]]\nsource = 'TERMS.md'\nco_selected_output = { group = 'terms', path = 'TERMS-ALT.md' }\n",
        )
        .unwrap();
        fs::write(pack.join("TERMS.md"), id).unwrap();
    }

    let catalog = Catalog::load(&catalog_root).unwrap();
    let destination = root.join("target");
    let error = workflow::prepare(
        &catalog,
        &["first".into(), "second".into()],
        &BTreeMap::new(),
        &destination,
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("duplicate output"), "{error}");
    assert!(!destination.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn co_selected_packs_with_different_groups_keep_the_collision_error() {
    let root = std::env::temp_dir().join(format!(
        "inifra-output-alternative-groups-{}",
        std::process::id()
    ));
    let catalog_root = root.join("catalog");
    for (id, group, alternative) in [
        ("first", "terms", "TERMS-FIRST.md"),
        ("second", "legal", "TERMS-SECOND.md"),
    ] {
        let pack = catalog_root.join("project").join(id);
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            pack.join("template.toml"),
            format!(
                "schema_version = 2\ndescription = 'Example'\n[[output_rules]]\nsource = 'TERMS.md'\nco_selected_output = {{ group = '{group}', path = '{alternative}' }}\n"
            ),
        )
        .unwrap();
        fs::write(pack.join("TERMS.md"), id).unwrap();
    }

    let catalog = Catalog::load(&catalog_root).unwrap();
    let destination = root.join("target");
    let error = workflow::prepare(
        &catalog,
        &["first".into(), "second".into()],
        &BTreeMap::new(),
        &destination,
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("duplicate output"), "{error}");
    assert!(!destination.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn selected_packs_share_matching_effective_input_definitions() {
    let root = std::env::temp_dir().join(format!("inifra-shared-input-{}", std::process::id()));
    let catalog_root = root.join("catalog");
    fs::create_dir_all(&catalog_root).unwrap();
    fs::write(
        catalog_root.join("inputs.toml"),
        "schema_version = 1\n[inputs.name]\nprompt = 'Name'\ndefault = 'Example'\n",
    )
    .unwrap();
    for (id, source) in [("first", "first.md.j2"), ("second", "second.md.j2")] {
        let pack = catalog_root.join("project").join(id);
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            pack.join("template.toml"),
            "schema_version = 2\ndescription = 'Example'\ninputs = ['name']\n",
        )
        .unwrap();
        fs::write(pack.join(source), "{{ name }}").unwrap();
    }

    let catalog = Catalog::load(&catalog_root).unwrap();
    let plan = workflow::prepare(
        &catalog,
        &["first".into(), "second".into()],
        &BTreeMap::new(),
        &root.join("target"),
        false,
    )
    .unwrap();
    assert_eq!(plan.files.len(), 2);
    assert!(plan.files.iter().all(|file| file.contents == b"Example"));
    assert!(!root.join("target").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn selected_packs_with_conflicting_effective_inputs_fail_before_planning() {
    let root = std::env::temp_dir().join(format!("inifra-input-conflict-{}", std::process::id()));
    let catalog_root = root.join("catalog");
    for (category, prompt, id) in [
        ("alpha", "Alpha name", "first"),
        ("beta", "Beta name", "second"),
    ] {
        let pack = catalog_root.join(category).join(id);
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            catalog_root.join(category).join("inputs.toml"),
            format!("schema_version = 1\n[inputs.name]\nprompt = '{prompt}'\n"),
        )
        .unwrap();
        fs::write(
            pack.join("template.toml"),
            "schema_version = 2\ndescription = 'Example'\ninputs = ['name']\n",
        )
        .unwrap();
        fs::write(pack.join("README.md.j2"), "{{ name }}").unwrap();
    }

    let catalog = Catalog::load(&catalog_root).unwrap();
    let destination = root.join("target");
    let error = workflow::prepare(
        &catalog,
        &["first".into(), "second".into()],
        &BTreeMap::new(),
        &destination,
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("conflicting definitions for input \"name\""),
        "{error}"
    );
    assert!(!destination.exists());
    fs::remove_dir_all(root).unwrap();
}
