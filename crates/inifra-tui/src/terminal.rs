use crossterm::execute;
use ratatui::{DefaultTerminal, TerminalOptions, Viewport};
use std::io::{self, Write};

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

pub(crate) struct InlineTerminal {
    terminal: Option<DefaultTerminal>,
    restore: Option<Restore>,
    height: u16,
    top: u16,
}

impl InlineTerminal {
    pub(crate) fn new(height: u16) -> io::Result<Self> {
        let restore = Restore;
        let terminal = ratatui::try_init_with_options(TerminalOptions {
            viewport: Viewport::Inline(height),
        })?;
        Ok(Self {
            terminal: Some(terminal),
            restore: Some(restore),
            height,
            top: 0,
        })
    }

    pub(crate) fn draw(&mut self, render: impl FnOnce(&mut ratatui::Frame<'_>)) -> io::Result<()> {
        let terminal = self
            .terminal
            .as_mut()
            .ok_or_else(|| io::Error::other("inline terminal is closed"))?;
        let mut top = 0;
        terminal.draw(|frame| {
            top = frame.area().y;
            render(frame);
        })?;
        self.top = top;
        Ok(())
    }

    pub(crate) fn autoresize(&mut self) -> io::Result<()> {
        self.terminal
            .as_mut()
            .ok_or_else(|| io::Error::other("inline terminal is closed"))?
            .autoresize()?;
        Ok(())
    }

    pub(crate) fn height(&self) -> u16 {
        self.height
    }

    /// Clears the current inline viewport and restores raw mode before another stage is mounted.
    pub(crate) fn close(mut self) -> io::Result<()> {
        let result = self.clear_from_top();
        self.terminal.take();
        self.restore.take();
        result
    }

    fn clear_from_top(&self) -> io::Result<()> {
        let mut stdout = io::stdout();
        execute!(
            stdout,
            crossterm::cursor::MoveTo(0, self.top),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown),
            crossterm::cursor::Show
        )?;
        stdout.flush()
    }
}

impl Drop for InlineTerminal {
    fn drop(&mut self) {
        if self.terminal.is_some() {
            let _ = self.clear_from_top();
        }
        self.terminal.take();
        self.restore.take();
    }
}

pub(crate) fn inline_height(maximum: u16, minimum: u16) -> io::Result<u16> {
    let (_, terminal_height) = crossterm::terminal::size()?;
    fit_inline_height(terminal_height, maximum, minimum)
}

pub(crate) fn inline_picker_height() -> io::Result<u16> {
    let (_, terminal_height) = crossterm::terminal::size()?;
    Ok(picker_inline_rows(terminal_height))
}

fn picker_inline_rows(terminal_height: u16) -> u16 {
    terminal_height.saturating_sub(6).clamp(1, 32)
}

fn fit_inline_height(terminal_height: u16, maximum: u16, minimum: u16) -> io::Result<u16> {
    let height = terminal_height.saturating_sub(2).min(maximum);
    if height < minimum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "terminal is too small for interactive setup; use a terminal at least {} rows tall",
                minimum.saturating_add(2)
            ),
        ));
    }

    Ok(height)
}

#[cfg(test)]
mod tests {
    use super::{fit_inline_height, picker_inline_rows};

    #[test]
    fn picker_viewport_tracks_terminal_height_with_the_reference_reserve() {
        assert_eq!(picker_inline_rows(50), 32);
        assert_eq!(picker_inline_rows(30), 24);
        assert_eq!(picker_inline_rows(5), 1);
    }

    #[test]
    fn inline_height_matches_the_available_terminal_rows() {
        assert_eq!(fit_inline_height(40, 26, 8).unwrap(), 26);
        assert_eq!(fit_inline_height(20, 26, 8).unwrap(), 18);
    }

    #[test]
    fn inline_height_rejects_a_terminal_below_the_minimum() {
        let error = fit_inline_height(9, 26, 8).unwrap_err();
        assert_eq!(
            error.to_string(),
            "terminal is too small for interactive setup; use a terminal at least 10 rows tall"
        );
    }
}
