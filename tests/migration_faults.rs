//! Real SQLite authorizer failures verify transaction rollback during import.
use rolling_timer::{Error, Result, storage::SqliteStore};
use rusqlite::Connection;

use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
#[test]
fn denied_migration_inserts_roll_back_schema_and_data() -> Result<()> {
    for blocked_table in ["sessions", "settings"] {
        let dir = tempfile::tempdir()?;
        let database = dir.path().join("faults.sqlite3");
        let connection = Connection::open(&database)?;
        connection.execute_batch("CREATE TABLE time_intervals(start_time TEXT,end_time TEXT); INSERT INTO time_intervals VALUES('2026-01-01T00:00:00Z','2026-01-01T00:00:10Z');")?;
        connection.authorizer(Some(move |context: AuthContext<'_>| match context.action {
            AuthAction::Insert { table_name } if table_name == blocked_table => Authorization::Deny,
            _ => Authorization::Allow,
        }));
        assert!(matches!(
            SqliteStore::from_connection(connection, None),
            Err(Error::Database(_))
        ));
        let connection = Connection::open(&database)?;
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
        assert_eq!(
            connection.query_row("SELECT COUNT(*) FROM time_intervals", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
    }
    Ok(())
}

#[test]
fn schema_and_commit_failures_never_leave_a_partial_migration() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let conflict = directory.path().join("conflict.sqlite3");
    let connection = Connection::open(&conflict)?;
    connection.execute_batch("CREATE TABLE sessions(existing TEXT);")?;
    assert!(SqliteStore::from_connection(connection, None).is_err());
    let connection = Connection::open(&conflict)?;
    assert_eq!(
        connection.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
        0
    );
    assert_eq!(
        connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='settings'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        0
    );

    let denied_commit = directory.path().join("commit.sqlite3");
    let connection = Connection::open(&denied_commit)?;
    connection.commit_hook(Some(|| true));
    assert!(SqliteStore::from_connection(connection, None).is_err());
    let connection = Connection::open(&denied_commit)?;
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
    Ok(())
}

#[test]
fn denied_version_update_and_transaction_start_are_errors() -> Result<()> {
    for failure in ["begin", "version-read", "version-write"] {
        let connection = Connection::open_in_memory()?;
        connection.authorizer(Some(move |context: AuthContext<'_>| {
            let deny = match context.action {
                AuthAction::Transaction { .. } => failure == "begin",
                AuthAction::Pragma {
                    pragma_name: "user_version",
                    pragma_value: None,
                } => failure == "version-read",
                AuthAction::Pragma {
                    pragma_name: "user_version",
                    pragma_value: Some(_),
                } => failure == "version-write",
                _ => false,
            };
            if deny {
                Authorization::Deny
            } else {
                Authorization::Allow
            }
        }));
        assert!(
            SqliteStore::from_connection(connection, None).is_err(),
            "{failure}"
        );
    }
    Ok(())
}

#[test]
fn invalid_legacy_column_types_and_end_timestamp_fail_atomically() -> Result<()> {
    for (start, end) in [
        (
            rusqlite::types::Value::Blob(vec![255]),
            rusqlite::types::Value::Text("2026-01-01T00:00:10Z".into()),
        ),
        (
            rusqlite::types::Value::Text("2026-01-01T00:00:00Z".into()),
            rusqlite::types::Value::Blob(vec![255]),
        ),
        (
            rusqlite::types::Value::Text("2026-01-01T00:00:00Z".into()),
            rusqlite::types::Value::Text("bad end timestamp".into()),
        ),
    ] {
        let directory = tempfile::tempdir()?;
        let file = directory.path().join("bad-row.sqlite3");
        let connection = Connection::open(&file)?;
        connection.execute_batch("CREATE TABLE time_intervals(start_time,end_time);")?;
        connection.execute(
            "INSERT INTO time_intervals VALUES(?1,?2)",
            rusqlite::params![start, end],
        )?;
        assert!(SqliteStore::from_connection(connection, None).is_err());
        let connection = Connection::open(&file)?;
        assert_eq!(
            connection.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='sessions'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
    }
    Ok(())
}
