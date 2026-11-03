---
name: esp32-hardware
description: Run and debug firmware on real ESP32 boards with pogo's MCP tools (flash, run, capture serial, send commands, run hardware test suites). Use when the user works on ESP32 firmware and a board is connected.
when_to_use: Building, flashing, running, or debugging ESP32 firmware on a connected board; reading a panic or boot log; checking a fix on hardware.
---

# Working with ESP32 boards through pogo

pogo's tools always finish, return `{ ok, data, error, warnings }`, and give errors a stable `code` with a `hint`. Read the code and hint before retrying.

## Before touching hardware

- Ask the user before the first `flash`, `run`, `reset`, `device_info`, or `serial_send` in a session, unless they already said the board is connected and may be flashed.
- Start with `list_devices`. If several boards are listed, pass `device` (a USB serial number or port) on every call; pogo refuses to guess.

## The loop

1. Build the firmware with the project's own build command.
2. Call `run` with the ELF and `until` set to the line the firmware prints when it is up (for example `^READY$`). It flashes, resets, and captures from the first boot byte.
3. Read `data.capture.outcome`:
   - `matched`: it worked; say which line matched and when (`t_ms`).
   - `fault`: read `data.capture.panic`. `location` is the file and line; `frames` are symbolized function names. Fix the code there, then go back to step 1.
   - `timeout`: the expected line never came; read the last lines of `data.capture.lines`.
4. Show the user the evidence: the matched line, or the panic location and message.

## Other tools

- `serial_send` with `expect` sends a command and waits for the reply; pass `elf` to symbolize a crash in the reply.
- `serial_capture` reads output without flashing (`reset: true` to capture a fresh boot).
- `run_tests` runs a `pogo-test.toml` suite and reports each test; failures include the step, message, and last lines.
- `doctor` diagnoses setup problems (permissions, missing boards).

## Errors

- `DEVICE_BUSY`: another command holds the board; wait, or pass `wait_lock` (for example `"30s"`).
- `AMBIGUOUS_DEVICE`: pass `device`.
- `GUARDRAIL_REFUSED`: pogo blocked a risky action on purpose. Tell the user instead of looking for a way around it.
