//! `power on|off|cycle` through the probe's load switch, with the rapid-cycling
//! guardrails (docs/guardrails.md §11).

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pogo_protocol::{Request, Response};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::OpResult;
use super::probe::{Probe, unexpected};
use super::target::{Selection, Target, acquire_probe, load_config};
use crate::config::Config;
use crate::envelope::{ErrorBody, Warning};
use crate::error::PogoError;
use crate::lock;

/// Used when `[guardrails]` does not set them.
pub const DEFAULT_MIN_OFF: Duration = Duration::from_millis(200);
pub const DEFAULT_MAX_CYCLES_PER_MINUTE: u32 = 10;

/// Recent switching, kept per probe beside its lock file so the limits hold across
/// separate commands. Times are Unix milliseconds.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    /// When power was last switched off, if it has not been switched on since.
    pub off_since: Option<u64>,
    /// Off-to-on transitions in the last minute.
    pub cycles: Vec<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub min_off: Duration,
    pub max_per_minute: u32,
}

impl Limits {
    pub fn from_config(c: &Config) -> Self {
        Limits {
            min_off: c
                .guardrails
                .min_power_off_time
                .map_or(DEFAULT_MIN_OFF, |d| d.0),
            max_per_minute: c
                .guardrails
                .max_power_cycles_per_minute
                .unwrap_or(DEFAULT_MAX_CYCLES_PER_MINUTE),
        }
    }
}

fn refused(reason: String) -> ErrorBody {
    PogoError::GuardrailRefused { reason }.into()
}

