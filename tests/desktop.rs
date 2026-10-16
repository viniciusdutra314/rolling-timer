#![cfg(feature = "gui")]
//! Run with `just gui-test`. A real display is required; no silent success if it
//! is missing. One test owns the desktop thread for the entire widget lifecycle.
use fltk::prelude::*;
use rolling_timer::{
    Result,
    domain::Timestamp,
    engine::Engine,
    gui::{Action, Desktop},
    preferences::{Theme, load_theme, save_theme},
    storage::SqliteStore,
};
fn t(ms: i64) -> Result<Timestamp> {
    // A modern epoch exercises full-sized history windows in captured widgets.
    Timestamp::new(1_792_152_000_000 + ms)
}
// Render actual FLTK widgets into an owned surface. capture_window in 1.5.23
// copies fl_read_image's allocation without freeing it; this API owns its image.
fn capture(window: &impl WindowExt) -> Result<fltk::image::RgbImage> {
    let surface = fltk::surface::ImageSurface::new(window.width(), window.height(), false);
    fltk::surface::ImageSurface::push_current(&surface);
    surface.draw(window, 0, 0);
    let image = surface.image();
    fltk::surface::ImageSurface::pop_current();
    image.ok_or(rolling_timer::Error::Invalid("Could not capture widgets"))
}
fn desktop_widgets() -> Result<()> {
    let _application = fltk::app::App::default();
    let (sender, receiver) = fltk::app::channel();
    let dir = tempfile::tempdir()?;
    let mut engine = Engine::new(SqliteStore::open(dir.path())?)?;
    let config = dir.path().join("config.toml");
    let save_path = config.clone();
    let mut desktop = Desktop::new(engine.settings(), sender, Theme::Dark, move |theme| {
        save_theme(&save_path, theme)
    });
    desktop.show();
    desktop.update(&mut engine, Err(rolling_timer::Error::ClockBackwards));
    desktop.update(&mut engine, t(1000));
    desktop.update(&mut engine, t(1000));
    fltk::app::wait_for(0.01).map_err(|e| rolling_timer::Error::Desktop(e.to_string()))?;
    desktop.refresh(&mut engine, t(1000)?)?;
    assert_eq!(desktop.used.label(), "00:00:00");
    desktop.toggle.do_callback();
    let action = receiver
        .recv()
        .ok_or(rolling_timer::Error::Invalid("Start callback missing"))?;
    desktop.dispatch(&mut engine, action, t(1000))?;
    assert_eq!(desktop.toggle.label(), "Stop");
    desktop.refresh(&mut engine, t(61_000)?)?;
    assert_eq!(desktop.used.label(), "00:01:00");
    desktop.window_hours.set_value("1");
    desktop.limit_hours.set_value("0.5");
    desktop.apply.do_callback();
    let action = receiver
        .recv()
        .ok_or(rolling_timer::Error::Invalid("Apply callback missing"))?;
    desktop.dispatch(&mut engine, action, t(61_000))?;
    assert_eq!(desktop.remaining.label(), "00:29:00 available");
    // Every theme uses the same live widgets, including an active Stop button.
    let mut original = None;
    for theme in Theme::ALL {
        desktop.theme_choice.set_value(theme.index());
        desktop.theme_choice.do_callback();
        let action = receiver
            .recv()
            .ok_or(rolling_timer::Error::Invalid("Theme callback missing"))?;
        assert!(!desktop.handle_action(
            &mut engine,
            action,
            Err(rolling_timer::Error::ClockBackwards)
        ));
        assert_eq!(desktop.theme(), theme);
        assert_eq!(load_theme(&config)?, theme);
        assert_eq!(desktop.toggle.label(), "Stop");
        let colors = (desktop.window.color(), desktop.window_hours.text_color());
        if let Some(previous) = original {
            assert_ne!(colors, previous);
        }
        original = Some(colors);
        fltk::app::flush();
    }
    let committed = desktop.theme();
    std::fs::write(&config, "=")?;
    desktop.theme_choice.set_value(Theme::Light.index());
    assert!(
        desktop
            .dispatch(&mut engine, Action::Theme, t(61_000))
            .is_err()
    );
    assert_eq!(desktop.theme(), committed);
    assert_eq!(desktop.theme_choice.value(), committed.index());
    std::fs::write(&config, "theme = 'cappuccino'\n")?;
    desktop.theme_choice.set_value(-1);
    assert!(
        desktop
            .dispatch(&mut engine, Action::Theme, t(61_000))
            .is_err()
    );
    // The graph action checkpoints active usage without toggling the session.
    desktop.charts_button.do_callback();
    let action = receiver
        .recv()
        .ok_or(rolling_timer::Error::Invalid("Chart callback missing"))?;
    desktop.dispatch(&mut engine, action, t(61_000))?;
    assert!(engine.recording());
    {
        let charts = desktop
            .charts
            .as_mut()
            .ok_or(rolling_timer::Error::Invalid("Charts missing"))?;
        assert!(charts.window.shown());
        assert!(charts.forecast_readout().contains("Used 00:01:00"));
        charts.inspect_forecast(Some(f64::NAN));
        assert!(charts.forecast_readout().contains("Used 00:01:00"));
        charts.inspect_forecast(Some(-1.0));
        assert!(charts.forecast_readout().contains("Used 00:01:00"));
        charts.inspect_forecast(Some(2.0));
    }
    {
        let charts = desktop
            .charts
            .as_ref()
            .ok_or(rolling_timer::Error::Invalid("Charts missing"))?;
        assert!(charts.forecast_readout().contains("Released 00:01:00"));
        assert!(charts.forecast_readout().contains("Available 00:30:00"));
    }
    for theme in Theme::ALL {
        desktop.theme_choice.set_value(theme.index());
        desktop.dispatch(&mut engine, Action::Theme, t(61_000))?;
        desktop.refresh(&mut engine, t(61_000)?)?;
        fltk::app::flush();
        if let Some(directory) = std::env::var_os("ROLLING_TIMER_THEME_SCREENSHOTS") {
            use std::io::Write;
            let directory = std::path::PathBuf::from(directory);
            // A deterministic day-long chart makes hourly divisions and the
            // return of separate sessions visible in the review screenshots.
            let now = t(61_000)?;
            let mut samples = Vec::new();
            for (start, end) in [
                (23 * 3_600_000, 22 * 3_600_000 + 1_800_000),
                (18 * 3_600_000, 17 * 3_600_000),
                (5 * 3_600_000, 4 * 3_600_000 + 2_400_000),
            ] {
                samples.push(rolling_timer::domain::Interval::new(
                    Timestamp::new(now.millis() - start)?,
                    Timestamp::new(now.millis() - end)?,
                )?);
            }
            let charts = desktop
                .charts
                .as_mut()
                .ok_or(rolling_timer::Error::Invalid("Charts missing"))?;
            charts.update(
                rolling_timer::charts::Timeline::new(
                    &samples,
                    now,
                    rolling_timer::domain::Settings {
                        window: rolling_timer::domain::Window::default(),
                        limit: Some(rolling_timer::domain::Window::parse_hours("3")?),
                    },
                ),
                theme,
            );
            charts.inspect_forecast(Some(0.5));
            for (name, window) in [
                (format!("theme-{}", theme.name()), &mut desktop.window),
                (
                    format!("graphs-{}", theme.name()),
                    &mut desktop
                        .charts
                        .as_mut()
                        .ok_or(rolling_timer::Error::Invalid("Charts missing"))?
                        .window,
                ),
            ] {
                // Wait for the native expose event before reading FLTK's
                // double buffer; a newly mapped window has no buffer yet.
                window.show();
                fltk::app::wait_for(0.05)?;
                fltk::app::flush();
                window.make_current();
                let image = capture(window)?;
                let mut output = std::fs::File::create(directory.join(format!("{name}.ppm")))?;
                write!(output, "P6\n{} {}\n255\n", image.width(), image.height())?;
                output.write_all(&image.to_rgb_data())?;
            }
            desktop.refresh(&mut engine, now)?;
        }
    }
    if let Some(path) = std::env::var_os("ROLLING_TIMER_SCREENSHOT") {
        use std::io::Write;
        fltk::app::flush();
        desktop.window.make_current();
        let image = capture(&desktop.window)?;
        let mut output = std::fs::File::create(path)?;
        write!(output, "P6\n{} {}\n255\n", image.width(), image.height())?;
        output.write_all(&image.to_rgb_data())?;
    }
    // A clock rollback must reach the UI error path without changing labels or
    // discarding the active recording. Settings writes fail atomically too.
    assert!(desktop.refresh(&mut engine, t(60_000)?).is_err());
    assert!(
        desktop
            .dispatch(&mut engine, Action::Charts, t(60_000))
            .is_err()
    );
    assert!(
        desktop
            .dispatch(&mut engine, Action::Toggle, t(60_000))
            .is_err()
    );
    assert!(
        desktop
            .dispatch(&mut engine, Action::Apply, t(60_000))
            .is_err()
    );
    let connection = rusqlite::Connection::open(dir.path().join("time_intervals.sqlite3"))?;
    connection.execute_batch("CREATE TRIGGER fail_settings BEFORE UPDATE ON settings BEGIN SELECT RAISE(ABORT,'test write failure'); END;")?;
    assert!(
        desktop
            .dispatch(&mut engine, Action::Apply, t(61_000))
            .is_err()
    );
    connection.execute_batch("DROP TRIGGER fail_settings;")?;
    desktop.window_hours.set_value("NaN");
    assert!(!desktop.handle_action(&mut engine, Action::Apply, t(61_000)));
    assert!(desktop.status.label().contains("positive"));
    desktop.update(&mut engine, Err(rolling_timer::Error::ClockBackwards));
    assert!(desktop.status.label().contains("backwards"));
    assert!(
        desktop
            .charts
            .as_ref()
            .is_some_and(|charts| charts.summary.label().contains("Could not refresh charts"))
    );
    desktop.update(&mut engine, t(61_000));
    assert!(desktop.status.label().starts_with("Recording."));
    assert!(!desktop.handle_action(
        &mut engine,
        Action::Apply,
        Err(rolling_timer::Error::ClockBackwards)
    ));
    assert!(
        desktop
            .dispatch(&mut engine, Action::Apply, t(61_000))
            .is_err()
    );
    assert_eq!(engine.settings().window.hours_text(), "1");
    desktop.window_hours.set_value("1");
    desktop.limit_hours.set_value("bad");
    assert!(
        desktop
            .dispatch(&mut engine, Action::Apply, t(61_000))
            .is_err()
    );
    desktop.limit_hours.set_value("");
    desktop.dispatch(&mut engine, Action::Apply, t(61_000))?;
    assert_eq!(desktop.remaining.label(), "No usage limit set");
    {
        let charts = desktop
            .charts
            .as_mut()
            .ok_or(rolling_timer::Error::Invalid("Charts missing"))?;
        assert!(!charts.forecast_readout().contains("Available"));
        charts.window.do_callback();
        assert!(!charts.window.shown());
    }
    assert!(desktop.window.shown());
    // Hidden charts use the ordinary tick path, including its clock failures.
    assert!(desktop.refresh(&mut engine, t(60_000)?).is_err());
    desktop.dispatch(&mut engine, Action::Charts, t(61_000))?;

    desktop.dispatch(&mut engine, Action::Toggle, t(61_000))?;
    assert_eq!(desktop.toggle.label(), "Start");
    desktop.refresh(&mut engine, t(3_631_000)?)?;
    assert_eq!(desktop.used.label(), "00:00:30");
    desktop.report("A recoverable\0error");
    assert_eq!(desktop.status.label(), "A recoverable�error");
    desktop.dispatch(&mut engine, Action::Toggle, t(3_631_000))?;
    assert!(
        desktop
            .dispatch(&mut engine, Action::Close, t(3_630_000))
            .is_err()
    );
    assert!(desktop.window.shown());
    desktop.window.do_callback();
    let action = receiver
        .recv()
        .ok_or(rolling_timer::Error::Invalid("Close callback missing"))?;
    assert!(desktop.dispatch(&mut engine, action, t(3_632_000))?);
    assert!(!desktop.window.shown());
    drop(engine);
    let mut reopened = Engine::new(SqliteStore::open(dir.path())?)?;
    assert!(!reopened.recording());
    assert_eq!(reopened.tick(t(3_632_000)?)?.used, 30_000);
    // Embedded callers may explicitly delete a root before releasing its Rust
    // adapter. Drop must tolerate that ownership path without a double delete.
    fltk::app::delete_widget(desktop.window.clone());
    fltk::app::check();
    drop(desktop);
    Ok(())
}

fn main() -> std::process::ExitCode {
    if !std::env::args().any(|argument| argument == "--ignored") {
        println!("desktop_widgets: ignored (requires a display; run just gui-test)");
        return std::process::ExitCode::SUCCESS;
    }
    match desktop_widgets() {
        Ok(()) => {
            println!("desktop_widgets: passed");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("desktop_widgets: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
