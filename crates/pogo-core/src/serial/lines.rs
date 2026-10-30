//! Splits a byte stream into lines on `\n`, `\r\n`, or a lone `\r`.

/// One line's bytes, without its terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLine {
    /// When the line's first byte arrived.
    pub t_ms: u64,
    pub bytes: Vec<u8>,
    /// Bytes past the buffer cap were dropped.
    pub truncated: bool,
}

#[derive(Debug)]
pub struct LineSplitter {
    buf: Vec<u8>,
    start_ms: Option<u64>,
    /// The previous byte was `\r`, so a following `\n` belongs to it.
    after_cr: bool,
    truncated: bool,
    max_bytes: usize,
}

impl LineSplitter {
    /// Lines longer than `max_bytes` keep their first `max_bytes` bytes, so a device that
    /// never sends a newline cannot grow memory without bound.
    pub fn new(max_bytes: usize) -> Self {
        LineSplitter {
            buf: Vec::new(),
            start_ms: None,
            after_cr: false,
            truncated: false,
            max_bytes,
        }
    }

    /// Feed bytes that arrived at `t_ms`; returns every line they complete.
    pub fn push(&mut self, data: &[u8], t_ms: u64) -> Vec<RawLine> {
        let mut out = Vec::new();
        for &b in data {
            if std::mem::take(&mut self.after_cr) && b == b'\n' {
                continue;
            }
            match b {
                b'\n' => out.push(self.take(t_ms)),
                b'\r' => {
                    out.push(self.take(t_ms));
                    self.after_cr = true;
                }
                _ => {
                    self.start_ms.get_or_insert(t_ms);
                    if self.buf.len() < self.max_bytes {
                        self.buf.push(b);
                    } else {
                        self.truncated = true;
                    }
                }
            }
        }
        out
    }

    /// The unterminated final line, if any (e.g. at the deadline).
    pub fn finish(&mut self) -> Option<RawLine> {
        self.after_cr = false;
        self.start_ms.map(|t| self.take(t))
    }

    fn take(&mut self, t_ms: u64) -> RawLine {
        RawLine {
            t_ms: self.start_ms.take().unwrap_or(t_ms),
            bytes: std::mem::take(&mut self.buf),
            truncated: std::mem::take(&mut self.truncated),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split_all(chunks: &[&[u8]]) -> Vec<Vec<u8>> {
        let mut s = LineSplitter::new(1024);
        let mut lines: Vec<_> = chunks
            .iter()
            .enumerate()
            .flat_map(|(i, c)| s.push(c, i as u64))
            .map(|l| l.bytes)
            .collect();
        lines.extend(s.finish().map(|l| l.bytes));
        lines
    }

    #[test]
    fn terminators() {
        let lines = split_all(&[b"a\nb\r\nc\rd\n\ne"]);
        let want: [&[u8]; 6] = [b"a", b"b", b"c", b"d", b"", b"e"];
        assert_eq!(lines, want);
    }

    #[test]
    fn crlf_split_across_chunks_is_one_terminator() {
        assert_eq!(split_all(&[b"a\r", b"\nb\n"]), [b"a", b"b"]);
    }

    #[test]
    fn every_chunking_gives_the_same_lines() {
        let input: &[u8] = b"I (1) boot: ok\r\n\x1b[0;32mREADY\x1b[0m\n\rx\r\r\ny\xff\npartial";
        let whole = split_all(&[input]);
        for i in 0..=input.len() {
            for j in i..=input.len() {
                let got = split_all(&[&input[..i], &input[i..j], &input[j..]]);
                assert_eq!(got, whole, "split at {i},{j}");
            }
        }
    }

    #[test]
    fn line_time_is_first_byte_time() {
        let mut s = LineSplitter::new(64);
        assert!(s.push(b"ab", 5).is_empty());
        let lines = s.push(b"c\nd", 9);
        assert_eq!(lines[0].t_ms, 5);
        assert_eq!(s.finish().unwrap().t_ms, 9);
        assert_eq!(s.finish(), None);
    }

    #[test]
    fn long_lines_are_capped_and_flagged() {
        let mut s = LineSplitter::new(4);
        let lines = s.push(b"abcdefgh\nok\n", 0);
        assert_eq!(lines[0].bytes, b"abcd");
        assert!(lines[0].truncated);
        assert_eq!(lines[1].bytes, b"ok");
        assert!(!lines[1].truncated);
    }
}
