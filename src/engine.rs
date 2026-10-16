use crate::{
    Error, Result,
    domain::{Interval, Settings, Timestamp, usage},
};

/// IDs are storage-owned tokens. Only the engine's recording state carries one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionId(i64);
impl SessionId {
    pub fn new(value: i64) -> Result<Self> {
        if value <= 0 {
            return Err(Error::Invalid("Session ID must be positive"));
        }
        Ok(Self(value))
    }
    pub fn value(self) -> i64 {
        self.0
    }
}
pub trait Store {
    fn settings(&self) -> Result<Settings>;
    fn save_settings(&mut self, settings: Settings) -> Result<()>;
    fn start(&mut self, now: Timestamp) -> Result<SessionId>;
    fn checkpoint(&mut self, id: SessionId, end: Timestamp) -> Result<()>;
    fn intervals(&self, now: Timestamp, settings: Settings) -> Result<Vec<Interval>>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tracking {
    Stopped,
    Recording { id: SessionId, last: Timestamp },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub used: u64,
    pub remaining: Option<u64>,
    pub recording: bool,
}

pub struct Engine<S> {
    store: S,
    settings: Settings,
    tracking: Tracking,
}
impl<S: Store> Engine<S> {
    pub fn new(store: S) -> Result<Self> {
        let settings = store.settings()?;
        Ok(Self {
            store,
            settings,
            tracking: Tracking::Stopped,
        })
    }
    pub fn settings(&self) -> Settings {
        self.settings
    }
    pub fn recording(&self) -> bool {
        matches!(self.tracking, Tracking::Recording { .. })
    }
    pub fn change_settings(&mut self, settings: Settings) -> Result<()> {
        // Publish new state only after commit succeeds.
        self.store.save_settings(settings)?;
        self.settings = settings;
        Ok(())
    }
    pub fn toggle(&mut self, now: Timestamp) -> Result<()> {
        match self.tracking {
            Tracking::Stopped => {
                let id = self.store.start(now)?;
                self.tracking = Tracking::Recording { id, last: now };
            }
            Tracking::Recording { .. } => self.stop(now)?,
        }
        Ok(())
    }
    pub fn stop(&mut self, now: Timestamp) -> Result<()> {
        self.checkpoint(now)?;
        self.tracking = Tracking::Stopped;
        Ok(())
    }
    fn checkpoint(&mut self, now: Timestamp) -> Result<()> {
        if let Tracking::Recording { id, last } = self.tracking {
            if now < last {
                return Err(Error::ClockBackwards);
            }
            self.store.checkpoint(id, now)?;
            self.tracking = Tracking::Recording { id, last: now };
        }
        Ok(())
    }
    /// Checkpoint once before reading, so active usage appears in both graphs.
    pub fn timeline(&mut self, now: Timestamp) -> Result<crate::charts::Timeline> {
        self.checkpoint(now)?;
        Ok(crate::charts::Timeline::new(
            &self.store.intervals(now, self.settings)?,
            now,
            self.settings,
        ))
    }
    pub fn tick(&mut self, now: Timestamp) -> Result<Snapshot> {
        self.checkpoint(now)?;
        let used = usage(
            self.store.intervals(now, self.settings)?,
            now,
            self.settings.window,
        );
        Ok(Snapshot {
            used,
            remaining: self
                .settings
                .limit
                .map(|limit| limit.millis().saturating_sub(used)),
            recording: self.recording(),
        })
    }
    /// A failed close remains recoverable: callers keep the window open and can
    /// retry. After a crash a new engine starts stopped at the last saved tick.
    pub fn close(&mut self, now: Timestamp) -> Result<()> {
        self.stop(now)
    }
}
