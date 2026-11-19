use crate::ReviewAction;
use crate::init::{App, Screen};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};

const EMPHASIS: Color = Color::Cyan;
const MUTED: Color = Color::Gray;
const SELECTED: Color = Color::Green;
const ERROR: Color = Color::Red;
const WARNING: Color = Color::Yellow;

pub(crate) fn draw<P>(frame: &mut Frame<'_>, app: &App<P>) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(area);
    let title = match app.screen {
        Screen::Select => format!(
            "Inifra · choose project files · {} selected",
            app.selected.iter().filter(|selected| **selected).count()
        ),
        Screen::Input { .. } => "Inifra · template inputs".to_owned(),
        Screen::Review { .. } => "Inifra · review changes".to_owned(),
    };
    frame.render_widget(
        Paragraph::new(title).style(Style::default().fg(EMPHASIS).add_modifier(Modifier::BOLD)),
        chunks[0],
    );
    match &app.screen {
        Screen::Select => draw_picker(frame, app, chunks[1]),
        Screen::Input { prompts, index } => {
            if let Some(prompt) = prompts.get(*index) {
                let requirement = if prompt.required {
                    "required"
                } else {
                    "optional"
                };
                let default = prompt
                    .default
                    .as_ref()
                    .map(|value| format!(" · default: {value}"))
                    .unwrap_or_default();
                let lines = vec![
                    Line::from(format!("{} ({requirement}{default})", prompt.prompt)),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("› ", Style::default().fg(EMPHASIS)),
                        Span::raw(&app.input),
                    ]),
                ];
                frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), chunks[1]);
                let x = chunks[1].x.saturating_add(2).saturating_add(
                    u16::try_from(Line::from(app.input.as_str()).width()).unwrap_or(u16::MAX),
                );
                if x < chunks[1].right() && chunks[1].height > 2 {
                    frame.set_cursor_position((x, chunks[1].y + 2));
                }
            }
        }
        Screen::Review {
            review, approve, ..
        } => {
            let areas = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(chunks[1]);
            frame.render_widget(
                Paragraph::new(format!("Target: {}", review.target)),
                areas[0],
            );
            let mut lines = Vec::new();
            for directory in &review.directories {
                lines.push(review_line(
                    directory.action,
                    format!("{}/", directory.path.display()),
                ));
            }
            for file in &review.files {
                lines.push(review_line(file.action, file.path.display().to_string()));
            }
            frame.render_widget(
                Paragraph::new(lines).scroll((app.preview_scroll, 0)),
                areas[1],
            );
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw("Apply these changes?  "),
                    choice("No", !approve),
                    Span::raw("   "),
                    choice("Yes", *approve),
                ])),
                areas[2],
            );
        }
    }
    let help = match app.screen {
        Screen::Select => {
            "↑/↓ j/k move · ←/→ h/l variant · Space select · Tab category · PgUp/Dn preview · Enter next · Esc cancel"
        }
        Screen::Input { .. } => "BackTab previous · Enter continue · Esc cancel",
        Screen::Review { .. } => {
            "↑/↓ scroll · ←/→ or h/l choose · b back · Enter confirm · Esc cancel"
        }
    };
    let footer = if let Some(error) = &app.error {
        vec![
            Line::styled(error, Style::default().fg(ERROR)),
            Line::from(help),
        ]
    } else {
        vec![Line::from(""), Line::from(help)]
    };
    frame.render_widget(Paragraph::new(footer), chunks[2]);
}

fn choice(value: &str, active: bool) -> Span<'_> {
    if active {
        Span::styled(
            format!("[ {value} ]"),
            Style::default().fg(EMPHASIS).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(format!("  {value}  "), Style::default().fg(MUTED))
    }
}

fn review_line(action: ReviewAction, path: String) -> Line<'static> {
    let color = match action {
        ReviewAction::Create => SELECTED,
        ReviewAction::Replace => WARNING,
        ReviewAction::Skip => MUTED,
    };
    Line::from(vec![
        Span::styled(
            format!("  {:22}", action.label()),
            Style::default().fg(color),
        ),
        Span::raw(path),
    ])
}

fn draw_picker<P>(frame: &mut Frame<'_>, app: &App<P>, area: Rect) {
    let columns = if area.width >= 70 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area)
    };
    let mut rows = Vec::new();
    let mut selected_row = None;
    let mut previous = None::<&str>;
    for (index, choice) in app.catalog.templates.iter().enumerate() {
        if previous != Some(&choice.category_id) {
            if previous.is_some() {
                rows.push(ListItem::new(""));
            }
            let name = app
                .catalog
                .categories
                .iter()
                .find(|category| category.id == choice.category_id)
                .map_or(choice.category_id.as_str(), |category| {
                    category.name.as_str()
                });
            rows.push(ListItem::new(Line::styled(
                name,
                Style::default().fg(MUTED),
            )));
            previous = Some(&choice.category_id);
        }
        if index == app.cursor {
            selected_row = Some(rows.len());
        }
        let marker = if app.selected[index] { "●" } else { "○" };
        let template = choice.variants.get(app.variants[index]);
        let output = template
            .and_then(|template| template.outputs.first())
            .map(|path| format!("  ({})", path.display()))
            .unwrap_or_default();
        let mut spans = vec![Span::styled(
            format!("{marker} "),
            Style::default().fg(if app.selected[index] { SELECTED } else { MUTED }),
        )];
        if choice.variants.len() == 1 {
            if let Some(template) = template {
                spans.push(Span::raw(&template.id));
            }
        } else {
            spans.push(Span::styled("‹ ", Style::default().fg(MUTED)));
            for (variant, template) in choice.variants.iter().enumerate() {
                if variant > 0 {
                    spans.push(Span::styled(" · ", Style::default().fg(MUTED)));
                }
                let style = if variant == app.variants[index] {
                    Style::default().fg(EMPHASIS).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(MUTED)
                };
                spans.push(Span::styled(&template.id, style));
            }
            spans.push(Span::styled(" ›", Style::default().fg(MUTED)));
        }
        spans.push(Span::styled(output, Style::default().fg(MUTED)));
        rows.push(ListItem::new(Line::from(spans)));
    }
    let mut state = ListState::default().with_selected(selected_row);
    frame.render_stateful_widget(
        List::new(rows)
            .highlight_symbol("› ")
            .highlight_style(Style::default().fg(EMPHASIS).add_modifier(Modifier::BOLD)),
        columns[0],
        &mut state,
    );
    if let Some(template) = app
        .catalog
        .templates
        .get(app.cursor)
        .and_then(|choice| choice.variants.get(app.variants[app.cursor]))
    {
        let preview_area =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(columns[1]);
        let heading = template
            .preview_source
            .as_ref()
            .map(|source| format!("Source preview · {}", source.display()))
            .unwrap_or_else(|| "Preview".to_owned());
        frame.render_widget(
            Paragraph::new(heading).style(Style::default().fg(MUTED)),
            preview_area[0],
        );
        let mut lines = vec![Line::from(template.description.as_str()), Line::from("")];
        for path in &template.outputs {
            lines.push(Line::styled(
                path.display().to_string(),
                Style::default().fg(MUTED),
            ));
        }
        for path in &template.directories {
            lines.push(Line::styled(
                format!("{}/", path.display()),
                Style::default().fg(MUTED),
            ));
        }
        lines.push(Line::from(""));
        lines.extend(template.preview.lines().map(Line::from));
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::default())
                .wrap(Wrap { trim: false })
                .scroll((app.preview_scroll, 0)),
            preview_area[1],
        );
    }
}
