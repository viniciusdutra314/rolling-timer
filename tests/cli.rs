use rolling_timer::{
    Result,
    cli::{Command, parse},
};
use std::ffi::OsString;
fn args(input: &[&str]) -> Vec<OsString> {
    input.iter().map(OsString::from).collect()
}
#[cfg(feature = "gui")]
fn executable() -> std::process::Command {
    let binary = std::env::var_os("ROLLING_TIMER_TEST_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_rolling-timer").into());
    if let Some(runner) = std::env::var_os("ROLLING_TIMER_TEST_RUNNER") {
        let mut command = std::process::Command::new(runner);
        command.arg(binary);
        command
    } else {
        std::process::Command::new(binary)
    }
}
#[test]
fn argument_contract() -> Result<()> {
    assert_eq!(parse(args(&[]))?, Command::Run { directory: None });
    assert_eq!(
        parse(args(&["--data-dir", "a path with spaces"]))?,
        Command::Run {
            directory: Some("a path with spaces".into())
        }
    );
    for flag in ["--help", "-h"] {
        assert_eq!(parse(args(&[flag]))?, Command::Help);
    }
    assert_eq!(parse(args(&["--version"]))?, Command::Version);
    for bad in [
        vec!["--data-dir"],
        vec!["--data-dir", ""],
        vec!["--help", "extra"],
        vec!["--version", "extra"],
        vec!["--data-dir", "folder", "extra"],
        vec!["--unknown"],
    ] {
        assert!(parse(args(&bad)).is_err());
    }
    Ok(())
}
#[test]
#[cfg(feature = "gui")]
fn executable_help_version_and_errors_do_not_need_a_display() -> Result<()> {
    for flag in ["--help", "-h", "--version"] {
        let output = executable()
            .arg(flag)
            .env_remove("DISPLAY")
            .env_remove("HOME")
            .env_remove("USERPROFILE")
            .output()?;
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
    }
    for bad in [
        vec!["--unknown"],
        vec!["--data-dir"],
        vec!["--help", "extra"],
    ] {
        let output = executable().args(bad).env_remove("DISPLAY").output()?;
        assert!(!output.status.success());
        assert!(!output.stderr.is_empty());
    }
    Ok(())
}
#[test]
#[cfg(all(feature = "gui", target_os = "linux"))]
fn startup_failures_are_actionable_and_nonzero() -> Result<()> {
    let missing_home = executable()
        .env_remove("DISPLAY")
        .env_remove("HOME")
        .output()?;
    assert!(!missing_home.status.success());
    assert!(String::from_utf8_lossy(&missing_home.stderr).contains("--data-dir"));
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("not-a-directory");
    std::fs::write(&file, "test")?;
    let invalid_dir = executable()
        .arg("--data-dir")
        .arg(file)
        .env_remove("DISPLAY")
        .output()?;
    assert!(!invalid_dir.status.success());
    assert!(String::from_utf8_lossy(&invalid_dir.stderr).contains("File error"));
    Ok(())
}
#[cfg(unix)]
#[test]
fn paths_need_not_be_utf8() -> Result<()> {
    use std::os::unix::ffi::OsStringExt;
    let path = OsString::from_vec(vec![b'/', 0xff]);
    assert_eq!(
        parse([OsString::from("--data-dir"), path.clone()])?,
        Command::Run {
            directory: Some(path.into())
        }
    );
    Ok(())
}
