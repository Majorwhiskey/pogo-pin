//! `pogo mcp` over stdio with a minimal client: tool list, results matching the CLI's
//! `data`, and errors carrying the CLI's codes. POGO_REPLAY stands in for the device.

#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

/// A private copy per test: the replay path is the lock key, and MCP tools do not wait
/// for a busy device, so tests sharing a fixture would collide.
fn fixture(name: &str) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let src = format!(
        "{}/../../tests/fixtures/synthetic/{name}.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let dir = std::env::temp_dir().join(format!("pogo-mcp-fx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join(format!("{name}-{}.jsonl", N.fetch_add(1, Ordering::SeqCst)));
    std::fs::copy(src, &dst).unwrap();
    dst.display().to_string()
}

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn start(replay: &str) -> Self {
        let mut child = Command::new(assert_cmd::cargo::cargo_bin("pogo"))
            .arg("mcp")
            .env_remove("POGO_CONFIG")
            .env("POGO_REPLAY", replay)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut c = Client {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        let init = c.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "pogo-test", "version": "0" }
            }),
        );
        assert_eq!(init["serverInfo"]["name"], "pogo");
        c.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        c
    }

    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Send a request and wait for its response; one call at a time.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "server closed"
            );
            let msg: Value = serde_json::from_str(&line).unwrap();
            if msg["id"] == id {
                return msg["result"].clone();
            }
        }
    }

    fn call(&mut self, tool: &str, args: Value) -> Value {
        self.request("tools/call", json!({ "name": tool, "arguments": args }))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Host timestamps differ between two runs; everything else must match.
fn without_times(mut v: Value) -> Value {
    match &mut v {
        Value::Object(map) => {
            map.remove("t_ms");
            for value in map.values_mut() {
                *value = without_times(value.take());
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                *item = without_times(item.take());
            }
        }
        _ => {}
    }
    v
}

#[test]
fn tools_are_listed_without_erase() {
    let mut c = Client::start(&fixture("clean_boot"));
    let tools = c.request("tools/list", json!({}));
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for want in [
        "list_devices",
        "device_info",
        "flash",
        "reset",
        "serial_capture",
        "serial_send",
        "run",
    ] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    let flash = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "flash")
        .unwrap();
    assert!(
        flash["inputSchema"]["properties"]
            .get("erase_all")
            .is_none()
    );
    assert_eq!(flash["annotations"]["destructiveHint"], true);
}

#[test]
fn serial_capture_data_equals_the_cli() {
    let replay = fixture("clean_boot");
    let mut c = Client::start(&replay);
    let mcp = c.call(
        "serial_capture",
        json!({ "until": ["READY"], "reset": true, "timeout": "300ms" }),
    );
    assert_eq!(mcp["isError"], false);
    let mcp_data = mcp["structuredContent"]["data"].clone();
    drop(c);

    let out = assert_cmd::Command::cargo_bin("pogo")
        .unwrap()
        .env_remove("POGO_CONFIG")
        .env("POGO_REPLAY", &replay)
        .args(["serial", "capture", "--reset", "--until", "READY"])
        .args(["--timeout", "300ms", "--wait-lock", "30s", "-f", "json"])
        .assert()
        .code(0)
        .get_output()
        .clone();
    let cli: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(without_times(mcp_data), without_times(cli["data"].clone()));
}

#[test]
fn errors_carry_cli_codes_and_partial_data() {
    let mut c = Client::start(&fixture("rust_panic"));
    let r = c.call(
        "serial_capture",
        json!({ "until": ["DONE"], "reset": true, "timeout": "300ms" }),
    );
    assert_eq!(r["isError"], true);
    let body = &r["structuredContent"];
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "TARGET_FAULT");
    assert!(body["error"]["hint"].is_string());
    assert_eq!(body["data"]["panic"]["location"]["line"], 47);
}

#[test]
fn bad_arguments_are_usage_errors() {
    let mut c = Client::start(&fixture("clean_boot"));
    let r = c.call("serial_capture", json!({ "timeout": "soon" }));
    assert_eq!(r["structuredContent"]["error"]["code"], "USAGE");
    let r = c.call("flash", json!({ "image": "/x.bin", "address": "zz" }));
    assert_eq!(r["structuredContent"]["error"]["code"], "USAGE");
}

#[test]
fn flash_guardrails_hold_over_mcp() {
    let dir = std::env::temp_dir().join(format!("pogo-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("boot.bin");
    std::fs::write(&bin, [0xe9, 0, 0, 0]).unwrap();
    let mut c = Client::start(&fixture("clean_boot"));
    let r = c.call(
        "flash",
        json!({ "image": bin.to_str().unwrap(), "address": "0x8000" }),
    );
    assert_eq!(r["structuredContent"]["error"]["code"], "GUARDRAIL_REFUSED");
}
