//! The hitch log: a frame that stalls is logged, and appended to
//! `hitches.log` next to the profile, with how hard the whole machine was
//! waiting on memory and disk at the time. Then a freeze can be traced after
//! the fact (the game's own work, the window, or the system) without a
//! terminal open.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("tankit").join("hitches.log"))
}

fn append(line: &str) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{stamp} {line}");
    }
}

/// Mark a new run in the log, so hitches can be told apart by session.
pub fn start() {
    append("--- game started");
}

/// Log a hitch, with the system's memory and disk pressure.
pub fn report(what: std::fmt::Arguments) {
    let line = format!("{what} | {}", system());
    log::warn!("{line}");
    append(&line);
}

/// Share of the last 10 s that tasks spent stalled on memory and on disk,
/// and free swap (Linux; empty elsewhere).
fn system() -> String {
    let pressure = |kind: &str| {
        std::fs::read_to_string(format!("/proc/pressure/{kind}"))
            .ok()
            .and_then(|s| {
                let some = s.lines().next()?;
                let avg = some.split_whitespace().find(|w| w.starts_with("avg10="))?;
                Some(format!("{kind} {}%", &avg[6..]))
            })
    };
    let swap = std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
        let kb: u64 = s
            .lines()
            .find(|l| l.starts_with("SwapFree:"))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()?;
        Some(format!("swap free {} MB", kb / 1024))
    });
    [pressure("memory"), pressure("io"), swap]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
}

/// A stopwatch over the parts of one frame, for the hitch log.
pub struct Laps {
    start: Instant,
    at: Instant,
    laps: Vec<(&'static str, f32)>,
}

impl Laps {
    pub fn new(start: Instant) -> Laps {
        Laps {
            start,
            at: start,
            laps: Vec::with_capacity(6),
        }
    }

    /// The part named `name` ended now.
    pub fn lap(&mut self, name: &'static str) {
        let now = Instant::now();
        self.laps.push((name, (now - self.at).as_secs_f32()));
        self.at = now;
    }

    pub fn total(&self) -> f32 {
        (self.at - self.start).as_secs_f32()
    }
}

impl std::fmt::Display for Laps {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for (i, (name, t)) in self.laps.iter().enumerate() {
            let sep = if i == 0 { "" } else { ", " };
            write!(f, "{sep}{name} {:.1} ms", t * 1e3)?;
        }
        Ok(())
    }
}
