# 7. Command reference

## Global flags

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

## 7.1 `pogo devices`

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

`alias` is the first config device (by name) whose `match` fields all match; an unrecognized port with an alias is shown without `--all`. On macOS only the `/dev/cu.*` node is listed, not its `/dev/tty.*` twin. Results are sorted by port. `locked_by` stays `null` until device locks land (Phase 1).

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

## 7.2 `pogo doctor`

```
pogo doctor [--deep]
```

Runs environment checks and reports each as `pass`, `warn`, or `fail` with a hint: config parses, devices visible, port permissions (Linux group membership), ModemManager interference, probe present and firmware compatible. `--deep` also opens ports and performs the probe handshake.

Check ids: `config`, `devices`, `permissions` and `modemmanager` (Linux only; left out elsewhere), `probe`, and with `--deep` one `open` check per board (its port is locked and opened without a reset). Probe firmware compatibility needs the probe handshake, which arrives with the probe in Phase 4. Warnings (no board, no probe, ModemManager running) do not fail the command. Exit codes: 0 when nothing failed; `CHECK_FAILED` (exit 1) when a check failed; `CONFIG_INVALID` (exit 10) when the config does not parse, with `data.checks` still reported.

```json
"data": {
  "checks": [
    { "id": "config", "status": "pass", "message": "Loaded ./pogo.toml" },
    { "id": "permissions", "status": "fail", "message": "User not in 'dialout' group",
      "hint": "sudo usermod -aG dialout $USER, then log out and back in" }
  ]
}
```

## 7.3 `pogo info`

```
pogo info
```

Connects to the ROM bootloader and reports chip, revision, features, MAC address, and flash size. **Note:** entering the bootloader resets the target; this is documented in its help text. Afterwards the target is reset back into run mode.

```json
"data": {
  "device": { "alias": null, "port": "/dev/cu.usbserial-0001", "usb_serial": "0001" },
  "chip": "esp32", "revision": "v3.1", "crystal": "40 MHz",
  "features": ["WiFi", "BT", "Dual Core", "240MHz"],
  "mac": "d4:8a:fc:d0:d4:10", "flash_size_bytes": 4194304
}
```

