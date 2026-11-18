use inifra_core::inputs::{InputDefinition, resolve};
use std::collections::BTreeMap;

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
