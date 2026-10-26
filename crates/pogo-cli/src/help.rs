//! Long help text: examples, exit codes, and data shape for every command (docs/agent-contract.md §6.11).
//!
//! Agents learn the tool by reading `--help`, so keep these accurate.

pub const ROOT: &str = "\
Every command terminates, writes results only to stdout, and returns a documented exit code.
Use -f json (or POGO_FORMAT=json) for one JSON envelope:
  { schema_version, pogo_version, command, ok, duration_ms, data, error, warnings }

Examples:
  pogo devices -f json
  pogo run app.elf --until READY --timeout 20s -f json

Exit codes:
  0 ok   1 internal/not implemented   2 usage   3 device not found/ambiguous
  4 timeout   5 permission denied   6 device I/O or probe protocol   7 target fault
  8 guardrail refused   9 device busy   10 config invalid   130 interrupted";

pub const DEVICES: &str = "\
Never opens ports, so it is fast and always safe.

Examples:
  pogo devices
  pogo devices --all -f json

Exit codes: 0 ok, 10 config invalid

JSON data: { devices: [ { alias, kind, port, usb: { vid, pid, serial_number, manufacturer,
product }, chip_hint, locked_by } ] }. kind is esp32_target, pogo_probe, or unknown_serial
(the last only with --all).";

pub const DOCTOR: &str = "\
Checks that the config parses, devices are visible, ports are accessible, and the probe
firmware is compatible.

Examples:
  pogo doctor
  pogo doctor --deep -f json

Exit codes: 0 all checks passed or warned, 1 a check failed, 10 config invalid

JSON data: { checks: [ { id, status: pass|warn|fail, message, hint } ] }";

pub const INFO: &str = "\
Entering the ROM bootloader RESETS the target.

Examples:
  pogo info
  pogo info -d target -f json

Exit codes: 0 ok, 3 device not found/ambiguous, 4 timeout, 5 permission denied,
6 device I/O, 9 device busy

JSON data: { device, chip, revision, features, mac, flash_size_bytes }";

pub const FLASH: &str = "\
An ELF is converted to an app image and flashed with bootloader and partition table.
A raw .bin requires --address. Writes below the app region need --allow-bootloader-write.
Verifies by default and resets into run mode unless --no-reset.

Examples:
  pogo flash target/riscv32imc-unknown-none-elf/release/app
  pogo flash app.bin --address 0x10000 -d target -f json
  pogo flash app.elf --erase-all --confirm F4:12:FA:9C:3A:10

Exit codes: 0 ok, 3 device not found/ambiguous, 4 timeout, 5 permission denied,
6 device I/O, 8 guardrail refused, 9 device busy

JSON data: { device, chip, flash_size_bytes, regions: [ { address, bytes, kind } ],
verified, reset_after, duration_ms }";

pub const RESET: &str = "\
--via auto uses the probe when target_en/target_boot pins are configured, otherwise DTR/RTS.
Waits for native USB boards to re-enumerate.

Examples:
  pogo reset
  pogo reset --mode bootloader --via probe -f json

Exit codes: 0 ok, 3 device not found/ambiguous, 4 timeout, 6 device I/O, 9 device busy

JSON data: { device, mode, via, reconnected }";

pub const SERIAL_CAPTURE: &str = "\
Reads lines until an --until pattern matches, a fault is detected, or the deadline
(default 10s). Built-in fail patterns catch panics, aborts, brownouts, and watchdogs.

Examples:
  pogo serial capture --reset --until READY --timeout 15s -f json
  pogo serial capture --fail-on 'E \\(\\d+\\) app' --elf app.elf
  pogo serial capture --timeout 3s --record session.jsonl

Exit codes: 0 matched (or completed with no --until), 4 timeout, 6 device vanished (eof),
7 target fault, 9 device busy

JSON data: { device, outcome: matched|completed|fault|timeout|eof, matched, line_count,
omitted_lines, boot_count, resets, panic, lines: [ { t_ms, level, tag, device_ts_ms, text } ] }";

pub const SERIAL_SEND: &str = "\
Writes data with a line ending. With --expect, captures until a response matches,
with the same fault detection as serial capture.

Examples:
  pogo serial send ping --expect pong -f json
  pogo serial send --hex 'de ad be ef' --line-ending none
  echo reboot | pogo serial send -

Exit codes: 0 ok, 4 timeout waiting for --expect, 6 device I/O, 7 target fault, 9 device busy

JSON data: { device, bytes_written, capture }";

pub const RUN: &str = "\
Locks the device once, flashes, resets into run mode, and captures from the first boot
byte. The exit code comes from the capture outcome unless flashing failed.

Examples:
  pogo run target/riscv32imc-unknown-none-elf/release/app --until READY -f json
  pogo run app.elf --no-flash --until READY --settle 2s

Exit codes: 0 matched, 3 device not found/ambiguous, 4 timeout, 6 device I/O,
7 target fault, 8 guardrail refused, 9 device busy

JSON data: { device, flash, capture } (see flash and serial capture)";

pub const I2C_SCAN: &str = "\
Requires the probe. Bus 0 is the user bus; bus 1 is reserved for the INA226.

Examples:
  pogo i2c scan
  pogo i2c scan --range 0x70-0x77 -f json

Exit codes: 0 ok, 3 probe not found, 6 probe I/O or protocol

JSON data: { bus, range, found: [ { addr, addr_dec, may_be } ] }";

