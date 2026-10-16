#![allow(clippy::expect_used)]
use rolling_timer::{Error, Result, domain::*, engine::*, storage::SqliteStore};
use std::{cell::Cell, rc::Rc};
fn t(ms: i64) -> Timestamp {
    Timestamp::new(ms).expect("test timestamp")
}
fn settings(window: u64, limit: Option<u64>) -> Settings {
    Settings {
        window: Window::new(window).expect("window"),
        limit: limit.map(|x| Window::new(x).expect("limit")),
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Settings,
    Save,
    Start,
    Checkpoint,
    Read,
}
struct FaultyStore {
    inner: SqliteStore,
    fault: Rc<Cell<Option<Operation>>>,
}
impl FaultyStore {
    fn check(&self, op: Operation) -> Result<()> {
        if self.fault.get() == Some(op) {
            Err(Error::Invalid("injected storage failure"))
        } else {
            Ok(())
        }
    }
}
impl Store for FaultyStore {
    fn settings(&self) -> Result<Settings> {
        self.check(Operation::Settings)?;
        self.inner.settings()
    }
    fn save_settings(&mut self, settings: Settings) -> Result<()> {
        self.check(Operation::Save)?;
        self.inner.save_settings(settings)
    }
    fn start(&mut self, now: Timestamp) -> Result<SessionId> {
        self.check(Operation::Start)?;
        self.inner.start(now)
    }
    fn checkpoint(&mut self, id: SessionId, now: Timestamp) -> Result<()> {
        self.check(Operation::Checkpoint)?;
        self.inner.checkpoint(id, now)
    }
    fn intervals(&self, now: Timestamp, settings: Settings) -> Result<Vec<Interval>> {
        self.check(Operation::Read)?;
        self.inner.intervals(now, settings)
    }
}
#[test]
fn full_lifecycle_usage_releases_while_stopped() -> Result<()> {
    let mut engine = Engine::new(SqliteStore::in_memory()?)?;
    engine.change_settings(settings(10_000, Some(3000)))?;
    assert_eq!(
        engine.tick(t(1000))?,
        Snapshot {
            used: 0,
            remaining: Some(3000),
            recording: false
        }
    );
    engine.toggle(t(2000))?;
    assert_eq!(
        engine.tick(t(4000))?,
        Snapshot {
            used: 2000,
            remaining: Some(1000),
            recording: true
        }
    );
    engine.toggle(t(6000))?;
    assert_eq!(engine.tick(t(6000))?.remaining, Some(0));
    assert_eq!(engine.tick(t(14_000))?.used, 2000);
    assert_eq!(engine.tick(t(15_000))?.remaining, Some(2000));
    assert_eq!(engine.tick(t(16_000))?.used, 0);
    engine.close(t(16_000))?;
    engine.close(t(16_000))?;
    Ok(())
}
#[test]
fn window_changes_recompute_saved_and_active_usage() -> Result<()> {
    let mut engine = Engine::new(SqliteStore::in_memory()?)?;
    engine.toggle(t(0))?;
    engine.tick(t(10_000))?;
    engine.change_settings(settings(2000, None))?;
    assert_eq!(engine.tick(t(10_000))?.used, 2000);
    assert_eq!(engine.tick(t(20_000))?.used, 2000);
    engine.change_settings(settings(40_000, None))?;
    assert_eq!(engine.tick(t(20_000))?.used, 20_000);
    engine.close(t(20_000))?;
    assert!(!engine.recording());
    Ok(())
}
#[test]
fn clock_rollback_does_not_shrink_persisted_session() -> Result<()> {
    let mut engine = Engine::new(SqliteStore::in_memory()?)?;
    engine.toggle(t(1000))?;
    engine.tick(t(5000))?;
    assert!(matches!(engine.tick(t(4999)), Err(Error::ClockBackwards)));
    assert!(matches!(engine.close(t(4999)), Err(Error::ClockBackwards)));
    assert!(engine.recording());
    assert_eq!(engine.tick(t(5000))?.used, 4000);
    engine.close(t(6000))?;
    assert_eq!(engine.tick(t(6000))?.used, 5000);
    Ok(())
}
#[test]
fn failures_never_publish_uncommitted_state_and_are_retryable() -> Result<()> {
    let fault = Rc::new(Cell::new(Some(Operation::Settings)));
    assert!(
        Engine::new(FaultyStore {
            inner: SqliteStore::in_memory()?,
            fault: fault.clone()
        })
        .is_err()
    );
    fault.set(None);
    let mut engine = Engine::new(FaultyStore {
        inner: SqliteStore::in_memory()?,
        fault: fault.clone(),
    })?;
    fault.set(Some(Operation::Save));
    assert!(engine.change_settings(settings(1000, None)).is_err());
    assert_eq!(engine.settings(), Settings::default());
    fault.set(Some(Operation::Start));
    assert!(engine.toggle(t(1000)).is_err());
    assert!(!engine.recording());
    fault.set(None);
    engine.toggle(t(1000))?;
    fault.set(Some(Operation::Checkpoint));
    assert!(engine.tick(t(5000)).is_err());
    assert!(engine.close(t(5000)).is_err());
    assert!(engine.recording());
    fault.set(Some(Operation::Read));
    assert!(engine.timeline(t(6000)).is_err());
    assert!(engine.tick(t(6000)).is_err());
    fault.set(None);
    assert_eq!(engine.tick(t(7000))?.used, 6000);
    engine.close(t(8000))?;
    assert_eq!(engine.tick(t(8000))?.used, 7000);
    Ok(())
}
#[test]
fn rapid_toggles_and_zero_duration_sessions() -> Result<()> {
    let mut engine = Engine::new(SqliteStore::in_memory()?)?;
    for _ in 0..20 {
        engine.toggle(t(1000))?;
    }
    assert_eq!(engine.tick(t(1000))?.used, 0);
    assert!(!engine.recording());
    Ok(())
}

#[test]
fn session_ids_reject_invalid_values() {
    assert!(SessionId::new(0).is_err());
    assert!(SessionId::new(-1).is_err());
}
