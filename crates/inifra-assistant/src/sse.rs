const MAX_EVENT_BYTES: usize = 1024 * 1024;

/// Parses server-sent events while preserving state across arbitrary HTTP chunks.
pub(crate) struct SseParser {
    line: Vec<u8>,
    data: String,
    has_data: bool,
    frame_bytes: usize,
    skip_lf: bool,
}

impl SseParser {
    pub(crate) fn new() -> Self {
        Self {
            line: Vec::new(),
            data: String::new(),
            has_data: false,
            frame_bytes: 0,
            skip_lf: false,
        }
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, &'static str> {
        let mut events = Vec::new();
        for byte in chunk {
            if self.skip_lf {
                self.skip_lf = false;
                if *byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\r' => {
                    self.process_line(&mut events)?;
                    self.skip_lf = true;
                }
                b'\n' => self.process_line(&mut events)?,
                byte => {
                    self.line.push(*byte);
                    self.frame_bytes += 1;
                    if self.frame_bytes > MAX_EVENT_BYTES {
                        return Err("provider stream event exceeded the 1 MiB limit");
                    }
                }
            }
        }
        Ok(events)
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<String>, &'static str> {
        let mut events = Vec::new();
        if !self.line.is_empty() {
            self.process_line(&mut events)?;
        }
        self.dispatch(&mut events);
        Ok(events)
    }

    fn process_line(&mut self, events: &mut Vec<String>) -> Result<(), &'static str> {
        if self.line.is_empty() {
            self.dispatch(events);
            return Ok(());
        }
        if self.line.first() == Some(&b':') {
            self.line.clear();
            return Ok(());
        }
        let line = std::str::from_utf8(&self.line)
            .map_err(|_| "provider stream contained invalid UTF-8")?;
        if let Some(value) = line.strip_prefix("data:") {
            if self.has_data {
                self.data.push('\n');
            }
            self.data.push_str(value.strip_prefix(' ').unwrap_or(value));
            self.has_data = true;
        }
        self.line.clear();
        Ok(())
    }

    fn dispatch(&mut self, events: &mut Vec<String>) {
        if self.has_data {
            events.push(std::mem::take(&mut self.data));
            self.has_data = false;
        }
        self.frame_bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_EVENT_BYTES, SseParser};

    #[test]
    fn parses_events_across_chunk_boundaries_and_multiple_data_lines() {
        let mut parser = SseParser::new();
        assert!(parser.push(b"data: first\r").unwrap().is_empty());
        assert_eq!(
            parser.push(b"\ndata: second\r\n\r\n").unwrap(),
            ["first\nsecond"]
        );
    }

    #[test]
    fn ignores_comments_and_flushes_a_final_event_without_a_blank_line() {
        let mut parser = SseParser::new();
        assert!(parser.push(b": keepalive\ndata: final").unwrap().is_empty());
        assert_eq!(parser.finish().unwrap(), ["final"]);
    }

    #[test]
    fn rejects_oversized_events() {
        let mut parser = SseParser::new();
        assert!(parser.push(&vec![b'x'; MAX_EVENT_BYTES + 1]).is_err());
    }
}
