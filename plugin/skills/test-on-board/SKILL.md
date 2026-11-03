---
name: test-on-board
description: Run the project's hardware test suite (pogo-test.toml) on the connected ESP32 and report the results, creating a starter suite if there is none.
argument-hint: "[suite file]"
---

# Test on the board

1. Find the suite: the argument if given, else `pogo-test.toml` in the project. If there is none, offer to write one: a top-level `firmware = "<ELF path>"` and `[[test]]` tables whose `steps` are `{ reset = true, until = "^READY$" }`, `{ send = "<cmd>", expect = "<regex>" }`, or `{ wait = "<regex>" }`. Ask which lines the firmware prints and which commands it accepts rather than guessing.
2. Confirm with the user that the board may be flashed, unless they already said so in this session.
3. Call the `run_tests` tool with the suite file.
4. Report each test as passed or failed. For a failure, give the step, the message, and the relevant lines; if a panic location is shown, open that file and line and explain the likely cause.
