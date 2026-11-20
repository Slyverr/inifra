use crate::init::{App, Screen};
use crate::presentation::{Tone, terminal_text, tui_style};
use crate::{GenerationReview, ReviewAction};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

pub(crate) fn draw<P>(frame: &mut Frame<'_>, app: &App<P>) {
    match &app.screen {
        Screen::Select => draw_select(frame, app),
        Screen::Input { prompts, index } => {
            if let Some(prompt) = prompts.get(*index) {
                draw_input(frame, prompt, app);
            }
        }
        Screen::Review {
            review, approve, ..
        } => draw_review(
            frame,
            review,
            *approve,
            app.preview_scroll,
            app.error_message(),
        ),
    }
}

fn draw_select<P>(frame: &mut Frame<'_>, app: &App<P>) {
    let area = frame.area();
    if area.height < 6 {
        draw_resize_notice(frame, area);
        return;
    }
    let [header, _top_gap, body, _footer_gap, footer] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);
    frame.render_widget(Paragraph::new(picker_title(app)), header);
    super::init_picker::draw(frame, app, body);
    draw_picker_footer(
        frame,
        footer,
        app.error_message(),
        super::init_picker::has_alternatives(app),
        app.search_active,
        !app.search_query.is_empty(),
    );
}

fn picker_title<P>(app: &App<P>) -> Line<'static> {
    let mut spans = vec![
        Span::styled("Inifra", tui_style(Tone::Accent, true)),
        normal(" · "),
        Span::styled("choose project files", tui_style(Tone::Accent, true)),
        normal("  ·  "),
        Span::styled(
            format!(
                "{} selected",
                app.selected.iter().filter(|selected| **selected).count()
            ),
            tui_style(Tone::Accent, true),
        ),
    ];
    if app.search_active {
        spans.extend([
            normal(" · "),
            Span::styled("/", tui_style(Tone::Accent, true)),
            Span::styled(
                if app.search_query.is_empty() {
                    " type to filter…".to_owned()
                } else {
                    format!(" {}", terminal_text(&app.search_query))
                },
                tui_style(Tone::Secondary, false),
            ),
        ]);
    } else if !app.search_query.is_empty() {
        spans.extend([
            normal(" · "),
            Span::styled("filter: ", tui_style(Tone::Muted, false)),
            Span::styled(
                terminal_text(&app.search_query),
                tui_style(Tone::Secondary, false),
            ),
        ]);
    }
    Line::from(spans)
}

