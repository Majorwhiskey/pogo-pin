# A/B test: Claude Code with and without pogo

Measures whether pogo helps an agent on a real hardware task. The task is the planted bug from [demo.md](demo.md); the only difference between the two runs is the tooling the agent is allowed to use. The result is the evidence for the README, the demo video, and the launch post.

## Setup

- One classic ESP32 board (a WROOM-32 works), connected for both runs, and the same board and cable for each.
- Same machine, same Claude Code version and model, same toolchain (`espup`, `. ~/export-esp.sh`).
- A clean checkout per run, with `firmware/demo/src/main.rs` unmodified. Between runs: `git checkout firmware/demo/src/main.rs` and rebuild.
- Two fresh sessions, no shared context. Do not tell either session about the other run.
- Run each arm at least 3 times, alternating A and B, so that board state, USB flakiness, and cache warmth do not favour one arm.

## The two arms

**A: plain CLI.** No pogo binary and no pogo MCP server. Allowed: `cargo`, `espflash`, `espup`, `screen` or `cu`, `addr2line`, and anything else already on the machine. Verify with `claude mcp list` that pogo is absent and that `pogo` is not on `$PATH` (rename `~/.cargo/bin/pogo` for the run and restore it afterwards).

**B: pogo.** The pogo MCP server is registered (`claude mcp add pogo -- pogo mcp`). `espflash` is not removed, but the prompt below says to use pogo.

## Prompts

Both arms start with the same first message, telling the agent the board is connected and may be flashed.

Arm A:

> The firmware in firmware/demo should print READY and then log the temperature every 100 ms, but it doesn't work on the board. Build it, run it on the ESP32, find out what is wrong, fix it, and show me it working on the board. The board is connected and you may flash it. Do not use pogo.

Arm B:

> The firmware in firmware/demo should print READY and then log the temperature every 100 ms, but it doesn't work on the board. Build it, run it on the ESP32 with pogo, find out what is wrong, fix it, and show me it working on the board. The board is connected and you may flash it.

Do not hint at the bug or the file. A run ends when the agent claims success, gives up, or hits the cap.

## Caps

- 30 minutes of wall time.
- 40 agent turns.
- If the agent is blocked on something only a human can do (a boot-mode button, a permission prompt that is not a pogo or hardware prompt), count one intervention, resolve it, and continue.

## What to record

| Metric | How |
|---|---|
| Fixed | Yes only if the final firmware, flashed and run, prints READY and at least 3 `temperature:` lines with no panic, confirmed by you from the transcript's last capture. A claimed fix with no on-board proof is No. |
| Correct fix | The diff is `0..=WINDOW` → `0..WINDOW` (or equivalent) and nothing else of substance. |
| Turns | Count of assistant turns until the end of the run. |
| Tool calls | Total, and the number that failed or returned an error. |
| Wall time | From the first message to the final answer, in minutes. |
| Tokens | Input and output from the session summary (`/cost`). |
| Interventions | Each time you had to type something other than the opening prompt. |
| Notes | Where it got stuck, for example reading raw serial output, an unsymbolized backtrace, a busy port, or a wrong baud rate. |

## Scoring sheet

Copy this table per run.

| Run | Arm | Fixed | Correct fix | Turns | Tool calls (failed) | Wall time (min) | Tokens in / out | Interventions | Notes |
|---|---|---|---|---|---|---|---|---|---|
| 1 | A | | | | | | | | |
| 1 | B | | | | | | | | |
| 2 | A | | | | | | | | |
| 2 | B | | | | | | | | |
| 3 | A | | | | | | | | |
| 3 | B | | | | | | | | |

Report the median per arm for turns, time, and tokens, and the count of Fixed runs out of 3 or more. With this few runs, say so: it is evidence about the workflow, not a statistical result.

## Rules for honest results

- Do not rerun an arm because it went badly; keep every run, and report all of them.
- Do not change the prompt or the firmware after the first run.
- Record failures in pogo's favour and against it with the same detail.
- Save each transcript (`/export`) next to the scoring sheet so the numbers can be checked.

[Back to README](../README.md)