impl History {
    /// May power go back on at `now`, after being off for at least `off_for`?
    pub fn check_on(
        &mut self,
        now: u64,
        off_for: Option<Duration>,
        l: Limits,
    ) -> Result<(), ErrorBody> {
        self.cycles.retain(|t| now.saturating_sub(*t) < 60_000);
        let min_ms = u64::try_from(l.min_off.as_millis()).unwrap_or(u64::MAX);
        let off_ms = match off_for {
            Some(d) => u64::try_from(d.as_millis()).unwrap_or(u64::MAX),
            None => self.off_since.map_or(u64::MAX, |t| now.saturating_sub(t)),
        };
        if off_ms < min_ms {
            return Err(refused(format!(
                "power has been off for {off_ms} ms; the minimum off time is {min_ms} ms \
                 ([guardrails] min_power_off_time)"
            )));
        }
        if self.cycles.len() >= l.max_per_minute as usize {
            return Err(refused(format!(
                "{} power cycles in the last minute; the limit is {} \
                 ([guardrails] max_power_cycles_per_minute)",
                self.cycles.len(),
                l.max_per_minute
            )));
        }
        Ok(())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn history_path(target: &Target) -> PathBuf {
    let d = &target.device;
    let key = d.usb_serial.as_deref().unwrap_or(&d.port);
    lock::lock_dir().join(format!("{}.power.json", lock::sanitize(key)))
}

fn load(path: &PathBuf) -> History {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// A lost history only loosens the limits for a minute, so a write failure is a warning.
fn save(path: &PathBuf, h: &History) -> Vec<Warning> {
    let written = serde_json::to_string(h)
        .map_err(|e| e.to_string())
        .and_then(|s| std::fs::write(path, s).map_err(|e| e.to_string()));
    match written {
        Ok(()) => Vec::new(),
        Err(e) => vec![Warning::new("POWER_HISTORY_FAILED", e)],
    }
}

fn switch(probe: &mut Probe, on: bool) -> Result<(), ErrorBody> {
    match probe.call(Request::PowerSwitch { on })? {
        Response::Ok => Ok(()),
        r => Err(unexpected(&r)),
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PowerSwitchData {
    pub on: bool,
    pub dry_run: bool,
}

pub fn set(sel: &Selection, on: bool, dry_run: bool) -> OpResult<PowerSwitchData> {
    let command = if on { "power.on" } else { "power.off" };
    let run = || -> Result<(PowerSwitchData, Vec<Warning>), ErrorBody> {
        let config = load_config(sel)?;
        let limits = Limits::from_config(&config);
        let target = acquire_probe(sel, config, command)?;
        let path = history_path(&target);
        let mut h = load(&path);
        let now = now_ms();
        if on && h.off_since.is_some() {
            h.check_on(now, None, limits)?;
        }
        let data = PowerSwitchData { on, dry_run };
        if dry_run {
            return Ok((data, Vec::new()));
        }
        let mut probe = Probe::connect(target)?;
        let result = switch(&mut probe, on);
        let mut warnings = probe.audit(command, serde_json::json!({ "on": on }), &result);
        result?;
        match (on, h.off_since) {
            (true, Some(_)) => {
                h.cycles.push(now);
                h.off_since = None;
            }
            (true, None) => {}
            (false, _) => h.off_since = Some(now),
        }
        warnings.extend(save(&path, &h));
        Ok((data, warnings))
    };
    match run() {
        Ok((data, warnings)) => OpResult::ok(data).with_warnings(warnings),
        Err(e) => OpResult::fail(e),
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PowerCycleData {
    pub off_time_ms: u64,
    pub dry_run: bool,
}

pub fn cycle(sel: &Selection, off_time: Duration, dry_run: bool) -> OpResult<PowerCycleData> {
    let run = || -> Result<(PowerCycleData, Vec<Warning>), ErrorBody> {
        let config = load_config(sel)?;
        let limits = Limits::from_config(&config);
        let target = acquire_probe(sel, config, "power.cycle")?;
        let path = history_path(&target);
        let mut h = load(&path);
        h.check_on(now_ms(), Some(off_time), limits)?;
        let data = PowerCycleData {
            off_time_ms: u64::try_from(off_time.as_millis()).unwrap_or(u64::MAX),
            dry_run,
        };
        if dry_run {
            return Ok((data, Vec::new()));
        }
        let mut probe = Probe::connect(target)?;
        let result = switch(&mut probe, false).and_then(|()| {
            std::thread::sleep(off_time);
            switch(&mut probe, true).map_err(|mut e| {
                e.message = format!("power is OFF: switching back on failed: {}", e.message);
                e
            })
        });
        let args = serde_json::json!({ "off_time_ms": data.off_time_ms });
        let mut warnings = probe.audit("power.cycle", args, &result);
        result?;
        h.off_since = None;
        h.cycles.push(now_ms());
        warnings.extend(save(&path, &h));
        Ok((data, warnings))
    };
    match run() {
        Ok((data, warnings)) => OpResult::ok(data).with_warnings(warnings),
        Err(e) => OpResult::fail(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: Limits = Limits {
        min_off: Duration::from_millis(200),
        max_per_minute: 2,
    };

    #[test]
    fn off_time_is_enforced_from_history_or_the_request() {
        let mut h = History {
            off_since: Some(1_000),
            cycles: Vec::new(),
        };
        assert!(h.check_on(1_150, None, L).is_err());
        assert!(h.check_on(1_200, None, L).is_ok());
        let e = h
            .check_on(0, Some(Duration::from_millis(50)), L)
            .unwrap_err();
        assert!(
            e.message.contains("minimum off time is 200 ms"),
            "{}",
            e.message
        );
    }

    #[test]
    fn cycles_per_minute_count_only_the_last_minute() {
        let mut h = History {
            off_since: None,
            cycles: vec![0, 30_000],
        };
        let e = h.check_on(59_000, Some(L.min_off), L).unwrap_err();
        assert!(e.message.contains("2 power cycles"), "{}", e.message);
        assert!(
            h.check_on(60_000, Some(L.min_off), L).is_ok(),
            "the first one aged out"
        );
        assert_eq!(h.cycles, [30_000]);
    }
}
