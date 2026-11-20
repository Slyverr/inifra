use crate::{
    TemplateChoice, TemplateView,
    init::App,
    init_preview,
    presentation::{Tone, terminal_text, tui_style},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const SIDE_BY_SIDE_MIN_WIDTH: u16 = 100;
const SIDE_BY_SIDE_MIN_HEIGHT: u16 = 18;
const LIST_WIDTH_PERCENT: usize = 42;
const LIST_MIN_WIDTH: usize = 36;
const DETAIL_MIN_WIDTH: usize = 48;
const STACKED_DETAIL_RESERVE: usize = 9;
const STACKED_HEIGHT_RESERVE: usize = 7;
const STACKED_LIST_MIN_ROWS: usize = 4;
const STACKED_LIST_MAX_ROWS: usize = 14;
const LIST_INDENT: &str = "  ";

enum Entry<'a> {
    Category(&'a str),
    Gap,
    Template(usize),
}

pub(super) fn draw<P>(frame: &mut Frame<'_>, app: &App<P>, area: Rect) {
    if area.is_empty() {
        return;
    }
    let [list_area, preview_area] = picker_areas(area, entries(app).len());
    draw_list(frame, list_area, app);
    if let Some(template) = current_template(app) {
        init_preview::draw(
            frame,
            preview_area,
            template,
            app.resolved_outputs.get(&template.id),
            app.preview_scroll,
        );
    } else if !preview_area.is_empty() {
        let message = if app.search_query.is_empty() {
            "No templates available"
        } else {
            "No matching templates"
        };
        frame.render_widget(
            Paragraph::new(Span::styled(message, tui_style(Tone::Muted, false))),
            preview_area,
        );
    }
}

pub(super) fn has_alternatives<P>(app: &App<P>) -> bool {
    if !app.visible_indices().contains(&app.cursor) {
        return false;
    }
    app.catalog
        .templates
        .get(app.cursor)
        .is_some_and(|choice| choice.variants.len() > 1)
}

fn picker_areas(area: Rect, entry_count: usize) -> [Rect; 2] {
    if area.width >= SIDE_BY_SIDE_MIN_WIDTH && area.height >= SIDE_BY_SIDE_MIN_HEIGHT {
        let width = usize::from(area.width);
        let list_width = (width * LIST_WIDTH_PERCENT / 100)
            .max(LIST_MIN_WIDTH)
            .min(width.saturating_sub(DETAIL_MIN_WIDTH));
        let detail_x = list_width + 4;
        [
            Rect::new(area.x, area.y, list_width as u16, area.height),
            Rect::new(
                area.x.saturating_add(detail_x as u16),
                area.y,
                width.saturating_sub(detail_x) as u16,
                area.height,
            ),
        ]
    } else {
        let height = usize::from(area.height);
        let max_list_rows = height.saturating_sub(STACKED_DETAIL_RESERVE).max(1);
        let list_rows = ((height.saturating_sub(STACKED_HEIGHT_RESERVE) / 2)
            .clamp(STACKED_LIST_MIN_ROWS, STACKED_LIST_MAX_ROWS))
        .min(max_list_rows)
        .min(entry_count.max(1));
        let detail_y = area.y.saturating_add(list_rows as u16).saturating_add(1);
        [
            Rect::new(area.x, area.y, area.width, list_rows as u16),
            Rect::new(
                area.x,
                detail_y,
                area.width,
                area.bottom().saturating_sub(detail_y),
            ),
        ]
    }
}

fn draw_list<P>(frame: &mut Frame<'_>, area: Rect, app: &App<P>) {
    if area.is_empty() {
        return;
    }
    let entries = entries(app);
    let cursor_entry = entries
        .iter()
        .position(|entry| matches!(entry, Entry::Template(index) if *index == app.cursor))
        .unwrap_or_default();
    let height = usize::from(area.height);
    let start = cursor_entry
        .saturating_sub(height.saturating_sub(1) / 2)
        .min(entries.len().saturating_sub(height));
    let lines = if entries.is_empty() {
        vec![Line::from(Span::styled(
            format!("{LIST_INDENT}No matching templates"),
            tui_style(Tone::Muted, false),
        ))]
    } else {
        entries
            .iter()
            .skip(start)
            .take(height)
            .map(|entry| match entry {
                Entry::Category(category) => Line::from(Span::styled(
                    format!("{LIST_INDENT}{}", terminal_text(category)),
                    crate::presentation::tui_dimmed_style(),
                )),
                Entry::Gap => Line::default(),
                Entry::Template(index) => template_line(*index, app),
            })
            .collect::<Vec<_>>()
    };
    frame.render_widget(Paragraph::new(lines), area);
}

fn entries<P>(app: &App<P>) -> Vec<Entry<'_>> {
    let mut entries = Vec::with_capacity(app.catalog.templates.len() * 2);
    let mut previous_category = None;
    for index in app.visible_indices() {
        let choice = &app.catalog.templates[index];
        let category = app
            .catalog
            .categories
            .iter()
            .find(|category| category.id == choice.category_id)
            .map_or(choice.category_id.as_str(), |category| {
                category.name.as_str()
            });
        if previous_category.is_some() && previous_category != Some(category) {
            entries.push(Entry::Gap);
        }
        if previous_category != Some(category) {
            entries.push(Entry::Category(category));
            previous_category = Some(category);
        }
        entries.push(Entry::Template(index));
    }
    entries
}

