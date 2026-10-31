# 8. Probe firmware and wire protocol

## 8.1 Responsibilities

The probe firmware is deliberately simple. It exposes a USB CDC-ACM serial device, executes one request at a time, enforces safe pin states, and streams raw measurements. All interpretation (statistics, unit conversion, guardrail policy) happens on the host.

## 8.2 Framing

- Each frame is a postcard-serialized `Frame<T>`, COBS-encoded and terminated by `0x00`.
- Maximum encoded frame size: 512 bytes.
- The host sends one request at a time; the probe echoes the request's `seq` in its response(s).
- The host applies a per-request timeout (default 500 ms) and reports `PROTOCOL` errors on timeout or decode failure.

## 8.3 Messages (`pogo-protocol`, `no_std`)

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Frame<T> { pub seq: u16, pub body: T }

pub const MAGIC: u32 = 0x504F_474F; // "POGO"

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Request {
    Hello { protocol_major: u8, protocol_minor: u8 },
    GetInfo,
    GpioConfigure { pin: u8, mode: PinMode },        // OpenDrain | PushPull | Input
    GpioSet { pin: u8, level: Level },                // Low | High | Release
    GpioGet { pin: u8 },
    GpioPulse { pin: u8, level: Level, duration_us: u32 },
    I2cScan { bus: u8, first: u8, last: u8 },
    I2cRead { bus: u8, addr: u8, reg: Option<u8>, len: u8 },
    I2cWrite { bus: u8, addr: u8, data: heapless::Vec<u8, 64> },
    PowerStart { interval_us: u32, duration_ms: u32 },
    PowerStop,
    PowerSwitch { on: bool },
    ReleaseAll,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Response {
    Hello { magic: u32, protocol_major: u8, protocol_minor: u8, fw_version: heapless::String<16> },
    Info { board: Board, uptime_ms: u64, driven_pins: u32 },
    Ok,
    Level(Level),
    I2cFound(heapless::Vec<u8, 112>),
    Bytes(heapless::Vec<u8, 64>),
    PowerChunk { index: u32, samples: heapless::Vec<PowerSample, 48> },
    PowerDone { total: u32, dropped: u32 },
    Err(ProbeError),
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct PowerSample { pub shunt_raw: i16, pub bus_raw: u16 }
```

`pogo-protocol` must build for both the host and `thumbv6m-none-eabi` / `thumbv8m.main-none-eabihf`. Enable `heapless`'s `serde` feature.

The crate also defines `PinMode` (`OpenDrain | PushPull | Input`), `Level` (`Low | High | Release`), `Board` (`Rp2040 | Rp2350`), and `ProbeError` (`PinNotAllowed`, `PinNotConfigured`, `InvalidLevel`, `InvalidBus`, `I2cNack`, `I2cBus`, `PowerUnavailable`, `Busy`, `Unsupported`, `BadFrame`), plus `PROTOCOL_MAJOR`/`PROTOCOL_MINOR` (1.0), `MAX_FRAME` (512), and `ALLOWED_PINS` with `pin_allowed()`, so host and firmware share one allow-list. `codec::{encode, decode, FrameReader}` do the framing without allocation; `FrameReader` drops a frame that exceeds `MAX_FRAME` and resynchronizes at the next `0x00`. Property tests cover round-trips of every message, reassembly under arbitrary chunking, and that the largest message of each kind fits in one frame. CI builds the crate for both probe targets.

## 8.4 Handshake and versioning

The host's first message on every connection is `Hello`. The probe answers with `MAGIC`, its protocol version, and firmware version. A major version mismatch fails with `PROTOCOL_MISMATCH` (exit 6) and a hint explaining how to update the firmware. Minor versions add messages without breaking older hosts.

## 8.5 Safety behavior (firmware)

- All target-facing pins start **high-Z** at boot.
- **Pin allow-list in firmware:** only GP2, GP3, GP8, and GP9–GP15 can be driven. Requests for any other pin return `Err(PinNotAllowed)`, so a buggy host can't drive I2C, USB, or flash pins.
- On USB disconnect or suspend, the firmware performs `ReleaseAll`.
- A hardware watchdog resets the probe if the main loop stalls; `panic-reset` does the same on panic. Both return pins to high-Z.
- `PWR_EN` defaults to **on** at probe boot so the target behaves like a normal bench setup. This is documented in [hardware.md](hardware.md).

## 8.6 Firmware structure

Embassy tasks: `usb_task` (CDC-ACM), `protocol_task` (decode, dispatch, encode), `power_task` (INA226 sampling into a channel), and a watchdog feeder. USB product string: `pogo-probe`. Use a pid.codes test PID during development and apply for a free one before publishing.

## 8.7 Building and flashing

Build with `cargo build --release` inside `firmware/probe/`, convert to `.uf2` with `picotool` (handles both RP2040 and RP2350), then hold BOOTSEL while plugging in the Pico and copy the `.uf2` onto the drive that appears. Release builds attach the `.uf2` files to GitHub releases.

[Back to README](../README.md)
