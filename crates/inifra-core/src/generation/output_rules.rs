//! Resolve per-source destination overrides and co-selected alternatives.

use crate::{catalog::Pack, plan, sources::Source};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// A destination used when another selected pack shares the same default path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoSelectedOutput {
    /// Name shared by packs that intentionally coordinate their destinations.
    pub group: String,
    /// Alternate destination path for this pack's source.
    pub path: PathBuf,
}

/// A path rule for one source file in a template pack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputRule {
    /// Source path relative to its pack directory.
    pub source: PathBuf,
    /// Destination to use when no co-selected alternative is activated.
    pub output: Option<PathBuf>,
    /// Alternate destination used when selected packs in the same group collide.
    pub co_selected_output: Option<CoSelectedOutput>,
}

/// A source and its resolved destination after applying selection-aware rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPackOutput {
    /// Template pack that provides the source.
    pub template_id: String,
    /// Discovered source file and its inferred destination.
    pub source: Source,
    /// Final destination relative to the generation root.
    pub output: PathBuf,
    /// Declared alternative, if the source participates in a co-selected group.
    pub co_selected_output: Option<CoSelectedOutput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OutputRuleDefinition {
    source: String,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    co_selected_output: Option<CoSelectedOutputDefinition>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CoSelectedOutputDefinition {
    group: String,
    path: String,
}

pub(crate) fn parse(
    definitions: Vec<OutputRuleDefinition>,
    manifest: &Path,
) -> Result<Vec<OutputRule>, String> {
    let mut sources = BTreeSet::new();
    definitions
        .into_iter()
        .map(|definition| {
            let source = PathBuf::from(definition.source);
            validate_path(&source, manifest)?;
            if !sources.insert(source.clone()) {
                return Err(format!(
                    "{}: duplicate output rule for source {}",
                    manifest.display(),
                    source.display()
                ));
            }

            let output = definition.output.map(PathBuf::from);
            if let Some(path) = &output {
                validate_path(path, manifest)?;
            }

            let co_selected_output = definition
                .co_selected_output
                .map(|alternative| {
                    if !valid_group(&alternative.group) {
                        return Err(format!(
                            "{}: invalid output-rule group {:?}",
                            manifest.display(),
                            alternative.group
                        ));
                    }

                    let path = PathBuf::from(alternative.path);
                    validate_path(&path, manifest)?;
                    Ok(CoSelectedOutput {
                        group: alternative.group,
                        path,
                    })
                })
                .transpose()?;
            if output.is_none() && co_selected_output.is_none() {
                return Err(format!(
                    "{}: output rule for {} needs output or co_selected_output",
                    manifest.display(),
                    source.display()
                ));
            }

            Ok(OutputRule {
                source,
                output,
                co_selected_output,
            })
        })
        .collect()
}

pub(crate) fn resolve(packs: &[(&Pack, &[Source])]) -> Result<Vec<ResolvedPackOutput>, String> {
    let mut candidates = Vec::new();
    for (pack, sources) in packs {
        for rule in &pack.output_rules {
            if !sources.iter().any(|source| source.source == rule.source) {
                return Err(format!(
                    "template {:?} output rule references missing source {}",
                    pack.id,
                    rule.source.display()
                ));
            }
        }

        for source in *sources {
            let rule = pack
                .output_rules
                .iter()
                .find(|rule| rule.source == source.source);
            let output = rule
                .and_then(|rule| rule.output.clone())
                .unwrap_or_else(|| source.output.clone());
            candidates.push(Candidate {
                template_id: pack.id.clone(),
                source: source.clone(),
                output,
                co_selected_output: rule.and_then(|rule| rule.co_selected_output.clone()),
            });
        }
    }

    let mut by_default = BTreeMap::<String, Vec<usize>>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        by_default
            .entry(plan::portable_path_key(&candidate.output))
            .or_default()
            .push(index);
    }

    for matching in by_default.values().filter(|matching| matching.len() > 1) {
        let unique_templates: BTreeSet<_> = matching
            .iter()
            .map(|index| candidates[*index].template_id.as_str())
            .collect();
        if unique_templates.len() != matching.len() {
            continue;
        }

        let Some(group) = candidates[matching[0]]
            .co_selected_output
            .as_ref()
            .map(|output| output.group.as_str())
        else {
            continue;
        };
        if matching.iter().all(|index| {
            candidates[*index]
                .co_selected_output
                .as_ref()
                .is_some_and(|output| output.group == group)
        }) {
            for index in matching {
                if let Some(alternative) = &candidates[*index].co_selected_output {
                    candidates[*index].output = alternative.path.clone();
                }
            }
        }
    }

    Ok(candidates
        .into_iter()
        .map(|candidate| ResolvedPackOutput {
            template_id: candidate.template_id,
            source: candidate.source,
            output: candidate.output,
            co_selected_output: candidate.co_selected_output,
        })
        .collect())
}

fn validate_path(path: &Path, manifest: &Path) -> Result<(), String> {
    plan::validate_paths(std::iter::once(path))
        .map_err(|error| format!("{}: {error}", manifest.display()))
}

fn valid_group(group: &str) -> bool {
    !group.is_empty()
        && group
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

struct Candidate {
    template_id: String,
    source: Source,
    output: PathBuf,
    co_selected_output: Option<CoSelectedOutput>,
}
