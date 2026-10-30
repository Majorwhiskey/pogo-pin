//! ANSI escape stripping: CSI (`ESC [ ... final`), OSC (`ESC ] ... BEL|ESC \`), and
//! two-character escapes. ESP-IDF colors its log lines with CSI codes.

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' {
                        chars.next_if_eq(&'\\');
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip_ansi;

    #[test]
    fn strips_esp_idf_colors() {
        assert_eq!(
            strip_ansi("\x1b[0;32mI (412) wifi: connected\x1b[0m"),
            "I (412) wifi: connected"
        );
    }

    #[test]
    fn strips_other_sequences_and_keeps_text() {
        assert_eq!(strip_ansi("a\x1b[2Kb\x1b[1;31;40mc"), "abc");
        assert_eq!(strip_ansi("\x1b]0;title\x07x"), "x");
        assert_eq!(strip_ansi("\x1b]8;;http://x\x1b\\link"), "link");
        assert_eq!(strip_ansi("a\x1bMb"), "ab");
        assert_eq!(strip_ansi("plain é ✓"), "plain é ✓");
    }

    #[test]
    fn truncated_sequences_do_not_panic() {
        assert_eq!(strip_ansi("x\x1b"), "x");
        assert_eq!(strip_ansi("x\x1b["), "x");
        assert_eq!(strip_ansi("x\x1b[0;3"), "x");
        assert_eq!(strip_ansi("x\x1b]unterminated"), "x");
    }
}
