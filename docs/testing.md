# 12. Testing strategy

| Layer | What it covers | Tooling |
|---|---|---|
| Unit | Log parsers, panic block parser, regex matching, durations, config merge, INA226 conversion | `cargo test` |
| Mock transport | Capture engine with scripted byte streams: partial lines, CRLF, lone CR, ANSI codes, garbage bytes, mid-line deadline | `MockTransport` |
| Record/replay | Real sessions recorded with `--record`, replayed in CI | `tests/fixtures/*.jsonl`, `ReplayTransport` |
| Snapshot | JSON outputs, human output, `--help` texts, schemas | `insta` |
| CLI integration | Exit codes, stdout/stderr separation, format flags | `assert_cmd`, `predicates` |
| Property | COBS/postcard round-trips, line splitter under arbitrary chunking | `proptest` |
| Hardware-in-the-loop | Real flash, capture, reset, probe commands | `tests/hil/`, opt-in |

**Fixture coverage required by Phase 1:** clean boot with `READY`, timeout with no output, Rust panic with backtrace, ESP-IDF Guru Meditation with `Backtrace:` line, boot loop, brownout, device disconnect mid-capture.

**Recording format** (`--record`, `tests/fixtures/*.jsonl`): one JSON object per line. The first line is a header; the rest are events in time order, with `t_ms` counted from the start of the recording. Byte chunks carry `text` when they are valid UTF-8 and lowercase `hex` otherwise, so fixtures can be read and edited by hand.

```jsonl
{"type":"header","format":1,"pogo_version":"0.1.0","port":"/dev/cu.usbmodem1101","baud":115200}
{"type":"dtr","t_ms":0,"asserted":false}
{"type":"rx","t_ms":12,"text":"ESP-ROM:esp32c3-api1-20210207\r\n"}
{"type":"rx","t_ms":13,"hex":"ff00"}
{"type":"tx","t_ms":900,"text":"ping\n"}
{"type":"disconnect","t_ms":1500}
```

Other event types are `rts` and `baud`. `ReplayTransport` delivers `rx` chunks in order as fast as they are read (timestamps are not replayed, so tests run instantly), ignores writes and control-line changes, and fails with `DEVICE_IO` at a `disconnect`. A recording without a `disconnect` behaves like a connected, silent device once its data runs out. Timing-sensitive tests (settle windows, mid-line deadlines) use `MockTransport` with explicit delays instead.

**Replaying through the CLI:** with `POGO_REPLAY=<file>` set, `serial capture` reads that recording instead of resolving and opening a device (`data.device.port` becomes `replay:<file>`). It is a test hook for exit-code and output tests, not a user feature. `tests/fixtures/synthetic/` holds hand-written sessions covering the Phase 1 fixture list; they model typical ESP32-C3 output and will be joined by real recordings made with `--record` on the bench boards.

**Test-target firmware** (`firmware/test-target/`, esp-hal) prints `READY` after init and accepts line commands over serial:

| Command | Behavior |
|---|---|
| `ping` | Replies `pong` |
| `panic` | Triggers an `unwrap()` on `None` at a known source line |
| `reboot` | Software reset |
| `busy <ms>` | Busy-loops (power tests) |
| `sleep <ms>` | Light sleep (power tests) |
| `gpio <n> <0\|1>` | Sets a target GPIO |

```console
cargo test --workspace                       # everything that doesn't need hardware
cargo insta review                           # review snapshot changes
POGO_HIL=1 cargo test -p pogo-cli --test hil -- --test-threads=1
```

HIL tests are skipped unless `POGO_HIL=1` is set, and they always run single-threaded.

[Back to README](../README.md)
