use std::{collections::BTreeMap, error::Error};

pub(crate) fn parse_assignments(
    assignments: &[String],
) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut values = BTreeMap::new();
    for assignment in assignments {
        let (key, value) = assignment
            .split_once('=')
            .ok_or("--set expects NAME=VALUE")?;
        if values.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(format!("duplicate input {key:?}").into());
        }
    }

    Ok(values)
}
