#![allow(clippy::expect_used)]
use rolling_timer::{
    Error, Result,
    domain::*,
    engine::{Engine, SessionId, Store},
    storage::*,
};
use rusqlite::{Connection, params};
use std::fs;
fn t(ms: i64) -> Timestamp {
    Timestamp::new(ms).expect("test time")
}
fn legacy(dir: &std::path::Path, start: &str, end: &str) -> Result<()> {
    let connection = Connection::open(dir.join("time_intervals.sqlite3"))?;
    connection.execute_batch(
        "CREATE TABLE time_intervals(start_time TEXT NOT NULL,end_time TEXT NOT NULL);",
    )?;
    connection.execute(
        "INSERT INTO time_intervals VALUES(?1,?2)",
        params![start, end],
    )?;
    Ok(())
}
#[test]
fn roundtrip_settings_and_sessions() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let settings = Settings {
        window: Window::parse_hours("0.5")?,
        limit: Some(Window::parse_hours("0.1")?),
    };
    {
        let mut store = SqliteStore::open(dir.path())?;
        assert_eq!(store.settings()?, Settings::default());
        store.save_settings(settings)?;
        let id = store.start(t(1000))?;
        store.checkpoint(id, t(2000))?;
        store.checkpoint(id, t(3000))?;
        assert!(store.checkpoint(id, t(2999)).is_err());
        assert!(store.checkpoint(SessionId::new(999)?, t(4000)).is_err());
        assert!(matches!(store.start(t(2999)), Err(Error::ClockBackwards)));
    }
    let store = SqliteStore::open(dir.path())?;
    assert_eq!(store.settings()?, settings);
    let intervals = store.intervals(t(5000), settings)?;
    assert_eq!(usage(intervals, t(5000), settings.window), 2000);
    assert!(store.intervals(t(10_000_000), settings)?.is_empty());
    Ok(())
}
#[test]
fn instance_lock_released_on_drop() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let first = SqliteStore::open(dir.path())?;
    assert!(matches!(
        SqliteStore::open(dir.path()),
        Err(Error::AlreadyRunning)
    ));
    drop(first);
    let _second = SqliteStore::open(dir.path())?;
    let other = tempfile::tempdir()?;
    let _independent = SqliteStore::open(other.path())?;
    Ok(())
}
#[test]
fn python_import_is_atomic_idempotent_and_retains_originals() -> Result<()> {
    let dir = tempfile::tempdir()?;
    legacy(
        dir.path(),
        "2026-01-01T01:00:00.123456+01:00",
        "2026-01-01T00:00:10.123456+00:00",
    )?;
    let config = "# prototype\nsliding_window_hours = 1.5\n";
    fs::write(dir.path().join("config.toml"), config)?;
    for _ in 0..2 {
        let store = SqliteStore::open(dir.path())?;
        let settings = store.settings()?;
        assert_eq!(settings.window, Window::parse_hours("1.5")?);
        let now = parse_legacy_timestamp("2026-01-01T00:00:20Z")?;
        assert_eq!(
            usage(store.intervals(now, settings)?, now, settings.window),
            10_000
        );
        assert_eq!(fs::read_to_string(dir.path().join("config.toml"))?, config);
    }
    let connection = Connection::open(dir.path().join("time_intervals.sqlite3"))?;
    assert_eq!(
        connection.query_row("SELECT COUNT(*) FROM time_intervals", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    assert_eq!(
        connection.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get::<_, i64>(0))?,
        1
    );
    Ok(())
}
#[test]
fn invalid_legacy_data_rolls_back_and_can_be_repaired() -> Result<()> {
    let dir = tempfile::tempdir()?;
    legacy(dir.path(), "not a date", "2026-01-01T00:00:00Z")?;
    assert!(SqliteStore::open(dir.path()).is_err());
    let connection = Connection::open(dir.path().join("time_intervals.sqlite3"))?;
    assert_eq!(
        connection.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
        0
    );
    assert_eq!(
        connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='sessions'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        0
    );
    connection.execute("UPDATE time_intervals SET start_time=end_time", [])?;
    drop(connection);
    let _store = SqliteStore::open(dir.path())?;
    Ok(())
}
#[test]
fn invalid_config_rolls_back_without_overwriting() -> Result<()> {
    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("config.toml"), "sliding_window_hours = -1")?;
    assert!(SqliteStore::open(dir.path()).is_err());
    fs::write(dir.path().join("config.toml"), "sliding_window_hours = 48")?;
    assert_eq!(
        SqliteStore::open(dir.path())?.settings()?.window,
        Window::parse_hours("48")?
    );
    Ok(())
}
#[test]
fn corruption_newer_schema_and_invalid_directory_are_errors() -> Result<()> {
    let dir = tempfile::tempdir()?;
    fs::write(
        dir.path().join("time_intervals.sqlite3"),
        "corrupt database",
    )?;
    assert!(SqliteStore::open(dir.path()).is_err());
    let file = dir.path().join("file");
    fs::write(&file, "not a directory")?;
    assert!(SqliteStore::open(&file).is_err());
    let newer = tempfile::tempdir()?;
    Connection::open(newer.path().join("time_intervals.sqlite3"))?.pragma_update(
        None,
        "user_version",
        99,
    )?;
    assert!(SqliteStore::open(newer.path()).is_err());
    Ok(())
}
#[test]
fn parsers_validate_legacy_boundaries() -> Result<()> {
    assert_eq!(parse_legacy_timestamp("1970-01-01T00:00:00Z")?, t(0));
    for bad in ["2020-01-01T00:00:00", "garbage", "1969-12-31T23:59:59Z"] {
        assert!(parse_legacy_timestamp(bad).is_err());
    }
    for bad in [
        "=",
        "sliding_window_hours = '24'",
        "sliding_window_hours = nan",
        "sliding_window_hours = 0",
    ] {
        assert!(parse_legacy_config(bad).is_err());
    }
    assert_eq!(
        parse_legacy_config("sliding_window_hours = 24")?,
        Window::default()
    );
    Ok(())
}
#[test]
fn checkpoints_survive_abrupt_end_without_counting_downtime() -> Result<()> {
    let dir = tempfile::tempdir()?;
    {
        let mut engine = Engine::new(SqliteStore::open(dir.path())?)?;
        engine.toggle(t(1000))?;
        assert_eq!(engine.tick(t(5000))?.used, 4000);
        // Deliberately do not call close: model process death after checkpoint.
    }
    let mut engine = Engine::new(SqliteStore::open(dir.path())?)?;
    assert!(!engine.recording());
    assert_eq!(engine.tick(t(10_000))?.used, 4000);
    engine.toggle(t(10_000))?;
    engine.close(t(12_000))?;
    assert_eq!(engine.tick(t(12_000))?.used, 6000);
    Ok(())
}