fn draw_input<P>(frame: &mut Frame<'_>, prompt: &crate::InputPrompt, app: &App<P>) {
    let area = frame.area();
    if area.height < 5 {
        draw_resize_notice(frame, area);
        return;
    }
    let [prompt_area, hint_area, input_area, _spacer, footer_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1); 5])
        .areas(area);
    let mut prompt_line = vec![Span::styled(
        terminal_text(&prompt.prompt),
        tui_style(Tone::Accent, true),
    )];
    if prompt.required {
        prompt_line.push(Span::styled(" *", tui_style(Tone::Accent, true)));
    }
    frame.render_widget(Paragraph::new(Line::from(prompt_line)), prompt_area);

    let hint = if let Some(error) = app.error_message() {
        Span::styled(terminal_text(error), tui_style(Tone::Error, false))
    } else if let Some(default) = &prompt.default {
        Span::styled(
            format!("Default: {}", terminal_text(default)),
            tui_style(Tone::Muted, false),
        )
    } else if prompt.required {
        Span::styled("Required", tui_style(Tone::Muted, false))
    } else {
        Span::styled(
            "Optional · press Enter to skip",
            tui_style(Tone::Muted, false),
        )
    };
    frame.render_widget(Paragraph::new(hint), hint_area);

    let marker_width = input_area.width.min(2);
    frame.render_widget(
        Paragraph::new(Span::styled("› ", tui_style(Tone::Accent, true))),
        Rect::new(input_area.x, input_area.y, marker_width, 1),
    );
    let edit_area = Rect::new(
        input_area.x.saturating_add(2),
        input_area.y,
        input_area.width.saturating_sub(2),
        1,
    );
    let input = terminal_text(&app.input);
    let displayed = if app.input.is_empty() {
        "Type a value…".to_owned()
    } else {
        input.clone()
    };
    let style = if app.input.is_empty() {
        tui_style(Tone::Muted, false)
    } else {
        ratatui::style::Style::default()
    };
    let cursor_prefix = app.input.chars().take(app.input_cursor).collect::<String>();
    let cursor_width = UnicodeWidthStr::width(terminal_text(&cursor_prefix).as_str());
    let visible_width = usize::from(edit_area.width).max(1);
    let horizontal_scroll = cursor_width.saturating_sub(visible_width.saturating_sub(1));
    frame.render_widget(
        Paragraph::new(Span::styled(displayed, style))
            .scroll((0, horizontal_scroll.min(u16::MAX as usize) as u16)),
        edit_area,
    );
    let cursor_x = edit_area
        .x
        .saturating_add(
            cursor_width
                .saturating_sub(horizontal_scroll)
                .min(u16::MAX as usize) as u16,
        )
        .min(
            edit_area
                .x
                .saturating_add(edit_area.width.saturating_sub(1)),
        );
    frame.set_cursor_position((cursor_x, input_area.y));
    draw_input_footer(frame, footer_area);
}

fn draw_review(
    frame: &mut Frame<'_>,
    review: &GenerationReview,
    approve: bool,
    scroll: u16,
    error: Option<&str>,
) {
    let area = frame.area();
    if area.height < 7 {
        draw_resize_notice(frame, area);
        return;
    }
    let [target, summary, items, question, _spacer, footer] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            muted("Target"),
            Span::raw("  "),
            Span::styled(
                terminal_text(&review.target),
                tui_style(Tone::Accent, false),
            ),
        ]))
        .wrap(Wrap { trim: false }),
        target,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!(
                "{} files · {} directories",
                review.files.len(),
                review.directories.len()
            ),
            tui_style(Tone::Muted, false),
        )),
        summary,
    );

    let mut rows = review
        .directories
        .iter()
        .map(|directory| {
            Line::from(vec![
                Span::styled("directory", tui_style(Tone::Muted, false)),
                Span::raw("  "),
                Span::styled(
                    format!("{}/", terminal_text(&directory.path.display().to_string())),
                    tui_style(Tone::Success, false),
                ),
            ])
        })
        .collect::<Vec<_>>();
    rows.extend(review.files.iter().map(|file| {
        let tone = match file.action {
            ReviewAction::Create => Tone::Success,
            ReviewAction::Replace => Tone::Warning,
            ReviewAction::Skip => Tone::Muted,
        };
        Line::from(vec![
            Span::styled(file.action.label(), tui_style(tone, false)),
            Span::raw("  "),
            Span::styled(
                terminal_text(&file.path.display().to_string()),
                ratatui::style::Style::default(),
            ),
        ])
    }));
    let total_rows = rows
        .iter()
        .map(|line| {
            line.width()
                .max(1)
                .div_ceil(usize::from(items.width.max(1)))
        })
        .sum::<usize>();
    let visible_rows = usize::from(items.height);
    let max_scroll = total_rows.saturating_sub(visible_rows);
    let scroll = usize::from(scroll).min(max_scroll);
    frame.render_widget(
        Paragraph::new(rows)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        items,
    );

    frame.render_widget(Paragraph::new(apply_choice(approve)), question);
    let footer_line = if let Some(error) = error {
        Line::from(Span::styled(
            terminal_text(error),
            tui_style(Tone::Error, false),
        ))
    } else {
        Line::from(vec![
            key("↑/↓ j/k"),
            muted(" choose  ·  "),
            key("PgUp/PgDn"),
            muted(" files  ·  "),
            key("Enter"),
            muted(" confirm  ·  "),
            key("q/Esc"),
            muted(" cancel"),
        ])
    };
    frame.render_widget(Paragraph::new(footer_line), footer);
}

