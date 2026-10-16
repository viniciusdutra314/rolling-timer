#![allow(clippy::expect_used)]
use chrono::{
    DateTime, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc,
};
use rolling_timer::chart_axes::*;
fn ms(text: &str) -> i64 {
    DateTime::parse_from_rfc3339(text)
        .expect("fixture date")
        .timestamp_millis()
}
fn offset(seconds: i32) -> FixedOffset {
    FixedOffset::east_opt(seconds).expect("fixture offset")
}
#[test]
fn integer_scales_round_up_to_readable_steps() {
    assert_eq!(minute_scale(22 * 60_000 + 18_000), (25, 5));
    assert_eq!(minute_scale(180 * 60_000), (200, 50));
    assert_eq!(minute_scale(0), (1, 1));
    for value in [
        1,
        30_000,
        60_000,
        600_001,
        3_600_000,
        253_402_300_799_999,
        u64::MAX,
    ] {
        let (top, step) = minute_scale(value);
        assert!(top >= value.div_ceil(60_000));
        assert!(step >= 1 && top % step == 0 && top / step <= 5);
    }
}
#[test]
fn absolute_labels_cross_dates_in_the_injected_zone() {
    let at = ms("2026-10-16T01:30:00Z");
    assert_eq!(date_label(at, &offset(-3 * 3600), false), "15/10\n22:30");
    assert_eq!(
        date_label(at, &offset(19_800), true),
        "16/10/2026 07:00 +05:30"
    );
    assert_eq!(date_label(i64::MAX, &Utc, true), "Date unavailable");
    let ticks = time_ticks(
        ms("2026-10-16T00:00:00Z"),
        ms("2026-10-17T00:00:00Z"),
        &offset(19_800),
    );
    assert_eq!(ticks.len(), 24);
    assert_eq!(ticks.first().copied(), Some(ms("2026-10-16T00:30:00Z")));
    assert!(ticks.iter().all(|&at| {
        offset(19_800)
            .timestamp_millis_opt(at)
            .single()
            .expect("date")
            .minute()
            == 0
    }));
}
/// A deterministic zone with one forward and one backward jump. The dates are
/// fixtures, not an assumption about the developer machine or real-world laws.
#[derive(Clone)]
struct Seasonal;
impl TimeZone for Seasonal {
    type Offset = FixedOffset;
    fn from_offset(_: &FixedOffset) -> Self {
        Self
    }
    fn offset_from_utc_datetime(&self, date: &NaiveDateTime) -> FixedOffset {
        let at = date.and_utc().timestamp_millis();
        offset(
            if (ms("2026-03-29T01:00:00Z")..ms("2026-10-25T01:00:00Z")).contains(&at) {
                3600
            } else {
                0
            },
        )
    }
    fn offset_from_utc_date(&self, date: &NaiveDate) -> FixedOffset {
        self.offset_from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight"))
    }
    fn offset_from_local_datetime(&self, date: &NaiveDateTime) -> LocalResult<FixedOffset> {
        let valid = |seconds| {
            let candidate = date
                .checked_sub_signed(chrono::TimeDelta::seconds(seconds as i64))
                .expect("fixture date range");
            self.offset_from_utc_datetime(&candidate).local_minus_utc() == seconds
        };
        match (valid(0), valid(3600)) {
            (true, true) => LocalResult::Ambiguous(offset(3600), offset(0)),
            (true, false) => LocalResult::Single(offset(0)),
            (false, true) => LocalResult::Single(offset(3600)),
            (false, false) => LocalResult::None,
        }
    }
    fn offset_from_local_date(&self, date: &NaiveDate) -> LocalResult<FixedOffset> {
        self.offset_from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight"))
    }
}
#[test]
fn hour_grids_skip_nonexistent_hours_and_keep_both_repeated_hours() {
    let spring = time_ticks(
        ms("2026-03-29T00:00:00Z"),
        ms("2026-03-29T06:00:00Z"),
        &Seasonal,
    );
    assert!(
        !spring
            .iter()
            .any(|&at| date_label(at, &Seasonal, false).ends_with("01:00"))
    );
    assert!(
        spring
            .iter()
            .any(|&at| date_label(at, &Seasonal, false).ends_with("02:00"))
    );
    let fall = time_ticks(
        ms("2026-10-25T00:00:00Z"),
        ms("2026-10-25T06:00:00Z"),
        &Seasonal,
    );
    let repeated: Vec<_> = fall
        .iter()
        .filter(|&&at| date_label(at, &Seasonal, false).ends_with("01:00"))
        .copied()
        .collect();
    assert_eq!(
        repeated,
        vec![ms("2026-10-25T00:00:00Z"), ms("2026-10-25T01:00:00Z")]
    );
    assert!(
        date_label(repeated.first().copied().expect("first"), &Seasonal, true).ends_with("+01:00")
    );
    assert!(
        date_label(repeated.last().copied().expect("last"), &Seasonal, true).ends_with("+00:00")
    );
}
#[test]
fn short_windows_crossing_a_fold_include_the_repeated_clock_boundary() {
    let ticks = time_ticks(
        ms("2026-10-25T00:55:00Z"),
        ms("2026-10-25T01:05:00Z"),
        &Seasonal,
    );
    assert!(ticks.contains(&ms("2026-10-25T01:00:00Z")));
    assert!(
        ticks
            .iter()
            .all(|&at| at >= ms("2026-10-25T00:55:00Z") && at <= ms("2026-10-25T01:05:00Z"))
    );
}
#[test]
fn huge_and_empty_ranges_have_bounded_ticks() {
    assert!(time_ticks(i64::MAX, i64::MAX, &Utc).is_empty());
    assert!(time_ticks(0, 253_402_300_799_999, &Utc).len() <= 53);
    assert!(time_ticks(0, i64::MAX, &Utc).len() <= 53);
    assert!(time_ticks(0, 0, &Utc).len() <= 1);
}
