use inifra_core::{
    apply,
    catalog::{Catalog, Pack},
    inputs::InputDefinition,
    output_rules::ResolvedPackOutput,
    plan::{Action, DirectoryAction, GenerationPlan},
    sources, workflow,
};
use inifra_tui::{
    CatalogView, CategoryView, GenerationReview, InitBackend, InitOutcome, InputPrompt,
    ReviewAction, ReviewedDirectory, ReviewedFile, TemplateChoice, TemplateView, presentation,
};
use std::{
    collections::BTreeMap,
    error::Error,
    fs::File,
    io::{self, IsTerminal, Read},
    path::{Path, PathBuf},
};

const PREVIEW_LIMIT: u64 = 16 * 1024;
type SharedOutputKey = (String, Vec<(String, String)>);

/// Runs the inline picker, collects inputs, and applies only the reviewed plan.
pub fn run(catalog: &Catalog, target: PathBuf, overwrite: bool) -> Result<(), Box<dyn Error>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "`inifra init` requires an interactive terminal; use `inifra generate <template-id>` for non-interactive use",
        )
        .into());
    }

    catalog.validate()?;
    let view = catalog_view(catalog)?;
    let backend = CoreInitBackend {
        catalog,
        target: target.clone(),
        overwrite,
    };
    match inifra_tui::run_init(view, backend)? {
        InitOutcome::Applied => println!(
            "{}",
            presentation::paint(
                format!(
                    "Initialized project files in {}",
                    presentation::terminal_text(&target.display().to_string())
                ),
                presentation::Tone::Success,
                false
            )
        ),
        InitOutcome::Cancelled => {}
    }
    Ok(())
}

struct CoreInitBackend<'a> {
    catalog: &'a Catalog,
    target: PathBuf,
    overwrite: bool,
}

impl InitBackend for CoreInitBackend<'_> {
    type Plan = GenerationPlan;

    fn input_prompts(&mut self, selected: &[String]) -> Result<Vec<InputPrompt>, String> {
        workflow::input_definitions_for_selection(self.catalog, selected)
            .map(|definitions| {
                definitions
                    .into_iter()
                    .map(|(name, definition)| input_prompt(name, definition))
                    .collect()
            })
            .map_err(|error| error.to_string())
    }

    fn prepare(
        &mut self,
        selected: &[String],
        values: &BTreeMap<String, String>,
    ) -> Result<(GenerationReview, Self::Plan), String> {
        let plan = workflow::prepare(self.catalog, selected, values, &self.target, self.overwrite)
            .map_err(|error| error.to_string())?;
        let review = GenerationReview {
            target: self.target.display().to_string(),
            files: plan
                .files
                .iter()
                .map(|file| ReviewedFile {
                    path: file.path.clone(),
                    action: match file.action {
                        Action::Create => ReviewAction::Create,
                        Action::Replace => ReviewAction::Replace,
                        Action::Skip => ReviewAction::Skip,
                    },
                })
                .collect(),
            directories: plan
                .directories
                .iter()
                .map(|directory| ReviewedDirectory {
                    path: directory.path.clone(),
                    action: match directory.action {
                        DirectoryAction::Create => ReviewAction::Create,
                        DirectoryAction::Existing => ReviewAction::Skip,
                    },
                })
                .collect(),
        };
        Ok((review, plan))
    }

    fn apply(&mut self, plan: Self::Plan) -> Result<(), String> {
        apply::apply_plan(&self.target, &plan).map_err(|error| error.to_string())
    }
}

fn input_prompt(name: String, definition: InputDefinition) -> InputPrompt {
    InputPrompt {
        name,
        prompt: definition.prompt,
        required: definition.required,
        default: definition.default,
    }
}

