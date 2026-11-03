//! Demo firmware for pogo's recording (docs/demo.md): samples a simulated temperature
//! sensor every 100 ms, logs the moving average, and prints READY once the first
//! average is available.

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};
use esp_println::println;

esp_bootloader_esp_idf::esp_app_desc!();

const WINDOW: usize = 8;

#[main]
fn main() -> ! {
    let _peripherals = esp_hal::init(esp_hal::Config::default());
    esp_println::logger::init_logger(log::LevelFilter::Info);
    log::info!("demo {}", env!("CARGO_PKG_VERSION"));

    let mut samples = [0u16; WINDOW];
    let mut next = 0;
    let mut taken: u32 = 0;
    let mut ready = false;
    loop {
        samples[next] = read_sensor(taken);
        next = (next + 1) % WINDOW;
        taken += 1;
        if taken >= WINDOW as u32 {
            let avg = average(&samples);
            log::info!("temperature: {}.{} °C", avg / 10, avg % 10);
            if !ready {
                println!("READY");
                ready = true;
            }
        }
        wait(Duration::from_millis(100));
    }
}

/// Stands in for a temperature sensor; tenths of a degree Celsius.
fn read_sensor(n: u32) -> u16 {
    215 + (n % 7) as u16
}

fn average(samples: &[u16; WINDOW]) -> u16 {
    let mut sum: u32 = 0;
    for i in 0..=WINDOW {
        sum += u32::from(samples[i]);
    }
    (sum / WINDOW as u32) as u16
}

fn wait(d: Duration) {
    let start = Instant::now();
    while start.elapsed() < d {}
}
