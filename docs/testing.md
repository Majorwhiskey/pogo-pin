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