fn template_line<P>(row_index: usize, app: &App<P>) -> Line<'static> {
    let choice = &app.catalog.templates[row_index];
    let active = row_index == app.cursor;
    let selected = app.selected[row_index];
    let current = choice.variants.get(app.variants[row_index]);
    let unavailable =
        current.is_some_and(|template| app.unavailable_templates.contains(&template.id));
    let cursor = if active { "›" } else { " " };
    let mut spans = vec![Span::styled(
        format!("{LIST_INDENT}{cursor} "),
        if active {
            tui_style(Tone::Accent, true)
        } else {
            ratatui::style::Style::default()
        },
    )];
    spans.push(Span::styled(
        format!(
            "{} ",
            if selected {
                "●"
            } else if unavailable {
                "×"
            } else {
                "○"
            }
        ),
        if unavailable {
            tui_style(Tone::Secondary, false)
        } else {
            tui_style(
                if selected {
                    Tone::Success
                } else {
                    Tone::Secondary
                },
                false,
            )
        },
    ));

    if let Some(template) = current {
        append_template_ids(&mut spans, choice, row_index, app);
        let summary = output_summary(template, app.resolved_outputs.get(&template.id));
        if !summary.is_empty() {
            let mut style = tui_style(Tone::Secondary, false);
            if unavailable {
                style = style.add_modifier(Modifier::CROSSED_OUT);
            }
            spans.push(Span::styled(format!("  ({summary})"), style));
        }
    }
    Line::from(spans)
}

fn append_template_ids<P>(
    spans: &mut Vec<Span<'static>>,
    choice: &TemplateChoice,
    row_index: usize,
    app: &App<P>,
) {
    let current_index = app.variants[row_index];
    let mut order = vec![current_index];
    order.extend((0..choice.variants.len()).filter(|index| *index != current_index));
    for (position, index) in order.into_iter().enumerate() {
        if position > 0 {
            spans.push(Span::raw("  ·  "));
        }
        let template = &choice.variants[index];
        let is_current = index == current_index;
        let id = if is_current && choice.variants.len() > 1 {
            format!("‹ {} ›", template.id)
        } else {
            template.id.clone()
        };
        let unavailable = app.unavailable_templates.contains(&template.id);
        let mut style = if is_current && (row_index == app.cursor || choice.variants.len() > 1) {
            tui_style(Tone::Accent, true)
        } else {
            tui_style(Tone::Secondary, false)
        };
        if unavailable {
            style = style.add_modifier(Modifier::CROSSED_OUT);
        }
        spans.push(Span::styled(terminal_text(&id), style));
    }
}

fn current_template<P>(app: &App<P>) -> Option<&TemplateView> {
    if !app.visible_indices().contains(&app.cursor) {
        return None;
    }
    app.catalog
        .templates
        .get(app.cursor)
        .and_then(|choice| choice.variants.get(app.variants[app.cursor]))
}

