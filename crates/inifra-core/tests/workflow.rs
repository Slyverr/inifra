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
fn input_definitions_for_selection_are_shared_and_conflicts_are_rejected() {
    let root = std::env::temp_dir().join(format!(
        "inifra-workflow-input-definitions-{}",
        std::process::id()
    ));
    let catalog_root = root.join("catalog");
    for (id, prompt) in [
        ("first", "Project name"),
        ("second", "Project name"),
        ("conflicting", "Name of the project"),
    ] {
        let pack = catalog_root.join("project").join(id);
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            pack.join("template.toml"),
            format!(
                "schema_version = 2\ndescription = 'Example'\n[local_inputs.project_name]\nprompt = '{prompt}'\nrequired = true\n"
            ),
        )
        .unwrap();
        fs::write(pack.join("README.md.j2"), "{{ project_name }}\n").unwrap();
    }

    let catalog = Catalog::load(&catalog_root).unwrap();
    let definitions =
        workflow::input_definitions_for_selection(&catalog, &["first".into(), "second".into()])
            .unwrap();
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions["project_name"].prompt, "Project name");
    assert!(definitions["project_name"].required);

    let error = workflow::input_definitions_for_selection(
        &catalog,
        &["first".into(), "conflicting".into()],
    )
    .unwrap_err();
    assert!(error.to_string().contains("conflicting definitions"));
    assert!(!root.join("target").exists());
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
fn references_use_final_target_paths_only_when_the_target_is_selected() {
    let root =
        std::env::temp_dir().join(format!("inifra-workflow-references-{}", std::process::id()));
    let catalog_root = root.join("catalog");
    let readme = catalog_root.join("project-files/readme");
    fs::create_dir_all(&readme).unwrap();
    fs::write(
        readme.join("template.toml"),
        "schema_version = 2\ndescription = 'Readme'\n[references.contributing]\ntemplate = 'contributing'\nsource = 'CONTRIBUTING.md.j2'\n",
    )
    .unwrap();
    fs::write(
        readme.join("README.md.j2"),
        "{% if references.contributing %}See {{ references.contributing }}{% else %}No guide{% endif %}\n",
    )
    .unwrap();

    let contributing = catalog_root.join("project-files/contributing");
    fs::create_dir_all(&contributing).unwrap();
    fs::write(
        contributing.join("template.toml"),
        "schema_version = 2\ndescription = 'Contributing'\n[[output_rules]]\nsource = 'CONTRIBUTING.md.j2'\nco_selected_output = { group = 'guides', path = 'docs/CONTRIBUTING.md' }\n",
    )
    .unwrap();
    fs::write(contributing.join("CONTRIBUTING.md.j2"), "Guide\n").unwrap();

    let duplicate = catalog_root.join("project-files/legacy-guide");
    fs::create_dir_all(&duplicate).unwrap();
    fs::write(
        duplicate.join("template.toml"),
        "schema_version = 2\ndescription = 'Legacy guide'\n[[output_rules]]\nsource = 'CONTRIBUTING.md'\nco_selected_output = { group = 'guides', path = 'docs/LEGACY-CONTRIBUTING.md' }\n",
    )
    .unwrap();
    fs::write(duplicate.join("CONTRIBUTING.md"), "Legacy\n").unwrap();

    let catalog = Catalog::load(&catalog_root).unwrap();
    catalog.validate().unwrap();
    let destination = root.join("target");
    let selected = workflow::prepare(
        &catalog,
        &[
            "readme".into(),
            "contributing".into(),
            "legacy-guide".into(),
        ],
        &BTreeMap::new(),
        &destination,
        false,
    )
    .unwrap();
    let readme = selected
        .files
        .iter()
        .find(|file| file.path.as_path() == std::path::Path::new("README.md"))
        .unwrap();
    assert_eq!(readme.contents, b"See docs/CONTRIBUTING.md\n");
    assert!(selected.files.iter().any(|file| {
        file.path.as_path() == std::path::Path::new("docs/CONTRIBUTING.md")
            && file.contents == b"Guide\n"
    }));

    let unselected = workflow::prepare(
        &catalog,
        &["readme".into()],
        &BTreeMap::new(),
        &destination,
        false,
    )
    .unwrap();
    assert_eq!(unselected.files[0].contents, b"No guide\n");
    assert!(!destination.exists());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_reference_sources_fail_before_planning_or_writing() {
    let root = std::env::temp_dir().join(format!(
        "inifra-workflow-invalid-reference-{}",
        std::process::id()
    ));
    let catalog_root = root.join("catalog");
    for (id, manifest, source) in [
        (
            "readme",
            "schema_version = 2\ndescription = 'Readme'\n[references.guide]\ntemplate = 'guide'\nsource = 'MISSING.md'\n",
            "README.md.j2",
        ),
        (
            "guide",
            "schema_version = 2\ndescription = 'Guide'\n",
            "GUIDE.md",
        ),
    ] {
        let pack = catalog_root.join("project").join(id);
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("template.toml"), manifest).unwrap();
        fs::write(pack.join(source), "Content").unwrap();
    }
    let catalog = Catalog::load(&catalog_root).unwrap();
    let destination = root.join("target");
    let error = workflow::prepare(
        &catalog,
        &["readme".into()],
        &BTreeMap::new(),
        &destination,
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("points to missing source"), "{error}");
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
