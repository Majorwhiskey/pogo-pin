# Measured numbers

Measured 2026-11-03 on a MacBook (Apple Silicon, macOS) with two classic ESP32 boards: an ESP32-WROOM-32 devkit (CH340 USB-serial) and a CP2102 devkit (ESP32 rev v3.1, 4 MB flash). Firmware: `firmware/test-target` release build (bootloader 25,856 B, partition table 3,072 B, app 91,200 B).

**Edit to verified boot** (`pogo run <elf> --until '^READY$'`, three runs per board, each flashing an image that differs from what the board holds):

| Step | Time |
|---|---|
| Whole command, flash included | 10.6–10.9 s on both boards |
| Write and verify the three images | about 3.2 s |
| Connect to the ROM bootloader | about 7 s, of which about 6 s is espflash waiting out two 3 s `GetSecurityInfo` timeouts that the classic ESP32 ROM does not answer |
| Reset, capture, and match `READY` (`run --no-flash`, whole command) | 252–258 ms; `READY` arrives 139 ms after capture starts |

Re-running an unchanged image takes about 7.8 s, because espflash skips regions whose contents already match. The connect delay is specific to the classic ESP32 and comes from espflash's fixed timeout, not from pogo; ESP32-C3/C6/S3 ROMs answer the command. Both boards gave the same results.

**Tests:** `cargo test --workspace` runs 254 tests: unit, mock-transport, replayed-session, snapshot, schema, CLI, MCP, and property tests. Six of them are the hardware-in-the-loop tests, which do nothing unless `POGO_HIL=1` is set; with it, all six pass on both boards in 25–35 s. Code coverage is not measured yet.

[Back to README](../README.md)
