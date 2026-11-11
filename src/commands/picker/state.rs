use super::text;
use crate::templates::Template;
use std::path::Path;

pub(super) enum PickerEntry<'a> {
    Category(&'a str),
    Blank,
    Template(usize, &'a TemplateRow),
}

pub(super) struct TemplateRow {
    pub(super) category: String,
    pub(super) alternatives: Vec<usize>,
}

pub(super) struct Picker {
    pub(super) selected: Vec<bool>,
    pub(super) choices: Vec<usize>,
    pub(super) cursor: usize,
    pub(super) preview_scroll: usize,
    pub(super) multiple: bool,
    pub(super) notice: Option<String>,
}

impl Picker {
    pub(super) fn move_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
        self.preview_scroll = 0;
        self.notice = None;
    }

    pub(super) fn move_down(&mut self, count: usize) {
        self.cursor = (self.cursor + 1).min(count.saturating_sub(1));
        self.preview_scroll = 0;
        self.notice = None;
    }

    pub(super) fn move_category(&mut self, rows: &[TemplateRow], backward: bool) {
        let starts = category_starts(rows);
        let current_group = starts
            .partition_point(|start| *start <= self.cursor)
            .saturating_sub(1);
        let target_group = if backward {
            current_group.checked_sub(1)
        } else {
            (current_group + 1 < starts.len()).then_some(current_group + 1)
        };
        if let Some(target_group) = target_group {
            self.cursor = starts[target_group];
            self.preview_scroll = 0;
            self.notice = None;
        }
    }

    pub(super) fn move_variant(
        &mut self,
        templates: &[&Template],
        rows: &[TemplateRow],
        backward: bool,
    ) {
        let row = &rows[self.cursor];
        if row.alternatives.len() < 2 {
            return;
        }
        let current = row
            .alternatives
            .iter()
            .position(|index| *index == self.choices[self.cursor])
            .unwrap_or_default();
        let next = if backward {
            (current + row.alternatives.len() - 1) % row.alternatives.len()
        } else {
            (current + 1) % row.alternatives.len()
        };
        let next_choice = row.alternatives[next];
        let row_selected = row.alternatives.iter().any(|index| self.selected[*index]);
        if row_selected {
            let mut selection = self.selected.clone();
            for index in &row.alternatives {
                selection[*index] = false;
            }
            selection[next_choice] = true;
            if let Some(conflict) = find_selection_conflict(templates, &selection) {
                self.set_conflict_notice(templates, conflict);
                return;
            }
            self.selected = selection;
        }
        self.choices[self.cursor] = next_choice;
        self.preview_scroll = 0;
        self.notice = None;
    }

    pub(super) fn toggle_category(&mut self, templates: &[&Template], rows: &[TemplateRow]) {
        if !self.multiple {
            return;
        }
        let starts = category_starts(rows);
        let current_group = starts
            .partition_point(|start| *start <= self.cursor)
            .saturating_sub(1);
        let start = starts[current_group];
        let end = starts.get(current_group + 1).copied().unwrap_or(rows.len());
        let select_group = rows[start..end]
            .iter()
            .any(|row| !row.alternatives.iter().any(|index| self.selected[*index]));
        let mut selection = self.selected.clone();
        for (offset, row) in rows[start..end].iter().enumerate() {
            for index in &row.alternatives {
                selection[*index] = false;
            }
            if select_group {
                selection[self.choices[start + offset]] = true;
            }
        }
        if select_group && let Some(conflict) = find_selection_conflict(templates, &selection) {
            self.set_conflict_notice(templates, conflict);
            return;
        }
        self.selected = selection;
        self.notice = None;
    }

    pub(super) fn scroll_preview_up(&mut self, amount: usize) {
        self.preview_scroll = self.preview_scroll.saturating_sub(amount);
    }

    pub(super) fn scroll_preview_down(&mut self, amount: usize) {
        self.preview_scroll = self.preview_scroll.saturating_add(amount);
    }

    pub(super) fn toggle_current(&mut self, templates: &[&Template], rows: &[TemplateRow]) {
        let row = &rows[self.cursor];
        let row_selected = row.alternatives.iter().any(|index| self.selected[*index]);
        if self.multiple {
            let selecting = !row_selected;
            let mut selection = self.selected.clone();
            for index in &row.alternatives {
                selection[*index] = false;
            }
            if selecting {
                selection[self.choices[self.cursor]] = true;
                if let Some(conflict) = find_selection_conflict(templates, &selection) {
                    self.set_conflict_notice(templates, conflict);
                    return;
                }
            }
            self.selected = selection;
        } else {
            self.selected.fill(false);
            self.selected[self.choices[self.cursor]] = true;
        }
        self.notice = None;
    }

    fn set_conflict_notice(&mut self, templates: &[&Template], conflict: SelectionConflict<'_>) {
        let first = &templates[conflict.first].id;
        let second = &templates[conflict.second].id;
        let description = text::OUTPUT_CONFLICT_NOTICE;
        let detail = conflict.path.to_string_lossy().escape_debug().to_string();
        self.notice = Some(format!(
            "{first} and {second}{description}{detail}{}",
            text::CONFLICT_NOTICE_SUFFIX
        ));
    }

    pub(super) fn confirm(&mut self, templates: &[&Template]) -> Option<Vec<String>> {
        let selected = self
            .selected
            .iter()
            .zip(templates)
            .filter(|(selected, _)| **selected)
            .map(|(_, template)| template.id.clone())
            .collect::<Vec<_>>();
        if selected.is_empty() {
            self.notice = Some(text::NOTHING_SELECTED.to_owned());
            return None;
        }
        if let Some(conflict) = find_selection_conflict(templates, &self.selected) {
            self.set_conflict_notice(templates, conflict);
            return None;
        }
        Some(selected)
    }
}

