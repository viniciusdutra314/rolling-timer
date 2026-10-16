//! Presentation arithmetic, independent of FLTK and of the host's timezone.
//! Production injects chrono::Local; tests inject zones with known transitions.
use chrono::{DateTime, LocalResult, Offset, TimeZone};

/// Integer minute ticks with familiar 1/2/5 multiples. Leave headroom so a
/// maximum such as 22.3 minutes produces 0, 5, 10, 15, 20, 25, never 11.15.
pub fn minute_scale(maximum_ms: u64) -> (u64, u64) {
    let minutes = maximum_ms.div_ceil(60_000).max(1);
    let target = minutes.div_ceil(5);
    let mut magnitude = 1_u64;
    while magnitude.saturating_mul(10) < target {
        magnitude *= 10;
    }
    let step = [1, 2, 5, 10]
        .into_iter()
        .map(|n| n * magnitude)
        .find(|&step| step >= target)
        .unwrap_or(magnitude);
    (minutes.div_ceil(step) * step, step)
}

pub fn date_label<Tz: TimeZone>(at: i64, zone: &Tz, detailed: bool) -> String
where
    Tz::Offset: std::fmt::Display,
{
    DateTime::from_timestamp_millis(at)
        .map(|date| {
            let date = date.with_timezone(zone);
            date.format(if detailed {
                "%d/%m/%Y %H:%M %:z"
            } else {
                "%d/%m\n%H:%M"
            })
            .to_string()
        })
        .unwrap_or_else(|| "Date unavailable".into())
}

/// Grid lines follow local civil boundaries. A repeated DST hour contributes
/// both instants; a missing hour contributes neither. Iteration stays bounded
/// even for the largest supported window, rather than allocating hourly ticks
/// across thousands of years.
pub fn time_ticks<Tz: TimeZone>(start: i64, end: i64, zone: &Tz) -> Vec<i64> {
    let span = end.saturating_sub(start).max(1);
    let minimum = (span as u64 / 48).max(if span >= 21_600_000 {
        3_600_000
    } else {
        60_000
    });
    let step = [
        60_000, 300_000, 900_000, 1_800_000, 3_600_000, 7_200_000, 10_800_000, 21_600_000,
        43_200_000, 86_400_000,
    ]
    .into_iter()
    .find(|&step| step >= minimum)
    .unwrap_or_else(|| minimum.div_ceil(86_400_000) * 86_400_000) as i64;
    let Some(date) = DateTime::from_timestamp_millis(start) else {
        return Vec::new();
    };
    let local_date = date.with_timezone(zone);
    let local_start = local_date.naive_local().and_utc().timestamp_millis();
    let shift = DateTime::from_timestamp_millis(end)
        .map(|end| {
            i64::from(
                end.with_timezone(zone).offset().fix().local_minus_utc()
                    - local_date.offset().fix().local_minus_utc(),
            ) * 1000
        })
        .unwrap_or(0);
    // A short window can start in the first occurrence of 01:55 and end in
    // the repeated 01:05. Look back through the fold instead of skipping its
    // 01:00 boundary. Forward date-line shifts need extra civil candidates.
    let floor = (local_start + shift.min(0)).div_euclid(step) * step;
    let candidates = 53 + shift.unsigned_abs().div_ceil(step as u64);
    let mut ticks = Vec::new();
    // Extra candidates cover UTC-offset shifts around civil-day transitions.
    for index in 0..candidates as i64 {
        let Some(candidate) = index
            .checked_mul(step)
            .and_then(|offset| floor.checked_add(offset))
            .and_then(DateTime::from_timestamp_millis)
        else {
            break;
        };
        let mut include = |date: DateTime<Tz>| {
            let at = date.timestamp_millis();
            if at >= start && at <= end {
                ticks.push(at);
            }
        };
        match zone.from_local_datetime(&candidate.naive_utc()) {
            LocalResult::Single(date) => include(date),
            LocalResult::Ambiguous(first, second) => {
                include(first);
                include(second);
            }
            LocalResult::None => {}
        }
    }
    ticks.sort_unstable();
    ticks.dedup();
    ticks
}
