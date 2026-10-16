#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use rolling_timer::{
    Error, Result,
    domain::Timestamp,
    engine::Engine,
    gui::Desktop,
    preferences::{load_theme, save_theme},
    storage::{SqliteStore, default_directory},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

fn run(directory: Option<PathBuf>) -> Result<()> {
    let directory = match directory {
        Some(path) => path,
        None => default_directory()?,
    };
    let mut engine = Engine::new(SqliteStore::open(&directory)?)?;
    // All widget ownership and mutation stay on the main thread.
    let _application = fltk::app::App::default();
    let (sender, receiver) = fltk::app::channel();
    let config = directory.join("config.toml");
    let theme = load_theme(&config)?;
    let mut desktop = Desktop::new(engine.settings(), sender, theme, move |theme| {
        save_theme(&config, theme)
    });
    desktop.show();
    let mut previous_tick: Option<Instant> = None;
    loop {
        // false means an idle timeout, not a closed window. Keep checkpointing
        // even when the user has not generated any input events.
        fltk::app::wait_for(0.05).map_err(Error::from)?;
        while let Some(action) = receiver.recv() {
            if desktop.handle_action(
                &mut engine,
                action,
                Timestamp::from_system_time(SystemTime::now()),
            ) {
                return Ok(());
            }
        }
        if previous_tick.is_none_or(|previous| previous.elapsed() >= Duration::from_secs(1)) {
            desktop.update(&mut engine, Timestamp::from_system_time(SystemTime::now()));
            previous_tick = Some(Instant::now());
        }
    }
}
fn main() -> std::process::ExitCode {
    use rolling_timer::cli::{self, Command};
    let directory = match cli::parse(std::env::args_os().skip(1)) {
        Ok(Command::Help) => {
            println!("{}", cli::HELP);
            return std::process::ExitCode::SUCCESS;
        }
        Ok(Command::Version) => {
            println!("rolling-timer {}", env!("CARGO_PKG_VERSION"));
            return std::process::ExitCode::SUCCESS;
        }
        Ok(Command::Run { directory }) => directory,
        Err(error) => {
            eprintln!("Rolling Timer: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match run(directory) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Rolling Timer: {error}");
            // CLI errors must work without a display. In a desktop launch the
            // native alert provides actionable startup/migration diagnostics.
            if cfg!(windows) || cfg!(target_os = "macos") || std::env::var_os("DISPLAY").is_some() {
                fltk::dialog::alert_default(&error.to_string().replace('\0', "�"));
            }
            std::process::ExitCode::FAILURE
        }
    }
}