fn output_summary(
    template: &TemplateView,
    resolved: Option<&BTreeMap<PathBuf, PathBuf>>,
) -> String {
    match (template.sources.as_slice(), template.directories.len()) {
        ([source], 0) => output_label(source, resolved),
        ([], 1) => format!("{}/", template.directories[0].display()),
        ([], 0) => "no files".to_owned(),
        (sources, 0) => format!("+{} files", sources.len()),
        ([], directories) => format!("+{directories} folders"),
        (sources, directories) => {
            format!("+{} files, +{directories} folders", sources.len())
        }
    }
}

fn output_label(
    source: &crate::TemplateSourceView,
    resolved: Option<&BTreeMap<PathBuf, PathBuf>>,
) -> String {
    if let Some(output) = resolved.and_then(|paths| paths.get(&source.source))
        && (output != &source.output || source.alternate_output.is_none())
    {
        return output.display().to_string();
    }
    source.alternate_output.as_ref().map_or_else(
        || source.output.display().to_string(),
        |alternate| conditional_output_summary(&source.output, alternate),
    )
}

fn conditional_output_summary(default: &Path, alternate: &Path) -> String {
    let default = default.to_string_lossy();
    let alternate = alternate.to_string_lossy();
    if default == alternate {
        return default.into_owned();
    }
    if let Some(suffix) = alternate.strip_prefix(default.as_ref())
        && suffix
            .chars()
            .next()
            .is_some_and(|character| "-_.".contains(character))
    {
        return format!("{default}[{suffix}]");
    }
    format!("{default}[{alternate}]")
}

