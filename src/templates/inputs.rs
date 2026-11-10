use std::collections::{BTreeMap, HashSet};
use std::fs;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::model::Template;

const BUILTIN_INPUTS: &str = include_str!("../../templates/inputs.toml");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputFile {
    schema_version: u32,
    inputs: BTreeMap<String, InputDefinition>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputDefinition {
    prompt: String,
    #[serde(default)]
    required: bool,
    default: Option<String>,
}

pub(crate) fn resolve_values(
    template: &Template,
    assignments: &[String],
) -> Result<BTreeMap<String, String>> {
    let mut definitions = parse(BUILTIN_INPUTS)?;
    if let Some(path) = user_input_path() {
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    bail!(
                        "user input definitions must be a regular file: {}",
                        path.display()
                    );
                }
                let source = fs::read_to_string(&path).with_context(|| {
                    format!("could not read user input definitions {}", path.display())
                })?;
                for (name, definition) in parse(&source)
                    .with_context(|| format!("invalid user input definitions {}", path.display()))?
                {
                    definitions.insert(name, definition);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "could not inspect user input definitions {}",
                        path.display()
                    )
                });
            }
        }
    }

    let mut values = BTreeMap::new();
    let declared = template
        .inputs
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    for assignment in assignments {
        let (name, value) = assignment
            .split_once('=')
            .with_context(|| format!("input must use KEY=VALUE syntax: {assignment:?}"))?;
        if !valid_name(name) {
            bail!("invalid input name in assignment: {name:?}");
        }
        if !declared.contains(name) {
            bail!("template {:?} does not use input {name:?}", template.id);
        }
        if values.insert(name.to_owned(), value.to_owned()).is_some() {
            bail!("input {name:?} was provided more than once");
        }
    }

    for name in &template.inputs {
        let definition = definitions.get(name).with_context(|| {
            format!(
                "template {:?} refers to undefined input {name:?}",
                template.id
            )
        })?;
        if !values.contains_key(name) {
            if let Some(default) = &definition.default {
                values.insert(name.clone(), default.clone());
            } else if definition.required {
                bail!(
                    "input {name:?} is required ({}); provide it with --set {name}=VALUE",
                    definition.prompt
                );
            } else {
                values.insert(name.clone(), String::new());
            }
        }
        if definition.required && values[name].trim().is_empty() {
            bail!("input {name:?} is required and cannot be empty");
        }
    }

    Ok(values)
}

fn parse(source: &str) -> Result<BTreeMap<String, InputDefinition>> {
    let file: InputFile = toml::from_str(source).context("could not parse input definitions")?;
    if file.schema_version != 1 {
        bail!(
            "unsupported input definition schema version {}; supported version is 1",
            file.schema_version
        );
    }
    for (name, definition) in &file.inputs {
        if !valid_name(name) {
            bail!("invalid input name: {name:?}");
        }
        if definition.prompt.trim().is_empty() {
            bail!("input {name:?} must have a prompt label");
        }
        if definition.required
            && definition
                .default
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
        {
            bail!("required input {name:?} must not have an empty default");
        }
    }
    Ok(file.inputs)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.as_bytes()[0].is_ascii_digit()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn user_input_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|path| path.join("inifra").join("inputs.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_input_definitions() {
        let source = "schema_version = 1\n[inputs.project_name]\nprompt = \"Project name\"\nrequired = true\n";
        assert!(parse(source).is_ok());
    }

    #[test]
    fn rejects_unknown_fields_and_invalid_names() {
        let unknown = "schema_version = 1\nextra = true\n";
        assert!(parse(unknown).is_err());
        let invalid_name = "schema_version = 1\n[inputs.ProjectName]\nprompt = \"Name\"\n";
        assert!(parse(invalid_name).is_err());
    }
}
