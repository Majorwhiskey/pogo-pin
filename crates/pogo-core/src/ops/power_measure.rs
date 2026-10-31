//! `power measure`: stream raw INA226 samples from the probe, convert, and summarize.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pogo_protocol::{PowerSample, Request, Response};
use schemars::JsonSchema;
use serde::Serialize;

use super::probe::{Probe, unexpected};
use super::target::{Selection, load_config};
use super::{OpResult, usage};
use crate::envelope::ErrorBody;
use crate::error::PogoError;

/// INA226 register LSBs (datasheet §7.6): shunt voltage 2.5 µV, bus voltage 1.25 mV.
const SHUNT_LSB_V: f64 = 2.5e-6;
const BUS_LSB_V: f64 = 1.25e-3;
/// The common INA226 breakout's shunt (docs/hardware.md), when `[power] shunt_ohms` is unset.
pub const DEFAULT_SHUNT_OHMS: f64 = 0.1;
/// Bounds that keep a run's samples in memory (10 M samples is about 40 MB raw).
const MIN_INTERVAL: Duration = Duration::from_micros(100);
const MAX_INTERVAL: Duration = Duration::from_secs(1);
const MAX_SAMPLES: u128 = 10_000_000;
/// Slack after `--duration` for the last chunks and `PowerDone` (agent-contract §6.6).
const MARGIN: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct CurrentStats {
    pub avg: f64,
    pub min: f64,
    pub max: f64,
    pub p50: f64,
    pub p95: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct VoltageStats {
    pub avg: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct PowerMeasureData {
    pub duration_ms: u64,
    pub interval_us: u32,
    /// Samples received.
    pub samples: u64,
    /// Samples the probe could not deliver (its own overruns plus any lost in transit).
    pub dropped: u64,
    /// Through the shunt, in mA; null when no samples arrived.
    pub current_ma: Option<CurrentStats>,
    pub bus_voltage_v: Option<VoltageStats>,
    /// Sum of bus voltage × current × interval over the samples received, in mJ.
    pub energy_mj: f64,
    /// The CSV file written with --out, if any.
    pub csv_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MeasureRequest {
    pub duration: Duration,
    pub interval: Duration,
    pub out: Option<PathBuf>,
}

fn round(x: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (x * f).round() / f
}

/// Current in mA and bus voltage in V for one raw sample.
pub fn convert(s: PowerSample, shunt_ohms: f64) -> (f64, f64) {
    let current_ma = f64::from(s.shunt_raw) * SHUNT_LSB_V / shunt_ohms * 1000.0;
    (current_ma, f64::from(s.bus_raw) * BUS_LSB_V)
}

/// Nearest-rank percentile of sorted values.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = (p / 100.0 * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

pub fn summarize(
    samples: &[PowerSample],
    shunt_ohms: f64,
    interval: Duration,
) -> (Option<CurrentStats>, Option<VoltageStats>, f64) {
    if samples.is_empty() {
        return (None, None, 0.0);
    }
    let n = samples.len() as f64;
    let converted: Vec<(f64, f64)> = samples.iter().map(|s| convert(*s, shunt_ohms)).collect();
    let mut currents: Vec<f64> = converted.iter().map(|c| c.0).collect();
    currents.sort_by(f64::total_cmp);
    let volts = converted.iter().map(|c| c.1);
    let energy_mj: f64 =
        converted.iter().map(|(ma, v)| ma * v).sum::<f64>() * interval.as_secs_f64();
    let current = CurrentStats {
        avg: round(currents.iter().sum::<f64>() / n, 3),
        min: round(currents[0], 3),
        max: round(currents[currents.len() - 1], 3),
        p50: round(percentile(&currents, 50.0), 3),
        p95: round(percentile(&currents, 95.0), 3),
    };
    let voltage = VoltageStats {
        avg: round(volts.clone().sum::<f64>() / n, 4),
        min: round(volts.clone().fold(f64::INFINITY, f64::min), 4),
        max: round(volts.fold(f64::NEG_INFINITY, f64::max), 4),
    };
    (Some(current), Some(voltage), round(energy_mj, 3))
}

fn write_csv(
    path: &Path,
    samples: &[PowerSample],
    shunt_ohms: f64,
    interval: Duration,
) -> Result<(), ErrorBody> {
    use std::io::Write;
    let fail = |e: std::io::Error| usage(format!("cannot write {}: {e}", path.display()));
    let file = std::fs::File::create(path).map_err(fail)?;
    let mut w = std::io::BufWriter::new(file);
    writeln!(w, "index,t_ms,shunt_raw,bus_raw,current_ma,bus_voltage_v").map_err(fail)?;
    for (i, s) in samples.iter().enumerate() {
        let (ma, v) = convert(*s, shunt_ohms);
        let t_ms = i as f64 * interval.as_secs_f64() * 1000.0;
        writeln!(
            w,
            "{i},{t_ms:.3},{},{},{ma:.4},{v:.5}",
            s.shunt_raw, s.bus_raw
        )
        .map_err(fail)?;
    }
    w.flush().map_err(fail)
}

pub fn measure(sel: &Selection, req: &MeasureRequest) -> OpResult<PowerMeasureData> {
    let run = || -> Result<PowerMeasureData, ErrorBody> {
        if !(MIN_INTERVAL..=MAX_INTERVAL).contains(&req.interval) {
            return Err(usage("--interval must be between 100µs and 1s"));
        }
        if req.duration.is_zero() || req.duration.as_nanos() / req.interval.as_nanos() > MAX_SAMPLES
        {
            return Err(usage(
                "--duration must be positive and give at most 10 million samples",
            ));
        }
        let duration_ms =
            u32::try_from(req.duration.as_millis()).map_err(|_| usage("--duration is too long"))?;
        let interval_us = u32::try_from(req.interval.as_micros()).unwrap_or(u32::MAX);
        let config = load_config(sel)?;
        let shunt_ohms = config
            .power
            .as_ref()
            .and_then(|p| p.shunt_ohms)
            .unwrap_or(DEFAULT_SHUNT_OHMS);
        let mut probe = Probe::open(sel, config, "power.measure")?;

        let mut samples: Vec<PowerSample> = Vec::new();
        let mut done = None;
        let mut bad = None;
        let start = Request::PowerStart {
            interval_us,
            duration_ms,
        };
        let streamed = probe.stream(start, req.duration + MARGIN, |r| match r {
            Response::PowerChunk { samples: chunk, .. } => {
                samples.extend_from_slice(&chunk);
                false
            }
            Response::PowerDone { total, dropped } => {
                done = Some((total, dropped));
                true
            }
            r => {
                bad = Some(unexpected(&r));
                true
            }
        });
        if let Err(e) = streamed {
            // Leave the probe idle for the next command; it may still be sampling.
            let _ = probe.send(Request::PowerStop);
            return Err(e);
        }
        if let Some(e) = bad {
            return Err(e);
        }
        let (total, probe_dropped) = done.ok_or_else(|| {
            ErrorBody::from(PogoError::Protocol(
                "measurement ended without PowerDone".into(),
            ))
        })?;
        let received = samples.len() as u64;
        let lost = u64::from(total).saturating_sub(received);
        let (current_ma, bus_voltage_v, energy_mj) = summarize(&samples, shunt_ohms, req.interval);
        let csv_path = match &req.out {
            Some(path) => {
                write_csv(path, &samples, shunt_ohms, req.interval)?;
                Some(path.display().to_string())
            }
            None => None,
        };
        Ok(PowerMeasureData {
            duration_ms: u64::from(duration_ms),
            interval_us,
            samples: received,
            dropped: u64::from(probe_dropped) + lost,
            current_ma,
            bus_voltage_v,
            energy_mj,
            csv_path,
        })
    };
    run().map_or_else(OpResult::fail, OpResult::ok)
}

#[cfg(test)]
#[path = "power_tests.rs"]
mod tests;
