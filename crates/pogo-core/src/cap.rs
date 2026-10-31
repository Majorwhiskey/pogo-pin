//! The soft cap on output size (docs/agent-contract.md §6.5): agents should not have their
//! context flooded, so an oversized `data` loses capture lines from the middle.

use serde_json::Value;

use crate::envelope::Warning;

/// About 200 KB of JSON per envelope.
pub const SOFT_CAP_BYTES: usize = 200 * 1024;

/// Trim `data` toward `limit` bytes of JSON by dropping lines from the middle of every
/// capture's `lines` (keeping the first 20%, like `--max-lines`), and counting them in its
/// `omitted_lines`. Returns the `OUTPUT_TRUNCATED` warning when the cap was hit.
pub fn apply(data: &mut Value, limit: usize) -> Option<Warning> {
    let start = size(data);
    if start <= limit {
        return None;
    }
    let mut dropped = 0usize;
    while size(data) > limit {
        let Some(captures) = largest_capture(data) else {
            break;
        };
        let removed = shrink(captures);
        if removed == 0 {
            break;
        }
        dropped += removed;
    }
    Some(Warning::new(
        "OUTPUT_TRUNCATED",
        format!(
            "output was {} KB, over the {} KB cap; omitted {dropped} more lines from the middle",
            start / 1024,
            limit / 1024
        ),
    ))
}

fn size(v: &Value) -> usize {
    serde_json::to_string(v).map_or(0, |s| s.len())
}

/// The capture object (one with `lines` and `omitted_lines`) holding the most lines,
/// if any has more than two left to trim.
fn largest_capture(v: &mut Value) -> Option<&mut serde_json::Map<String, Value>> {
    let n = max_lines(v);
    if n <= 2 {
        return None;
    }
    find_with(v, n)
}

fn max_lines(v: &Value) -> usize {
    match v {
        Value::Object(map) => {
            let own = match (map.get("lines"), map.get("omitted_lines")) {
                (Some(Value::Array(lines)), Some(_)) => lines.len(),
                _ => 0,
            };
            map.values().map(max_lines).fold(own, usize::max)
        }
        Value::Array(items) => items.iter().map(max_lines).fold(0, usize::max),
        _ => 0,
    }
}

fn find_with(v: &mut Value, n: usize) -> Option<&mut serde_json::Map<String, Value>> {
    match v {
        Value::Object(map) => {
            let own = matches!(
                (map.get("lines"), map.get("omitted_lines")),
                (Some(Value::Array(lines)), Some(_)) if lines.len() == n
            );
            if own {
                return Some(map);
            }
            map.values_mut().find_map(|c| find_with(c, n))
        }
        Value::Array(items) => items.iter_mut().find_map(|i| find_with(i, n)),
        _ => None,
    }
}

/// Drop a quarter of the lines just after the first 20%; returns how many were dropped.
fn shrink(capture: &mut serde_json::Map<String, Value>) -> usize {
    let Some(Value::Array(lines)) = capture.get_mut("lines") else {
        return 0;
    };
    let len = lines.len();
    let head = len / 5;
    let count = (len / 4).max(1).min(len.saturating_sub(head + 1));
    lines.drain(head..head + count);
    let omitted = capture
        .get("omitted_lines")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    capture.insert("omitted_lines".into(), Value::from(omitted + count as u64));
    count
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn capture(n: usize, width: usize) -> Value {
        let lines: Vec<Value> = (0..n)
            .map(|i| json!({ "t_ms": i, "text": format!("{i:05} {}", "x".repeat(width)) }))
            .collect();
        json!({ "outcome": "completed", "line_count": n, "omitted_lines": 0, "lines": lines })
    }

    #[test]
    fn small_output_is_untouched() {
        let mut v = json!({ "device": {}, "capture": capture(10, 10) });
        let before = v.clone();
        assert_eq!(apply(&mut v, SOFT_CAP_BYTES), None);
        assert_eq!(v, before);
    }

    #[test]
    fn big_capture_loses_middle_lines_and_counts_them() {
        let mut v = json!({ "device": {}, "capture": capture(4000, 100) });
        let w = apply(&mut v, 50 * 1024).unwrap();
        assert_eq!(w.code, "OUTPUT_TRUNCATED");
        assert!(size(&v) <= 50 * 1024, "{}", size(&v));
        let c = &v["capture"];
        let kept = c["lines"].as_array().unwrap();
        assert_eq!(c["line_count"], 4000, "line_count still counts every line");
        assert_eq!(
            kept.len() as u64 + c["omitted_lines"].as_u64().unwrap(),
            4000,
            "kept + omitted = seen"
        );
        assert!(
            kept[0]["text"].as_str().unwrap().starts_with("00000"),
            "head kept"
        );
        assert!(
            kept.last().unwrap()["text"]
                .as_str()
                .unwrap()
                .starts_with("03999"),
            "tail kept"
        );
        assert!(w.message.contains("omitted"));
    }

    #[test]
    fn data_without_lines_only_warns() {
        let mut v = json!({ "blob": "y".repeat(2000) });
        let w = apply(&mut v, 100).unwrap();
        assert_eq!(w.code, "OUTPUT_TRUNCATED");
        assert!(w.message.contains("omitted 0 more lines"));
    }
}