fn catalog_view(catalog: &Catalog) -> Result<CatalogView, Box<dyn Error>> {
    let categories = catalog
        .categories
        .values()
        .map(|category| CategoryView {
            id: category.id.clone(),
            name: category.name.clone(),
        })
        .collect();
    let entries = catalog
        .packs
        .iter()
        .map(|pack| {
            let selected = [pack.id.clone()];
            let resolved = catalog.resolve_outputs(&selected)?;
            Ok((
                pack.category.clone(),
                choice_key(pack, &resolved),
                template_view(pack, &resolved)?,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let mut key_counts = BTreeMap::new();
    for (_, key, _) in &entries {
        if let Some(key) = key {
            *key_counts.entry(key.clone()).or_insert(0usize) += 1;
        }
    }
    let mut consumed = vec![false; entries.len()];
    let mut templates = Vec::new();
    for index in 0..entries.len() {
        if consumed[index] {
            continue;
        }
        let (category_id, key, template) = &entries[index];
        let variants = if key
            .as_ref()
            .is_some_and(|key| key_counts.get(key).is_some_and(|count| *count > 1))
        {
            entries
                .iter()
                .enumerate()
                .filter_map(
                    |(variant_index, (variant_category, variant_key, template))| {
                        if variant_category == category_id && variant_key.as_ref() == key.as_ref() {
                            consumed[variant_index] = true;
                            Some(template.clone())
                        } else {
                            None
                        }
                    },
                )
                .collect()
        } else {
            consumed[index] = true;
            vec![template.clone()]
        };
        templates.push(TemplateChoice {
            category_id: category_id.clone(),
            variants,
        });
    }
    Ok(CatalogView {
        categories,
        templates,
    })
}

fn choice_key(pack: &Pack, outputs: &[ResolvedPackOutput]) -> Option<SharedOutputKey> {
    let mut shared_paths = outputs
        .iter()
        .filter_map(|output| {
            output.co_selected_output.as_ref().map(|alternative| {
                (
                    alternative.group.clone(),
                    portable_output_key(&output.output),
                )
            })
        })
        .collect::<Vec<_>>();
    shared_paths.sort();
    (!shared_paths.is_empty()).then(|| (pack.category.clone(), shared_paths))
}

fn portable_output_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn template_view(
    pack: &Pack,
    resolved: &[ResolvedPackOutput],
) -> Result<TemplateView, Box<dyn Error>> {
    let outputs = resolved
        .iter()
        .map(|output| output.output.clone())
        .collect();
    let discovered = sources::discover(&pack.directory)?;
    let preview_source = discovered.first().map(|source| source.source.clone());
    let preview = match discovered.first() {
        Some(source) => source_preview(&pack.directory.join(&source.source), &source.source)?,
        None if pack.directories.is_empty() => "No file or directory outputs.".to_owned(),
        None => "Directory structure only.".to_owned(),
    };
    Ok(TemplateView {
        id: pack.id.clone(),
        description: pack.description.clone(),
        outputs,
        directories: pack.directories.clone(),
        preview_source,
        preview,
    })
}

fn source_preview(path: &Path, relative: &Path) -> Result<String, Box<dyn Error>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(PREVIEW_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() as u64 > PREVIEW_LIMIT;
    if truncated {
        bytes.truncate(PREVIEW_LIMIT as usize);
    }
    let mut preview = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => format!("[Binary source: {}]", relative.display()),
    };
    if truncated {
        preview.push_str("\n… preview truncated");
    }
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_with_the_same_declared_shared_output_form_one_choice() {
        let root = tempfile::tempdir().unwrap();
        for (id, alternate) in [
            ("terms-public", "TERMS-PUBLIC.md"),
            ("terms-private", "TERMS-PRIVATE.md"),
        ] {
            let pack = root.path().join("documents").join(id);
            std::fs::create_dir_all(&pack).unwrap();
            std::fs::write(
                pack.join("template.toml"),
                format!(
                    "schema_version = 2\ndescription = 'Terms'\n[[output_rules]]\nsource = 'TERMS.md'\nco_selected_output = {{ group = 'terms', path = '{alternate}' }}\n"
                ),
            )
            .unwrap();
            std::fs::write(pack.join("TERMS.md"), "Terms\n").unwrap();
        }
        let catalog = Catalog::load(root.path()).unwrap();
        let view = catalog_view(&catalog).unwrap();
        let choice = view
            .templates
            .iter()
            .find(|choice| {
                choice
                    .variants
                    .iter()
                    .any(|template| template.id == "terms-public")
            })
            .unwrap();

        assert_eq!(
            choice
                .variants
                .iter()
                .map(|template| template.id.as_str())
                .collect::<Vec<_>>(),
            ["terms-private", "terms-public"]
        );
        assert_eq!(choice.variants.len(), 2);
    }
}
