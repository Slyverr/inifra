use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

use crate::presentation;

pub(crate) fn compact_output_paths(
    outputs: &[inifra_core::output_rules::ResolvedPackOutput],
    directories: &[PathBuf],
) -> Vec<String> {
    let files = compact_paths(outputs.iter().map(|output| output.output.as_path()), false);
    let directories = compact_paths(directories.iter().map(PathBuf::as_path), true);
    files.into_iter().chain(directories).collect()
}

#[derive(Default)]
struct OutputPathNode {
    terminal: bool,
    children: BTreeMap<String, OutputPathNode>,
}

fn compact_paths<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
    directories: bool,
) -> Option<String> {
    let mut root = OutputPathNode::default();
    for path in paths {
        let mut node = &mut root;
        let mut has_component = false;
        for component in path.components() {
            if let Component::Normal(name) = component {
                node = node
                    .children
                    .entry(name.to_string_lossy().into_owned())
                    .or_default();
                has_component = true;
            }
        }

        if has_component {
            node.terminal = true;
        }
    }

    let entries = root
        .children
        .iter()
        .map(|(name, node)| compact_path_entry(name, node, directories))
        .collect::<Vec<_>>();
    (!entries.is_empty()).then(|| entries.join(", "))
}

fn compact_path_entry(name: &str, node: &OutputPathNode, directories: bool) -> String {
    let mut path = name.to_owned();
    let mut current = node;
    while !current.terminal && current.children.len() == 1 {
        let Some((child_name, child)) = current.children.first_key_value() else {
            break;
        };
        path.push('/');
        path.push_str(child_name);
        current = child;
    }

    if current.children.is_empty() {
        if directories && current.terminal {
            path.push('/');
        }

        return path;
    }

    let children = current
        .children
        .iter()
        .map(|(child_name, child)| compact_path_entry(child_name, child, directories))
        .collect::<Vec<_>>()
        .join(", ");
    let nested = format!("{path}/{{{children}}}");
    if directories && current.terminal {
        format!("{path}/, {nested}")
    } else {
        nested
    }
}

pub(crate) fn compact_output_summary(
    outputs: &[inifra_core::output_rules::ResolvedPackOutput],
    directories: &[PathBuf],
) -> String {
    let files = outputs.first().map(|first| {
        let remaining = outputs.len() - 1;
        if remaining == 0 {
            first.output.display().to_string()
        } else {
            format!("{} +{remaining} other files", first.output.display())
        }
    });
    let directories = match directories {
        [directory] if outputs.is_empty() => Some(format!("{}/", directory.display())),
        [] => None,
        directories => {
            let suffix = if directories.len() == 1 {
                "directory"
            } else {
                "directories"
            };

            Some(format!("{} {suffix}", directories.len()))
        }
    };

    match (files, directories) {
        (Some(files), Some(directories)) => format!("{files}, {directories}"),
        (Some(files), None) => files,
        (None, Some(directories)) => directories,
        (None, None) => String::new(),
    }
}

pub(crate) fn fit_list_description(id: &str, description: &str, width: Option<usize>) -> String {
    let Some(width) = width else {
        return description.to_owned();
    };

    let prefix_width = 2 + presentation::visible_width(id) + 2;
    presentation::truncate_to_width(description, width.saturating_sub(prefix_width))
}

pub(crate) fn fit_list_content(
    id: &str,
    description: &str,
    summary: &str,
    width: Option<usize>,
) -> (String, Option<String>) {
    let Some(width) = width else {
        return (
            description.to_owned(),
            (!summary.is_empty()).then(|| summary.to_owned()),
        );
    };

    let prefix_width = 2 + presentation::visible_width(id) + 2;
    let full_width = prefix_width
        + presentation::visible_width(description)
        + if summary.is_empty() {
            0
        } else {
            2 + presentation::visible_width(summary)
        };
    if full_width <= width {
        return (
            description.to_owned(),
            (!summary.is_empty()).then(|| summary.to_owned()),
        );
    }

    (
        presentation::truncate_to_width(description, width.saturating_sub(prefix_width)),
        None,
    )
}

#[cfg(test)]
mod presentation_tests {
    use super::fit_list_content;
    use inifra_tui::presentation::visible_width;

    #[test]
    fn list_omits_secondary_file_summary_when_it_would_wrap() {
        let id = "github-pull-request";
        let description = "GitHub pull request description template";
        let summary = ".github/pull_request_template.md";
        let (fitted_description, fitted_summary) =
            fit_list_content(id, description, summary, Some(55));

        assert!(fitted_summary.is_none());
        assert!(fitted_description.ends_with('…'));
        assert!(visible_width(&format!("  {id}  {fitted_description}")) <= 55);
    }

    #[test]
    fn list_keeps_file_summary_when_it_fits_on_the_same_line() {
        let id = "readme";
        let description = "Project overview";
        let summary = "README.md";
        let (fitted_description, fitted_summary) =
            fit_list_content(id, description, summary, Some(80));

        assert_eq!(fitted_description, description);
        assert_eq!(fitted_summary.as_deref(), Some(summary));
    }

    #[test]
    fn list_keeps_complete_content_without_a_terminal_width() {
        let id = "github-pull-request";
        let description = "GitHub pull request description template";
        let summary = ".github/pull_request_template.md";
        let (fitted_description, fitted_summary) = fit_list_content(id, description, summary, None);

        assert_eq!(fitted_description, description);
        assert_eq!(fitted_summary.as_deref(), Some(summary));
    }
}