fn apply_choice(approve: bool) -> Line<'static> {
    Line::from(vec![
        Span::styled("Apply this plan?", tui_style(Tone::Accent, true)),
        Span::raw("  "),
        Span::styled(
            if approve { "[ Yes ]" } else { "Yes" },
            tui_style(if approve { Tone::Success } else { Tone::Muted }, approve),
        ),
        Span::raw("    "),
        Span::styled(
            if approve { "No" } else { "[ No ]" },
            tui_style(if approve { Tone::Muted } else { Tone::Accent }, !approve),
        ),
    ])
}

fn draw_picker_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    notice: Option<&str>,
    alternatives: bool,
    search_active: bool,
    filter_active: bool,
) {
    if let Some(notice) = notice {
        frame.render_widget(
            Paragraph::new(Span::styled(
                terminal_text(notice),
                tui_style(Tone::Error, false),
            )),
            area,
        );
        return;
    }

    if search_active {
        frame.render_widget(Paragraph::new(search_footer(area.width)), area);
        return;
    }

    frame.render_widget(
        Paragraph::new(picker_footer(alternatives, filter_active, area.width)),
        area,
    );
}

fn search_footer(width: u16) -> Line<'static> {
    let full = Line::from(vec![
        key("Type"),
        normal(" to filter · "),
        key("↑/↓"),
        normal(" move · "),
        key("Enter"),
        normal(" done · "),
        key("Backspace"),
        normal(" delete · "),
        key("Esc"),
        normal(" clear"),
    ]);
    if full.width() <= usize::from(width) {
        return full;
    }
    let compact = Line::from(vec![
        key("Type"),
        normal(" filter · "),
        key("Enter"),
        normal(" done · "),
        key("Esc"),
        normal(" clear"),
    ]);
    if compact.width() <= usize::from(width) {
        compact
    } else {
        let minimal = Line::from(vec![key("Enter"), normal(" · "), key("Esc")]);
        if minimal.width() <= usize::from(width) {
            minimal
        } else {
            Line::from(key("/"))
        }
    }
}

fn picker_footer(alternatives: bool, filter_active: bool, width: u16) -> Line<'static> {
    let mut spans = vec![
        key("↑/↓ j/k"),
        normal(" move · "),
        key("Space"),
        normal(" select · "),
    ];
    if alternatives {
        spans.extend([key("←/→ h/l"), normal(" cycle · ")]);
    }
    spans.extend([
        key("a"),
        normal(" toggle group · "),
        key("/"),
        normal(" search · "),
        key("Tab/⇧Tab"),
        normal(" category · "),
        key("PgUp/Dn"),
        normal(" preview · "),
        key("Enter"),
        normal(" next · "),
    ]);
    if filter_active {
        spans.extend([
            key("q"),
            normal(" quit · "),
            key("Esc"),
            normal(" clear filter"),
        ]);
    } else {
        spans.extend([key("q/Esc"), normal(" quit")]);
    }
    let full = Line::from(spans);
    if full.width() <= usize::from(width) {
        return full;
    }

    let mut compact = vec![
        key("↑/↓"),
        normal(" move · "),
        key("Space"),
        normal(" select"),
    ];
    if alternatives {
        compact.extend([normal(" · "), key("←/→"), normal(" cycle")]);
    }
    let mut ending = vec![key("Enter"), normal(" next · ")];
    if filter_active {
        ending.extend([
            key("q"),
            normal(" quit · "),
            key("Esc"),
            normal(" clear filter"),
        ]);
    } else {
        ending.extend([key("q/Esc"), normal(" quit")]);
    }
    let ending_width = Line::from(ending.clone()).width() + " · ".len();

    for (shortcut, label) in [
        ("/", " search"),
        ("a", " group"),
        ("Tab", " category"),
        ("PgUp/Dn", " preview"),
    ] {
        let mut candidate = compact.clone();
        candidate.extend([normal(" · "), key(shortcut), normal(label)]);
        if Line::from(candidate.clone()).width() + ending_width <= usize::from(width) {
            compact = candidate;
        }
    }
    compact.extend([normal(" · ")]);
    compact.extend(ending);
    let compact = Line::from(compact);
    if compact.width() <= usize::from(width) {
        return compact;
    }

    let mut essential_spans = vec![
        key("↑/↓"),
        normal(" move · "),
        key("Space"),
        normal(" select · "),
        key("Enter"),
        normal(" next · "),
    ];
    if filter_active {
        essential_spans.extend([
            key("q"),
            normal(" quit · "),
            key("Esc"),
            normal(" clear filter"),
        ]);
    } else {
        essential_spans.extend([key("q/Esc"), normal(" quit")]);
    }
    let essential = Line::from(essential_spans);
    if essential.width() <= usize::from(width) {
        essential
    } else {
        Line::from(vec![
            key("↑/↓"),
            normal(" · "),
            key("Space"),
            normal(" · "),
            key("Enter"),
            normal(" · "),
            key("Esc"),
        ])
    }
}

