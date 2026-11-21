use crate::transport::stream::FrameParser;

const MAX_LINE_BYTES: usize = 1024 * 1024;

/// Parses newline-delimited JSON frames across arbitrary response chunks.
pub(crate) struct JsonlParser {
    line: Vec<u8>,
    skip_lf: bool,
}

impl JsonlParser {
    pub(crate) fn new() -> Self {
        Self {
            line: Vec::new(),
            skip_lf: false,
        }
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, &'static str> {
        let mut frames = Vec::new();
        for byte in chunk {
            if self.skip_lf {
                self.skip_lf = false;
                if *byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\r' => {
                    self.dispatch(&mut frames)?;
                    self.skip_lf = true;
                }
                b'\n' => self.dispatch(&mut frames)?,
                byte => {
                    self.line.push(*byte);
                    if self.line.len() > MAX_LINE_BYTES {
                        return Err("provider stream JSON line exceeded the 1 MiB limit");
                    }
                }
            }
        }
        Ok(frames)
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<String>, &'static str> {
        let mut frames = Vec::new();
        self.dispatch(&mut frames)?;
        Ok(frames)
    }

    fn dispatch(&mut self, frames: &mut Vec<String>) -> Result<(), &'static str> {
        if self.line.is_empty() {
            return Ok(());
        }

        let line = std::str::from_utf8(&self.line)
            .map_err(|_| "provider stream JSON line contained invalid UTF-8")?;
        frames.push(line.to_owned());
        self.line.clear();
        Ok(())
    }
}

impl FrameParser for JsonlParser {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, &'static str> {
        JsonlParser::push(self, chunk)
    }

    fn finish(&mut self) -> Result<Vec<String>, &'static str> {
        JsonlParser::finish(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonlParser, MAX_LINE_BYTES};

    #[test]
    fn parses_json_lines_across_chunks_and_crlf_boundaries() {
        let mut parser = JsonlParser::new();
        assert_eq!(parser.push(b"{\"one\":1}\r").unwrap(), ["{\"one\":1}"]);
        assert!(parser.push(b"\n{\"two\":").unwrap().is_empty());
        assert!(parser.push(b"2}").unwrap().is_empty());
        assert_eq!(parser.finish().unwrap(), ["{\"two\":2}"]);
    }

    #[test]
    fn skips_blank_lines_and_rejects_invalid_utf8_and_oversized_lines() {
        let mut parser = JsonlParser::new();
        assert_eq!(parser.push(b"\n{}\n").unwrap(), ["{}"]);
        assert!(parser.push(&[0xff]).unwrap().is_empty());
        assert!(parser.finish().is_err());

        let mut parser = JsonlParser::new();
        assert!(parser.push(&vec![b'x'; MAX_LINE_BYTES + 1]).is_err());
    }
}
