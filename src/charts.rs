//! Pure chart data: disjoint history buckets and an exact piecewise-linear
//! forecast. No plotting framework, clock, database or timezone is needed here.
use crate::domain::{Interval, Settings, Timestamp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub at: i64,
    pub used: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timeline {
    pub now: Timestamp,
    pub settings: Settings,
    /// Minutes used *within* each bucket, rather than a running cumulative sum.
    pub history: Vec<Point>,
    /// Rolling usage if no more sessions are added, including every slope change.
    pub forecast: Vec<Point>,
    pub clears_at: i64,
}
impl Timeline {
    pub fn new(intervals: &[Interval], now: Timestamp, settings: Settings) -> Self {
        let window = settings.window.millis() as i64;
        let floor = (now.millis() - window).max(0);
        // Clip and merge before shifting endpoints: overlapping imports must
        // never create fictitious usage or duplicate release events.
        let mut ranges: Vec<_> = intervals
            .iter()
            .filter_map(|interval| {
                let start = interval.start().millis().max(floor);
                let end = interval.end().millis().min(now.millis());
                (end > start).then_some((start, end))
            })
            .collect();
        ranges.sort_unstable();
        let mut merged: Vec<(i64, i64)> = Vec::new();
        for (start, end) in ranges {
            match merged.last_mut() {
                Some(last) if start <= last.1 => last.1 = last.1.max(end),
                _ => merged.push((start, end)),
            }
        }
        // At most 48 buckets. Near the epoch/sub-millisecond-sized windows use
        // fewer, nonempty buckets; every observed millisecond belongs to one.
        let span = now.millis() - floor;
        let buckets = span.clamp(1, 48);
        let history = (1..=buckets)
            .map(|bucket| {
                let start = floor + span * (bucket - 1) / buckets;
                let end = floor + span * bucket / buckets;
                let used = merged
                    .iter()
                    .map(|&(a, b)| (b.min(end) - a.max(start)).max(0) as u64)
                    .sum();
                Point { at: end, used }
            })
            .collect();
        let mut times = vec![now.millis(), now.millis() + window];
        for &(start, end) in &merged {
            times.extend([start + window, end + window]);
        }
        times.sort_unstable();
        times.dedup();
        // Shifted times may exceed Timestamp's civil-date bound near year 9999;
        // arithmetic still fits comfortably in i64. Integrate the clipped union
        // directly instead of inventing invalid public Timestamp values.
        let forecast = times
            .into_iter()
            .map(|at| Point {
                at,
                used: merged
                    .iter()
                    .map(|&(start, end)| (end - start.max(at - window)).max(0) as u64)
                    .sum(),
            })
            .collect();
        let clears_at = merged
            .last()
            .map(|&(_, end)| end + window)
            .unwrap_or(now.millis());
        Self {
            now,
            settings,
            history,
            forecast,
            clears_at,
        }
    }
    /// The exact curve has slopes 0 or -1 millisecond/millisecond after union.
    /// Queries are clamped to the displayed future window, whose end is zero.
    pub fn usage_after(&self, offset: u64) -> u64 {
        let at = self.now.millis() + offset.min(self.settings.window.millis()) as i64;
        self.forecast
            .windows(2)
            .find_map(|pair| match pair {
                [a, b] if at <= b.at => Some(a.used.saturating_sub(
                    ((at - a.at).max(0) as u64).min(a.used.saturating_sub(b.used)),
                )),
                _ => None,
            })
            .unwrap_or(0)
    }
    pub fn used_now(&self) -> u64 {
        self.forecast.first().map(|point| point.used).unwrap_or(0)
    }
    pub fn available(&self, used: u64) -> Option<u64> {
        self.settings
            .limit
            .map(|limit| limit.millis().saturating_sub(used))
    }
}
