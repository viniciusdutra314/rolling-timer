//! SQLite is bundled at compile time. Migration is transactional and leaves
//! Python's table/config intact; a failed import never marks the schema migrated.
use crate::{
    Error, Result,
    domain::{Interval, Settings, Timestamp, Window},
    engine::{SessionId, Store},
};
use rusqlite::{Connection, params};
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
    time::Duration,
};

pub struct SqliteStore {
    connection: Connection,
    _lock: Option<File>,
}
impl SqliteStore {
    pub fn open(directory: &Path) -> Result<Self> {
        fs::create_dir_all(directory)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("instance.lock"))?;
        lock.try_lock().map_err(Error::from)?;
        let connection = Connection::open(directory.join("time_intervals.sqlite3"))?;
        let config = directory.join("config.toml");
        let mut store = Self::from_connection(connection, Some(&config))?;
        store._lock = Some(lock);
        Ok(store)
    }
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?, None)
    }
    /// Inject a connection and optional legacy configuration. Embedded callers
    /// own exclusivity for that connection; desktop profiles should use `open`
    /// to acquire the directory's OS lock before migration or tracking begins.
    pub fn from_connection(connection: Connection, config: Option<&Path>) -> Result<Self> {
        let mut store = Self {
            connection,
            _lock: None,
        };
        // FULL synchronous checkpoints intentionally trade a tiny amount of I/O
        // for durable session time. No background service survives app closure.
        store.connection.busy_timeout(Duration::from_secs(2))?;
        store
            .connection
            .pragma_update(None, "synchronous", "FULL")?;
        store.initialize(config)?;
        Ok(store)
    }
    fn initialize(&mut self, config: Option<&Path>) -> Result<()> {
        let transaction = self.connection.transaction()?;
        let version: i64 = transaction.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version != 0 && version != 1 {
            return Err(Error::Invalid("Unsupported database schema version"));
        }
        if version == 1 {
            return Ok(());
        }
        transaction.execute_batch(
            "
            CREATE TABLE sessions (
                id INTEGER PRIMARY KEY CHECK(id > 0),
                start_ms INTEGER NOT NULL CHECK(start_ms BETWEEN 0 AND 253402300799999),
                end_ms INTEGER NOT NULL CHECK(end_ms BETWEEN start_ms AND 253402300799999)
            );
            CREATE INDEX sessions_end ON sessions(end_ms);
            CREATE TABLE settings (
                id INTEGER PRIMARY KEY CHECK(id = 1),
                window_ms INTEGER NOT NULL CHECK(window_ms BETWEEN 1 AND 253402300799999),
                limit_ms INTEGER CHECK(limit_ms BETWEEN 1 AND 253402300799999)
            );",
        )?;
        let legacy_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='time_intervals')", [], |r| r.get(0))?;
        if legacy_exists {
            let mut read =
                transaction.prepare("SELECT start_time, end_time FROM time_intervals")?;
            let mut records = read.query([])?;
            while let Some(record) = records.next()? {
                let start: String = record.get(0)?;
                let end: String = record.get(1)?;
                let interval = Interval::new(
                    parse_legacy_timestamp(&start)?,
                    parse_legacy_timestamp(&end)?,
                )?;
                transaction.execute(
                    "INSERT INTO sessions(start_ms,end_ms) VALUES (?1,?2)",
                    params![interval.start().millis(), interval.end().millis()],
                )?;
            }
        }
        let window = match config {
            Some(path) => match fs::read_to_string(path) {
                Ok(contents) => parse_legacy_config(&contents)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Window::default(),
                Err(error) => return Err(error.into()),
            },
            None => Window::default(),
        };
        transaction.execute(
            "INSERT INTO settings(id,window_ms) VALUES(1,?1)",
            [window.millis()],
        )?;
        transaction.pragma_update(None, "user_version", 1)?;
        transaction.commit()?;
        Ok(())
    }
}
impl Store for SqliteStore {
    fn settings(&self) -> Result<Settings> {
        let (window, limit): (u64, Option<u64>) = self.connection.query_row(
            "SELECT window_ms,limit_ms FROM settings WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(Settings {
            window: Window::new(window)?,
            limit: limit.map(Window::new).transpose()?,
        })
    }
    fn save_settings(&mut self, settings: Settings) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE settings SET window_ms=?1,limit_ms=?2 WHERE id=1",
            params![settings.window.millis(), settings.limit.map(Window::millis)],
        )?;
        if changed != 1 {
            return Err(Error::Invalid("Missing settings record"));
        }
        Ok(())
    }
    fn start(&mut self, now: Timestamp) -> Result<SessionId> {
        // Avoid silently placing a new session before the end of a saved one
        // after the wall clock has moved backwards between application runs.
        let last: Option<i64> =
            self.connection
                .query_row("SELECT MAX(end_ms) FROM sessions", [], |r| r.get(0))?;
        if last.is_some_and(|last| now.millis() < last) {
            return Err(Error::ClockBackwards);
        }
        self.connection.execute(
            "INSERT INTO sessions(start_ms,end_ms) VALUES(?1,?1)",
            [now.millis()],
        )?;
        SessionId::new(self.connection.last_insert_rowid())
    }
    fn checkpoint(&mut self, id: SessionId, end: Timestamp) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE sessions SET end_ms=?1 WHERE id=?2 AND end_ms<=?1",
            params![end.millis(), id.value()],
        )?;
        if changed != 1 {
            return Err(Error::Invalid(
                "Session is missing or checkpoint moves backwards",
            ));
        }
        Ok(())
    }
    fn intervals(&self, now: Timestamp, settings: Settings) -> Result<Vec<Interval>> {
        let floor = now
            .millis()
            .saturating_sub(settings.window.millis() as i64)
            .max(0);
        let mut statement = self.connection.prepare("SELECT start_ms,end_ms FROM sessions WHERE end_ms>?1 AND start_ms<?2 ORDER BY start_ms")?;
        let mut rows = statement.query(params![floor, now.millis()])?;
        let mut intervals = Vec::new();
        while let Some(row) = rows.next()? {
            let start = Timestamp::new(row.get(0)?)?;
            let end = Timestamp::new(row.get(1)?)?;
            intervals.push(Interval::new(start, end)?);
        }
        Ok(intervals)
    }
}