fn draw_input_footer(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            key("Enter"),
            muted(" continue  ·  "),
            key("Esc"),
            muted(" cancel"),
        ])),
        area,
    );
}

fn draw_resize_notice(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Resize the terminal to continue",
            tui_style(Tone::Warning, false),
        )),
        area,
    );
}

fn key(text: &'static str) -> Span<'static> {
    Span::styled(text, tui_style(Tone::Accent, true))
}

fn muted(text: &'static str) -> Span<'static> {
    Span::styled(text, tui_style(Tone::Muted, false))
}

fn normal(text: &'static str) -> Span<'static> {
    Span::raw(text)
}

#[cfg(test)]
mod tests {
    use super::{apply_choice, draw, draw_picker_footer, picker_footer, search_footer};
    use crate::{
        CatalogView, CategoryView, GenerationReview, InputPrompt, ReviewAction, ReviewedFile,
        TemplateChoice, TemplateSourceView, TemplateView,
        init::{App, Screen},
        presentation::{Tone, tui_style},
    };
    use ratatui::{Terminal, backend::TestBackend};
    use std::path::PathBuf;

    fn app() -> App<()> {
        App::new(CatalogView {
            categories: vec![CategoryView {
                id: "guidance".into(),
                name: "Project guidance".into(),
            }],
            templates: vec![TemplateChoice {
                category_id: "guidance".into(),
                variants: vec![TemplateView {
                    id: "agents".into(),
                    description: "Root instructions".into(),
                    inputs: vec![],
                    directories: vec![],
                    sources: vec![TemplateSourceView {
                        source: PathBuf::from("AGENTS.md"),
                        output: PathBuf::from("AGENTS.md"),
                        alternate_output: None,
                        content: "# Instructions".into(),
                    }],
                }],
            }],
        })
    }

    #[test]
    fn picker_keeps_the_reference_header_footer_and_focus_glyph() {
        let app = app();
        let mut terminal = Terminal::new(TestBackend::new(100, 26)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = format!("{:?}", terminal.backend().buffer());
        assert!(text.contains("Inifra"));
        assert!(text.contains("choose project files"));
        assert!(text.contains("  Project guidance"));
        assert!(text.contains("› ○ agents"));
        assert!(text.contains("a group"));
        assert!(text.contains("Enter next"));
    }

    #[test]
    fn picker_title_matches_reference_spacing_and_emphasis() {
        let line = super::picker_title(&app());
        let text = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert_eq!(text, "Inifra · choose project files  ·  0 selected");
        assert_eq!(line.spans[0].style, tui_style(Tone::Accent, true));
        assert_eq!(line.spans[1].style, ratatui::style::Style::default());
        assert_eq!(line.spans[2].style, tui_style(Tone::Accent, true));
    }

    #[test]
    fn search_mode_shows_the_query_and_its_own_controls() {
        let mut app = app();
        app.search_active = true;
        app.search_query = "agents".into();
        let mut terminal = Terminal::new(TestBackend::new(140, 8)).unwrap();

        terminal.draw(|frame| draw(frame, &app)).unwrap();

        let text = format!("{:?}", terminal.backend().buffer());
        assert!(text.contains("/ agents"));
        assert!(text.contains("Backspace delete"));
        assert!(!text.contains("toggle group"));
    }

    #[test]
    fn input_uses_the_five_row_inline_prompt_layout() {
        let mut app = app();
        app.screen = Screen::Input {
            prompts: vec![InputPrompt {
                name: "name".into(),
                prompt: "Project name".into(),
                required: true,
                default: None,
            }],
            index: 0,
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 5)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = format!("{:?}", terminal.backend().buffer());
        assert!(text.contains("Project name *"));
        assert!(text.contains("Required"));
        assert!(text.contains("Type a value"));
        assert!(text.contains("Enter"));
        assert!(!text.contains("template inputs"));
    }

    #[test]
    fn review_shows_summary_actions_and_no_as_the_initial_choice() {
        let mut app = app();
        app.screen = Screen::Review {
            review: GenerationReview {
                target: "/target".into(),
                files: vec![ReviewedFile {
                    path: PathBuf::from("README.md"),
                    action: ReviewAction::Create,
                }],
                directories: vec![],
            },
            plan: (),
            approve: false,
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = format!("{:?}", terminal.backend().buffer());
        assert!(text.contains("Target"));
        assert!(text.contains("1 files · 0 directories"));
        assert!(text.contains("create  README.md"));
        assert!(text.contains("Apply this plan?"));
        assert!(!text.contains("review changes"));
        assert_eq!(apply_choice(false).width(), 31);
    }

    #[test]
    fn picker_footer_is_one_flat_row_with_the_reference_hints() {
        let mut terminal = Terminal::new(TestBackend::new(140, 1)).unwrap();
        terminal
            .draw(|frame| draw_picker_footer(frame, frame.area(), None, true, false, false))
            .unwrap();
        let text = format!("{:?}", terminal.backend().buffer());
        assert!(text.contains("←/→ h/l cycle"));
        assert!(text.contains("Tab/⇧Tab category"));
        assert!(text.contains("PgUp/Dn preview"));
        assert!(text.contains("/ search"));
        assert!(text.contains("Enter next"));
        assert!(text.contains("q/Esc quit"));
        assert!(!text.contains("Enter review"));
    }

    #[test]
    fn picker_footer_stays_on_one_unclipped_row_at_common_widths() {
        for width in [80, 100, 120, 150] {
            for alternatives in [false, true] {
                for filter_active in [false, true] {
                    assert!(
                        picker_footer(alternatives, filter_active, width).width()
                            <= usize::from(width),
                        "footer exceeds {width} columns with alternatives={alternatives} and filter_active={filter_active}"
                    );
                }
            }
        }
    }

    #[test]
    fn filtered_picker_footer_says_escape_clears_the_filter() {
        let text = picker_footer(true, true, 140)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(text.contains("q quit"));
        assert!(text.contains("Esc clear filter"));
        assert!(!text.contains("Esc cancel"));
    }

    #[test]
    fn search_footer_stays_on_one_unclipped_row() {
        for width in 1..=100 {
            assert!(
                search_footer(width).width() <= usize::from(width),
                "search footer exceeds {width} columns"
            );
        }
    }
}
