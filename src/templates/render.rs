use std::collections::BTreeMap;

use anyhow::{Context, Result};
use minijinja::{Environment, UndefinedBehavior};

use super::model::Template;

pub(crate) fn render_text_files(
    template: &mut Template,
    values: &BTreeMap<String, String>,
) -> Result<()> {
    for file in &mut template.files {
        if !file.render_text {
            continue;
        }

        let rendered = {
            let source = std::str::from_utf8(&file.contents)
                .with_context(|| {
                    format!(
                        "template {:?} source {} is not UTF-8",
                        template.id,
                        file.path.display()
                    )
                })?
                .to_owned();
            let name = file.path.to_string_lossy().into_owned();
            let mut environment = Environment::new();
            environment.set_undefined_behavior(UndefinedBehavior::Strict);
            environment.set_keep_trailing_newline(true);
            environment.add_template(&name, &source).with_context(|| {
                format!(
                    "template {:?} has invalid syntax in {}",
                    template.id,
                    file.path.display()
                )
            })?;
            environment
                .get_template(&name)
                .with_context(|| format!("could not load template source {}", file.path.display()))?
                .render(values)
                .with_context(|| {
                    format!(
                        "could not render template {:?} output {}",
                        template.id,
                        file.path.display()
                    )
                })?
        };
        file.contents = rendered.into_bytes();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::templates::{TemplateFile, TemplateOrigin};

    fn template(source: &str) -> Template {
        Template {
            id: "test".to_owned(),
            description: "test".to_owned(),
            category: None,
            inputs: Vec::new(),
            origin: TemplateOrigin::Local,
            files: vec![TemplateFile {
                path: PathBuf::from("README.md"),
                contents: source.as_bytes().to_vec(),
                render_text: true,
            }],
        }
    }

    #[test]
    fn renders_values_into_text_files() {
        let mut template = template("# {{ project_name }}\n");
        let values = BTreeMap::from([("project_name".to_owned(), "Example".to_owned())]);

        render_text_files(&mut template, &values).expect("render template");

        assert_eq!(template.files[0].contents, b"# Example\n");
    }

    #[test]
    fn errors_on_undefined_values() {
        let mut template = template("# {{ missing_name }}\n");

        assert!(render_text_files(&mut template, &BTreeMap::new()).is_err());
    }
}
