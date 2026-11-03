//! `pogo serve` over HTTP: token checks, the MCP handshake, and a tool call against a
//! replayed board. Plain HTTP/1.1 over a TcpStream keeps the test free of client crates.

#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const TOKEN: &str = "test-token";

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn start(replay: &str) -> Server {
    let port = free_port();
    let child = Command::new(assert_cmd::cargo::cargo_bin("pogo"))
        .env_remove("POGO_CONFIG")
        .env("POGO_REPLAY", replay)
        .env("POGO_SERVE_TOKEN", TOKEN)
        .args(["serve", "--bind", &format!("127.0.0.1:{port}")])
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "server did not start"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    Server { child, port }
}

/// POST `body` to `path`; returns the status, the session header, and the JSON-RPC
/// message carrying `id` (from a JSON or an event-stream body).
fn post(
    port: u16,
    path: &str,
    token: Option<&str>,
    session: Option<&str>,
    body: &Value,
) -> (u16, Option<String>, Option<Value>) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let body = body.to_string();
    let mut req = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(t) = token {
        req.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    if let Some(id) = session {
        req.push_str(&format!("Mcp-Session-Id: {id}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(&body);
    s.write_all(req.as_bytes()).unwrap();
    let mut text = String::new();
    let mut buf = [0u8; 4096];
    let id = body_id(&body);
    loop {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => text.push_str(&String::from_utf8_lossy(&buf[..n])),
        }
        if id.is_some_and(|id| message(&text, id).is_some()) {
            break;
        }
    }
    let status = text
        .split(' ')
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let session = text.lines().find_map(|l| {
        l.to_ascii_lowercase()
            .strip_prefix("mcp-session-id: ")
            .map(|_| l[16..].trim().to_string())
    });
    (status, session, id.and_then(|id| message(&text, id)))
}

fn body_id(body: &str) -> Option<u64> {
    serde_json::from_str::<Value>(body)
        .ok()?
        .get("id")?
        .as_u64()
}

/// The first JSON object in the response whose `id` is `id`.
fn message(text: &str, id: u64) -> Option<Value> {
    text.lines()
        .map(|l| l.strip_prefix("data: ").unwrap_or(l))
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["id"] == id)
}

fn fixture(name: &str) -> String {
    let src = format!(
        "{}/../../tests/fixtures/synthetic/{name}.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    let dir = std::env::temp_dir().join(format!("pogo-serve-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join(format!("{name}.jsonl"));
    std::fs::copy(src, &dst).unwrap();
    dst.display().to_string()
}

fn initialize() -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": { "name": "pogo-test", "version": "0" } } })
}

#[test]
fn token_and_path_are_enforced() {
    let s = start(&fixture("clean_boot"));
    assert_eq!(post(s.port, "/mcp", None, None, &initialize()).0, 401);
    assert_eq!(
        post(s.port, "/mcp", Some("wrong"), None, &initialize()).0,
        401
    );
    assert_eq!(
        post(s.port, "/other", Some(TOKEN), None, &initialize()).0,
        404
    );
}

#[test]
fn tools_work_over_http() {
    let s = start(&fixture("clean_boot"));
    let (status, session, init) = post(s.port, "/mcp", Some(TOKEN), None, &initialize());
    assert_eq!(status, 200);
    assert_eq!(init.unwrap()["result"]["serverInfo"]["name"], "pogo");
    let session = session.unwrap();
    let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    post(s.port, "/mcp", Some(TOKEN), Some(&session), &note);
    let call = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
        "name": "serial_capture",
        "arguments": { "until": ["READY"], "reset": true, "timeout": "300ms" } } });
    let (_, _, reply) = post(s.port, "/mcp", Some(TOKEN), Some(&session), &call);
    let result = &reply.unwrap()["result"];
    assert_eq!(result["isError"], false, "{result:#}");
    assert_eq!(result["structuredContent"]["data"]["outcome"], "matched");
}

#[test]
fn non_loopback_without_a_token_is_refused() {
    let out = Command::new(assert_cmd::cargo::cargo_bin("pogo"))
        .env_remove("POGO_SERVE_TOKEN")
        .env_remove("POGO_FORMAT")
        .args(["serve", "--bind", "0.0.0.0:0", "-f", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("without a token")
    );
}