pub fn parse_legacy_timestamp(text: &str) -> Result<Timestamp> {
    let datetime = chrono::DateTime::parse_from_rfc3339(text)
        .map_err(|error| Error::Legacy(format!("Invalid timestamp {text:?}: {error}")))?;
    Timestamp::new(datetime.timestamp_millis())
}
pub fn parse_legacy_config(text: &str) -> Result<Window> {
    let value = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| Error::Legacy(error.to_string()))?;
    let Some(hours) = value.get("sliding_window_hours") else {
        // Modern profiles may contain only UI preferences. The legacy window
        // remains optional, while an explicitly invalid value still aborts.
        return Ok(Window::default());
    };
    // TOML accepts integer and floating point hours, just as the prototype did.
    match hours {
        toml_edit::Item::Value(toml_edit::Value::Integer(value)) => {
            Window::parse_hours(&value.value().to_string())
        }
        toml_edit::Item::Value(toml_edit::Value::Float(value)) => {
            Window::parse_hours(&value.value().to_string())
        }
        _ => Err(Error::Legacy(
            "sliding_window_hours must be a number".into(),
        )),
    }
}

/// Keep the prototype's location on all platforms. An explicit --data-dir makes
/// removable-drive storage possible without changing the process environment.
pub fn default_directory() -> Result<std::path::PathBuf> {
    #[cfg(windows)]
    const HOME_VARIABLE: &str = "USERPROFILE";
    #[cfg(not(windows))]
    const HOME_VARIABLE: &str = "HOME";
    let home = std::env::var_os(HOME_VARIABLE)
        .filter(|value| !value.is_empty())
        .ok_or(Error::Invalid(
            "Home directory unavailable; use --data-dir PATH",
        ))?;
    Ok(std::path::PathBuf::from(home).join(".rolling_timer"))
}
