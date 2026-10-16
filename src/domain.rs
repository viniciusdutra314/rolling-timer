use crate::{Error, Result};
use std::{num::NonZeroU64, time::SystemTime};

/// Milliseconds since Unix epoch, bounded to make all interval arithmetic safe.
/// Dates before 1970 and beyond 9999 are rejected at the boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(i64);
pub const MAX_MILLIS: i64 = 253_402_300_799_999;
impl Timestamp {
    pub fn new(millis: i64) -> Result<Self> {
        if (0..=MAX_MILLIS).contains(&millis) {
            Ok(Self(millis))
        } else {
            Err(Error::Invalid("Timestamp must be between 1970 and 9999"))
        }
    }
    pub fn millis(self) -> i64 {
        self.0
    }
    pub fn from_system_time(time: SystemTime) -> Result<Self> {
        let duration = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| Error::Invalid("System clock is before 1970"))?;
        let millis = i64::try_from(duration.as_millis())
            .map_err(|_| Error::Invalid("System clock is out of range"))?;
        Self::new(millis)
    }
}

/// A positive duration no larger than the supported timestamp range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window(NonZeroU64);
impl Window {
    pub fn new(millis: u64) -> Result<Self> {
        match NonZeroU64::new(millis) {
            Some(value) if millis <= MAX_MILLIS as u64 => Ok(Self(value)),
            _ => Err(Error::Invalid(
                "Duration must be positive and within the supported range",
            )),
        }
    }
    /// Decimal hours, rounded to the nearest millisecond. Rejects NaN/infinity
    /// and values that round to zero rather than creating an empty window.
    pub fn parse_hours(text: &str) -> Result<Self> {
        let hours = text
            .trim()
            .parse::<f64>()
            .map_err(|_| Error::Invalid("Enter a positive number of hours"))?;
        let millis = (hours * 3_600_000.0).round();
        if !hours.is_finite() || hours <= 0.0 || millis < 1.0 || millis > MAX_MILLIS as f64 {
            return Err(Error::Invalid(
                "Enter positive, finite hours (at least one millisecond)",
            ));
        }
        Self::new(millis as u64)
    }
    pub fn millis(self) -> u64 {
        self.0.get()
    }
    pub fn hours_text(self) -> String {
        (self.millis() as f64 / 3_600_000.0).to_string()
    }
}
impl Default for Window {
    fn default() -> Self {
        Self(NonZeroU64::new(86_400_000).unwrap_or(NonZeroU64::MIN))
    }
}

/// Half-open [start, end) interval. Zero-length checkpoints are valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interval {
    start: Timestamp,
    end: Timestamp,
}
impl Interval {
    pub fn new(start: Timestamp, end: Timestamp) -> Result<Self> {
        if end < start {
            return Err(Error::Invalid("Session ends before it starts"));
        }
        Ok(Self { start, end })
    }
    pub fn start(self) -> Timestamp {
        self.start
    }
    pub fn end(self) -> Timestamp {
        self.end
    }
    pub fn overlap(self, now: Timestamp, window: Window) -> u64 {
        let floor = now.0.saturating_sub(window.millis() as i64).max(0);
        self.end
            .0
            .min(now.0)
            .saturating_sub(self.start.0.max(floor))
            .max(0) as u64
    }
}

/// Measure the union of sessions, so overlapping imported records never count
/// the same millisecond twice. Caller supplies data; no hidden database or clock.
pub fn usage(intervals: impl IntoIterator<Item = Interval>, now: Timestamp, window: Window) -> u64 {
    let mut sorted: Vec<_> = intervals.into_iter().collect();
    sorted.sort_unstable_by_key(|i| i.start);
    let mut merged: Option<Interval> = None;
    let mut total = 0_u64;
    for next in sorted {
        match merged {
            Some(current) if next.start <= current.end => {
                merged = Some(Interval {
                    start: current.start,
                    end: current.end.max(next.end),
                });
            }
            Some(current) => {
                total += current.overlap(now, window);
                merged = Some(next);
            }
            None => merged = Some(next),
        }
    }
    if let Some(last) = merged {
        total += last.overlap(now, window);
    }
    // Disjoint intervals fit within the bounded window, so total cannot overflow.
    total
}

pub fn format_duration(millis: u64) -> String {
    let seconds = millis / 1000;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub window: Window,
    pub limit: Option<Window>,
}
