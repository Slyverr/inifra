use super::text;
use crate::templates::Template;
use std::path::Path;

pub(super) enum PickerEntry<'a> {
    Category(&'a str),
    Blank,
    NoResults,
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
    pub(super) query: String,
    pub(super) search_mode: bool,
}

impl Picker {
    pub(super) fn move_up(&mut self, visible_rows: &[usize]) {
        let Some(position) = visible_rows.iter().position(|row| *row == self.cursor) else {
            return;
        };
        self.cursor = visible_rows[position.saturating_sub(1)];
        self.preview_scroll = 0;
        self.notice = None;
    }

    pub(super) fn move_down(&mut self, visible_rows: &[usize]) {
        let Some(position) = visible_rows.iter().position(|row| *row == self.cursor) else {
            return;
        };
        self.cursor = visible_rows[(position + 1).min(visible_rows.len().saturating_sub(1))];
        self.preview_scroll = 0;
        self.notice = None;
    }

    pub(super) fn move_category(
        &mut self,
        rows: &[TemplateRow],
        visible_rows: &[usize],
        backward: bool,
    ) {
        let starts = category_starts(rows, visible_rows);
        if starts.is_empty() {
            return;
        }
        let position = visible_rows
            .iter()
            .position(|row| *row == self.cursor)
            .unwrap_or_default();
        let current_group = starts
            .partition_point(|start| *start <= position)
            .saturating_sub(1);
        let target_group = if backward {
            current_group.checked_sub(1)
        } else {
            (current_group + 1 < starts.len()).then_some(current_group + 1)
        };
        if let Some(target_group) = target_group {
            self.cursor = visible_rows[starts[target_group]];
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

    pub(super) fn toggle_category(
        &mut self,
        templates: &[&Template],
        rows: &[TemplateRow],
        visible_rows: &[usize],
    ) {
        if !self.multiple {
            return;
        }
        let starts = category_starts(rows, visible_rows);
        if starts.is_empty() {
            return;
        }
        let position = visible_rows
            .iter()
            .position(|row| *row == self.cursor)
            .unwrap_or_default();
        let current_group = starts
            .partition_point(|start| *start <= position)
            .saturating_sub(1);
        let start = starts[current_group];
        let end = starts
            .get(current_group + 1)
            .copied()
            .unwrap_or(visible_rows.len());
        let group_rows = &visible_rows[start..end];
        let select_group = group_rows.iter().any(|row| {
            !rows[*row]
                .alternatives
                .iter()
                .any(|index| self.selected[*index])
        });
        let mut selection = self.selected.clone();
        for row_index in group_rows {
            let row = &rows[*row_index];
            for index in &row.alternatives {
                selection[*index] = false;
            }
            if select_group {
                selection[self.choices[*row_index]] = true;
            }
        }
        if select_group && let Some(conflict) = find_selection_conflict(templates, &selection) {
            self.set_conflict_notice(templates, conflict);
            return;
        }
        self.selected = selection;
        self.notice = None;
    }

    pub(super) fn visible_rows(&self, templates: &[&Template], rows: &[TemplateRow]) -> Vec<usize> {
        if self.query.trim().is_empty() {
            return (0..rows.len()).collect();
        }

        let terms = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>();
        rows.iter()
            .enumerate()
            .filter(|(_, row)| {
                row.alternatives
                    .iter()
                    .any(|index| template_matches_query(templates[*index], &row.category, &terms))
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub(super) fn update_search(&mut self, templates: &[&Template], rows: &[TemplateRow]) {
        let visible_rows = self.visible_rows(templates, rows);
        if !visible_rows.contains(&self.cursor)
            && let Some(first) = visible_rows.first()
        {
            self.cursor = *first;
            self.preview_scroll = 0;
        }
        self.notice = None;
    }

    pub(super) fn clear_search(&mut self, templates: &[&Template], rows: &[TemplateRow]) {
        self.query.clear();
        self.search_mode = false;
        self.update_search(templates, rows);
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

pub(super) fn category_starts(rows: &[TemplateRow], visible_rows: &[usize]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut previous = None;
    for (position, index) in visible_rows.iter().enumerate() {
        let category = rows[*index].category.as_str();
        if previous != Some(category) {
            starts.push(position);
            previous = Some(category);
        }
    }
    starts
}

fn template_matches_query(template: &Template, category: &str, terms: &[String]) -> bool {
    let id = template.id.to_lowercase();
    let description = template.description.to_lowercase();
    let category = category.to_lowercase();
    let paths = template
        .files
        .iter()
        .map(|file| file.path.to_string_lossy().to_lowercase())
        .collect::<Vec<_>>();

    terms.iter().all(|term| {
        id.contains(term)
            || description.contains(term)
            || category.contains(term)
            || paths.iter().any(|path| path.contains(term))
    })
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
            query: String::new(),
            search_mode: false,
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
            query: String::new(),
            search_mode: false,
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

    #[test]
    fn search_matches_ids_descriptions_categories_and_output_paths() {
        let mut templates = [
            template("agents-notes", "Guidance", &["AGENTS.md"]),
            template("agents-security", "Guidance", &["SECURITY.md"]),
            template("release-guide", "Workflow", &["docs/release.md"]),
        ];
        templates[2].description = "Release workflow instructions".to_owned();
        let references = templates.iter().collect::<Vec<_>>();
        let rows = template_rows(&references);
        let mut picker = new_picker(rows.len());

        picker.query = "AGENTS".to_owned();
        assert_eq!(picker.visible_rows(&references, &rows), [0, 1]);
        picker.query = "workflow".to_owned();
        assert_eq!(picker.visible_rows(&references, &rows), [2]);
        picker.query = "docs/release.md".to_owned();
        assert_eq!(picker.visible_rows(&references, &rows), [2]);
    }

    #[test]
    fn filtering_keeps_hidden_selections_and_navigates_visible_rows() {
        let templates = [
            template("agents-notes", "Guidance", &["AGENTS.md"]),
            template("agents-security", "Guidance", &["SECURITY.md"]),
            template("readme", "Project", &["README.md"]),
        ];
        let references = templates.iter().collect::<Vec<_>>();
        let rows = template_rows(&references);
        let mut picker = new_picker(rows.len());
        picker.selected[2] = true;
        picker.query = "agents".to_owned();
        picker.update_search(&references, &rows);

        let visible = picker.visible_rows(&references, &rows);
        assert_eq!(visible, [0, 1]);
        assert_eq!(picker.cursor, 0);
        assert_eq!(picker.selected, [false, false, true]);

        picker.move_down(&visible);
        assert_eq!(picker.cursor, 1);
        picker.move_up(&visible);
        assert_eq!(picker.cursor, 0);

        picker.clear_search(&references, &rows);
        assert_eq!(picker.visible_rows(&references, &rows), [0, 1, 2]);
        assert_eq!(picker.selected, [false, false, true]);
    }

    #[test]
    fn toggling_a_filtered_category_changes_only_visible_templates() {
        let templates = [
            template("agents-root", "Guidance", &["AGENTS.md"]),
            template("agents-security", "Guidance", &["SECURITY.md"]),
            template("readme", "Project", &["README.md"]),
        ];
        let references = templates.iter().collect::<Vec<_>>();
        let rows = template_rows(&references);
        let mut picker = new_picker(rows.len());
        picker.selected[1] = true;
        picker.query = "agents-root".to_owned();
        picker.update_search(&references, &rows);

        let visible = picker.visible_rows(&references, &rows);
        picker.toggle_category(&references, &rows, &visible);
        assert_eq!(picker.selected, [true, true, false]);

        picker.toggle_category(&references, &rows, &visible);
        assert_eq!(picker.selected, [false, true, false]);
    }

    fn new_picker(row_count: usize) -> Picker {
        Picker {
            selected: vec![false; row_count],
            choices: (0..row_count).collect(),
            cursor: 0,
            preview_scroll: 0,
            multiple: true,
            notice: None,
            query: String::new(),
            search_mode: false,
        }
    }
}
