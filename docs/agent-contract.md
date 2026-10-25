# 6. The agent-friendly CLI contract

This section is the heart of the project. Treat it as a stable public API.

## 6.1 Stream discipline

- **stdout carries results only.** One JSON document in `json` mode, one JSON object per line in `jsonl` mode, or readable text in `human` mode.
- **stderr carries everything else:** logs, progress, spinners, warnings in human mode.
- Progress indicators are disabled automatically when stderr is not a TTY or the format is not `human`.

## 6.2 Output formats

`--format human|json|jsonl` (short `-f`), or set `POGO_FORMAT=json` once for a whole session. Default is `human`.

- `json`: a single envelope ([§6.3](#63-the-json-envelope)) printed when the command finishes.
- `jsonl`: streaming commands (`serial capture`, `run`, `power measure`) emit events as they happen, such as `{"type":"line", ...}`, and always finish with `{"type":"result", ...envelope}`. Non-streaming commands emit just the result line.
- Colors appear only in `human` mode and are disabled by `--no-color`, the `NO_COLOR` environment variable, or a non-TTY stdout.

## 6.3 The JSON envelope

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

## 6.4 Exit codes

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

## 6.5 Termination and bounded output

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

## 6.6 No interactivity

- Never prompt. Never wait for a keypress.
- Never read stdin except where documented (`serial send -`).
- Confirmations are flags (`--erase-all --confirm <usb_serial>`), never y/n questions.

## 6.7 Device selection

A selector can be a config alias (`target`), a USB serial number, a port path, or `auto`.

- `auto` succeeds only when exactly one device of the required kind is connected. Otherwise it fails with `AMBIGUOUS_DEVICE` and lists the candidates. **Never guess.**
- After a reset, the device is re-found by USB serial number, waiting up to `--reconnect-timeout` (default 5s).
- Every device command echoes the resolved device in `data.device`, so the agent can confirm it touched the board it meant to.

## 6.8 Concurrency

Each device has a lock file in the runtime directory, keyed by USB serial number (or port path as a fallback). Commands hold the lock for their whole duration. If it's taken, the command fails immediately with `DEVICE_BUSY` (including the holder's PID and command), unless `--wait-lock <duration>` is passed.

## 6.9 Units and naming

- JSON keys are `snake_case`. Units go in key suffixes: `_ms`, `_ma`, `_v`, `_mj`, `_hz`.
- I2C addresses and memory addresses are hex strings (`"0x76"`); where useful a decimal field is also included.
- Duration inputs accept `humantime` formats: `500ms`, `10s`, `2m`.

## 6.10 Stability

- `schema_version` covers the envelope and all `data` shapes. Within a version, changes are additive only. Renames or removals bump the version.
- `pogo schema <command>` prints the JSON Schema for that command's envelope. Generated schemas are committed to `schemas/` and CI fails if they drift from the code.

## 6.11 Help text

Every command's `--help` includes a one-line summary, at least two examples, the relevant exit codes, and a sentence describing the JSON `data` shape. Agents learn tools by reading help, so treat help text as documentation.

[Back to README](../README.md)
