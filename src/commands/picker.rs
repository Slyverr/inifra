use anyhow::{Context, Result};
use dialoguer::{MultiSelect, console::Term, theme::ColorfulTheme};

use crate::templates::Template;

pub(super) fn choose(templates: &[Template]) -> Result<Option<Vec<String>>> {
    let term = Term::stderr();
    let (rows, columns) = term.size();
    let visible_rows = usize::from(rows.saturating_sub(5)).max(1);
    let label_width = usize::from(columns).saturating_sub(12).max(24);

    let mut ordered = templates.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.category
            .as_deref()
            .unwrap_or("Other")
            .cmp(right.category.as_deref().unwrap_or("Other"))
            .then_with(|| left.id.cmp(&right.id))
    });
    let labels = ordered
        .iter()
        .map(|template| {
            let category = template.category.as_deref().unwrap_or("Other");
            let label = format!("{category} · {} — {}", template.id, template.description);
            truncate(&label, label_width)
        })
        .collect::<Vec<_>>();

    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Choose project files (Space toggles, Enter continues)")
        .items(&labels)
        .max_length(visible_rows)
        .interact_opt()
        .context("could not read template selection")?;

    Ok(selections.map(|indices| {
        indices
            .into_iter()
            .filter_map(|index| ordered.get(index).map(|template| template.id.clone()))
            .collect()
    }))
}

fn truncate(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let visible = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{visible}…")
    } else {
        visible
    }
}