#[test]
fn unreadable_config_is_reported_and_lock_released() -> Result<()> {
    let dir = tempfile::tempdir()?;
    fs::create_dir(dir.path().join("config.toml"))?;
    assert!(matches!(SqliteStore::open(dir.path()), Err(Error::Io(_))));
    fs::remove_dir(dir.path().join("config.toml"))?;
    fs::write(dir.path().join("config.toml"), [0xff, 0xfe])?;
    assert!(matches!(SqliteStore::open(dir.path()), Err(Error::Io(_))));
    fs::remove_file(dir.path().join("config.toml"))?;
    let _store = SqliteStore::open(dir.path())?;
    Ok(())
}
#[test]
fn external_database_damage_is_rejected_by_typed_boundaries() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut store = SqliteStore::open(dir.path())?;
    let connection = Connection::open(dir.path().join("time_intervals.sqlite3"))?;
    connection.execute("DELETE FROM settings", [])?;
    assert!(store.settings().is_err());
    assert!(store.save_settings(Settings::default()).is_err());
    connection.pragma_update(None, "ignore_check_constraints", true)?;
    connection.execute("INSERT INTO settings VALUES(1,0,NULL)", [])?;
    assert!(store.settings().is_err());
    connection.execute("UPDATE settings SET window_ms=1000,limit_ms=0", [])?;
    assert!(store.settings().is_err());
    connection.execute("INSERT INTO sessions(start_ms,end_ms) VALUES(-1,100)", [])?;
    assert!(store.intervals(t(1000), Settings::default()).is_err());
    connection.execute("DELETE FROM sessions", [])?;
    connection.execute("INSERT INTO sessions(start_ms,end_ms) VALUES(200,100)", [])?;
    assert!(store.intervals(t(1000), Settings::default()).is_err());
    Ok(())
}
#[test]
fn sqlite_constraints_enforce_domain_ranges() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let _store = SqliteStore::open(dir.path())?;
    let connection = Connection::open(dir.path().join("time_intervals.sqlite3"))?;
    for sql in [
        "INSERT INTO sessions(start_ms,end_ms) VALUES(-1,1)",
        "INSERT INTO sessions(start_ms,end_ms) VALUES(5,4)",
        "UPDATE settings SET window_ms=0",
        "UPDATE settings SET limit_ms=-1",
        "INSERT INTO settings VALUES(2,1,NULL)",
    ] {
        assert!(connection.execute(sql, []).is_err(), "{sql}");
    }
    Ok(())
}

