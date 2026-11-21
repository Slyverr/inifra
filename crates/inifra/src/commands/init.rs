use clap::Args;
use inifra_core::{
    apply,
    catalog::{Catalog, Pack},
    inputs::InputDefinition,
    output_rules::ResolvedPackOutput,
    plan::{Action, DirectoryAction, GenerationPlan},
    workflow,
};
use inifra_tui::{
    CatalogView, CategoryView, GenerationReview, InitBackend, InitOutcome, InputPrompt,
    ReviewAction, ReviewedDirectory, ReviewedFile, SelectionPreview, TemplateChoice,
    TemplateInputView, TemplateSourceView, TemplateView, presentation,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs::File,
    io::{self, IsTerminal, Read},
    path::{Path, PathBuf},
};

const PREVIEW_LIMIT: u64 = 16 * 1024;

#[derive(Debug, Args)]
pub(crate) struct InitArgs {
    /// Destination directory.
    #[arg(long, default_value = ".")]
    pub(crate) target: PathBuf,
    /// Replace existing files after approval.
    #[arg(long)]
    pub(crate) overwrite: bool,
}

/// Runs the inline picker, collects inputs, and applies only the reviewed plan.
pub fn run(catalog: &Catalog, target: PathBuf, overwrite: bool) -> Result<(), Box<dyn Error>> {
    run_with_selection(catalog, target, overwrite, &[], BTreeMap::new())
}

pub fn run_with_selection(
    catalog: &Catalog,
    target: PathBuf,
    overwrite: bool,
    selected: &[String],
    values: BTreeMap<String, String>,
) -> Result<(), Box<dyn Error>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "requires an interactive terminal; use `inifra generate <template-id>` for non-interactive use",
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

    match inifra_tui::run_init_with_selection(view, backend, selected, values)? {
        InitOutcome::Applied(review) => print_applied_summary(&review),
        InitOutcome::Cancelled => {}
    }

    Ok(())
}

fn print_applied_summary(review: &GenerationReview) {
    println!(
        "{} {}",
        presentation::paint_emphasis("Target:", presentation::Tone::Accent),
        presentation::paint(
            presentation::terminal_text(&review.target),
            presentation::Tone::Path,
            false
        )
    );

    let mut has_status = false;
    for directory in &review.directories {
        if directory.action != ReviewAction::Create {
            continue;
        }
        println!(
            "{}  {}/",
            presentation::paint("created directory", presentation::Tone::Success, false),
            presentation::paint(
                presentation::terminal_text(&directory.path.display().to_string()),
                presentation::Tone::Path,
                false
            )
        );
        has_status = true;
    }

    for file in &review.files {
        let (label, tone) = match file.action {
            ReviewAction::Create => ("created", presentation::Tone::Success),
            ReviewAction::Replace => ("replaced", presentation::Tone::Warning),
            ReviewAction::Skip => ("kept", presentation::Tone::Muted),
        };
        println!(
            "{}  {}",
            presentation::paint(label, tone, false),
            presentation::paint(
                presentation::terminal_text(&file.path.display().to_string()),
                presentation::Tone::Path,
                false
            )
        );
        has_status = true;
    }

    if !has_status {
        println!(
            "{}",
            presentation::paint(
                "No outputs changed; requested paths already existed.",
                presentation::Tone::Muted,
                false
            )
        );
    }
}