## 7.4 `pogo flash`

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
- `--erase-all` is guarded ([§11](guardrails.md#11-safety-guardrails)): `--confirm` must equal the resolved device's USB serial, so a device without one cannot be erased.
- Guardrails are checked before anything touches the device; a refusal is `GUARDRAIL_REFUSED` (exit 8).
- `--dry-run` checks guardrails and returns `data.plan` (`image`, `bytes`, `address`, `erase_all`, `verify`, `reset_after`) instead of the flash report, without connecting.
- Flashing uses the espflash library with its bundled bootloader. The ESP32 bootloader lands at `0x1000`; newer chips use `0x0`.
- Every real flash is appended to the audit log ([§11](guardrails.md#11-safety-guardrails)); a failure to write it is an `AUDIT_FAILED` warning.

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

## 7.5 `pogo reset`

```
pogo reset [--mode run|bootloader] [--via auto|dtr-rts|probe]
```

`auto` uses the probe when `target_en`/`target_boot` pins are configured and the probe is connected, otherwise DTR/RTS toggling. Waits for re-enumeration on native USB boards.

Until the probe exists (Phase 4), `auto` means `dtr-rts` and `--via probe` returns `NOT_IMPLEMENTED`. The DTR/RTS sequences follow esptool's hard reset (run) and classic reset (bootloader); native USB-Serial-JTAG boards use esptool's USB-JTAG sequence. After a reset of a native USB board, pogo waits up to 1s for it to drop off USB and up to 5s for it to return; `reconnected` reports whether it did.

```json
"data": { "device": { ... }, "mode": "run", "via": "dtr-rts", "reconnected": false, "dry_run": false }
```

`--dry-run` returns the same shape with `"dry_run": true` and changes nothing. Resets are recorded in the audit log.

## 7.6 `pogo serial capture`

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

**Symbolization:** with `--elf` (or the config device's `elf`), each backtrace address is resolved to `function`, `file`, and `line` in `panic.frames`; inlined calls appear as extra frames marked `"inlined": true`. Both formats are read: ESP-IDF's single `Backtrace: 0xPC:0xSP ...` line, and esp-backtrace's bare `Backtrace:` followed by one address per line. If the ELF cannot be read, the capture still succeeds with a `SYMBOLIZE_FAILED` warning.

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

**Reconnect:** if the device disappears mid-capture (native USB boards re-enumerate on reset), it is re-found by USB serial number and reopened, and the capture continues. `--reconnect-timeout` (default `5s`, `0` disables) bounds the wait; after that the outcome is `eof`. Devices without a USB serial number are not reconnected.

**Ctrl-C / SIGTERM:** the capture stops, the device lock is released, and the partial envelope is printed with outcome `interrupted`, error `INTERRUPTED`, and exit 130. A second Ctrl-C exits immediately.

Additional fields: `device` also carries `port`; `fault` is `{ kind: fail_on|panic|unexpected_reboot, pattern, line_index, t_ms }` when the outcome is `fault` (otherwise `null`); a line cut at `--max-line-len` carries `"truncated": true`. `--reset` toggles DTR/RTS following esptool's hard-reset sequence (not yet verified on the bench boards). Without `--reset`, boots are counted but never treated as unexpected. In human mode lines stream to stdout as they arrive, followed by a one-line summary; in `jsonl` mode each line is a `{"type":"line", ...}` event.

`--record <FILE>` saves the raw byte stream with timestamps as JSONL, which is useful for bug reports and for test fixtures ([§12](testing.md#12-testing-strategy)).

## 7.7 `pogo serial send`

```
pogo serial send <DATA | -> [--line-ending lf|crlf|none] [--hex]
                     [--expect <REGEX>] [--timeout <DURATION>]
```

Writes data (default line ending `lf`; `--hex` accepts `"de ad be ef"`; `-` reads from stdin). With `--expect`, it then runs the capture engine until the pattern matches, inheriting fail-on detection. Output includes `bytes_written` and the response capture.

```json
"data": { "device": { "alias": null, "port": "/dev/cu.usbserial-0001", "usb_serial": "0001" },
          "bytes_written": 5, "response": { "outcome": "matched", ... } }
```

`response` has the same shape as `serial capture`'s data (without `device`) and is `null` without `--expect`. The line ending is appended to `--hex` and stdin data too; pass `--line-ending none` to avoid it. Default fail-on patterns and the config's `extra_fail_on` apply to the response.

## 7.8 `pogo run` (the main agent command)

```
pogo run <IMAGE> [flash flags] [capture flags] [--no-flash]
```

Locks the device once, flashes, resets into run mode, and captures as if `--reset` were given. `data` contains `device`, `flash`, and `capture`. The exit code comes from the capture outcome unless flashing failed. `--no-flash` skips flashing (reset and capture only), which is useful for rerunning tests.

After flashing, the target stays in the bootloader; the capture's own reset then starts the application, so the capture begins at the first boot byte. When `<IMAGE>` is an ELF it also symbolizes any panic backtrace, unless `--elf` or the config names another. With `--dry-run`, `data.plan` describes the flash and `capture` is `null`.

```json
"data": { "device": { ... }, "flash": { "chip": "esp32", "regions": [ ... ], ... },
          "plan": null, "capture": { "outcome": "matched", ... } }
```

## 7.9 `pogo i2c` (requires probe)

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

## 7.10 `pogo gpio` (requires probe)

```
pogo gpio get   <PIN>
pogo gpio set   <PIN> <low|high|release>
pogo gpio pulse <PIN> [--level low|high] [--duration 100ms]
```

`<PIN>` is a name declared in the config's `[pins]` table. Open-drain pins accept only `low` and `release`. Pulses are timed on the probe, not the host, so they're precise. Pin states persist after the command exits, until changed, released, or the probe loses USB.

## 7.11 `pogo power` (requires probe)

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

## 7.12 `pogo probe`

```
pogo probe info       # firmware version, protocol version, board, uptime, driven pins
pogo probe selftest   # INA226 present, pins released correctly, I2C buses idle
```

## 7.13 Utility commands

```
pogo config show | path | validate   # merged config with source of each value
pogo schema <COMMAND> | --all [--out schemas/]
pogo completions <bash|zsh|fish|powershell>
pogo mcp                              # start the MCP server on stdio (§9)
```

### `pogo schema`

Prints the JSON Schema (2020-12) for a command's envelope as a bare document, not wrapped in an envelope. `--all` prints an object keyed by command name; `--all --out DIR` writes `DIR/<command>.json` and replies with `"data": { "files": [...] }`. Only commands with a defined data shape have schemas (currently `devices`, `config.show`, `config.path`, `config.validate`); other known commands give `NOT_IMPLEMENTED` (exit 1) and unknown names give `USAGE` (exit 2). `cargo test` fails if `schemas/` is stale.

### `pogo config`

Files are discovered and merged as described in [§10](configuration.md). Tables merge key by key and later files win; any other value (including arrays and `match` tables) replaces the earlier one whole. User and project files are optional; a file given with `--config`/`POGO_CONFIG` must exist.

- `config path`: candidate files in merge order, whether or not they exist. Never fails.
  `"data": { "files": [ { "path": "/repo/pogo.toml", "kind": "user|project|explicit", "exists": true } ] }`
- `config show`: only the values that are set, plus the file each one came from.
  `"data": { "config": { "defaults": { "baud": 115200 } }, "sources": [ { "key": "defaults.baud", "source": "/repo/pogo.toml" } ] }`
- `config validate`: `"data": { "files": [ "/repo/pogo.toml" ], "valid": true }`

Unknown keys, wrong types, bad durations, invalid regexes in `serial.extra_fail_on`, malformed `i2c_protected_addrs`, an unsupported `schema`, and devices missing `kind` or `match` (or pins missing `probe_pin`) after merging all fail with `CONFIG_INVALID` (exit 10). The message names the file, and the line for per-file errors.

[Back to README](../README.md)
