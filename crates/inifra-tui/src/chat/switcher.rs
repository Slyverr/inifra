//! In-chat picker for the session's profile, model, and reasoning effort.

use super::{ModelList, SwitchRequest, SwitcherProfile};
use crate::{
    controls::chrome::{
        Badge, Heading, Row, RowState, essential, footer, hint, label_width, row_line,
    },
    presentation::{Tone, terminal_text, tui_style},
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
    Profiles,
    Models,
}

/// What the chat should do after a key press in the switcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SwitcherOutcome {
    /// Keep the switcher open.
    Stay,
    /// Ask the application for this profile's model IDs.
    LoadModels(String),
    /// Close without changes.
    Close,
    /// Apply the selection and close.
    Switch(SwitchRequest),
}

pub(super) struct Switcher {
    profiles: Vec<SwitcherProfile>,
    profile: usize,
    active_profile: usize,
    current_model: String,
    models: BTreeMap<String, ModelList>,
    model: usize,
    efforts: Vec<String>,
    effort: usize,
    focus: Focus,
}

impl Switcher {
    /// `efforts[0]` is the "provider default" label; `effort` names the current level, if any.
    pub(super) fn new(
        profiles: Vec<SwitcherProfile>,
        active: &str,
        current_model: String,
        efforts: Vec<String>,
        effort: Option<&str>,
    ) -> Self {
        let active_profile = profiles
            .iter()
            .position(|profile| profile.id == active)
            .unwrap_or_default();
        let effort = effort
            .and_then(|effort| efforts.iter().position(|label| label == effort))
            .unwrap_or_default();
        Self {
            profiles,
            profile: active_profile,
            active_profile,
            current_model,
            models: BTreeMap::new(),
            model: 0,
            efforts,
            effort,
            focus: Focus::Models,
        }
    }

    /// The profile whose models should be loaded when the switcher opens.
    pub(super) fn initial_request(&mut self) -> Option<String> {
        self.request_models()
    }

    pub(super) fn set_models(&mut self, profile: String, models: ModelList) {
        let selected = self.model_choices().get(self.model).cloned();
        self.models.insert(profile, models);
        if let Some(selected) = selected {
            self.model = self
                .model_choices()
                .iter()
                .position(|model| *model == selected)
                .unwrap_or_default();
        }
    }

    fn highlighted(&self) -> Option<&SwitcherProfile> {
        self.profiles.get(self.profile)
    }

    /// The highlighted profile's models: its current or default model first, then listed IDs.
    fn model_choices(&self) -> Vec<String> {
        let Some(profile) = self.highlighted() else {
            return Vec::new();
        };
        let first = if self.profile == self.active_profile {
            self.current_model.clone()
        } else {
            profile.default_model.clone()
        };
        let mut choices = vec![first.clone()];
        if let Some(ModelList::Ready(models)) = self.models.get(&profile.id) {
            choices.extend(models.iter().filter(|model| **model != first).cloned());
        }
        choices
    }

    fn request_models(&mut self) -> Option<String> {
        let id = self.highlighted()?.id.clone();
        if self.models.contains_key(&id) {
            return None;
        }
        self.models.insert(id.clone(), ModelList::Loading);
        Some(id)
    }

    pub(super) fn handle(&mut self, key: KeyEvent) -> SwitcherOutcome {
        match key.code {
            KeyCode::Esc => SwitcherOutcome::Close,
            KeyCode::Enter => self.switch(false),
            KeyCode::Char('s') => self.switch(true),
            KeyCode::Char('e') => {
                if !self.efforts.is_empty() {
                    self.effort = (self.effort + 1) % self.efforts.len();
                }
                SwitcherOutcome::Stay
            }
            KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Char('h')
            | KeyCode::Char('l') => {
                self.focus = match (key.code, self.focus) {
                    (KeyCode::Left | KeyCode::Char('h'), _) => Focus::Profiles,
                    (KeyCode::Right | KeyCode::Char('l'), _) => Focus::Models,
                    (_, Focus::Profiles) => Focus::Models,
                    (_, Focus::Models) => Focus::Profiles,
                };
                SwitcherOutcome::Stay
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            _ => SwitcherOutcome::Stay,
        }
    }

    fn move_by(&mut self, amount: isize) -> SwitcherOutcome {
        match self.focus {
            Focus::Profiles => {
                let last = self.profiles.len().saturating_sub(1) as isize;
                let next = (self.profile as isize + amount).clamp(0, last) as usize;
                if next != self.profile {
                    self.profile = next;
                    self.model = 0;
                    if let Some(id) = self.request_models() {
                        return SwitcherOutcome::LoadModels(id);
                    }
                }
            }
            Focus::Models => {
                let last = self.model_choices().len().saturating_sub(1) as isize;
                self.model = (self.model as isize + amount).clamp(0, last) as usize;
            }
        }
        SwitcherOutcome::Stay
    }

    fn switch(&self, save_default: bool) -> SwitcherOutcome {
        let Some(profile) = self.highlighted() else {
            return SwitcherOutcome::Close;
        };
        let Some(model) = self.model_choices().get(self.model).cloned() else {
            return SwitcherOutcome::Close;
        };
        SwitcherOutcome::Switch(SwitchRequest {
            profile: profile.id.clone(),
            model,
            effort: (self.effort > 0)
                .then(|| self.efforts.get(self.effort).cloned())
                .flatten(),
            save_default,
        })
    }
}

pub(super) fn draw(frame: &mut Frame<'_>, switcher: &Switcher) {
    let area = frame.area();
    let [
        heading,
        _gap,
        body,
        _effort_gap,
        effort,
        _footer_gap,
        footer_area,
    ] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    frame.render_widget(
        Paragraph::new(Heading::new("switch model").line(None, heading.width)),
        heading,
    );

    let [profiles_area, _separator, models_area] = Layout::horizontal([
        Constraint::Percentage(45),
        Constraint::Length(2),
        Constraint::Min(10),
    ])
    .areas(body);
    draw_profiles(frame, switcher, profiles_area);
    draw_models(frame, switcher, models_area);
    draw_effort(frame, switcher, effort);

    frame.render_widget(
        Paragraph::new(footer(
            &[
                essential("↑/↓", "move"),
                hint("Tab", "switch side"),
                essential("e", "effort"),
                essential("Enter", "use"),
                essential("s", "save as default"),
                essential("Esc", "close"),
            ],
            footer_area.width,
        )),
        footer_area,
    );
}

/// Pane headings sit left of the item gutter and never use the focus color, so they read as
/// labels rather than choices.
fn pane_heading(title: String, focused: bool) -> Line<'static> {
    let style = if focused {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        tui_style(Tone::Muted, false)
    };
    Line::from(Span::styled(terminal_text(&title), style))
}

