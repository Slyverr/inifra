use super::{App, InitBackend, InitOutcome, Screen};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl<P> App<P> {
    pub(super) fn handle<B: InitBackend<Plan = P>>(
        &mut self,
        key: KeyEvent,
        backend: &mut B,
    ) -> Option<InitOutcome> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(InitOutcome::Cancelled);
        }

        let accepting_text = matches!(&self.screen, Screen::Input { .. })
            || (matches!(&self.screen, Screen::Select) && self.search_active);
        if key.code == KeyCode::Char('q') && key.modifiers == KeyModifiers::NONE && !accepting_text
        {
            return Some(InitOutcome::Cancelled);
        }

        if key.code == KeyCode::Esc {
            match &self.screen {
                Screen::Select if self.search_active || !self.search_query.is_empty() => {
                    self.clear_search();
                }
                Screen::Select => return Some(InitOutcome::Cancelled),
                Screen::Input { prompts, index } => {
                    let current_prompt = prompts.get(*index).map(|prompt| prompt.name.clone());
                    if let Some(name) = current_prompt {
                        self.values.insert(name, self.input.clone());
                    }
                    self.screen = Screen::Select;
                    self.clear_error();
                }
                Screen::Review { .. } => {
                    self.screen = Screen::Select;
                    self.clear_error();
                }
            }

            return None;
        }

        if matches!(&self.screen, Screen::Select) && self.search_active {
            match key.code {
                KeyCode::Enter => {
                    self.search_active = false;
                    self.clear_error();
                    return None;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.search_changed();
                    return None;
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.search_query.push(character);
                    self.search_changed();
                    return None;
                }
                _ => {}
            }
        }

        match &mut self.screen {
            Screen::Select => match key.code {
                KeyCode::Down | KeyCode::Char('j') => self.select_move(true),
                KeyCode::Up | KeyCode::Char('k') => self.select_move(false),
                KeyCode::Right | KeyCode::Char('l') => self.variant_move(true, backend),
                KeyCode::Left | KeyCode::Char('h') => self.variant_move(false, backend),
                KeyCode::Tab => self.category_move(true),
                KeyCode::BackTab => self.category_move(false),
                KeyCode::PageDown => self.preview_scroll = self.preview_scroll.saturating_add(8),
                KeyCode::PageUp => self.preview_scroll = self.preview_scroll.saturating_sub(8),
                KeyCode::Char(' ') => self.toggle_current(backend),
                KeyCode::Char('a') => self.toggle_category(backend),
                KeyCode::Char('/') => {
                    self.search_active = true;
                    self.clear_error();
                }
                KeyCode::Enter => {
                    let selected = self.selected_ids();
                    match backend.validate_selection(&selected) {
                        Err(error) => self.show_error(error),
                        Ok(()) => match backend.input_prompts(&selected) {
                            Ok(prompts) if prompts.is_empty() => self.prepare(backend),
                            Ok(mut prompts) => {
                                prompts.sort_by_key(|prompt| !prompt.required);
                                if let Some(prompt) = prompts.first() {
                                    let value = self
                                        .values
                                        .get(&prompt.name)
                                        .cloned()
                                        .or_else(|| prompt.default.clone())
                                        .unwrap_or_default();
                                    self.input_value(value);
                                }
                                self.clear_error();
                                self.screen = Screen::Input { prompts, index: 0 };
                            }
                            Err(error) => self.show_error(error),
                        },
                    }
                }
                _ => {}
            },
            Screen::Input { prompts, index } => match key.code {
                KeyCode::Left => self.input_cursor = self.input_cursor.saturating_sub(1),
                KeyCode::Right => {
                    self.input_cursor = (self.input_cursor + 1).min(self.input.chars().count())
                }
                KeyCode::Home => self.input_cursor = 0,
                KeyCode::End => self.input_cursor = self.input.chars().count(),
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    let byte_index = self
                        .input
                        .char_indices()
                        .nth(self.input_cursor)
                        .map_or(self.input.len(), |(index, _)| index);
                    self.input.insert(byte_index, ch);
                    self.input_cursor += 1;
                    self.error = None;
                }
                KeyCode::Backspace if self.input_cursor > 0 => {
                    let start = self
                        .input
                        .char_indices()
                        .nth(self.input_cursor - 1)
                        .map_or(self.input.len(), |(index, _)| index);
                    let end = self
                        .input
                        .char_indices()
                        .nth(self.input_cursor)
                        .map_or(self.input.len(), |(index, _)| index);
                    self.input.replace_range(start..end, "");
                    self.input_cursor -= 1;
                    self.error = None;
                }
                KeyCode::Delete if self.input_cursor < self.input.chars().count() => {
                    let start = self
                        .input
                        .char_indices()
                        .nth(self.input_cursor)
                        .map_or(self.input.len(), |(index, _)| index);
                    let end = self
                        .input
                        .char_indices()
                        .nth(self.input_cursor + 1)
                        .map_or(self.input.len(), |(index, _)| index);
                    self.input.replace_range(start..end, "");
                    self.error = None;
                }
                KeyCode::Enter => {
                    let prompt = &prompts[*index];
                    let supplied = if self.input.is_empty() {
                        prompt.default.as_deref().unwrap_or("")
                    } else {
                        &self.input
                    };

                    if prompt.required && supplied.trim().is_empty() {
                        self.show_error("A value is required.");
                    } else {
                        self.values.insert(prompt.name.clone(), supplied.to_owned());
                        self.error = None;
                        *index += 1;
                        if *index == prompts.len() {
                            self.prepare(backend);
                        } else {
                            let value = self
                                .values
                                .get(&prompts[*index].name)
                                .cloned()
                                .or_else(|| prompts[*index].default.clone())
                                .unwrap_or_default();
                            self.input_value(value);
                        }
                    }
                }
                _ => {}
            },
            Screen::Review { approve, .. } => match key.code {
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Char('h' | 'j' | 'k' | 'l')
                | KeyCode::Char(' ') => *approve = !*approve,
                KeyCode::PageDown => self.preview_scroll = self.preview_scroll.saturating_add(8),
                KeyCode::PageUp => self.preview_scroll = self.preview_scroll.saturating_sub(8),
                KeyCode::Enter if !*approve => return Some(InitOutcome::Cancelled),
                KeyCode::Enter => {
                    let Screen::Review { review, plan, .. } =
                        std::mem::replace(&mut self.screen, Screen::Select)
                    else {
                        unreachable!()
                    };

                    match backend.apply(plan) {
                        Ok(()) => return Some(InitOutcome::Applied(review)),
                        Err(error) => self.show_error(error),
                    }
                }
                _ => {}
            },
        }

        None
    }
}