#[cfg(test)]
mod tests {
    use super::{current_template, draw, output_summary, picker_areas, template_line};
    use crate::{
        CatalogView, CategoryView, TemplateChoice, TemplateInputView, TemplateSourceView,
        TemplateView,
        init::App,
        presentation::{Tone, tui_style},
    };
    use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Modifier, text::Line};
    use std::path::PathBuf;

    fn template(id: &str, outputs: &[&str]) -> TemplateView {
        let sources = outputs
            .iter()
            .map(|output| TemplateSourceView {
                source: PathBuf::from(output),
                output: PathBuf::from(output),
                alternate_output: None,
                content: "# Guide".into(),
            })
            .collect();
        TemplateView {
            id: id.into(),
            description: String::new(),
            inputs: vec![TemplateInputView {
                name: "project_name".into(),
                prompt: "Project name".into(),
            }],
            directories: vec![],
            sources,
        }
    }

    fn license_template(id: &str, alternative: &str) -> TemplateView {
        TemplateView {
            id: id.into(),
            description: "License".into(),
            inputs: vec![],
            directories: vec![],
            sources: vec![TemplateSourceView {
                source: "LICENSE.j2".into(),
                output: "LICENSE".into(),
                alternate_output: Some(alternative.into()),
                content: format!("{id} source"),
            }],
        }
    }

    fn license_app() -> App<()> {
        App::new(CatalogView {
            categories: vec![CategoryView {
                id: "policies".into(),
                name: "Project policies".into(),
            }],
            templates: vec![
                TemplateChoice {
                    category_id: "policies".into(),
                    variants: vec![license_template("license-apache", "LICENSE-APACHE")],
                },
                TemplateChoice {
                    category_id: "policies".into(),
                    variants: vec![license_template("license-mit", "LICENSE-MIT")],
                },
            ],
        })
    }

    fn app() -> App<()> {
        App::new(CatalogView {
            categories: vec![CategoryView {
                id: "guidance".into(),
                name: "Project guidance".into(),
            }],
            templates: vec![
                TemplateChoice {
                    category_id: "guidance".into(),
                    variants: vec![
                        template("agents", &["AGENTS.md"]),
                        template("rust-agents", &["AGENTS.md"]),
                    ],
                },
                TemplateChoice {
                    category_id: "guidance".into(),
                    variants: vec![template("readme", &["README.md"])],
                },
            ],
        })
    }

    #[test]
    fn empty_search_results_are_clear_and_hide_the_previous_preview() {
        let mut app = app();
        app.search_query = "no-such-template".into();
        let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();

        terminal
            .draw(|frame| draw(frame, &app, frame.area()))
            .unwrap();

        let text = format!("{:?}", terminal.backend().buffer());
        assert!(text.contains("No matching templates"));
        assert!(!text.contains("AGENTS.md"));
    }

    #[test]
    fn current_variant_is_accented_and_other_variant_stays_secondary() {
        let app = app();
        let line = template_line(0, &app);

        assert_eq!(
            joined(&line),
            "  › ○ ‹ agents ›  ·  rust-agents  (AGENTS.md)"
        );
        assert_eq!(line.spans[0].content.as_ref(), "  › ");
        assert_eq!(line.spans[0].style, tui_style(Tone::Accent, true));
        assert!(line.spans.iter().any(|span| {
            span.content.as_ref() == "‹ agents ›" && span.style == tui_style(Tone::Accent, true)
        }));
        assert!(line.spans.iter().any(|span| {
            span.content.as_ref() == "rust-agents"
                && span.style == tui_style(Tone::Secondary, false)
        }));
    }

    #[test]
    fn selected_marker_uses_the_success_role() {
        let mut app = app();
        app.selected[0] = true;
        let line = template_line(0, &app);

        assert_eq!(line.spans[0].content.as_ref(), "  › ");
        assert_eq!(line.spans[1].content.as_ref(), "● ");
        assert_eq!(line.spans[1].style, tui_style(Tone::Success, false));
    }

    #[test]
    fn unavailable_templates_are_crossed_out_and_use_a_disabled_marker() {
        let mut app = app();
        app.unavailable_templates.insert("agents".into());

        let line = template_line(0, &app);
        assert_eq!(line.spans[1].content.as_ref(), "× ");
        assert_eq!(line.spans[1].style, tui_style(Tone::Secondary, false));
        assert!(line.spans.iter().any(|span| {
            span.content.as_ref() == "‹ agents ›"
                && span.style.add_modifier.contains(Modifier::CROSSED_OUT)
        }));
        assert_eq!(
            line.spans
                .iter()
                .find(|span| span.content.as_ref() == "‹ agents ›")
                .unwrap()
                .style
                .fg,
            tui_style(Tone::Accent, true).fg
        );
        assert!(line.spans.iter().any(|span| {
            span.content.as_ref() == "  (AGENTS.md)"
                && span.style.add_modifier.contains(Modifier::CROSSED_OUT)
        }));
        assert_eq!(
            line.spans
                .iter()
                .find(|span| span.content.as_ref() == "  (AGENTS.md)")
                .unwrap()
                .style
                .fg,
            tui_style(Tone::Secondary, false).fg
        );
    }

    #[test]
    fn incompatible_replacement_variant_stays_colored_but_is_crossed_out() {
        let mut app = app();
        app.selected[0] = true;
        app.unavailable_templates.insert("rust-agents".into());

        let line = template_line(0, &app);
        let alternative = line
            .spans
            .iter()
            .find(|span| span.content.as_ref() == "rust-agents")
            .unwrap();

        assert!(
            alternative
                .style
                .add_modifier
                .contains(Modifier::CROSSED_OUT)
        );
        assert_eq!(alternative.style.fg, tui_style(Tone::Secondary, false).fg);
    }

    #[test]
    fn active_and_inactive_rows_keep_the_marker_and_template_id_aligned() {
        let app = app();
        let active = template_line(0, &app);
        let inactive = template_line(1, &app);

        assert_eq!(active.spans[0].content.as_ref(), "  › ");
        assert_eq!(inactive.spans[0].content.as_ref(), "    ");
        assert_eq!(joined(&inactive), "    ○ readme  (README.md)");
    }

    #[test]
    fn variant_marquee_highlights_only_the_current_template() {
        let mut app = app();
        app.variants[0] = 1;
        let line = template_line(0, &app);

        assert_eq!(
            joined(&line),
            "  › ○ ‹ rust-agents ›  ·  agents  (AGENTS.md)"
        );
        assert!(line.spans.iter().any(|span| {
            span.content.as_ref() == "‹ rust-agents ›"
                && span.style == tui_style(Tone::Accent, true)
        }));
        assert!(line.spans.iter().any(|span| {
            span.content.as_ref() == "agents" && span.style == tui_style(Tone::Secondary, false)
        }));
    }

    #[test]
    fn output_summary_compacts_multi_file_templates() {
        let template = template("docs", &["docs/README.md", "docs/architecture.md"]);
        assert_eq!(output_summary(&template, None), "+2 files");
    }

    #[test]
    fn license_rows_show_conditional_paths_then_resolved_paths() {
        let mut app = license_app();
        let apache = template_line(0, &app);
        let mit = template_line(1, &app);
        assert!(joined(&apache).ends_with("(LICENSE[-APACHE])"));
        assert!(joined(&mit).ends_with("(LICENSE[-MIT])"));
        let template = current_template(&app).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(70, 24)).unwrap();
        terminal
            .draw(|frame| {
                crate::init_preview::draw(frame, Rect::new(0, 0, 70, 24), template, None, 0);
            })
            .unwrap();
        let text = format!("{:?}", terminal.backend().buffer());
        assert!(text.contains("LICENSE"));
        assert!(!text.contains("LICENSE[-APACHE]"));

        app.selected = vec![true, false];
        app.resolved_outputs = [(
            "license-apache".into(),
            [("LICENSE.j2".into(), "LICENSE".into())].into(),
        )]
        .into();
        assert!(joined(&template_line(0, &app)).ends_with("(LICENSE[-APACHE])"));

        app.selected = vec![true, true];
        app.resolved_outputs = [
            (
                "license-apache".into(),
                [("LICENSE.j2".into(), "LICENSE-APACHE".into())].into(),
            ),
            (
                "license-mit".into(),
                [("LICENSE.j2".into(), "LICENSE-MIT".into())].into(),
            ),
        ]
        .into();
        assert!(joined(&template_line(0, &app)).ends_with("(LICENSE-APACHE)"));
        assert!(joined(&template_line(1, &app)).ends_with("(LICENSE-MIT)"));

        let template = current_template(&app).unwrap();
        let resolved = app.resolved_outputs.get(&template.id);
        assert_eq!(
            template.sources[0].resolved_output(resolved),
            PathBuf::from("LICENSE-APACHE")
        );
        let mut terminal = Terminal::new(TestBackend::new(70, 24)).unwrap();
        terminal
            .draw(|frame| {
                crate::init_preview::draw(frame, Rect::new(0, 0, 70, 24), template, resolved, 0);
            })
            .unwrap();
        let text = format!("{:?}", terminal.backend().buffer());
        assert!(text.contains("LICENSE-APACHE"));
    }

    #[test]
    fn alternate_paths_outside_the_default_prefix_remain_explicit() {
        let template = license_template("license", "docs/LICENSE");
        assert_eq!(
            super::conditional_output_summary(
                &template.sources[0].output,
                template.sources[0].alternate_output.as_ref().unwrap(),
            ),
            "LICENSE[docs/LICENSE]"
        );
    }

    #[test]
    fn long_output_summaries_remain_intact_for_pane_clipping() {
        let mut app = app();
        app.catalog.templates[0].variants[0] =
            template("agents", &[".github/ISSUE_TEMPLATE/feature-request.yml"]);
        let line = template_line(0, &app);
        let visible = joined(&line);

        assert!(line.width() > 40);
        assert!(visible.starts_with("  › ○ ‹ agents ›"));
        assert!(visible.ends_with("(.github/ISSUE_TEMPLATE/feature-request.yml)"));
    }

    fn joined(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn picker_uses_the_reference_breakpoints_and_split() {
        let wide = picker_areas(Rect::new(0, 0, 100, 30), 10);
        assert_eq!(wide, [Rect::new(0, 0, 42, 30), Rect::new(46, 0, 54, 30)]);

        let stacked = picker_areas(Rect::new(0, 0, 99, 30), 20);
        assert_eq!(stacked[0].height, 11);
        assert_eq!(stacked[1].y, 12);

        let short = picker_areas(Rect::new(0, 0, 99, 13), 20);
        assert_eq!(short[0].height, 4);
        assert_eq!(short[1].y, 5);
    }
}
