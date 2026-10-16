use rolling_timer::{Result, preferences::*};
use std::fs;
#[test]
fn themes_are_closed_and_missing_preferences_keep_the_original_dark_theme() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    assert_eq!(load_theme(&path)?, Theme::Dark);
    fs::write(
        &path,
        "# Python settings\nsliding_window_hours = 1.5\n\n[custom]\nmessage = 'keep me'\n",
    )?;
    assert_eq!(load_theme(&path)?, Theme::Dark);
    for theme in Theme::ALL {
        assert_eq!(Theme::parse(theme.name())?, theme);
        assert_eq!(Theme::from_index(theme.index())?, theme);
        save_theme(&path, theme)?;
        assert_eq!(load_theme(&path)?, theme);
        let contents = fs::read_to_string(&path)?;
        assert!(contents.contains("# Python settings"));
        assert!(contents.contains("sliding_window_hours = 1.5"));
        assert!(contents.contains("message = 'keep me'"));
        assert_eq!(
            rolling_timer::storage::parse_legacy_config(&contents)?.hours_text(),
            "1.5"
        );
    }
    fs::write(&path, "theme = 'dark' # keep this preference comment\n")?;
    save_theme(&path, Theme::Light)?;
    assert!(fs::read_to_string(&path)?.contains("# keep this preference comment"));
    for text in [
        "theme = 3",
        "theme = 'unknown'",
        "theme = []",
        "theme = true",
        "=",
    ] {
        fs::write(&path, text)?;
        assert!(load_theme(&path).is_err());
    }
    assert!(save_theme(&path, Theme::Light).is_err());
    assert_eq!(fs::read_to_string(&path)?, "=");
    assert!(Theme::from_index(-1).is_err());
    assert!(Theme::from_index(4).is_err());
    Ok(())
}
#[test]
fn configuration_replacement_is_atomic_and_reports_io_failures() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    save_theme(&path, Theme::Mocha)?;
    assert_eq!(fs::read_to_string(&path)?, "theme = \"mocha\"\n");
    fs::write(&path, [0xff])?;
    assert!(load_theme(&path).is_err());
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    assert!(load_theme(&path).is_err());
    assert!(replace_config(&path, b"theme = 'light'").is_err());
    assert!(path.is_dir());
    assert_eq!(
        fs::read_dir(dir.path())?.count(),
        1,
        "failed publication cleans temporary files"
    );
    assert!(replace_config(&path.join("missing/file"), b"x").is_err());
    assert!(replace_config(std::path::Path::new("/"), b"x").is_err());
    Ok(())
}
#[test]
fn theme_only_configuration_is_valid_on_a_fresh_profile() -> Result<()> {
    use rolling_timer::{
        engine::{Engine, Store},
        storage::SqliteStore,
    };
    let dir = tempfile::tempdir()?;
    save_theme(&dir.path().join("config.toml"), Theme::Cappuccino)?;
    let store = SqliteStore::open(dir.path())?;
    assert_eq!(store.settings()?.window.hours_text(), "24");
    let _engine = Engine::new(store)?;
    assert_eq!(
        rolling_timer::storage::parse_legacy_config("")?.hours_text(),
        "24"
    );
    Ok(())
}
