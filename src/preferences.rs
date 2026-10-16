//! UI preferences are independent of usage accounting. A typed, closed theme
//! list keeps file identifiers, menu order and palette selection in agreement.
use crate::{Error, Result};
use std::{fs, io::Write, path::Path};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Mocha,
    Cappuccino,
}
impl Theme {
    pub const ALL: [Self; 4] = [Self::Dark, Self::Light, Self::Mocha, Self::Cappuccino];
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Mocha => "mocha",
            Self::Cappuccino => "cappuccino",
        }
    }
    pub fn parse(text: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|theme| theme.name() == text)
            .ok_or(Error::Invalid(
                "Theme must be dark, light, mocha or cappuccino",
            ))
    }
    pub fn from_index(index: i32) -> Result<Self> {
        Self::ALL
            .into_iter()
            .nth(index as usize)
            .ok_or(Error::Invalid("Select a theme"))
    }
    pub fn index(self) -> i32 {
        // The enum declaration and ALL have the same explicit order.
        self as i32
    }
}

fn document(path: &Path) -> Result<toml_edit::DocumentMut> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    text.parse()
        .map_err(|error: toml_edit::TomlError| Error::Configuration(error.to_string()))
}
pub fn load_theme(path: &Path) -> Result<Theme> {
    match document(path)?.get("theme") {
        None => Ok(Theme::default()),
        Some(value) => Theme::parse(
            value
                .as_str()
                .ok_or(Error::Invalid("Theme must be a string"))?,
        ),
    }
}
/// Re-read before editing to preserve manual changes, comments and unrelated
/// keys. The profile's OS lock must be held by the caller during writes.
pub fn save_theme(path: &Path, theme: Theme) -> Result<()> {
    let mut config = document(path)?;
    let mut value = toml_edit::Value::from(theme.name());
    if let Some(old) = config.get("theme").and_then(toml_edit::Item::as_value) {
        *value.decor_mut() = old.decor().clone();
    }
    config.insert("theme", toml_edit::Item::Value(value));
    replace_config(path, config.to_string().as_bytes())
}

/// Atomic file replacement, separated from parsing so callers can reuse it and
/// tests can exercise publication failures without filesystem timing races.
pub fn replace_config(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or(Error::Invalid("Configuration needs a parent directory"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(contents)?;
    file.as_file().sync_all()?;
    // persist replaces the destination atomically. On any earlier failure the
    // previous file stays intact and the temporary file removes itself.
    file.persist(path).map_err(|error| Error::Io(error.error))?;
    Ok(())
}
