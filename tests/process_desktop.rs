#![cfg(all(feature = "gui", target_os = "linux"))]
//! Drive the actual executable through X11 input, then inspect the durable DB.
//! Every subprocess is bounded and cleaned up even if an assertion fails.
use rolling_timer::{Error, Result};
use rusqlite::Connection;
use std::{
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn executable() -> std::path::PathBuf {
    std::env::var_os("ROLLING_TIMER_TEST_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_rolling-timer").into())
}
fn xdo(args: &[&str]) -> Result<String> {
    let output = Command::new("xdotool").args(args).output()?;
    if !output.status.success() {
        return Err(Error::Desktop(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
fn wait_until(stage: &str, mut condition: impl FnMut() -> bool) -> Result<()> {
    let start = Instant::now();
    while !condition() {
        if start.elapsed() > Duration::from_secs(10) {
            return Err(Error::Desktop(format!("Desktop test timed out: {stage}")));
        }
        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
#[test]
#[ignore = "requires X11 and xdotool: just gui-e2e"]
fn desktop_process_accepts_real_input_and_saves_on_close() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut child = Process(
        Command::new(executable())
            .arg("--data-dir")
            .arg(dir.path())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    let pid = child.0.id().to_string();
    let mut window = String::new();
    wait_until("visible process window", || {
        window = xdo(&[
            "search",
            "--all",
            "--onlyvisible",
            "--pid",
            &pid,
            "--name",
            "^Rolling Timer$",
        ])
        .unwrap_or_default();
        !window.is_empty()
    })?;
    xdo(&["windowfocus", &window])?;
    xdo(&["key", "Escape"])?;
    thread::sleep(Duration::from_millis(100));
    assert!(
        child.0.try_wait()?.is_none(),
        "Escape must not discard an active window"
    );
    xdo(&["mousemove", "--window", &window, "210", "200", "click", "1"])?;
    let database = dir.path().join("time_intervals.sqlite3");
    let connection = Connection::open(database)?;
    let saved_length = || -> rusqlite::Result<i64> {
        connection.query_row(
            "SELECT COALESCE(MAX(end_ms-start_ms),0) FROM sessions",
            [],
            |row| row.get(0),
        )
    };
    wait_until("recording checkpoint", || {
        saved_length().is_ok_and(|length| length >= 1000)
    })?;
    // SQLite FAIL preserves earlier trigger changes. The marker proves that
    // the idle checkpoint reached a genuine database failure, rather than just
    // sleeping and hoping a timer fired. The app must stay available for retry.
    connection.execute_batch("CREATE TABLE fault_seen(id INTEGER); CREATE TRIGGER fail_checkpoint BEFORE UPDATE ON sessions BEGIN INSERT INTO fault_seen VALUES(1); SELECT RAISE(FAIL,'injected checkpoint failure'); END;")?;
    wait_until("failed checkpoint reported", || {
        connection
            .query_row("SELECT COUNT(*) FROM fault_seen", [], |row| {
                row.get::<_, i64>(0)
            })
            .is_ok_and(|count| count > 0)
    })?;
    assert!(child.0.try_wait()?.is_none());
    let length_before_retry = saved_length()?;
    connection.execute_batch("DROP TRIGGER fail_checkpoint;")?;
    wait_until("checkpoints resume after storage recovers", || {
        saved_length().is_ok_and(|length| length > length_before_retry)
    })?;
    // Exercise the event-loop validation failure too, then correct the field.
    xdo(&[
        "mousemove",
        "--window",
        &window,
        "215",
        "254",
        "click",
        "1",
        "key",
        "ctrl+a",
    ])?;
    xdo(&["type", "--clearmodifiers", "NaN"])?;
    xdo(&["key", "Return"])?;
    thread::sleep(Duration::from_millis(100));
    assert_eq!(
        connection.query_row("SELECT window_ms FROM settings WHERE id=1", [], |r| r
            .get::<_, i64>(0))?,
        86_400_000
    );
    // Change the window through the actual text field and Enter callback.
    xdo(&[
        "mousemove",
        "--window",
        &window,
        "215",
        "254",
        "click",
        "1",
        "key",
        "ctrl+a",
    ])?;
    xdo(&["type", "--clearmodifiers", "2"])?;
    xdo(&["key", "Return"])?;
    wait_until("settings persisted", || {
        connection
            .query_row("SELECT window_ms FROM settings WHERE id=1", [], |r| {
                r.get::<_, i64>(0)
            })
            .is_ok_and(|value| value == 7_200_000)
    })?;
    // Set an optional quota through its own input; it must reach SQLite too.
    xdo(&[
        "mousemove",
        "--window",
        &window,
        "215",
        "292",
        "click",
        "1",
        "key",
        "ctrl+a",
    ])?;
    xdo(&["type", "--clearmodifiers", "1"])?;
    xdo(&["key", "Return"])?;
    wait_until("quota persisted", || {
        connection
            .query_row("SELECT limit_ms FROM settings WHERE id=1", [], |r| {
                r.get::<_, i64>(0)
            })
            .is_ok_and(|value| value == 3_600_000)
    })?;
    // Choose a theme through the actual popup, then inspect its TOML entry.
    xdo(&["mousemove", "--window", &window, "280", "334", "click", "1"])?;
    xdo(&["key", "Down", "Down", "Down", "Return"])?;
    let config = dir.path().join("config.toml");
    wait_until("theme persisted", || {
        rolling_timer::preferences::load_theme(&config)
            .is_ok_and(|theme| theme == rolling_timer::preferences::Theme::Cappuccino)
    })?;
    xdo(&["mousemove", "--window", &window, "210", "380", "click", "1"])?;
    let mut chart_window = String::new();
    wait_until("history and forecast window", || {
        chart_window = xdo(&[
            "search",
            "--all",
            "--onlyvisible",
            "--pid",
            &pid,
            "--name",
            "^Usage history and forecast$",
        ])
        .unwrap_or_default();
        !chart_window.is_empty()
    })?;
    xdo(&["windowfocus", &chart_window])?;
    xdo(&[
        "mousemove",
        "--window",
        &chart_window,
        "600",
        "509",
        "mousedown",
        "1",
        "mousemove",
        "--window",
        &chart_window,
        "650",
        "520",
        "mouseup",
        "1",
    ])?;
    // Inspect a historical bar too, then leave both plots to clear the crosshair.
    xdo(&["mousemove", "--window", &chart_window, "250", "160"])?;
    xdo(&["mousemove", "1010", "740"])?;
    xdo(&["windowfocus", &chart_window, "key", "Escape"])?;
    wait_until("chart closed independently", || {
        xdo(&[
            "search",
            "--all",
            "--onlyvisible",
            "--pid",
            &pid,
            "--name",
            "^Usage history and forecast$",
        ])
        .unwrap_or_default()
        .is_empty()
    })?;
    assert!(child.0.try_wait()?.is_none());
    xdo(&["windowfocus", &window])?;
    // Close while still recording: this must perform the final checkpoint.
    xdo(&["key", "ctrl+q"])?;
    wait_until("graceful process exit", || {
        child.0.try_wait().is_ok_and(|status| status.is_some())
    })?;
    assert!(child.0.wait()?.success());
    assert!(saved_length()? >= 1000);
    assert_eq!(
        connection.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get::<_, i64>(0))?,
        1
    );
    // Reopen the same profile: the persisted preference is loaded on startup.
    let mut reopened = Process(
        Command::new(executable())
            .arg("--data-dir")
            .arg(dir.path())
            .spawn()?,
    );
    let reopened_pid = reopened.0.id().to_string();
    let mut reopened_window = String::new();
    wait_until("reopened themed window", || {
        reopened_window = xdo(&[
            "search",
            "--all",
            "--onlyvisible",
            "--pid",
            &reopened_pid,
            "--name",
            "^Rolling Timer$",
        ])
        .unwrap_or_default();
        !reopened_window.is_empty()
    })?;
    xdo(&[
        "mousemove",
        "--window",
        &reopened_window,
        "210",
        "380",
        "click",
        "1",
    ])?;
    let mut reopened_chart = String::new();
    wait_until("reopened chart window", || {
        reopened_chart = xdo(&[
            "search",
            "--all",
            "--onlyvisible",
            "--pid",
            &reopened_pid,
            "--name",
            "^Usage history and forecast$",
        ])
        .unwrap_or_default();
        !reopened_chart.is_empty()
    })?;
    xdo(&["windowfocus", &reopened_chart, "key", "ctrl+q"])?;
    wait_until("quit through chart window", || {
        reopened.0.try_wait().is_ok_and(|status| status.is_some())
    })?;
    assert!(reopened.0.wait()?.success());
    // A desktop launch must show startup failures, then exit with failure after
    // the user dismisses the dialog. This process has its own FLTK state.
    let invalid_directory = dir.path().join("file");
    std::fs::write(&invalid_directory, "not a directory")?;
    let mut failed = Process(
        Command::new(executable())
            .arg("--data-dir")
            .arg(&invalid_directory)
            .stderr(Stdio::null())
            .spawn()?,
    );
    let failed_pid = failed.0.id().to_string();
    let mut dialog = String::new();
    wait_until("startup error dialog", || {
        dialog =
            xdo(&["search", "--all", "--onlyvisible", "--pid", &failed_pid]).unwrap_or_default();
        !dialog.is_empty()
    })?;
    xdo(&["windowfocus", &dialog, "key", "Return"])?;
    wait_until("dismissed error exit", || {
        failed.0.try_wait().is_ok_and(|status| status.is_some())
    })?;
    assert!(!failed.0.wait()?.success());
    Ok(())
}
