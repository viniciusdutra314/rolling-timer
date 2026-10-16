#![allow(clippy::expect_used)]
use proptest::prelude::*;
use rolling_timer::{Result, charts::*, domain::*, engine::Engine, storage::SqliteStore};
fn t(ms: i64) -> Timestamp {
    Timestamp::new(ms).expect("timestamp")
}
fn interval(a: i64, b: i64) -> Interval {
    Interval::new(t(a), t(b)).expect("interval")
}
fn settings(window: u64, limit: Option<u64>) -> Settings {
    Settings {
        window: Window::new(window).expect("window"),
        limit: limit.map(|x| Window::new(x).expect("limit")),
    }
}
#[test]
fn history_partitions_usage_and_forecast_releases_at_exact_session_boundaries() {
    // At t=100, [10,30) starts expiring at 110, finishes at 130.
    // Overlaps [20,40) merge into one session; later [70,90) is separate.
    let timeline = Timeline::new(
        &[interval(10, 30), interval(20, 40), interval(70, 90)],
        t(100),
        settings(100, Some(60)),
    );
    assert_eq!(timeline.used_now(), 50);
    assert_eq!(
        timeline.history.iter().map(|point| point.used).sum::<u64>(),
        50
    );
    assert_eq!(timeline.clears_at, 190);
    assert_eq!(timeline.available(timeline.used_now()), Some(10));
    assert_eq!(
        timeline.forecast,
        vec![
            Point { at: 100, used: 50 },
            Point { at: 110, used: 50 },
            Point { at: 140, used: 20 },
            Point { at: 170, used: 20 },
            Point { at: 190, used: 0 },
            Point { at: 200, used: 0 }
        ]
    );
    for (offset, expected) in [
        (0, 50),
        (10, 50),
        (20, 40),
        (40, 20),
        (60, 20),
        (80, 10),
        (90, 0),
        (100, 0),
        (u64::MAX, 0),
    ] {
        assert_eq!(timeline.usage_after(offset), expected);
    }
    assert_eq!(timeline.available(100), Some(0));
}
#[test]
fn clipping_zero_usage_epoch_and_calendar_boundaries() {
    for (now, window) in [(0, 1), (1, 1), (2, 100), (MAX_MILLIS, MAX_MILLIS as u64)] {
        let empty = Timeline::new(&[], t(now), settings(window, None));
        assert_eq!(empty.used_now(), 0);
        assert_eq!(empty.available(0), None);
        assert_eq!(empty.clears_at, now);
        assert_eq!(empty.usage_after(window), 0);
        assert!(!empty.history.is_empty());
        assert!(empty.history.len() <= 48);
    }
    let clipped = Timeline::new(
        &[interval(0, 100), interval(100, 200), interval(0, 0)],
        t(50),
        settings(20, None),
    );
    assert_eq!(clipped.used_now(), 20);
    assert_eq!(clipped.clears_at, 70);
    assert_eq!(clipped.usage_after(10), 10);
    assert_eq!(
        clipped.history.iter().map(|point| point.used).sum::<u64>(),
        20
    );
    let future = Timeline::new(
        &[interval(MAX_MILLIS - 1, MAX_MILLIS)],
        t(MAX_MILLIS),
        settings(2, None),
    );
    assert_eq!(future.clears_at, MAX_MILLIS + 2);
    assert_eq!(future.usage_after(1), 1);
    assert_eq!(future.usage_after(2), 0);
}
#[test]
fn engine_chart_data_includes_active_checkpoint_and_does_not_stop_recording() -> Result<()> {
    let mut engine = Engine::new(SqliteStore::in_memory()?)?;
    engine.change_settings(settings(100, Some(30)))?;
    engine.toggle(t(10))?;
    let timeline = engine.timeline(t(50))?;
    assert_eq!(timeline.used_now(), 40);
    assert_eq!(timeline.available(40), Some(0));
    assert_eq!(timeline.clears_at, 150);
    assert!(engine.recording());
    assert!(engine.timeline(t(49)).is_err());
    assert_eq!(engine.tick(t(60))?.used, 50);
    Ok(())
}
proptest! {
    #![proptest_config(if cfg!(miri) {
        proptest::test_runner::Config {
            cases: 8, failure_persistence: None,
            rng_seed: proptest::test_runner::RngSeed::Fixed(42),
            .. proptest::test_runner::Config::default()
        }
    } else { proptest::test_runner::Config::default() })]
    #[test]
    fn forecast_matches_recomputation_and_history_is_a_partition(
        pairs in prop::collection::vec((0_i64..400,0_i64..400),0..30),
        now in 1_i64..300, window in 1_u64..200, offset in 0_u64..200,
    ) {
        let intervals: Vec<_> = pairs.into_iter().map(|(a,b)|interval(a.min(b),a.max(b))).collect();
        let settings = settings(window,None);
        let chart = Timeline::new(&intervals,t(now),settings);
        let clipped: Vec<_> = intervals.iter().filter_map(|i| {
            let start=i.start().millis(); let end=i.end().millis().min(now);
            (start<=end).then(|| interval(start,end))
        }).collect();
        let expected=usage(clipped,t(now+offset.min(window) as i64),settings.window);
        prop_assert_eq!(chart.usage_after(offset),expected);
        prop_assert_eq!(chart.history.iter().map(|point|point.used).sum::<u64>(),usage(intervals,t(now),settings.window));
        prop_assert!(chart.forecast.windows(2).all(|p| matches!(p,[a,b] if a.at<b.at && a.used>=b.used)));
        prop_assert_eq!(chart.forecast.last().map(|p|p.used),Some(0));
    }
}
