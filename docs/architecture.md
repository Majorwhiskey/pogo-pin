# 4. Architecture

## 4.1 Overview

```
 ┌────────────────────┐      ┌────────────────────┐
 │   pogo (CLI)   │      │  pogo mcp      │
 │ clap, output, exit │      │ rmcp, stdio, tokio │
 └─────────┬──────────┘      └─────────┬──────────┘
           └──────────────┬────────────┘
                          ▼
                 ┌──────────────────┐
                 │  pogo-core   │  sync, no printing, typed results
                 └──────────────────┘
   ┌──────────┬──────────┼───────────┬─────────────┬──────────┐
   ▼          ▼          ▼           ▼             ▼          ▼
discovery  transport   serial     flash/reset   probe      symbolize
(USB enum) (serial,    (capture,  (espflash)    client     (ELF →
           mock,       send,                    (protocol)  file:line)
           replay)     parsers)
   │                                               │
   ▼                                               ▼
 ESP32 target (USB serial)                Pico probe (USB CDC)
                                                   │ GPIO / I2C / INA226
                                                   ▼
                                             ESP32 target pins
```

## 4.2 Workspace layout

```
pogo-pin/
├── Cargo.toml                    # workspace root; excludes firmware/*
├── README.md                     # overview and index of the spec
├── CLAUDE.md                     # agent working rules (see §14.3)
├── pogo.example.toml
├── crates/
│   ├── pogo-core/            # library: all logic, no printing, no process::exit
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs          # PogoError and hints; error/code.rs: ErrorCode, exit codes
│   │       ├── deadline.rs
│   │       ├── config/           # discovery, merge with per-key sources, validation
│   │       ├── discovery.rs      # USB enumeration, classification; discovery/resolve.rs: --device
│   │       ├── lock.rs           # per-device lock files
│   │       ├── transport/        # Transport trait; serial, mock, replay, recording, reconnecting
│   │       ├── serial/           # line splitter, ANSI strip, parsers, capture engine, panic detection
│   │       ├── flash/            # Flasher trait, guardrail checks; esp.rs: espflash implementation
│   │       ├── reset.rs          # DTR/RTS sequences, re-enumeration wait
│   │       ├── symbolize.rs      # ELF + addr2line
│   │       ├── ops/              # whole commands (select, lock, act, map outcomes) for CLI and MCP
│   │       ├── doctor.rs         # environment checks
│   │       ├── cap.rs            # ~200 KB soft cap on output
│   │       ├── audit.rs
│   │       ├── probe/            # probe client over pogo-protocol (Phase 4)
│   │       └── power.rs          # INA226 conversion, stats (Phase 5)
│   ├── pogo-protocol/        # no_std shared host↔probe types (serde + postcard)
│   ├── pogo-cli/             # binary `pogo`: clap, output formatting, exit mapping
│   └── pogo-mcp/             # MCP server built on core; launched via `pogo mcp`
├── firmware/                     # separate cargo projects (different targets)
│   ├── probe/                    # Embassy firmware for RP2040/RP2350
│   └── test-target/              # esp-hal fixture firmware for tests
├── tests/
│   ├── fixtures/                 # recorded serial sessions (.jsonl)
│   └── hil/                      # hardware-in-the-loop tests (opt-in)
├── schemas/                      # generated JSON Schemas (committed, drift-checked)
└── docs/                         # the spec, one file per README section
    ├── agent-contract.md
    ├── hardware.md
    ├── ...
    └── decisions/                # short architecture decision records (ADRs)
```

The firmware projects must be excluded from the root workspace (`exclude = ["firmware/*"]`) because they build for different targets.

## 4.3 Design rules

1. **The core never prints and never exits.** It returns typed results or errors. Only `pogo-cli` writes to stdout/stderr and sets exit codes.
2. **The core is synchronous.** Blocking I/O with explicit deadlines is easier to reason about for serial work. `pogo-mcp` calls the core through `tokio::task::spawn_blocking`.
3. **Every operation takes a deadline.** No core function can block forever.
4. **Device identity is the USB serial number,** not the port path, which changes across reboots and re-enumerations.
5. **All I/O goes through a `Transport` trait** so everything is testable without hardware.
6. **Outcomes are data, not errors.** A capture that times out or detects a panic returns `Ok(CaptureResult { outcome: Timeout | Fault, .. })` with its partial data. Rust errors are reserved for when an operation couldn't run at all (device missing, I/O failure). The CLI maps outcomes to error codes and exit codes.
7. **All result types derive `Serialize` and `JsonSchema`,** so JSON output and schemas come from the same source.

## 4.4 Core types (sketch)

