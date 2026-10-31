# 10. Configuration

**Discovery order** (later wins on conflicts): user config (`~/.config/pogo/config.toml` or the platform equivalent) → nearest `pogo.toml` walking up from the current directory → `POGO_CONFIG` → `--config`. `pogo config show` displays the merged result and where each value came from.

**How settings apply:** a flag (or MCP argument) wins, then the config, then the built-in default.

- `defaults.format`: output format when neither `-f` nor `POGO_FORMAT` is given.
- `defaults.timeout`: the timeout for commands that take one when `--timeout` is not given (instead of 10 s for capture/run, 5 s for send).
- `defaults.baud`: serial baud rate when `--baud` is not given.
- `devices.<alias>.*` applies when that device is the one selected (by `--device <alias>`, or because the config's `target` alias matched): `chip` skips chip detection when flashing, `parser` is the log parser when `--parser` is not given, `elf` symbolizes panics when `--elf` is not given (relative paths are relative to the config file), and `reset = "probe"` makes resets fail with `NOT_IMPLEMENTED` until the probe exists (Phase 4); `auto` and `dtr-rts` use DTR/RTS.

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

[Back to README](../README.md)