pub(crate) fn validate_picker_selection(
    catalog: &Catalog,
    selected_ids: &[String],
) -> Result<(), Box<dyn Error>> {
    let view = catalog_view(catalog)?;
    let mut seen_ids: Vec<&str> = Vec::new();
    for id in selected_ids {
        if !view
            .templates
            .iter()
            .any(|choice| choice.variants.iter().any(|template| template.id == *id))
        {
            return Err(
                format!("template {id:?} cannot be selected in the initialization picker").into(),
            );
        }

        if seen_ids.contains(&id.as_str()) {
            return Err(format!("preset selects template {id:?} more than once").into());
        }
        seen_ids.push(id);
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

    fn validate_selection(&mut self, selected: &[String]) -> Result<(), String> {
        resolve_selection(self.catalog, selected).map(|_| ())
    }

    fn selection_preview(&mut self, selected: &[String]) -> Result<SelectionPreview, String> {
        if selected.is_empty() {
            return Ok(SelectionPreview::default());
        }

        let resolved = resolve_selection(self.catalog, selected)?;
        let mut outputs = BTreeMap::<String, BTreeMap<PathBuf, PathBuf>>::new();
        for output in resolved {
            outputs
                .entry(output.template_id)
                .or_default()
                .insert(output.source.source, output.output);
        }

        let mut unavailable = BTreeSet::new();
        for pack in &self.catalog.packs {
            if selected.contains(&pack.id) {
                continue;
            }

            let mut candidate = selected.to_vec();
            candidate.push(pack.id.clone());
            if resolve_selection(self.catalog, &candidate).is_err() {
                unavailable.insert(pack.id.clone());
            }
        }

        Ok(SelectionPreview {
            outputs,
            unavailable,
        })
    }
}

fn resolve_selection(
    catalog: &Catalog,
    selected: &[String],
) -> Result<Vec<ResolvedPackOutput>, String> {
    let outputs = catalog
        .resolve_outputs(selected)
        .map_err(|error| error.to_string())?;
    workflow::input_definitions_for_selection(catalog, selected)
        .map_err(|error| error.to_string())?;
    Ok(outputs)
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
    let mut packs = catalog.packs.iter().collect::<Vec<_>>();
    packs.sort_by(|left, right| left.id.cmp(&right.id));
    let entries = packs
        .into_iter()
        .map(|pack| {
            let selected = [pack.id.clone()];
            let resolved = catalog.resolve_outputs(&selected)?;
            Ok((
                pack.category.clone(),
                exclusive_choice_key(&resolved),
                template_view(catalog, pack, &resolved)?,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let mut category_order = Vec::new();
    for (category_id, _, _) in &entries {
        if !category_order.contains(category_id) {
            category_order.push(category_id.clone());
        }
    }

    let mut templates = Vec::<TemplateChoice>::new();
    let mut grouped_choices = BTreeMap::<(String, Vec<PathBuf>), usize>::new();
    for category_id in category_order {
        for (entry_category, choice_key, template) in &entries {
            if entry_category != &category_id {
                continue;
            }

            if let Some(key) = choice_key {
                let key = (category_id.clone(), key.clone());
                if let Some(index) = grouped_choices.get(&key).copied() {
                    templates[index].variants.push(template.clone());
                    continue;
                }
                grouped_choices.insert(key, templates.len());
            }
            templates.push(TemplateChoice {
                category_id: category_id.clone(),
                variants: vec![template.clone()],
            });
        }
    }

    Ok(CatalogView {
        categories,
        templates,
    })
}

fn template_view(
    catalog: &Catalog,
    pack: &Pack,
    resolved: &[ResolvedPackOutput],
) -> Result<TemplateView, Box<dyn Error>> {
    let inputs = catalog
        .input_definitions_for(pack)?
        .into_iter()
        .map(|(name, definition)| TemplateInputView {
            name,
            prompt: definition.prompt,
        })
        .collect();
    let sources = resolved
        .iter()
        .map(|output| {
            Ok(TemplateSourceView {
                source: output.source.source.clone(),
                output: output.output.clone(),
                alternate_output: output
                    .co_selected_output
                    .as_ref()
                    .map(|alternative| alternative.path.clone()),
                content: source_preview(
                    &pack.directory.join(&output.source.source),
                    &output.source.source,
                )?,
            })
        })
        .collect::<Result<_, Box<dyn Error>>>()?;
    Ok(TemplateView {
        id: pack.id.clone(),
        description: pack.description.clone(),
        inputs,
        directories: pack.directories.clone(),
        sources,
    })
}

fn exclusive_choice_key(outputs: &[ResolvedPackOutput]) -> Option<Vec<PathBuf>> {
    if outputs.is_empty()
        || outputs
            .iter()
            .any(|output| output.co_selected_output.is_some())
    {
        return None;
    }

    let mut paths = outputs
        .iter()
        .map(|output| output.output.clone())
        .collect::<Vec<_>>();
    paths.sort();
    Some(paths)
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
    fn co_selected_templates_remain_separate_and_resolve_together() {
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
        assert_eq!(view.templates.len(), 2);
        assert_eq!(
            view.templates
                .iter()
                .map(|choice| choice.variants[0].id.as_str())
                .collect::<Vec<_>>(),
            ["terms-private", "terms-public"]
        );
        assert!(
            view.templates
                .iter()
                .all(|choice| choice.variants.len() == 1)
        );

        let selected = ["terms-private".into(), "terms-public".into()];
        validate_picker_selection(&catalog, &selected).unwrap();
        let mut backend = CoreInitBackend {
            catalog: &catalog,
            target: root.path().join("target"),
            overwrite: false,
        };
        assert!(backend.selection_preview(&[]).unwrap().outputs.is_empty());
        assert_eq!(
            backend.validate_selection(&[]).unwrap_err(),
            "select at least one template"
        );
        let preview = backend.selection_preview(&selected).unwrap();
        assert_eq!(
            preview.outputs["terms-private"][Path::new("TERMS.md")],
            PathBuf::from("TERMS-PRIVATE.md")
        );
        assert_eq!(
            preview.outputs["terms-public"][Path::new("TERMS.md")],
            PathBuf::from("TERMS-PUBLIC.md")
        );
        assert!(!preview.unavailable.contains("terms-private"));
        assert!(!preview.unavailable.contains("terms-public"));
        let (review, _) = backend.prepare(&selected, &BTreeMap::new()).unwrap();
        let mut paths = review
            .files
            .iter()
            .map(|file| file.path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        paths.sort();
        assert_eq!(paths, ["TERMS-PRIVATE.md", "TERMS-PUBLIC.md"]);
    }

    #[test]
    fn templates_with_identical_unconditional_outputs_remain_exclusive_choices() {
        let root = tempfile::tempdir().unwrap();
        for id in ["agents", "rust-agents"] {
            let pack = root.path().join("guidance").join(id);
            std::fs::create_dir_all(&pack).unwrap();
            std::fs::write(
                pack.join("template.toml"),
                "schema_version = 2\ndescription = 'Agent instructions'\n",
            )
            .unwrap();
            std::fs::write(pack.join("AGENTS.md"), format!("# {id}\n")).unwrap();
        }

        let catalog = Catalog::load(root.path()).unwrap();
        let view = catalog_view(&catalog).unwrap();

        assert_eq!(view.templates.len(), 1);
        assert_eq!(
            view.templates[0]
                .variants
                .iter()
                .map(|template| template.id.as_str())
                .collect::<Vec<_>>(),
            ["agents", "rust-agents"]
        );
    }

    #[test]
    fn picker_preview_includes_each_source_and_effective_input_prompt() {
        let root = tempfile::tempdir().unwrap();
        let pack = root.path().join("guidance").join("project-guide");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(
            root.path().join("inputs.toml"),
            "schema_version = 1\n\n[inputs.project_name]\nprompt = 'Project name'\nrequired = true\n",
        )
        .unwrap();
        std::fs::write(
            pack.join("template.toml"),
            "schema_version = 2\ndescription = 'Project guidance'\ninputs = ['project_name']\n",
        )
        .unwrap();
        std::fs::write(pack.join("README.md.j2"), "# {{ project_name }}\n").unwrap();
        std::fs::write(pack.join("AGENTS.md"), "# Agent rules\n").unwrap();

        let catalog = Catalog::load(root.path()).unwrap();
        let view = catalog_view(&catalog).unwrap();
        let template = &view.templates[0].variants[0];

        assert_eq!(template.inputs.len(), 1);
        assert_eq!(template.inputs[0].name, "project_name");
        assert_eq!(template.inputs[0].prompt, "Project name");
        assert_eq!(template.sources.len(), 2);
        assert_eq!(template.sources[0].output, PathBuf::from("AGENTS.md"));
        assert_eq!(template.sources[0].content, "# Agent rules\n");
        assert_eq!(template.sources[1].output, PathBuf::from("README.md"));
        assert_eq!(template.sources[1].content, "# {{ project_name }}\n");
    }

    #[test]
    fn picker_sorts_ids_then_keeps_each_category_together() {
        let root = tempfile::tempdir().unwrap();
        for (category, id) in [("zulu", "z-pack"), ("alpha", "b-pack"), ("zulu", "a-pack")] {
            let pack = root.path().join(category).join(id);
            std::fs::create_dir_all(&pack).unwrap();
            std::fs::write(
                pack.join("template.toml"),
                "schema_version = 2\ndescription = 'Test pack'\n",
            )
            .unwrap();
            std::fs::write(pack.join(format!("{id}.md")), id).unwrap();
        }

        let view = catalog_view(&Catalog::load(root.path()).unwrap()).unwrap();

        assert_eq!(
            view.templates
                .iter()
                .map(|choice| choice.category_id.as_str())
                .collect::<Vec<_>>(),
            ["zulu", "zulu", "alpha"]
        );
        assert_eq!(
            view.templates
                .iter()
                .flat_map(|choice| choice.variants.iter().map(|template| template.id.as_str()))
                .collect::<Vec<_>>(),
            ["a-pack", "z-pack", "b-pack"]
        );
    }

    #[test]
    fn picker_rejects_a_selection_with_conflicting_outputs_before_review() {
        let root = tempfile::tempdir().unwrap();
        for (category, id) in [("docs", "one"), ("docs", "two")] {
            let pack = root.path().join(category).join(id);
            std::fs::create_dir_all(&pack).unwrap();
            std::fs::write(
                pack.join("template.toml"),
                "schema_version = 2\ndescription = 'Test pack'\n",
            )
            .unwrap();
            std::fs::write(pack.join("README.md"), id).unwrap();
        }

        let catalog = Catalog::load(root.path()).unwrap();
        let mut backend = CoreInitBackend {
            catalog: &catalog,
            target: root.path().join("target"),
            overwrite: false,
        };

        let error = backend
            .validate_selection(&["one".into(), "two".into()])
            .unwrap_err();

        assert_eq!(error, "duplicate output README.md");
        let preview = backend.selection_preview(&["one".into()]).unwrap();
        assert!(preview.unavailable.contains("two"));
    }
}
