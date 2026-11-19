use crossterm::execute;
use ratatui::{DefaultTerminal, TerminalOptions, Viewport};
use std::io;

pub(crate) struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), crossterm::cursor::Show);
    }
}

pub(crate) fn inline(height: u16) -> io::Result<(Restore, DefaultTerminal)> {
    let restore = Restore;
    let terminal = ratatui::try_init_with_options(TerminalOptions {
        viewport: Viewport::Inline(height),
    })?;
    Ok((restore, terminal))
}