pub const I2C_READ: &str = "\
Requires the probe. With --reg, writes the register address then does a repeated-start read.

Examples:
  pogo i2c read 0x68 --reg 0x75 --len 1
  pogo i2c read 0x76 --reg 0xd0 --len 1 -f json

Exit codes: 0 ok, 3 probe not found, 6 probe I/O, NACK, or protocol

JSON data: { bus, addr, reg, bytes_hex, bytes_dec }";

pub const I2C_WRITE: &str = "\
Requires the probe. Writes to protected addresses (default 0x50-0x57) are refused.

Examples:
  pogo i2c write 0x68 0x6b 0x00
  pogo i2c write 0x3c 0x00 0xaf --dry-run -f json

Exit codes: 0 ok, 3 probe not found, 6 probe I/O or NACK, 8 guardrail refused

JSON data: { bus, addr, bytes_written }";

pub const GPIO_GET: &str = "\
Requires the probe. <PIN> is a name from the config's [pins] table.

Examples:
  pogo gpio get target_en
  pogo gpio get target_boot -f json

Exit codes: 0 ok, 3 probe not found, 6 probe I/O, 8 pin not declared in config

JSON data: { pin, probe_pin, level: low|high }";

pub const GPIO_SET: &str = "\
Requires the probe. Open-drain pins accept only low and release. The state persists after
the command exits until changed, released, or the probe loses USB.

Examples:
  pogo gpio set target_boot low
  pogo gpio set target_boot release -f json

Exit codes: 0 ok, 3 probe not found, 6 probe I/O, 8 guardrail refused (incl. pin not declared)

JSON data: { pin, probe_pin, state }";

pub const GPIO_PULSE: &str = "\
Requires the probe. The pulse is timed on the probe, so it is precise.

Examples:
  pogo gpio pulse target_en
  pogo gpio pulse target_en --level low --duration 50ms -f json

Exit codes: 0 ok, 3 probe not found, 6 probe I/O, 8 guardrail refused (incl. pin not declared)

JSON data: { pin, probe_pin, level, duration_ms }";

pub const POWER_MEASURE: &str = "\
Requires the probe and an INA226. The target must be powered through the shunt, not USB.

Examples:
  pogo power measure --duration 10s -f json
  pogo power measure --interval 2ms --out samples.csv

Exit codes: 0 ok, 3 probe not found, 6 probe I/O or protocol

JSON data: { duration_ms, samples, dropped, current_ma: { avg, min, max, p50, p95 },
bus_voltage_v: { avg, min, max }, energy_mj, csv_path }";

pub const POWER_SWITCH: &str = "\
Requires the probe and a load switch on the configured switch pin.

Examples:
  pogo power off
  pogo power on -f json

Exit codes: 0 ok, 3 probe not found, 6 probe I/O, 8 guardrail refused

JSON data: { on }";

pub const POWER_CYCLE: &str = "\
Requires the probe and a load switch. Limited by min_power_off_time and
max_power_cycles_per_minute guardrails.

Examples:
  pogo power cycle
  pogo power cycle --off-time 1s -f json

Exit codes: 0 ok, 3 probe not found, 6 probe I/O, 8 guardrail refused

JSON data: { off_time_ms }";

pub const PROBE_INFO: &str = "\
Examples:
  pogo probe info
  pogo probe info -f json

Exit codes: 0 ok, 3 probe not found, 6 probe I/O or protocol mismatch

JSON data: { fw_version, protocol_version, board, uptime_ms, driven_pins }";

pub const PROBE_SELFTEST: &str = "\
Examples:
  pogo probe selftest
  pogo probe selftest -f json

Exit codes: 0 all checks passed, 1 a check failed, 3 probe not found, 6 probe I/O

JSON data: { checks: [ { id, status: pass|warn|fail, message, hint } ] }";

pub const CONFIG_SHOW: &str = "\
Merge order (later wins): user config, nearest pogo.toml walking up, POGO_CONFIG, --config.

Examples:
  pogo config show
  pogo config show -f json

Exit codes: 0 ok, 10 config invalid

JSON data: { config, sources: [ { key, source } ] }";

pub const CONFIG_PATH: &str = "\
Examples:
  pogo config path
  pogo config path -f json

Exit codes: 0 ok

JSON data: { files: [ { path, kind, exists } ] }";

pub const CONFIG_VALIDATE: &str = "\
Examples:
  pogo config validate
  pogo config validate --config bench.toml -f json

Exit codes: 0 valid, 10 config invalid

JSON data: { files: [ path ], valid }";

pub const SCHEMA: &str = "\
Examples:
  pogo schema serial.capture
  pogo schema --all --out schemas/

Exit codes: 0 ok, 1 command has no schema yet, 2 unknown command

JSON data: the JSON Schema document itself (not wrapped in an envelope)";

pub const COMPLETIONS: &str = "\
Examples:
  pogo completions zsh > ~/.zfunc/_pogo
  pogo completions bash > /etc/bash_completion.d/pogo

Exit codes: 0 ok, 2 unknown shell

Output: the completion script (not wrapped in an envelope)";

pub const MCP: &str = "\
Exposes the same operations as MCP tools. Register with Claude Code via `claude mcp add`
or a project .mcp.json.

Examples:
  pogo mcp
  claude mcp add pogo -- pogo mcp

Exit codes: 0 client disconnected cleanly, 1 internal error";
