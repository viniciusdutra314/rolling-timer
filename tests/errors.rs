use rolling_timer::Error;
#[cfg(feature = "gui")]
#[test]
fn desktop_errors_preserve_the_toolkit_diagnostic() {
    let error = Error::from(fltk::prelude::FltkError::Unknown(
        "display interrupted".into(),
    ));
    assert!(error.to_string().contains("display interrupted"));
}
#[test]
fn errors_keep_sources_and_descriptions() {
    use std::error::Error as _;
    let io = Error::from(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
    assert!(io.source().is_some());
    let sql = Error::from(rusqlite::Error::QueryReturnedNoRows);
    assert!(sql.source().is_some());
    for error in [
        Error::Invalid("invalid input"),
        Error::ClockBackwards,
        Error::Configuration("bad theme".into()),
        Error::Legacy("bad legacy data".into()),
        Error::AlreadyRunning,
        Error::Desktop("display unavailable".into()),
        io,
        sql,
    ] {
        assert!(!error.to_string().is_empty());
    }
}
