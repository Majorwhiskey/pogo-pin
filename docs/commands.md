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

Connects to the ROM bootloader and reports chip, revision, features, MAC address, and flash size. **Note:** entering the bootloader resets the target; this is documented in its help text.

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
- `--erase-all` is guarded ([§11](guardrails.md#11-safety-guardrails)).

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

`--record <FILE>` saves the raw byte stream with timestamps as JSONL, which is useful for bug reports and for test fixtures ([§12](testing.md#12-testing-strategy)).

## 7.7 `pogo serial send`

```
pogo serial send <DATA | -> [--line-ending lf|crlf|none] [--hex]
                     [--expect <REGEX>] [--timeout <DURATION>]
```

Writes data (default line ending `lf`; `--hex` accepts `"de ad be ef"`; `-` reads from stdin). With `--expect`, it then runs the capture engine until the pattern matches, inheriting fail-on detection. Output includes `bytes_written` and the response capture.

## 7.8 `pogo run` (the main agent command)

```
pogo run <IMAGE> [flash flags] [capture flags] [--no-flash]
```

Locks the device once, flashes, resets into run mode, and captures as if `--reset` were given. `data` contains `device`, `flash`, and `capture`. The exit code comes from the capture outcome unless flashing failed. `--no-flash` skips flashing (reset and capture only), which is useful for rerunning tests.

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

[Back to README](../README.md)
