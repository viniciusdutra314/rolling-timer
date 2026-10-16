//! Parse arguments without reading process-global state, so launchers and tests
//! can choose their own argument source (including non-UTF-8 filesystem paths).
use crate::{Error, Result};
use std::{ffi::OsString, path::PathBuf};
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run { directory: Option<PathBuf> },
    Help,
    Version,
}
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command> {
    let mut args = args.into_iter();
    let command = match args.next() {
        None => Command::Run { directory: None },
        Some(flag) if flag == "--help" || flag == "-h" => Command::Help,
        Some(flag) if flag == "--version" => Command::Version,
        Some(flag) if flag == "--data-dir" => {
            let path = args
                .next()
                .filter(|path| !path.is_empty())
                .ok_or(Error::Invalid("--data-dir requires a nonempty path"))?;
            Command::Run {
                directory: Some(path.into()),
            }
        }
        _ => return Err(Error::Invalid("Usage: rolling-timer [--data-dir PATH]")),
    };
    if args.next().is_some() {
        return Err(Error::Invalid("Unexpected arguments"));
    }
    Ok(command)
}
pub const HELP: &str = "Usage: rolling-timer [--data-dir PATH]\n\nStart/Stop measures usage within a sliding window.\nData defaults to ~/.rolling_timer; --data-dir supports portable storage.\nAn optional quota shows remaining time. No online account integration.";
