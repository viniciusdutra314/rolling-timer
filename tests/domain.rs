#![allow(clippy::expect_used)]
use proptest::prelude::*;
use rolling_timer::domain::*;
use std::time::{Duration, SystemTime};
fn t(ms: i64) -> Timestamp {
    Timestamp::new(ms).expect("test timestamp")
}
fn w(ms: u64) -> Window {
    Window::new(ms).expect("test window")
}
fn i(start: i64, end: i64) -> Interval {
    Interval::new(t(start), t(end)).expect("test interval")
}

#[test]
fn sliding_window_boundaries() {
    let records = [i(0, 10), i(20, 40), i(50, 70), i(100, 120)];
    assert_eq!(usage(records, t(60), w(40)), 30);
    assert_eq!(usage(records, t(80), w(40)), 20);
    assert_eq!(usage(records, t(110), w(40)), 10);
    assert_eq!(usage(records, t(200), w(40)), 0);
    assert_eq!(usage([i(0, 10)], t(10), w(10)), 10);
    assert_eq!(usage([i(0, 10)], t(20), w(10)), 0);
    assert_eq!(usage([i(10, 20)], t(10), w(10)), 0);
}
#[test]
fn overlapping_legacy_records_are_a_union() {
    assert_eq!(
        usage(
            [i(5, 30), i(0, 10), i(10, 20), i(5, 30), i(7, 8)],
            t(40),
            w(40)
        ),
        30
    );
    assert_eq!(usage([i(0, 0), i(10, 10)], t(10), w(10)), 0);
    assert_eq!(usage([], t(0), w(1)), 0);
}
#[test]
fn rejects_invalid_states() {
    for invalid in [
        "",
        " ",
        "NaN",
        "inf",
        "-inf",
        "0",
        "-1",
        "1e100",
        "1e-20",
        "hours",
        "0.00000000001",
    ] {
        assert!(Window::parse_hours(invalid).is_err(), "{invalid}");
    }
    assert!(Window::new(0).is_err());
    assert!(Window::new(u64::MAX).is_err());
    assert!(Timestamp::new(-1).is_err());
    assert!(Timestamp::new(i64::MAX).is_err());
    assert!(Interval::new(t(2), t(1)).is_err());
}
#[test]
fn decimal_hours_and_formatting() {
    assert_eq!(
        Window::parse_hours(" 1.25 ").expect("hours").millis(),
        4_500_000
    );
    assert_eq!(Window::parse_hours("1e-3").expect("hours").millis(), 3600);
    assert_eq!(Window::default().hours_text(), "24");
    assert_eq!(format_duration(999), "00:00:00");
    assert_eq!(format_duration(3_661_999), "01:01:01");
    assert_eq!(format_duration(360_000_000), "100:00:00");
    assert_eq!(format_duration(u64::MAX), "5124095576030:25:51");
}
#[test]
fn timestamp_range_and_system_clock() {
    assert_eq!(
        Timestamp::from_system_time(SystemTime::UNIX_EPOCH).expect("epoch"),
        t(0)
    );
    assert!(Timestamp::from_system_time(SystemTime::UNIX_EPOCH - Duration::from_secs(1)).is_err());
    assert_eq!(
        Timestamp::from_system_time(
            SystemTime::UNIX_EPOCH + Duration::from_millis(MAX_MILLIS as u64)
        )
        .expect("max"),
        t(MAX_MILLIS)
    );
    assert!(
        Timestamp::from_system_time(
            SystemTime::UNIX_EPOCH + Duration::from_millis(MAX_MILLIS as u64 + 1)
        )
        .is_err()
    );
    assert_eq!(
        usage([i(0, MAX_MILLIS)], t(MAX_MILLIS), w(MAX_MILLIS as u64)),
        MAX_MILLIS as u64
    );
}
fn property_config() -> ProptestConfig {
    let mut config = ProptestConfig::default();
    if cfg!(miri) {
        // Miri isolates the host filesystem. Failure persistence would call
        // getcwd before testing any application code; use a fixed seed instead.
        config.failure_persistence = None;
        config.cases = 8;
        config.rng_seed = proptest::test_runner::RngSeed::Fixed(42);
    }
    config
}
proptest! {
    #![proptest_config(property_config())]
    #[test]
    fn agrees_with_independent_discrete_oracle(raw in prop::collection::vec((0_i64..100, 0_i64..100), 0..30), now in 0_i64..150, window in 1_u64..150) {
        let intervals: Vec<_> = raw.iter().map(|&(a,b)| i(a.min(b), a.max(b))).collect();
        // Enumerate individual milliseconds instead of merging intervals: a
        // deliberately different algorithm from the production implementation.
        let expected = (0..now).filter(|&point| point >= now.saturating_sub(window as i64)
            && raw.iter().any(|&(a,b)| point >= a.min(b) && point < a.max(b))).count() as u64;
        prop_assert_eq!(usage(intervals, t(now), w(window)), expected);
    }
    #[test]
    fn splitting_and_duplicates_preserve_usage(a in 0_i64..1_000_000, length in 0_i64..1_000_000, split in 0_i64..1_000_000, now in 0_i64..2_000_000, window in 1_u64..2_000_000) {
        let end = a + length;
        let middle = a + split.min(length);
        let whole = usage([i(a,end)], t(now), w(window));
        prop_assert_eq!(whole, usage([i(middle,end),i(a,middle)], t(now), w(window)));
        prop_assert_eq!(whole, usage([i(a,end),i(a,end)], t(now), w(window)));
        prop_assert!(whole <= window);
    }
    #[test]
    fn stopped_usage_never_grows_after_last_end(a in 0_i64..1_000_000, length in 0_i64..1_000_000, elapsed in 0_i64..1_000_000, window in 1_u64..1_000_000) {
        let session = i(a,a+length);
        prop_assert!(usage([session], t(a+length+elapsed), w(window)) <= usage([session], t(a+length), w(window)));
    }
    #[test]
    fn larger_windows_never_lose_usage(a in 0_i64..1_000_000, b in 0_i64..1_000_000, now in 0_i64..2_000_000, small in 1_u64..1_000_000, extra in 0_u64..1_000_000) {
        let session = i(a.min(b),a.max(b));
        prop_assert!(usage([session], t(now), w(small)) <= usage([session], t(now), w(small+extra)));
    }
    #[test]
    fn parsing_arbitrary_input_is_fallible(text in ".{0,100}") {
        if let Ok(window) = Window::parse_hours(&text) {
            prop_assert!(window.millis() > 0 && window.millis() <= MAX_MILLIS as u64);
            prop_assert_eq!(Window::parse_hours(&window.hours_text()).expect("roundtrip"), window);
        }
    }
}