fn draw_profiles(frame: &mut Frame<'_>, switcher: &Switcher, area: Rect) {
    let [heading, list] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let focused = switcher.focus == Focus::Profiles;
    frame.render_widget(
        Paragraph::new(pane_heading("Profiles".to_owned(), focused)),
        heading,
    );
    let rows = switcher
        .profiles
        .iter()
        .enumerate()
        .map(|(index, profile)| {
            let row = Row::new(profile.id.clone()).detail(profile.detail.clone());
            if index == switcher.active_profile {
                row.badge(Badge::new("current", Tone::Success))
            } else {
                row
            }
        })
        .collect::<Vec<_>>();
    let lines = list_lines(&rows, switcher.profile, focused, list.height);
    frame.render_widget(Paragraph::new(lines), list);
}

fn draw_models(frame: &mut Frame<'_>, switcher: &Switcher, area: Rect) {
    let [heading, list] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let focused = switcher.focus == Focus::Models;
    let title = switcher.highlighted().map_or_else(
        || "Models".to_owned(),
        |profile| format!("Models · {}", profile.id),
    );
    frame.render_widget(Paragraph::new(pane_heading(title, focused)), heading);

    let choices = switcher.model_choices();
    let is_active = switcher.profile == switcher.active_profile;
    let rows = choices
        .iter()
        .enumerate()
        .map(|(index, model)| {
            let row = Row::new(model.clone());
            match (index, is_active) {
                (0, true) => row.badge(Badge::new("current", Tone::Success)),
                (0, false) => row.badge(Badge::new("default", Tone::Secondary)),
                _ => row,
            }
        })
        .collect::<Vec<_>>();
    let status = match switcher
        .highlighted()
        .and_then(|profile| switcher.models.get(&profile.id))
    {
        Some(ModelList::Loading) => Some("Loading model IDs…".to_owned()),
        Some(ModelList::Unavailable(reason)) => Some(reason.clone()),
        Some(ModelList::Ready(models)) if models.is_empty() => {
            Some("The provider listed no model IDs.".to_owned())
        }
        _ => None,
    };
    let status_height = if status.is_some() { 2 } else { 0 };
    let row_height = (rows.len() as u16).min(list.height.saturating_sub(status_height).max(1));
    let [rows_area, status_area] =
        Layout::vertical([Constraint::Length(row_height), Constraint::Min(0)]).areas(list);
    frame.render_widget(
        Paragraph::new(list_lines(&rows, switcher.model, focused, rows_area.height)),
        rows_area,
    );
    if let Some(status) = status {
        let [_indent, text] =
            Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).areas(status_area);
        frame.render_widget(
            Paragraph::new(Span::styled(
                terminal_text(&status),
                tui_style(Tone::Muted, false),
            ))
            .wrap(Wrap { trim: true }),
            text,
        );
    }
}

fn list_lines(rows: &[Row], selected: usize, focused: bool, height: u16) -> Vec<Line<'static>> {
    let count = usize::from(height).max(1);
    let start = selected
        .saturating_sub(count.saturating_sub(1))
        .min(rows.len().saturating_sub(count));
    let width = label_width(rows);
    rows.iter()
        .enumerate()
        .skip(start)
        .take(count)
        .map(|(index, row)| {
            let state = match (index == selected, focused) {
                (true, true) => RowState::Focused,
                _ => RowState::Idle,
            };
            row_line(row, state, width)
        })
        .collect()
}

fn draw_effort(frame: &mut Frame<'_>, switcher: &Switcher, area: Rect) {
    let label = switcher
        .efforts
        .get(switcher.effort)
        .cloned()
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("Effort", tui_style(Tone::Muted, false)),
            Span::raw("   "),
            Span::styled("‹ ", tui_style(Tone::Accent, true)),
            Span::raw(terminal_text(&label)),
            Span::styled(" ›", tui_style(Tone::Accent, true)),
            Span::styled("   press e to change", tui_style(Tone::Secondary, false)),
        ])),
        area,
    );
}
