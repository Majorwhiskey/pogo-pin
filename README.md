# pogo-pin

Give AI coding agents eyes and hands on real hardware. Flash, monitor, and probe ESP32 boards from a CLI or MCP server, with commands that always terminate, return JSON, and use meaningful exit codes.

**An agent-friendly hardware workbench CLI (`pogo`).** Flash, reset, monitor, probe, and measure ESP32 boards from the terminal, with deterministic, machine-readable output built for AI coding agents (like Claude Code) and humans alike.

> **Status:** pre-alpha. This README is the project specification. The project is `pogo-pin`; the binary is `pogo` and crates are `pogo-*`. Check crates.io and Homebrew for availability before the first public release.
>
> **Last updated:** 2026-10-26

---

## Table of contents

1. [Overview](#1-overview)
2. [Goals and non-goals](#2-goals-and-non-goals)
3. [Hardware setup](#3-hardware-setup)
4. [Architecture](#4-architecture)
5. [Tech stack](#5-tech-stack)
6. [The agent-friendly CLI contract](#6-the-agent-friendly-cli-contract)
7. [Command reference](#7-command-reference)
8. [Probe firmware and wire protocol](#8-probe-firmware-and-wire-protocol)
9. [MCP server](#9-mcp-server)
10. [Configuration](#10-configuration)
11. [Safety guardrails](#11-safety-guardrails)
12. [Testing strategy](#12-testing-strategy)
13. [Roadmap](#13-roadmap)
14. [Instructions for Claude Code](#14-instructions-for-claude-code)
15. [CI, release, and distribution](#15-ci-release-and-distribution)
16. [Launch and showcase checklist](#16-launch-and-showcase-checklist)

---

## 1. Overview

### 1.1 The problem

AI coding agents can write embedded firmware, compile it, and fix compiler errors on their own. But once the firmware is running on a real board, the agent is blind. Did it boot? Did it panic? Did the sensor respond? Today a human has to open a serial monitor, watch the output, and paste logs back into the conversation.

Existing tools don't help because they were designed for a human at a keyboard. `screen`, `minicom`, `idf.py monitor`, and `espflash monitor` stream forever, need a keypress to exit, and emit unstructured text full of ANSI color codes. An agent that runs them either hangs until its tool call times out or floods its context window with noise.

### 1.2 The solution

`pogo` turns the bench into commands that **always terminate**, return **structured JSON**, and use **meaningful exit codes**. With it, an agent can close the loop by itself:

```
edit code → build → pogo run → read JSON result → fix → repeat
```

### 1.3 Example

```console
$ cargo build --release
$ pogo run target/riscv32imc-unknown-none-elf/release/test-target \
    --until "READY" --timeout 20s --format json
$ echo $?
7
```

```json
{
  "schema_version": 1,
  "pogo_version": "0.1.0",
  "command": "run",
  "ok": false,
  "duration_ms": 6412,
  "data": {
    "device": { "alias": "target", "port": "/dev/ttyACM0", "usb_serial": "F4:12:FA:9C:3A:10" },
    "flash": { "chip": "esp32c3", "bytes_written": 214016, "verified": true, "duration_ms": 4870 },
    "capture": {
      "outcome": "fault",
      "matched": null,
      "boot_count": 1,
      "line_count": 3,
      "omitted_lines": 0,
      "lines": [
        { "t_ms": 1210, "level": "info", "tag": null, "text": "INFO - wifi connected" },
        { "t_ms": 1388, "level": "info", "tag": null, "text": "INFO - reading sensor at 0x76" },
        { "t_ms": 1402, "level": null, "tag": null, "text": "====================== PANIC ======================" }
      ],
      "panic": {
        "kind": "panic",
        "message": "called `Option::unwrap()` on a `None` value",
        "location": { "file": "src/sensors.rs", "line": 42, "column": 18 },
        "backtrace": [
          { "addr": "0x42001a3c", "function": "test_target::sensors::read_temp", "file": "src/sensors.rs", "line": 42 },
          { "addr": "0x42000f10", "function": "test_target::main", "file": "src/main.rs", "line": 77 }
        ]
      }
    }
  },
  "error": {
    "code": "TARGET_FAULT",
    "message": "Target panicked 1.4s after boot",
    "hint": "See data.capture.panic.location (src/sensors.rs:42)."
  },
  "warnings": []
}
```

Exit code `7` means the target faulted. The agent opens `src/sensors.rs:42`, fixes the bug, reruns the same command, and gets exit code `0`.

### 1.4 Interfaces

`pogo` exposes the same capabilities through two thin frontends over one core library:

- **CLI** (`pogo <command>`): for humans, shell scripts, CI, and any agent that can run shell commands.
- **MCP server** (`pogo mcp`): the same operations exposed as MCP tools over stdio.

---

## 2. Goals and non-goals

### Goals (v1)

- **G1. Deterministic agent contract.** Every command terminates, writes results only to stdout, offers JSON output, and returns documented exit codes (§6).
- **G2. ESP32 family support.** Flash, reset, serial capture, serial send, and panic symbolization for ESP32, ESP32-S3, ESP32-C3, and ESP32-C6.
- **G3. Probe board.** A Raspberry Pi Pico running companion firmware for GPIO control, I2C access, and current measurement with an INA226.
- **G4. MCP parity.** Every safe CLI operation is also available as an MCP tool with identical results.
- **G5. Cross-platform.** Linux (x86_64 and aarch64, so it runs on a Raspberry Pi), macOS, and Windows. Single binary install.
- **G6. Safe by default.** Irreversible operations are impossible, destructive ones need explicit flags (§11).
- **G7. Pleasant for humans too.** Readable default output, colors, and error messages with actionable hints.

### Non-goals (v1)

- **eFuse burning, secure boot, flash encryption.** Never implemented. See §11.
- **Interactive TUI monitor.** Possible stretch goal; v1 is command-oriented.
- **Non-Espressif targets.** The design leaves room for them (a `Target` trait), but STM32 and nRF support via probe-rs comes later.
- **High-speed logic analysis.** Out of scope for v1.
- **Wireless/OTA flashing and networked shared benches.** Out of scope for v1.

---

## 3. Hardware setup

### 3.1 Minimum setup (Phases 0–3)

Only an ESP32-family dev board and a **data-capable** USB cable (many cheap cables are power-only).

Recommended: **ESP32-C3 or ESP32-C6**. They are RISC-V chips, so the Rust test firmware builds with the standard Rust toolchain. ESP32 and ESP32-S3 (Xtensa) are fully supported by `pogo` itself, but building the Rust test firmware for them requires the Espressif toolchain installed via `espup`.

### 3.2 Full bench (Phase 4+)

| Part | Purpose | Notes |
|---|---|---|
| ESP32-C3/S3 dev board | Target under test | Any Espressif dev board works |
| Raspberry Pi Pico (RP2040) or Pico 2 (RP2350) | Probe board | Runs `pogo` probe firmware |
| INA226 breakout | Current and voltage measurement | Common modules use a 0.1 Ω shunt, giving roughly 819 mA full scale |
| BME280 or MPU6050 breakout | Known I2C device for tests | Predictable scan and register-read results |
| Load switch or logic-level MOSFET module (optional) | Power-cycling the target | Rated for at least 1 A |
| Breadboard, jumper wires, 4.7 kΩ resistors | Wiring and I2C pull-ups | Many breakouts already include pull-ups |

### 3.3 Default probe wiring

| Pico pin | Function | Connects to |
|---|---|---|
| GP2 | `TARGET_EN` (open-drain, active low) | ESP32 EN/RST pin |
| GP3 | `TARGET_BOOT` (open-drain, active low) | GPIO0 (ESP32, ESP32-S3) or GPIO9 (ESP32-C3) |
| GP4 | I2C0 SDA | Sensor-under-test SDA |
| GP5 | I2C0 SCL | Sensor-under-test SCL |
| GP6 | I2C1 SDA | INA226 SDA |
| GP7 | I2C1 SCL | INA226 SCL |
| GP8 | `PWR_EN` (push-pull) | Load switch enable (optional) |
| 3V3(OUT) | 3.3 V supply | Sensor and INA226 VCC |
| GND | Ground | Target GND, sensor GND, INA226 GND |

```
                    ┌──────────── Raspberry Pi Pico (probe) ────────────┐
   Host USB ────────┤ USB                                               │
                    │ GP2  ── open-drain ─────────► Target EN/RST       │
                    │ GP3  ── open-drain ─────────► Target BOOT strap   │
                    │ GP4/5 (I2C0) ───────────────► Sensor under test   │
                    │ GP6/7 (I2C1) ───────────────► INA226              │
                    │ GP8  ───────────────────────► Load switch EN      │
                    │ GND  ───────────────────────► Common ground       │
                    └───────────────────────────────────────────────────┘

   Supply (+) ──► INA226 VIN+ ──[shunt]──► INA226 VIN- ──► Target 5V/VIN
```

### 3.4 Wiring notes and gotchas

- **Common ground is mandatory.** Probe, target, sensor, and INA226 must share GND.
- **EN and BOOT are driven open-drain only.** The probe either pulls them low or releases them (high-impedance). It never drives them high, because dev boards have their own pull-ups and auto-reset circuits.
- **I2C0 is for sensor bring-up.** Wire sensors directly to the probe to test them. Don't connect I2C0 to a bus the ESP32 is actively mastering, since two masters will conflict.
- **USB power bypasses the shunt.** To measure current, power the target from an external supply through the INA226 and use USB for data only (a USB power-blocker adapter isolates VBUS), or use a separate USB-UART adapter on TX/RX/GND. **Never connect an external supply and USB VBUS to the same rail at the same time**, because backfeeding can damage the board or the host's USB port.
- **Native USB boards re-enumerate.** ESP32-C3/S3/C6 boards using the built-in USB-Serial-JTAG may briefly drop off the bus on reset. `pogo` identifies devices by USB serial number and reconnects automatically (§6.7). The first ROM boot lines may be lost during re-enumeration.
- **Cheap CH340 adapters often lack a unique USB serial number.** `pogo` falls back to the port path and emits a warning.

### 3.5 OS setup

**Linux.** Add your user to the serial group (`dialout` on Debian/Ubuntu, `uucp` on Arch), then log out and back in. Building requires `libudev-dev` (Debian/Ubuntu) or the equivalent. ModemManager may grab `/dev/ttyACM*` devices briefly when they appear, so add a udev rule:

```
# /etc/udev/rules.d/99-pogo.rules
SUBSYSTEM=="tty", ATTRS{idVendor}=="303a", ENV{ID_MM_DEVICE_IGNORE}="1"
SUBSYSTEM=="tty", ATTRS{idVendor}=="2e8a", ENV{ID_MM_DEVICE_IGNORE}="1"
```

Then run `sudo udevadm control --reload-rules && sudo udevadm trigger`.

**macOS.** Use `/dev/cu.*` ports, not `/dev/tty.*` (the `tty` variants block waiting for carrier detect). `pogo` picks `cu` ports automatically.

**Windows.** Install CP210x or CH340 drivers if your board uses those bridges. Ports appear as `COMn`.

---

## 4. Architecture

### 4.1 Overview

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

### 4.2 Workspace layout

```
pogo-pin/
├── Cargo.toml                    # workspace root; excludes firmware/*
├── README.md                     # this spec
├── CLAUDE.md                     # agent working rules (see §14.3)
├── pogo.example.toml
├── crates/
│   ├── pogo-core/            # library: all logic, no printing, no process::exit
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs          # BenchError, ErrorCode, exit codes, hints
│   │       ├── deadline.rs
│   │       ├── config.rs
│   │       ├── discovery.rs      # USB enumeration and device classification
│   │       ├── lock.rs           # per-device lock files
│   │       ├── transport/        # Transport trait; serial, mock, replay impls
│   │       ├── serial/           # line splitter, ANSI strip, parsers, capture engine, panic detection
│   │       ├── flash.rs          # Flasher trait + espflash implementation
│   │       ├── reset.rs
│   │       ├── symbolize.rs      # ELF + addr2line
│   │       ├── probe/            # probe client over pogo-protocol
│   │       ├── power.rs          # INA226 conversion, stats
│   │       ├── guardrails.rs
│   │       └── audit.rs
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
└── docs/
    ├── agent-contract.md
    ├── wiring.md
    └── decisions/                # short architecture decision records (ADRs)
```

The firmware projects must be excluded from the root workspace (`exclude = ["firmware/*"]`) because they build for different targets.

### 4.3 Design rules

1. **The core never prints and never exits.** It returns typed results or errors. Only `pogo-cli` writes to stdout/stderr and sets exit codes.
2. **The core is synchronous.** Blocking I/O with explicit deadlines is easier to reason about for serial work. `pogo-mcp` calls the core through `tokio::task::spawn_blocking`.
3. **Every operation takes a deadline.** No core function can block forever.
4. **Device identity is the USB serial number,** not the port path, which changes across reboots and re-enumerations.
5. **All I/O goes through a `Transport` trait** so everything is testable without hardware.
6. **Outcomes are data, not errors.** A capture that times out or detects a panic returns `Ok(CaptureResult { outcome: Timeout | Fault, .. })` with its partial data. Rust errors are reserved for when an operation couldn't run at all (device missing, I/O failure). The CLI maps outcomes to error codes and exit codes.
7. **All result types derive `Serialize` and `JsonSchema`,** so JSON output and schemas come from the same source.

### 4.4 Core types (sketch)

```rust
// transport/mod.rs
pub trait Transport: Send {
    /// Read available bytes, waiting at most `timeout`. Ok(0) means no data yet.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, BenchError>;
    fn write_all(&mut self, data: &[u8]) -> Result<(), BenchError>;
    fn set_dtr(&mut self, asserted: bool) -> Result<(), BenchError>;
    fn set_rts(&mut self, asserted: bool) -> Result<(), BenchError>;
    fn set_baud(&mut self, baud: u32) -> Result<(), BenchError>;
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
pub enum BenchError {
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
impl BenchError {
    pub fn code(&self) -> ErrorCode;       // stable SCREAMING_SNAKE_CASE string
    pub fn exit_code(&self) -> u8;         // see §6.4
    pub fn hint(&self) -> Option<String>;  // actionable next step
}

// serial/capture.rs
pub fn capture(
    t: &mut dyn Transport,
    opts: &CaptureOptions,
    deadline: Deadline,
) -> Result<CaptureResult, BenchError>;

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

---

## 5. Tech stack

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

---

## 6. The agent-friendly CLI contract

This section is the heart of the project. Treat it as a stable public API.

### 6.1 Stream discipline

- **stdout carries results only.** One JSON document in `json` mode, one JSON object per line in `jsonl` mode, or readable text in `human` mode.
- **stderr carries everything else:** logs, progress, spinners, warnings in human mode.
- Progress indicators are disabled automatically when stderr is not a TTY or the format is not `human`.

### 6.2 Output formats

`--format human|json|jsonl` (short `-f`), or set `POGO_FORMAT=json` once for a whole session. Default is `human`.

- `json`: a single envelope (§6.3) printed when the command finishes.
- `jsonl`: streaming commands (`serial capture`, `run`, `power measure`) emit events as they happen, such as `{"type":"line", ...}`, and always finish with `{"type":"result", ...envelope}`. Non-streaming commands emit just the result line.
- Colors appear only in `human` mode and are disabled by `--no-color`, the `NO_COLOR` environment variable, or a non-TTY stdout.

### 6.3 The JSON envelope

Every command in `json` mode prints exactly this shape:

| Field | Type | Meaning |
|---|---|---|
| `schema_version` | integer | Envelope/schema version (starts at 1) |
| `pogo_version` | string | Tool version |
| `command` | string | Dotted command name, e.g. `serial.capture` |
| `ok` | bool | `true` only when the exit code is 0 |
| `duration_ms` | integer | Wall time of the command |
| `data` | object or null | Command-specific result; **may be present even when `ok` is false** (partial results) |
| `error` | object or null | `{ code, message, hint, details }` |
| `warnings` | array | `{ code, message }` items that didn't cause failure |

Error example:

```json
{
  "schema_version": 1,
  "pogo_version": "0.1.0",
  "command": "flash",
  "ok": false,
  "duration_ms": 38,
  "data": null,
  "error": {
    "code": "AMBIGUOUS_DEVICE",
    "message": "2 devices matched; pass --device to choose one",
    "hint": "Run `pogo devices` and pass --device <alias|usb_serial|port>.",
    "details": {
      "candidates": [
        { "port": "/dev/ttyACM0", "usb_serial": "F4:12:FA:9C:3A:10", "kind": "esp32_target" },
        { "port": "/dev/ttyUSB0", "usb_serial": null, "kind": "esp32_target" }
      ]
    }
  },
  "warnings": []
}
```

### 6.4 Exit codes

| Code | Name | Meaning |
|---|---|---|
| 0 | `OK` | Success; for capture/run, an `--until` pattern matched (or capture completed with no `--until`) |
| 1 | `INTERNAL` | Unexpected internal error or `NOT_IMPLEMENTED` |
| 2 | `USAGE` | Invalid arguments (clap's default) |
| 3 | `DEVICE_NOT_FOUND` / `AMBIGUOUS_DEVICE` | Device selection failed |
| 4 | `TIMEOUT` | Deadline reached before the expected condition |
| 5 | `PERMISSION_DENIED` | OS refused access to the port |
| 6 | `DEVICE_IO` / `PROTOCOL` / `PROTOCOL_MISMATCH` | Communication failed, device vanished, probe sent a bad or late frame, or probe firmware incompatible |
| 7 | `TARGET_FAULT` | Panic, fail-on pattern, unexpected reboot, brownout |
| 8 | `GUARDRAIL_REFUSED` | A safety guardrail blocked the operation |
| 9 | `DEVICE_BUSY` | Another process holds the device lock |
| 10 | `CONFIG_INVALID` | Configuration file problem |
| 130 | `INTERRUPTED` | SIGINT/Ctrl-C; partial results are still printed |

Exit codes are enforced by tests. Changing one is a breaking change.

### 6.5 Termination and bounded output

Every command has a deadline. `--timeout` overrides the default.

| Command | Default timeout |
|---|---|
| `devices`, `doctor` | 3s (10s with `doctor --deep`) |
| `flash` | 120s |
| `reset` | 5s |
| `serial capture` | 10s |
| `serial send` | 5s |
| `run` | flash timeout + capture timeout |
| `i2c`, `gpio`, `probe` | 2s |
| `power measure` | `--duration` + 2s |

Output is bounded so agents don't flood their context:

- `--max-lines` (default 500). When exceeded, keep the first 20% and last 80% of lines and report `omitted_lines`. Boot output and the end of a crash are both preserved.
- `--max-line-len` (default 1000 characters); longer lines are cut and marked `"truncated": true`.
- A soft cap of about 200 KB per envelope, with a `OUTPUT_TRUNCATED` warning when hit.

On Ctrl-C or SIGTERM, `pogo` stops cleanly, releases locks and probe pins it was pulsing, prints the partial envelope, and exits with 130.

### 6.6 No interactivity

- Never prompt. Never wait for a keypress.
- Never read stdin except where documented (`serial send -`).
- Confirmations are flags (`--erase-all --confirm <usb_serial>`), never y/n questions.

### 6.7 Device selection

A selector can be a config alias (`target`), a USB serial number, a port path, or `auto`.

- `auto` succeeds only when exactly one device of the required kind is connected. Otherwise it fails with `AMBIGUOUS_DEVICE` and lists the candidates. **Never guess.**
- After a reset, the device is re-found by USB serial number, waiting up to `--reconnect-timeout` (default 5s).
- Every device command echoes the resolved device in `data.device`, so the agent can confirm it touched the board it meant to.

### 6.8 Concurrency

Each device has a lock file in the runtime directory, keyed by USB serial number (or port path as a fallback). Commands hold the lock for their whole duration. If it's taken, the command fails immediately with `DEVICE_BUSY` (including the holder's PID and command), unless `--wait-lock <duration>` is passed.

### 6.9 Units and naming

- JSON keys are `snake_case`. Units go in key suffixes: `_ms`, `_ma`, `_v`, `_mj`, `_hz`.
- I2C addresses and memory addresses are hex strings (`"0x76"`); where useful a decimal field is also included.
- Duration inputs accept `humantime` formats: `500ms`, `10s`, `2m`.

### 6.10 Stability

- `schema_version` covers the envelope and all `data` shapes. Within a version, changes are additive only. Renames or removals bump the version.
- `pogo schema <command>` prints the JSON Schema for that command's envelope. Generated schemas are committed to `schemas/` and CI fails if they drift from the code.

### 6.11 Help text

Every command's `--help` includes a one-line summary, at least two examples, the relevant exit codes, and a sentence describing the JSON `data` shape. Agents learn tools by reading help, so treat help text as documentation.

---

## 7. Command reference

### Global flags

```
-d, --device <SELECTOR>        Target device: alias, USB serial, port, or "auto"
    --probe <SELECTOR>         Probe device [default: "probe" alias, else auto]
-f, --format <FORMAT>          human | json | jsonl  [env: POGO_FORMAT]
    --config <PATH>            Config file  [env: POGO_CONFIG]
    --timeout <DURATION>       Overall deadline for this command
    --wait-lock <DURATION>     Wait for a busy device instead of failing
    --dry-run                  Describe what a mutating command would do; change nothing
-v, --verbose                  More logs on stderr (repeatable)
-q, --quiet                    Errors only on stderr
    --no-color
```

### 7.1 `pogo devices`

```
pogo devices [--all]
```

Lists connected serial devices and classifies each as `esp32_target`, `pogo_probe`, or `unknown_serial` (shown only with `--all`). Never opens ports, so it is fast and always safe.

Classification uses USB IDs:

| VID:PID | Meaning |
|---|---|
| `303a:*` | Espressif native USB (ESP32-S2/S3/C3/C6/H2) |
| `10c4:ea60` | Silicon Labs CP210x bridge (possible ESP32 board) |
| `1a86:7523`, `1a86:55d4` | WCH CH340 / CH9102 bridge (possible ESP32 board) |
| `0403:6001`, `0403:6015` | FTDI bridge (possible ESP32 board) |
| `2e8a:*` + product string `pogo-probe` | pogo probe |

```json
"data": {
  "devices": [
    {
      "alias": "target",
      "kind": "esp32_target",
      "port": "/dev/ttyACM0",
      "usb": {
        "vid": "0x303a", "pid": "0x1001",
        "serial_number": "F4:12:FA:9C:3A:10",
        "manufacturer": "Espressif",
        "product": "USB JTAG/serial debug unit"
      },
      "chip_hint": "Espressif native USB (C3/S3/C6)",
      "locked_by": null
    }
  ]
}
```

### 7.2 `pogo doctor`

```
pogo doctor [--deep]
```

Runs environment checks and reports each as `pass`, `warn`, or `fail` with a hint: config parses, devices visible, port permissions (Linux group membership), ModemManager interference, probe present and firmware compatible. `--deep` also opens ports and performs the probe handshake.

```json
"data": {
  "checks": [
    { "id": "config", "status": "pass", "message": "Loaded ./pogo.toml" },
    { "id": "permissions", "status": "fail", "message": "User not in 'dialout' group",
      "hint": "sudo usermod -aG dialout $USER, then log out and back in" }
  ]
}
```

### 7.3 `pogo info`

```
pogo info
```

Connects to the ROM bootloader and reports chip, revision, features, MAC address, and flash size. **Note:** entering the bootloader resets the target; this is documented in its help text.

### 7.4 `pogo flash`

```
pogo flash <IMAGE> [--address <ADDR>] [--chip <CHIP>] [--flash-baud <BAUD>]
               [--bootloader <BIN>] [--partition-table <CSV|BIN>]
               [--no-verify] [--no-reset]
               [--erase-all --confirm <USB_SERIAL>]
               [--allow-bootloader-write]
```

- `<IMAGE>` may be an **ELF** (preferred; converted to an app image and flashed with bootloader and partition table, following espflash defaults) or a raw **`.bin`**, which requires `--address`.
- Raw writes below the app region (bootloader and partition table area) are refused unless `--allow-bootloader-write` is passed.
- Verifies by default. Resets into run mode afterwards unless `--no-reset`.
- `--erase-all` is guarded (§11).

```json
"data": {
  "device": { "alias": "target", "usb_serial": "F4:12:FA:9C:3A:10" },
  "chip": "esp32c3",
  "flash_size_bytes": 4194304,
  "regions": [
    { "address": "0x0", "bytes": 20160, "kind": "bootloader" },
    { "address": "0x8000", "bytes": 3072, "kind": "partition_table" },
    { "address": "0x10000", "bytes": 214016, "kind": "app" }
  ],
  "verified": true,
  "reset_after": true,
  "duration_ms": 4870
}
```

### 7.5 `pogo reset`

```
pogo reset [--mode run|bootloader] [--via auto|dtr-rts|probe]
```

`auto` uses the probe when `target_en`/`target_boot` pins are configured and the probe is connected, otherwise DTR/RTS toggling. Waits for re-enumeration on native USB boards.

### 7.6 `pogo serial capture`

```
pogo serial capture [--baud <N>] [--reset]
                        [--until <REGEX>]... [--fail-on <REGEX>]...
                        [--no-default-fail-patterns] [--settle <DURATION>]
                        [--max-lines <N>] [--max-line-len <N>]
                        [--parser auto|esp-idf|rust-log|raw]
                        [--elf <PATH>] [--raw] [--record <FILE>]
```

**Behavior:**

1. Resolve and lock the device. Open the port with DTR and RTS set explicitly, because opening a port with default line states can reset some boards unexpectedly.
2. With `--reset`, reset into run mode and capture from the first boot byte. Without it, capture starts whenever the port opens.
3. Split bytes into lines (handling `\n`, `\r\n`, lone `\r`, and a partial final line at the deadline). Strip ANSI codes unless `--raw`. Parse level, tag, and device timestamp with the selected parser.
4. Check each line against fail-on patterns first, then `--until` patterns.
5. On a panic marker, keep reading up to 2s (or until the panic block ends) to collect the full backtrace, then symbolize it with `--elf` (or the config's `elf`).
6. Count boots by `rst:0x..` lines and extract reset reasons. With `--reset`, one boot is expected; any further boot is an `unexpected_reboot` fault (including brownout or watchdog reasons).
7. After an `--until` match, keep reading for `--settle` (default 0) to catch crashes right after a "ready" message. A fault during settle turns the outcome into `fault`.

**Outcomes and exit codes:**

| Outcome | When | Exit |
|---|---|---|
| `matched` | An `--until` pattern matched (and settle passed cleanly) | 0 |
| `completed` | No `--until` given and the deadline was reached | 0 |
| `fault` | Fail-on match, panic, unexpected reboot | 7 |
| `timeout` | `--until` given but never matched before the deadline | 4 |
| `eof` | Device disappeared and didn't come back | 6 |

**Default fail-on patterns** (disable with `--no-default-fail-patterns`, extend in config):

```
Guru Meditation Error
abort\(\) was called
=+ PANIC =+
panicked at
Brownout detector was triggered
stack overflow
Task watchdog got triggered
```

**Parsers:**

- `esp-idf`: lines like `I (412) wifi: connected` → level, device timestamp, tag, message.
- `rust-log`: lines like `INFO - wifi connected` (esp-println with `log`).
- `auto`: tries both per line; falls back to `raw`.

**Panic location:** for Rust panics, `file:line:column` is parsed directly from the `panicked at` message, so a location is available even without an ELF. ESP-IDF style `Backtrace: 0xPC:0xSP ...` pairs and esp-backtrace address lists are symbolized against the ELF when one is provided.

```json
"data": {
  "device": { "alias": "target", "usb_serial": "F4:12:FA:9C:3A:10" },
  "outcome": "matched",
  "matched": { "pattern": "READY", "line_index": 41, "t_ms": 812 },
  "line_count": 42,
  "omitted_lines": 0,
  "boot_count": 1,
  "resets": [ { "t_ms": 0, "code": "0x1", "reason": "POWERON" } ],
  "panic": null,
  "lines": [
    { "t_ms": 0, "level": null, "tag": null, "device_ts_ms": null, "text": "ESP-ROM:esp32c3-api1-20210207" },
    { "t_ms": 412, "level": "info", "tag": "wifi", "device_ts_ms": 412, "text": "I (412) wifi: connected" }
  ]
}
```

`--record <FILE>` saves the raw byte stream with timestamps as JSONL, which is useful for bug reports and for test fixtures (§12).

### 7.7 `pogo serial send`

```
pogo serial send <DATA | -> [--line-ending lf|crlf|none] [--hex]
                     [--expect <REGEX>] [--timeout <DURATION>]
```

Writes data (default line ending `lf`; `--hex` accepts `"de ad be ef"`; `-` reads from stdin). With `--expect`, it then runs the capture engine until the pattern matches, inheriting fail-on detection. Output includes `bytes_written` and the response capture.

### 7.8 `pogo run` (the main agent command)

```
pogo run <IMAGE> [flash flags] [capture flags] [--no-flash]
```

Locks the device once, flashes, resets into run mode, and captures as if `--reset` were given. `data` contains `device`, `flash`, and `capture`. The exit code comes from the capture outcome unless flashing failed. `--no-flash` skips flashing (reset and capture only), which is useful for rerunning tests.

### 7.9 `pogo i2c` (requires probe)

```
pogo i2c scan  [--bus 0] [--range 0x08-0x77]
pogo i2c read  <ADDR> [--reg <REG>] --len <N> [--bus 0]
pogo i2c write <ADDR> <BYTE>... [--bus 0]
```

Bus 0 is the user bus; bus 1 is reserved for the INA226. `read` with `--reg` writes the register address and then does a repeated-start read. Scan results include a `may_be` hint from a small built-in table (for example, `0x76` → BME280/BMP280, `0x68` → MPU6050/DS3231, `0x3c` → SSD1306, `0x50–0x57` → EEPROM).

```json
"data": {
  "bus": 0,
  "range": ["0x08", "0x77"],
  "found": [ { "addr": "0x76", "addr_dec": 118, "may_be": ["BME280", "BMP280"] } ]
}
```

```console
$ pogo i2c read 0x68 --reg 0x75 --len 1 -f json   # MPU6050 WHO_AM_I
```

```json
"data": { "bus": 0, "addr": "0x68", "reg": "0x75", "bytes_hex": ["0x68"], "bytes_dec": [104] }
```

### 7.10 `pogo gpio` (requires probe)

```
pogo gpio get   <PIN>
pogo gpio set   <PIN> <low|high|release>
pogo gpio pulse <PIN> [--level low|high] [--duration 100ms]
```

`<PIN>` is a name declared in the config's `[pins]` table. Open-drain pins accept only `low` and `release`. Pulses are timed on the probe, not the host, so they're precise. Pin states persist after the command exits, until changed, released, or the probe loses USB.

### 7.11 `pogo power` (requires probe)

```
pogo power measure [--duration 5s] [--interval 1ms] [--out samples.csv]
pogo power on | off
pogo power cycle [--off-time 500ms]
```

The probe streams raw INA226 samples; the host converts and computes statistics.

```json
"data": {
  "duration_ms": 10000,
  "samples": 9874,
  "dropped": 0,
  "current_ma": { "avg": 42.7, "min": 11.2, "max": 312.4, "p50": 31.0, "p95": 118.0 },
  "bus_voltage_v": { "avg": 3.29, "min": 3.21, "max": 3.31 },
  "energy_mj": 1404.8,
  "csv_path": null
}
```

### 7.12 `pogo probe`

```
pogo probe info       # firmware version, protocol version, board, uptime, driven pins
pogo probe selftest   # INA226 present, pins released correctly, I2C buses idle
```

### 7.13 Utility commands

```
pogo config show | path | validate   # merged config with source of each value
pogo schema <COMMAND> | --all [--out schemas/]
pogo completions <bash|zsh|fish|powershell>
pogo mcp                              # start the MCP server on stdio (§9)
```

---

## 8. Probe firmware and wire protocol

### 8.1 Responsibilities

The probe firmware is deliberately simple. It exposes a USB CDC-ACM serial device, executes one request at a time, enforces safe pin states, and streams raw measurements. All interpretation (statistics, unit conversion, guardrail policy) happens on the host.

### 8.2 Framing

- Each frame is a postcard-serialized `Frame<T>`, COBS-encoded and terminated by `0x00`.
- Maximum encoded frame size: 512 bytes.
- The host sends one request at a time; the probe echoes the request's `seq` in its response(s).
- The host applies a per-request timeout (default 500 ms) and reports `PROTOCOL` errors on timeout or decode failure.

### 8.3 Messages (`pogo-protocol`, `no_std`)

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Frame<T> { pub seq: u16, pub body: T }

pub const MAGIC: u32 = 0x4245_4E43; // "BENC"

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Request {
    Hello { protocol_major: u8, protocol_minor: u8 },
    GetInfo,
    GpioConfigure { pin: u8, mode: PinMode },        // OpenDrain | PushPull | Input
    GpioSet { pin: u8, level: Level },                // Low | High | Release
    GpioGet { pin: u8 },
    GpioPulse { pin: u8, level: Level, duration_us: u32 },
    I2cScan { bus: u8, first: u8, last: u8 },
    I2cRead { bus: u8, addr: u8, reg: Option<u8>, len: u8 },
    I2cWrite { bus: u8, addr: u8, data: heapless::Vec<u8, 64> },
    PowerStart { interval_us: u32, duration_ms: u32 },
    PowerStop,
    PowerSwitch { on: bool },
    ReleaseAll,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Response {
    Hello { magic: u32, protocol_major: u8, protocol_minor: u8, fw_version: heapless::String<16> },
    Info { board: Board, uptime_ms: u64, driven_pins: u32 },
    Ok,
    Level(Level),
    I2cFound(heapless::Vec<u8, 112>),
    Bytes(heapless::Vec<u8, 64>),
    PowerChunk { index: u32, samples: heapless::Vec<PowerSample, 48> },
    PowerDone { total: u32, dropped: u32 },
    Err(ProbeError),
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct PowerSample { pub shunt_raw: i16, pub bus_raw: u16 }
```

`pogo-protocol` must build for both the host and `thumbv6m-none-eabi` / `thumbv8m.main-none-eabihf`. Enable `heapless`'s `serde` feature.

### 8.4 Handshake and versioning

The host's first message on every connection is `Hello`. The probe answers with `MAGIC`, its protocol version, and firmware version. A major version mismatch fails with `PROTOCOL_MISMATCH` (exit 6) and a hint explaining how to update the firmware. Minor versions add messages without breaking older hosts.

### 8.5 Safety behavior (firmware)

- All target-facing pins start **high-Z** at boot.
- **Pin allow-list in firmware:** only GP2, GP3, GP8, and GP9–GP15 can be driven. Requests for any other pin return `Err(PinNotAllowed)`, so a buggy host can't drive I2C, USB, or flash pins.
- On USB disconnect or suspend, the firmware performs `ReleaseAll`.
- A hardware watchdog resets the probe if the main loop stalls; `panic-reset` does the same on panic. Both return pins to high-Z.
- `PWR_EN` defaults to **on** at probe boot so the target behaves like a normal bench setup. This is documented in `docs/wiring.md`.

### 8.6 Firmware structure

Embassy tasks: `usb_task` (CDC-ACM), `protocol_task` (decode, dispatch, encode), `power_task` (INA226 sampling into a channel), and a watchdog feeder. USB product string: `pogo-probe`. Use a pid.codes test PID during development and apply for a free one before publishing.

### 8.7 Building and flashing

Build with `cargo build --release` inside `firmware/probe/`, convert to `.uf2` with `picotool` (handles both RP2040 and RP2350), then hold BOOTSEL while plugging in the Pico and copy the `.uf2` onto the drive that appears. Release builds attach the `.uf2` files to GitHub releases.

---

## 9. MCP server

`pogo mcp` starts an MCP server on stdio using `rmcp`. It is a thin adapter: each tool calls the same core function as the CLI command (via `spawn_blocking`) and returns the same `data` object as JSON.

| MCP tool | CLI equivalent | Mutates hardware? |
|---|---|---|
| `list_devices` | `devices` | No |
| `doctor` | `doctor` | No |
| `device_info` | `info` | Resets target |
| `flash` | `flash` (no `--erase-all`) | Yes |
| `reset` | `reset` | Yes |
| `serial_capture` | `serial capture` | Only with `reset: true` |
| `serial_send` | `serial send` | Yes |
| `run` | `run` | Yes |
| `i2c_scan` / `i2c_read` | `i2c scan` / `i2c read` | No |
| `i2c_write` | `i2c write` | Yes |
| `gpio_get` | `gpio get` | No |
| `gpio_set` / `gpio_pulse` | `gpio set` / `gpio pulse` | Yes |
| `power_measure` | `power measure` | No |
| `power_switch` | `power on/off/cycle` | Yes |

**Rules:**

- Guardrails are enforced in the core, so the MCP layer cannot bypass them. `--erase-all` is not exposed via MCP at all.
- Tool descriptions are the agent's only documentation. Each must state units, default timeouts, and whether the tool changes hardware state. Set MCP tool annotations (such as read-only or destructive hints) where the SDK supports them.
- Every tool takes an optional `timeout` and `device` argument with the same semantics as the CLI.
- Errors return the same `{ code, message, hint }` object as the CLI.

**Using it with Claude Code:** the CLI already works through Claude Code's shell from day one. To add the MCP server too, register `pogo mcp` with `claude mcp add` or a project-level `.mcp.json` file; check the Claude Code MCP documentation for the current syntax.

---

## 10. Configuration

**Discovery order** (later wins on conflicts): user config (`~/.config/pogo/config.toml` or the platform equivalent) → nearest `pogo.toml` walking up from the current directory → `POGO_CONFIG` → `--config`. `pogo config show` displays the merged result and where each value came from.

```toml
# pogo.toml
schema = 1

[defaults]
format = "human"            # human | json | jsonl
baud = 115200
timeout = "10s"

[devices.target]
kind = "esp32"
match = { usb_serial = "F4:12:FA:9C:3A:10" }   # or { port = "/dev/ttyACM0" } or { vid = 0x303a }
chip = "esp32c3"            # optional; auto-detected when flashing
elf = "firmware/test-target/target/riscv32imc-unknown-none-elf/release/test-target"
reset = "auto"              # auto | dtr-rts | probe
parser = "auto"             # auto | esp-idf | rust-log | raw

[devices.probe]
kind = "pogo-probe"
match = { usb_serial = "E6614103E7452D2F" }

[pins.target_en]
probe_pin = 2
mode = "open-drain"
active = "low"
description = "ESP32 EN/RST"

[pins.target_boot]
probe_pin = 3
mode = "open-drain"
active = "low"
description = "ESP32-C3 GPIO9 boot strap"

[power]
ina226_addr = 0x40
shunt_ohms = 0.1
switch_pin = 8

[serial]
extra_fail_on = ["E \\(\\d+\\) app: FATAL"]

[guardrails]
i2c_protected_addrs = ["0x50-0x57"]
min_power_off_time = "200ms"
max_power_cycles_per_minute = 10
```

---

## 11. Safety guardrails

| Operation | Risk | Guardrail |
|---|---|---|
| eFuse burning, secure boot, flash encryption | Permanently locks or bricks the chip | **Not implemented, ever.** Requests return `GUARDRAIL_REFUSED` with an explanation |
| Full chip erase | Wipes NVS (Wi-Fi credentials, calibration, keys) | Requires `--erase-all` **and** `--confirm <usb_serial>` matching the resolved device; not exposed via MCP |
| Flashing the wrong board | Overwrites another device | `auto` refuses when more than one candidate exists; output echoes the device's USB serial |
| Raw writes to bootloader/partition table area | Unbootable device | Refused unless `--allow-bootloader-write` |
| GPIO on undeclared pins | Short circuits, damaged pins | Only pins declared in config; firmware allow-list as a second layer |
| I2C writes to protected addresses | Corrupted EEPROM contents | `i2c_protected_addrs` (default `0x50–0x57`) |
| Rapid power cycling | Brownouts, flash corruption | Minimum off-time and cycles-per-minute limits |
| Concurrent access | Corrupted flashing or interleaved output | Per-device lock files (§6.8) |

**`--dry-run`:** every mutating command supports it and returns the planned actions in `data.plan` without touching hardware.

**Audit log:** every mutating action is appended as JSONL to the platform state directory (for example `~/.local/state/pogo/audit.jsonl`) with timestamp, command, device serial, arguments, and result. This gives a human a clear record of what an agent did to their hardware.

---

## 12. Testing strategy

| Layer | What it covers | Tooling |
|---|---|---|
| Unit | Log parsers, panic block parser, regex matching, durations, config merge, INA226 conversion | `cargo test` |
| Mock transport | Capture engine with scripted byte streams: partial lines, CRLF, lone CR, ANSI codes, garbage bytes, mid-line deadline | `MockTransport` |
| Record/replay | Real sessions recorded with `--record`, replayed in CI | `tests/fixtures/*.jsonl`, `ReplayTransport` |
| Snapshot | JSON outputs, human output, `--help` texts, schemas | `insta` |
| CLI integration | Exit codes, stdout/stderr separation, format flags | `assert_cmd`, `predicates` |
| Property | COBS/postcard round-trips, line splitter under arbitrary chunking | `proptest` |
| Hardware-in-the-loop | Real flash, capture, reset, probe commands | `tests/hil/`, opt-in |

**Fixture coverage required by Phase 1:** clean boot with `READY`, timeout with no output, Rust panic with backtrace, ESP-IDF Guru Meditation with `Backtrace:` line, boot loop, brownout, device disconnect mid-capture.

**Test-target firmware** (`firmware/test-target/`, esp-hal) prints `READY` after init and accepts line commands over serial:

| Command | Behavior |
|---|---|
| `ping` | Replies `pong` |
| `panic` | Triggers an `unwrap()` on `None` at a known source line |
| `reboot` | Software reset |
| `busy <ms>` | Busy-loops (power tests) |
| `sleep <ms>` | Light sleep (power tests) |
| `gpio <n> <0\|1>` | Sets a target GPIO |

```console
cargo test --workspace                       # everything that doesn't need hardware
cargo insta review                           # review snapshot changes
POGO_HIL=1 cargo test -p pogo-cli --test hil -- --test-threads=1
```

HIL tests are skipped unless `POGO_HIL=1` is set, and they always run single-threaded.

---

## 13. Roadmap

Each phase ends only when its acceptance criteria pass and a human confirms.

### Phase 0: Skeleton and contract

- [ ] Workspace with the four crates; root `exclude = ["firmware/*"]`; `[workspace.lints]` denying `clippy::unwrap_used` and `clippy::expect_used` in non-test code
- [ ] `BenchError`, `ErrorCode`, exit code mapping, hints
- [ ] Output module: envelope, `human`/`json`/`jsonl`, stream discipline, colors
- [ ] Config loading and merge; `config show|path|validate`
- [ ] clap definitions for **all** commands; unimplemented ones return `NOT_IMPLEMENTED`
- [ ] `devices` fully implemented; `schema` command; `completions`
- [ ] CI (§15)

**Acceptance:** `cargo test` passes on Linux, macOS, and Windows. `pogo devices -f json` prints a valid envelope matching its schema. Every exit code has a test. Help snapshots exist for every command.

### Phase 1: Serial

- [ ] `Transport` trait with serial, mock, and replay implementations
- [ ] Line splitter, ANSI stripping, `esp-idf` and `rust-log` parsers
- [ ] Capture engine: `--until`, `--fail-on`, defaults, `--settle`, `--max-lines`, boot counting, reset reasons, panic block detection
- [ ] `serial capture`, `serial send`, `--record`
- [ ] Lock files, `DEVICE_BUSY`, `--wait-lock`
- [ ] Reconnect by USB serial after re-enumeration
- [ ] Ctrl-C handling with partial results

**Acceptance:** all required fixtures (§12) replay with the correct outcome and exit code. On real hardware, `serial capture --reset --until "rst:0x"` exits 0. Unplugging the board mid-capture returns `DEVICE_IO` with a hint instead of hanging.

### Phase 2: Flash, reset, run, symbolization

- [ ] `Flasher` trait with espflash implementation; `info`; `flash` for ELF and `.bin`
- [ ] `reset` (DTR/RTS)
- [ ] `run` composite
- [ ] ELF symbolization; Rust panic location parsing
- [ ] Test-target firmware

**Acceptance:** `pogo run <test-target ELF> --until READY` exits 0. `serial send panic` (or `run` with a planted panic) exits 7 with `panic.location` pointing at the correct line. With two boards connected and no `--device`, `flash` exits 3 with candidates listed.

### Phase 3: MCP server

- [ ] `pogo-mcp` crate; `pogo mcp` subcommand; all tools in §9
- [ ] Verify with the MCP Inspector
- [ ] Document setup with Claude Code

**Acceptance:** in a Claude Code session, the agent completes "flash the test target, confirm READY, trigger a panic, and report the file and line" using only MCP tools. MCP results equal CLI `data` for the same operations.

### Phase 4: Probe firmware, GPIO, I2C

- [ ] `pogo-protocol` crate (no_std) with property tests
- [ ] Probe firmware: USB CDC, framing, handshake, pin allow-list, safe states, GPIO, I2C
- [ ] Host probe client; `probe info|selftest`, `gpio`, `i2c`
- [ ] `reset --via probe`
- [ ] `.uf2` build

**Acceptance:** `probe info` returns versions. `i2c scan` finds a BME280 at `0x76` or `0x77`. `reset --via probe` reboots the target and capture sees the boot. Unplugging the probe releases all pins within 100 ms (verified with an LED or multimeter).

### Phase 5: Power

- [ ] INA226 sampling and streaming on the probe; host statistics and CSV export
- [ ] `power on|off|cycle` with guardrails

**Acceptance:** idle current is within ±5% of a multimeter reading. Wi-Fi transmit peaks are visible in `max`/`p95`. A script doing `power cycle` then `serial capture --until READY` works end to end.

### Phase 6: Polish and release

- [ ] `docs/agent-contract.md`, `docs/wiring.md`, wiring photo
- [ ] Release automation, Homebrew tap, crates.io publish
- [ ] Demo recording (§16)

**Acceptance:** on a fresh machine, install via Homebrew or the shell installer, `pogo doctor` passes, and the demo is reproducible.

### Stretch goals

- Hardware-in-the-loop CI on a Raspberry Pi self-hosted GitHub Actions runner
- STM32 and nRF targets via probe-rs
- Arduino (AVR) boards and Lattice ECP5 FPGAs
- `pogo watch`: a ratatui TUI for humans
- Logic-analyzer mode using RP2040 PIO
- Multi-board farm support

---

## 14. Instructions for Claude Code

### 14.1 Kickoff prompt (for the human to paste)

> Read README.md in full. It's the specification for this project. Create CLAUDE.md from the template in §14.3, ask me the setup questions in §14.2, then start Phase 0. Work one phase at a time and stop for my confirmation when a phase's acceptance criteria pass.

### 14.2 Start here

1. Read this entire README. §6 (the CLI contract) and §11 (guardrails) are non-negotiable.
2. Create `CLAUDE.md` from §14.3.
3. Ask the human: Which target boards and OS are you using? Is a board connected right now? Do you have probe hardware yet?
4. Start Phase 0. Do not start a later phase until the current phase's acceptance criteria pass and the human confirms.
5. For each task: briefly plan, implement, write tests, run the checks in §14.3, and commit with a Conventional Commits message.

### 14.3 CLAUDE.md template

```markdown
# CLAUDE.md — working rules for pogo

## Project
pogo is an agent-friendly hardware workbench CLI and MCP server for ESP32 boards.
README.md is the spec. §6 (CLI contract) and §11 (guardrails) are non-negotiable.

## Current phase
Phase 0 (update this line as phases complete)

## Before every commit
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

## Architecture rules
- pogo-core never prints and never calls process::exit; it returns typed results.
- Outcomes (timeout, fault) are data in Ok(...); Rust errors mean "couldn't run".
- Every blocking operation takes a Deadline.
- All I/O goes through the Transport trait. Tests use MockTransport or ReplayTransport.
- No unwrap()/expect() outside tests.

## New command checklist
1. Core function returning a Serialize + JsonSchema result
2. clap definition with long help: summary, 2+ examples, exit codes, data shape
3. Snapshot tests for human and JSON output
4. Exit code tests
5. Regenerate schemas/ (pogo schema --all --out schemas/)
6. MCP tool (Phase 3 onward)
7. Update the command reference in README.md

## Dependencies
- Check crates.io/docs.rs for current versions and APIs. Do not rely on memory.
- Ask before adding any dependency not listed in README §5.

## Hardware safety
- Never run commands that touch real hardware (flash, reset, info, gpio, i2c write,
  power, serial send) unless the human has said in this session that hardware is
  connected and it's OK. `devices` and `doctor` (without --deep) are always fine.
- Never implement eFuse, secure boot, or flash encryption features, even if a task
  seems to ask for them. Flag it to the human instead.
- When hardware behavior is unclear, ask the human to run a specific command and
  paste the output. Don't guess timing or protocol details.

## Ask the human before
- Changing the JSON envelope, exit codes, or schema_version
- Changing default guardrails
- Moving to the next phase
```

### 14.4 Working rules (summary)

- Keep commits small and focused on one task.
- Prefer the mock and replay transports for development; real hardware is for acceptance checks.
- When a design decision isn't covered by this spec, write a short ADR in `docs/decisions/NNNN-title.md` (context, decision, consequences) and mention it to the human.
- If the spec seems wrong or contradictory, stop and ask rather than silently diverging.

---

## 15. CI, release, and distribution

**CI (`.github/workflows/ci.yml`):**

- Matrix: `ubuntu-latest`, `macos-latest`, `windows-latest`. Install `libudev-dev` on Linux.
- Jobs: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace`, `cargo deny check` (licenses and advisories).
- Schema drift: regenerate schemas and fail on `git diff --exit-code schemas/`.
- Firmware: build `firmware/probe` for `thumbv6m-none-eabi` (RP2040) and `thumbv8m.main-none-eabihf` (RP2350); build `firmware/test-target` for `riscv32imc-unknown-none-elf` (ESP32-C3).

**Release:**

- Use cargo-dist (`dist`) to build binaries for Linux x86_64 and aarch64, macOS arm64 and x86_64, and Windows x86_64, plus shell and PowerShell installers and a Homebrew formula in a tap repository.
- Publish crates to crates.io (`pogo-protocol`, `pogo-core`, `pogo-cli`, `pogo-mcp`).
- Attach probe `.uf2` files to each GitHub release.
- Semantic versioning for the tool; `schema_version` versions the JSON contract independently.
- Generate `CHANGELOG.md` from Conventional Commits with `git-cliff`.

---

## 16. Launch and showcase checklist

- [ ] **Hero demo** at the top of this README: a 60–90 second recording of Claude Code finding and fixing a planted firmware bug on a real board, using only `pogo`.
- [ ] Terminal recordings (asciinema) of `devices`, `run`, `i2c scan`, and `power measure`.
- [ ] Architecture diagram and a photo of the wired bench in `docs/`.
- [ ] Measured numbers: time from code edit to verified boot, capture overhead, test count and coverage.
- [ ] A write-up: "Designing CLIs for AI agents: lessons from pogo."
- [ ] Share with the ESP32 and Rust embedded communities, and collect feedback from real users.

---

## License

Licensed under either of Apache License 2.0 or MIT license, at your option.