pub(super) fn template_rows(templates: &[&Template]) -> Vec<TemplateRow> {
    let mut rows: Vec<TemplateRow> = Vec::new();
    for (index, template) in templates.iter().enumerate() {
        let category = template_category(template);
        // Group only identical output sets, so each marquee option is a true alternative.
        if let Some(row) = rows.iter_mut().find(|row| {
            row.category == category && same_output_paths(templates[row.alternatives[0]], template)
        }) {
            row.alternatives.push(index);
        } else {
            rows.push(TemplateRow {
                category: category.to_owned(),
                alternatives: vec![index],
            });
        }
    }
    rows
}

fn same_output_paths(left: &Template, right: &Template) -> bool {
    left.files.len() == right.files.len()
        && left.files.iter().all(|left_file| {
            right
                .files
                .iter()
                .any(|right_file| right_file.path == left_file.path)
        })
}

pub(super) fn category_starts(rows: &[TemplateRow]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut previous = None;
    for (index, row) in rows.iter().enumerate() {
        let category = row.category.as_str();
        if previous != Some(category) {
            starts.push(index);
            previous = Some(category);
        }
    }
    starts
}

struct SelectionConflict<'a> {
    first: usize,
    second: usize,
    path: &'a Path,
}

fn find_selection_conflict<'a>(
    templates: &'a [&Template],
    selected: &[bool],
) -> Option<SelectionConflict<'a>> {
    for first in 0..templates.len() {
        if !selected[first] {
            continue;
        }
        for second in first + 1..templates.len() {
            if !selected[second] {
                continue;
            }
            for first_file in &templates[first].files {
                if templates[second].files.iter().any(|second_file| {
                    second_file.path.starts_with(&first_file.path)
                        || first_file.path.starts_with(&second_file.path)
                }) {
                    return Some(SelectionConflict {
                        first,
                        second,
                        path: &first_file.path,
                    });
                }
            }
        }
    }
    None
}

fn template_category(template: &Template) -> &str {
    template.category.as_deref().unwrap_or(text::OTHER_CATEGORY)
}

pub(super) fn templates_in_picker_order<'a>(templates: &[&'a Template]) -> Vec<&'a Template> {
    let mut groups: Vec<(&str, Vec<&Template>)> = Vec::new();
    for template in templates {
        let category = template_category(template);
        if let Some((_, group)) = groups.iter_mut().find(|(name, _)| *name == category) {
            group.push(*template);
        } else {
            groups.push((category, vec![*template]));
        }
    }
    groups.into_iter().flat_map(|(_, group)| group).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::{TemplateFile, TemplateOrigin};
    use std::path::PathBuf;

    fn template(id: &str, category: &str, paths: &[&str]) -> Template {
        Template {
            id: id.to_owned(),
            description: id.to_owned(),
            category: Some(category.to_owned()),
            inputs: Vec::new(),
            origin: TemplateOrigin::Builtin,
            files: paths
                .iter()
                .map(|path| TemplateFile {
                    path: PathBuf::from(path),
                    contents: Vec::new(),
                    render_text: false,
                })
                .collect(),
        }
    }

    #[test]
    fn groups_templates_with_the_same_outputs_as_alternatives() {
        let templates = [
            template("license-mit", "Legal", &["LICENSE", "NOTICE"]),
            template("license-apache", "Legal", &["NOTICE", "LICENSE"]),
            template("agents", "Guidance", &["AGENTS.md"]),
        ];
        let references = templates.iter().collect::<Vec<_>>();

        let rows = template_rows(&references);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].alternatives, [0, 1]);
        assert_eq!(rows[1].alternatives, [2]);
    }

    #[test]
    fn cycling_a_selected_alternative_keeps_only_one_template_selected() {
        let templates = [
            template("license-mit", "Legal", &["LICENSE"]),
            template("license-apache", "Legal", &["LICENSE"]),
        ];
        let references = templates.iter().collect::<Vec<_>>();
        let rows = template_rows(&references);
        let mut picker = Picker {
            selected: vec![true, false],
            choices: vec![0],
            cursor: 0,
            preview_scroll: 0,
            multiple: true,
            notice: None,
        };

        picker.move_variant(&references, &rows, false);

        assert_eq!(picker.choices, [1]);
        assert_eq!(picker.selected, [false, true]);
    }

    #[test]
    fn selection_rejects_templates_with_overlapping_outputs() {
        let templates = [
            template("readme", "Guidance", &["README.md"]),
            template("other", "Setup", &["README.md/generated"]),
        ];
        let references = templates.iter().collect::<Vec<_>>();
        let rows = template_rows(&references);
        let mut picker = Picker {
            selected: vec![true, false],
            choices: vec![0, 1],
            cursor: 1,
            preview_scroll: 0,
            multiple: true,
            notice: None,
        };

        picker.toggle_current(&references, &rows);

        assert_eq!(picker.selected, [true, false]);
        assert!(
            picker
                .notice
                .as_deref()
                .is_some_and(|notice| notice.contains("README.md"))
        );
    }
}
