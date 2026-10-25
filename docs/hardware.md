# 3. Hardware setup

## 3.1 Minimum setup (Phases 0–3)

Only an ESP32-family dev board and a **data-capable** USB cable (many cheap cables are power-only).

Recommended: **ESP32-C3 or ESP32-C6**. They are RISC-V chips, so the Rust test firmware builds with the standard Rust toolchain. ESP32 and ESP32-S3 (Xtensa) are fully supported by `pogo` itself, but building the Rust test firmware for them requires the Espressif toolchain installed via `espup`.

## 3.2 Full bench (Phase 4+)

| Part | Purpose | Notes |
|---|---|---|
| ESP32-C3/S3 dev board | Target under test | Any Espressif dev board works |
| Raspberry Pi Pico (RP2040) or Pico 2 (RP2350) | Probe board | Runs `pogo` probe firmware |
| INA226 breakout | Current and voltage measurement | Common modules use a 0.1 Ω shunt, giving roughly 819 mA full scale |
| BME280 or MPU6050 breakout | Known I2C device for tests | Predictable scan and register-read results |
| Load switch or logic-level MOSFET module (optional) | Power-cycling the target | Rated for at least 1 A |
| Breadboard, jumper wires, 4.7 kΩ resistors | Wiring and I2C pull-ups | Many breakouts already include pull-ups |

## 3.3 Default probe wiring

| Pico pin | Function | Connects to |
|---|---|---|
| GP2 | `TARGET_EN` (open-drain, active low) | ESP32 EN/RST pin |
| GP3 | `TARGET_BOOT` (open-drain, active low) | GPIO0 (ESP32, ESP32-S3) or GPIO9 (ESP32-C3) |
| GP4 | I2C0 SDA | Sensor-under-test SDA |
| GP5 | I2C0 SCL | Sensor-under-test SCL |
| GP6 | I2C1 SDA | INA226 SDA |
| GP7 | I2C1 SCL | INA226 SCL |
| GP8 | `PWR_EN` (push-pull) | Load switch enable (optional) |
| 3V3(OUT) | 3.3 V supply | Sensor and INA226 VCC |
| GND | Ground | Target GND, sensor GND, INA226 GND |

```
                    ┌──────────── Raspberry Pi Pico (probe) ────────────┐
   Host USB ────────┤ USB                                               │
                    │ GP2  ── open-drain ─────────► Target EN/RST       │
                    │ GP3  ── open-drain ─────────► Target BOOT strap   │
                    │ GP4/5 (I2C0) ───────────────► Sensor under test   │
                    │ GP6/7 (I2C1) ───────────────► INA226              │
                    │ GP8  ───────────────────────► Load switch EN      │
                    │ GND  ───────────────────────► Common ground       │
                    └───────────────────────────────────────────────────┘

   Supply (+) ──► INA226 VIN+ ──[shunt]──► INA226 VIN- ──► Target 5V/VIN
```

## 3.4 Wiring notes and gotchas

- **Common ground is mandatory.** Probe, target, sensor, and INA226 must share GND.
- **EN and BOOT are driven open-drain only.** The probe either pulls them low or releases them (high-impedance). It never drives them high, because dev boards have their own pull-ups and auto-reset circuits.
- **I2C0 is for sensor bring-up.** Wire sensors directly to the probe to test them. Don't connect I2C0 to a bus the ESP32 is actively mastering, since two masters will conflict.
- **USB power bypasses the shunt.** To measure current, power the target from an external supply through the INA226 and use USB for data only (a USB power-blocker adapter isolates VBUS), or use a separate USB-UART adapter on TX/RX/GND. **Never connect an external supply and USB VBUS to the same rail at the same time**, because backfeeding can damage the board or the host's USB port.
- **Native USB boards re-enumerate.** ESP32-C3/S3/C6 boards using the built-in USB-Serial-JTAG may briefly drop off the bus on reset. `pogo` identifies devices by USB serial number and reconnects automatically ([§6.7](agent-contract.md#67-device-selection)). The first ROM boot lines may be lost during re-enumeration.
- **Cheap CH340 adapters often lack a unique USB serial number.** `pogo` falls back to the port path and emits a warning.

## 3.5 OS setup

**Linux.** Add your user to the serial group (`dialout` on Debian/Ubuntu, `uucp` on Arch), then log out and back in. Building requires `libudev-dev` (Debian/Ubuntu) or the equivalent. ModemManager may grab `/dev/ttyACM*` devices briefly when they appear, so add a udev rule:

```
# /etc/udev/rules.d/99-pogo.rules
SUBSYSTEM=="tty", ATTRS{idVendor}=="303a", ENV{ID_MM_DEVICE_IGNORE}="1"
SUBSYSTEM=="tty", ATTRS{idVendor}=="2e8a", ENV{ID_MM_DEVICE_IGNORE}="1"
```

Then run `sudo udevadm control --reload-rules && sudo udevadm trigger`.

**macOS.** Use `/dev/cu.*` ports, not `/dev/tty.*` (the `tty` variants block waiting for carrier detect). `pogo` picks `cu` ports automatically.

**Windows.** Install CP210x or CH340 drivers if your board uses those bridges. Ports appear as `COMn`.

[Back to README](../README.md)
