# pogo-pin

Give AI coding agents eyes and hands on real hardware. Flash, monitor, and probe ESP32 boards from a CLI or MCP server, with commands that always terminate, return JSON, and use meaningful exit codes.

**An agent-friendly hardware workbench CLI (`pogo`).** Flash, reset, monitor, probe, and measure ESP32 boards from the terminal, with deterministic, machine-readable output built for AI coding agents (like Claude Code) and humans alike.

> **Status:** pre-alpha. This README and the files in [`docs/`](docs/) are the project specification. The project is `pogo-pin`; the binary is `pogo` and crates are `pogo-*`. Check crates.io and Homebrew for availability before the first public release.
>
> **Last updated:** 2026-10-26

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

- **G1. Deterministic agent contract.** Every command terminates, writes results only to stdout, offers JSON output, and returns documented exit codes ([§6](docs/agent-contract.md#6-the-agent-friendly-cli-contract)).
- **G2. ESP32 family support.** Flash, reset, serial capture, serial send, and panic symbolization for ESP32, ESP32-S3, ESP32-C3, and ESP32-C6.
- **G3. Probe board.** A Raspberry Pi Pico running companion firmware for GPIO control, I2C access, and current measurement with an INA226.
- **G4. MCP parity.** Every safe CLI operation is also available as an MCP tool with identical results.
- **G5. Cross-platform.** Linux (x86_64 and aarch64, so it runs on a Raspberry Pi), macOS, and Windows. Single binary install.
- **G6. Safe by default.** Irreversible operations are impossible, destructive ones need explicit flags ([§11](docs/guardrails.md#11-safety-guardrails)).
- **G7. Pleasant for humans too.** Readable default output, colors, and error messages with actionable hints.

### Non-goals (v1)

- **eFuse burning, secure boot, flash encryption.** Never implemented. See [§11](docs/guardrails.md#11-safety-guardrails).
- **Interactive TUI monitor.** Possible stretch goal; v1 is command-oriented.
- **Non-Espressif targets.** The design leaves room for them (a `Target` trait), but STM32 and nRF support via probe-rs comes later.
- **High-speed logic analysis.** Out of scope for v1.
- **Wireless/OTA flashing and networked shared benches.** Out of scope for v1.

---

## Documentation

The full specification lives in [`docs/`](docs/). Section numbers (§) are stable across files.

| § | Document |
|---|---|
| 3 | [Hardware setup](docs/hardware.md) |
| 4 | [Architecture](docs/architecture.md) |
| 5 | [Tech stack](docs/architecture.md) |
| 6 | [The agent-friendly CLI contract](docs/agent-contract.md) |
| 7 | [Command reference](docs/commands.md) |
| 8 | [Probe firmware and wire protocol](docs/probe-firmware.md) |
| 9 | [MCP server](docs/mcp.md) |
| 10 | [Configuration](docs/configuration.md) |
| 11 | [Safety guardrails](docs/guardrails.md) |
| 12 | [Testing strategy](docs/testing.md) |
| 13 | [Roadmap](docs/roadmap.md) |
| 14 | [Instructions for Claude Code](docs/claude-code.md) |
| 15 | [CI, release, and distribution](docs/release.md) |
| 16 | [Launch and showcase checklist](docs/launch.md) |

---

## License

Licensed under either of Apache License 2.0 or MIT license, at your option.
