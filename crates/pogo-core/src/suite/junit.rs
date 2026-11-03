//! JUnit XML for CI systems (GitHub Actions, GitLab, Jenkins all read it).

use super::{Status, SuiteData};

/// Escape text for an XML attribute or element, dropping characters XML 1.0 forbids
/// (device output can contain raw control bytes).
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

fn secs(ms: u64) -> String {
    format!("{:.3}", ms as f64 / 1000.0)
}

pub fn render(d: &SuiteData) -> String {
    let total_ms: u64 = d.tests.iter().map(|t| t.duration_ms).sum();
    let suite = escape(&d.file);
    let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str(&format!(
        "<testsuites name=\"pogo\" tests=\"{}\" failures=\"{}\" time=\"{}\">\n",
        d.tests.len(),
        d.failed,
        secs(total_ms)
    ));
    x.push_str(&format!(
        "  <testsuite name=\"{suite}\" tests=\"{}\" failures=\"{}\" time=\"{}\">\n",
        d.tests.len(),
        d.failed,
        secs(total_ms)
    ));
    for t in &d.tests {
        let open = format!(
            "    <testcase name=\"{}\" classname=\"{suite}\" time=\"{}\"",
            escape(&t.name),
            secs(t.duration_ms)
        );
        match (&t.status, &t.failure) {
            (Status::Failed, Some(f)) => {
                x.push_str(&open);
                x.push_str(">\n");
                x.push_str(&format!(
                    "      <failure message=\"step {}: {}\">{}</failure>\n",
                    f.step,
                    escape(&f.message),
                    escape(&f.lines.join("\n"))
                ));
                x.push_str("    </testcase>\n");
            }
            _ => {
                x.push_str(&open);
                x.push_str("/>\n");
            }
        }
    }
    x.push_str("  </testsuite>\n</testsuites>\n");
    x
}