#[test]
fn database_write_errors_are_propagated_without_partial_changes() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut store = SqliteStore::open(dir.path())?;
    let connection = Connection::open(dir.path().join("time_intervals.sqlite3"))?;
    connection.execute_batch("CREATE TRIGGER fail_settings BEFORE UPDATE ON settings BEGIN SELECT RAISE(ABORT,'test write failure'); END;")?;
    assert!(matches!(
        store.save_settings(Settings::default()),
        Err(Error::Database(_))
    ));
    connection.execute_batch("CREATE TRIGGER fail_start BEFORE INSERT ON sessions BEGIN SELECT RAISE(ABORT,'test write failure'); END;")?;
    assert!(matches!(store.start(t(1000)), Err(Error::Database(_))));
    connection.execute_batch("DROP TRIGGER fail_start;")?;
    let id = store.start(t(1000))?;
    connection.execute_batch("CREATE TRIGGER fail_checkpoint BEFORE UPDATE ON sessions BEGIN SELECT RAISE(ABORT,'test write failure'); END;")?;
    assert!(matches!(
        store.checkpoint(id, t(2000)),
        Err(Error::Database(_))
    ));
    assert_eq!(
        store
            .intervals(t(2000), Settings::default())?
            .first()
            .map(|i| i.end()),
        Some(t(1000))
    );
    Ok(())
}
#[test]
fn reversed_legacy_session_does_not_migrate() -> Result<()> {
    let dir = tempfile::tempdir()?;
    legacy(dir.path(), "2026-01-01T00:00:10Z", "2026-01-01T00:00:00Z")?;
    assert!(SqliteStore::open(dir.path()).is_err());
    Ok(())
}
#[test]
fn environment_home_and_lock_errors_have_actionable_messages() -> Result<()> {
    assert!(default_directory()?.ends_with(".rolling_timer"));
    let io = Error::from(std::fs::TryLockError::Error(std::io::Error::from(
        std::io::ErrorKind::PermissionDenied,
    )));
    assert!(io.to_string().contains("File error"));
    assert!(
        Error::from(std::fs::TryLockError::WouldBlock)
            .to_string()
            .contains("Another")
    );
    Ok(())
}

#[test]
fn unknown_negative_schema_is_rejected() -> Result<()> {
    let directory = tempfile::tempdir()?;
    Connection::open(directory.path().join("time_intervals.sqlite3"))?.pragma_update(
        None,
        "user_version",
        -1,
    )?;
    assert!(SqliteStore::open(directory.path()).is_err());
    Ok(())
}
