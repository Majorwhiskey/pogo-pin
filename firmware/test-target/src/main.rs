//! Fixture firmware for pogo's hardware tests (docs/testing.md §12).
//!
//! Prints `READY` after init, then reads line commands on UART0:
//! `ping` -> `pong`, `panic` -> an `unwrap()` on `None` at a known line,
//! `reboot` -> software reset, `busy <ms>` -> spin for that long, then `done`.

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};
use esp_hal::uart::{Config, Uart};
use esp_println::println;

esp_bootloader_esp_idf::esp_app_desc!();

const LINE_MAX: usize = 64;

#[main]
fn main() -> ! {
    let p = esp_hal::init(esp_hal::Config::default());
    esp_println::logger::init_logger(log::LevelFilter::Info);
    let mut uart = match Uart::new(p.UART0, Config::default()) {
        Ok(u) => u.with_rx(p.GPIO3),
        Err(_) => esp_hal::system::software_reset(),
    };

    log::info!("test-target {}", env!("CARGO_PKG_VERSION"));
    println!("READY");

    let mut line = [0u8; LINE_MAX];
    let mut len = 0;
    loop {
        let mut byte = [0u8; 1];
        if !uart.read_ready() || uart.read(&mut byte).unwrap_or(0) == 0 {
            continue;
        }
        match byte[0] {
            b'\r' | b'\n' => {
                if len > 0 {
                    handle(core::str::from_utf8(&line[..len]).unwrap_or(""));
                    len = 0;
                }
            }
            b if len < LINE_MAX => {
                line[len] = b;
                len += 1;
            }
            _ => len = 0,
        }
    }
}

fn handle(cmd: &str) {
    let mut words = cmd.split_whitespace();
    match (words.next(), words.next()) {
        (Some("ping"), None) => println!("pong"),
        (Some("panic"), None) => trigger_panic(),
        (Some("reboot"), None) => esp_hal::system::software_reset(),
        (Some("busy"), Some(ms)) => match ms.parse::<u64>() {
            Ok(ms) => {
                let start = Instant::now();
                while start.elapsed() < Duration::from_millis(ms) {}
                println!("done");
            }
            Err(_) => log::warn!("bad busy duration: {}", ms),
        },
        _ => log::warn!("unknown command: {}", cmd),
    }
}

/// pogo's hardware tests expect this panic at a fixed line; keep it in place.
#[inline(never)]
fn trigger_panic() {
    let nothing: Option<u32> = None;
    let value = nothing.unwrap();
    println!("{}", value);
}
