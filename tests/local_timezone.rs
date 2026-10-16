#![cfg(all(feature = "gui", unix))]
use rolling_timer::{Result, chart_axes::date_label};
/// Each OS-zone lookup gets its own process: TZ mutation is unsafe in a threaded
/// Rust process, and Chrono caches timezone data. No shared environment mutation.
#[test]
fn os_timezone_controls_calendar_labels() -> Result<()> {
    let at = rolling_timer::storage::parse_legacy_timestamp("2026-10-16T01:30:00Z")?.millis();
    if let Ok(expected) = std::env::var("ROLLING_TIMER_EXPECT_LOCAL_DATE") {
        assert_eq!(date_label(at, &chrono::Local, true), expected);
        if expected.ends_with("+01:00") {
            use chrono::TimeZone;
            let gap_text = if std::env::var("TZ").unwrap_or_default().contains("/1:30") {
                "2026-03-29T02:00:00Z"
            } else {
                "2026-03-29T01:30:00Z"
            };
            let gap = chrono::DateTime::parse_from_rfc3339(gap_text)
                .map_err(|_| rolling_timer::Error::Invalid("Invalid date fixture"))?
                .naive_utc();
            assert!(
                matches!(
                    chrono::Local.from_local_datetime(&gap),
                    chrono::LocalResult::None
                ),
                "spring gap must not map to an instant"
            );
        }
        use rolling_timer::chart_axes::time_ticks;
        assert!(time_ticks(i64::MAX, i64::MAX, &chrono::Local).is_empty());
        assert!(time_ticks(0, i64::MAX, &chrono::Local).len() <= 53);
        for (start, end) in [
            ("2026-03-29T00:00:00Z", "2026-03-29T06:00:00Z"),
            ("2026-10-25T00:00:00Z", "2026-10-25T06:00:00Z"),
        ] {
            let start = rolling_timer::storage::parse_legacy_timestamp(start)?.millis();
            let end = rolling_timer::storage::parse_legacy_timestamp(end)?.millis();
            let ticks = time_ticks(start, end, &chrono::Local);
            assert!(!ticks.is_empty());
        }
    } else {
        for (zone, expected) in [
            ("UTC0", "16/10/2026 01:30 +00:00"),
            ("STD0DST,M3.5.0/1,M10.5.0/2", "16/10/2026 02:30 +01:00"),
            (
                "STD0DST,M3.5.0/1:30,M10.5.0/2:30",
                "16/10/2026 02:30 +01:00",
            ),
            ("<-03>3", "15/10/2026 22:30 -03:00"),
            ("IST-5:30", "16/10/2026 07:00 +05:30"),
        ] {
            // Cross builds execute guest children through the same explicit
            // QEMU runner as the existing CLI tests, without host binfmt.
            let binary = std::env::current_exe()?;
            let mut command = if let Some(runner) = std::env::var_os("ROLLING_TIMER_TEST_RUNNER") {
                let mut command = std::process::Command::new(runner);
                command.arg(binary);
                command
            } else {
                std::process::Command::new(binary)
            };
            let output = command
                .args([
                    "--exact",
                    "os_timezone_controls_calendar_labels",
                    "--nocapture",
                ])
                .env("TZ", zone)
                .env("ROLLING_TIMER_EXPECT_LOCAL_DATE", expected)
                .output()?;
            assert!(
                output.status.success(),
                "zone {zone}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    Ok(())
}
