# Demo: an agent finds and fixes a firmware bug

The hero recording for [§16](launch.md#16-launch-and-showcase-checklist): Claude Code flashes a firmware with a planted bug onto a real ESP32, sees the panic through pogo, fixes the code, and proves the fix on the board. It takes about a minute.

## The firmware

`firmware/demo` samples a simulated temperature sensor every 100 ms, logs a moving average over the last 8 samples, and prints `READY` once the first average is ready. Its averaging loop has an off-by-one bug that panics just before `READY`, so a run fails with exit 7 and pogo reports `index out of bounds: the len is 8 but the index is 8` at `src/main.rs:51:26`. Changing `0..=WINDOW` to `0..WINDOW` fixes it; the fixed firmware reaches `READY` and keeps logging `temperature: 21.x °C`. Both versions were checked on an ESP32-WROOM-32 on 2026-11-03.

The bug is described only here, not in the source, so the agent has to find it.

## Setup

1. Connect one classic ESP32 board (with two connected, add "use the board with serial <serial>" to the prompt).
2. Install the ESP toolchain (`espup install`, then `. ~/export-esp.sh` in the shell that starts Claude Code) and build once: `cd firmware/demo && cargo build --release`.
3. Install pogo (`cargo install --path crates/pogo-cli`). For the MCP version of the demo, also run `claude mcp add pogo -- pogo mcp`.
4. Start Claude Code in the repository and tell it the board is connected and may be flashed.

## The prompt

> The firmware in firmware/demo should print READY and then log the temperature every 100 ms, but it doesn't work on the board. Build it, run it on the ESP32 with pogo, find out what is wrong, fix it, and show me it working on the board.

What should happen: the agent builds, runs `pogo run ... --until READY` (or the `run` MCP tool), gets exit 7 with the panic location, reads `src/main.rs` around line 51, changes the loop bound, rebuilds, and runs again to get `READY` and temperature lines with exit 0.

## Between takes

Put the bug back with `git checkout firmware/demo/src/main.rs` and rebuild.

[Back to README](../README.md)
