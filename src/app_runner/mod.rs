//! App runner: starts the Target app on the hidden terminal, relays resizes
//! and returns the app's exit status.

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{printed_by, run_directly, Host, RunningApp, ShippedFiles};

/// Runs `program` with `args` with no hidden terminal: it reads and writes
/// whatever farcli itself was given. Returns the program's exit status.
#[cfg(not(windows))]
pub fn run_directly(
    program: &std::ffi::OsStr,
    args: &[std::ffi::OsString],
) -> std::io::Result<i32> {
    let status = std::process::Command::new(program).args(args).status()?;
    Ok(status.code().unwrap_or(1))
}