```rust
// transport/mod.rs
pub trait Transport: Send {
    /// Read available bytes, waiting at most `timeout`. Ok(0) means no data yet.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, PogoError>;
    fn write_all(&mut self, data: &[u8]) -> Result<(), PogoError>;
    fn set_dtr(&mut self, asserted: bool) -> Result<(), PogoError>;
    fn set_rts(&mut self, asserted: bool) -> Result<(), PogoError>;
    fn set_baud(&mut self, baud: u32) -> Result<(), PogoError>;
}
// Implementations: SerialTransport (serialport crate), MockTransport (scripted, timed
// chunks), ReplayTransport (reads a recorded .jsonl session).

// deadline.rs
pub struct Deadline(Instant);
impl Deadline {
    pub fn after(d: Duration) -> Self;
    pub fn remaining(&self) -> Option<Duration>; // None once expired
}

// error.rs
#[derive(Debug, thiserror::Error)]
pub enum PogoError {
    #[error("no device matched '{selector}'")]
    DeviceNotFound { selector: String },
    #[error("{} devices matched; pass --device to choose one", candidates.len())]
    AmbiguousDevice { candidates: Vec<DeviceSummary> },
    #[error("device is in use by pid {pid} ({command})")]
    DeviceBusy { pid: u32, command: String },
    #[error("permission denied opening {port}")]
    PermissionDenied { port: String },
    #[error("device I/O error: {0}")]
    DeviceIo(String),
    #[error("probe protocol error: {0}")]
    Protocol(String),
    #[error("refused by guardrail: {reason}")]
    GuardrailRefused { reason: String },
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
}
impl PogoError {
    pub fn code(&self) -> ErrorCode;       // stable SCREAMING_SNAKE_CASE string
    pub fn exit_code(&self) -> u8;         // see §6.4
    pub fn hint(&self) -> Option<String>;  // actionable next step
}

// serial/capture.rs
pub fn capture(
    t: &mut dyn Transport,
    opts: &CaptureOptions,
    deadline: Deadline,
) -> Result<CaptureResult, PogoError>;

#[derive(Serialize, JsonSchema)]
pub struct CaptureResult {
    pub outcome: Outcome,               // matched | completed | fault | timeout | eof
    pub matched: Option<PatternMatch>,
    pub lines: Vec<LogLine>,
    pub line_count: usize,
    pub omitted_lines: usize,
    pub boot_count: u32,
    pub resets: Vec<ResetEvent>,        // parsed reset reasons
    pub panic: Option<PanicReport>,
}
```

# 5. Tech stack

**Language:** Rust, stable toolchain, edition 2024. License: MIT OR Apache-2.0.

| Concern | Crate(s) | Why |
|---|---|---|
| CLI parsing | `clap` (derive, env), `clap_complete` | Standard, generates help and completions |
| Serialization | `serde`, `serde_json` | JSON output |
| JSON Schema | `schemars` | Schemas from the same types as output; use the major version compatible with `rmcp` |
| Serial I/O | `serialport` | Cross-platform, exposes USB VID/PID/serial number |
| ESP flashing | `espflash` (library) | Mature ESP flashing and chip detection; wrap behind a `Flasher` trait |
| Patterns | `regex` | `--until`, `--fail-on`, log parsing |
| ANSI stripping | `strip-ansi-escapes` | Clean log text |
| Durations | `humantime` | `500ms`, `10s`, `2m` |
| Errors | `thiserror` | Typed errors in core |
| Logging | `tracing`, `tracing-subscriber` | Logs to stderr, `-v` levels |
| Symbolization | `object`, `addr2line` | ELF address → function/file/line |
| Locking | `fd-lock` | Per-device lock files |
| Paths | `directories` | Platform config/state/runtime dirs |
| Config | `toml` | `pogo.toml` |
| Signals | `ctrlc` | Clean shutdown with partial results |
| Probe protocol | `postcard` (with its COBS helpers), `heapless` | Compact `no_std` binary messages |
| MCP | `rmcp` (features: server, macros, schemars, transport-io), `tokio` | Official Rust MCP SDK, stdio transport |
| Testing | `insta` (json), `assert_cmd`, `predicates`, `proptest` | Snapshots, CLI tests, property tests |

**Probe firmware:** `embassy-rp`, `embassy-executor`, `embassy-time`, `embassy-sync`, `embassy-usb` (CDC-ACM), `heapless`, `postcard`, `panic-reset` (a panic resets the probe, which returns all pins to high-Z).

**Test-target firmware:** `esp-hal`, `esp-println` (with the `log` integration), `esp-backtrace`, `log`.

> **Versions:** do not pin versions from memory. Check crates.io and docs.rs for current releases and APIs when adding each dependency. `espflash`, `rmcp`, and the Embassy crates change quickly. If the `espflash` library API proves awkward, shelling out to the `espflash` CLI behind the same `Flasher` trait is an acceptable fallback; record the decision in `docs/decisions/`.

[Back to README](../README.md)
