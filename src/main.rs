use std::env;
use std::ffi::OsString;
use std::process::ExitCode;

use farcli::app_profile::AppProfile;

const USAGE: &str = "\
farcli makes Farsi readable and typeable in Claude Code CLI and Codex CLI.

Usage:
  farcli claude [ARGS]...     Run Claude Code CLI
  farcli codex [ARGS]...      Run Codex CLI
  farcli PROGRAM [ARGS]...    Run any other program the same way
  farcli setup                Give this PC's Terminals the Farsi font

Everything after the app's name is passed to the app unchanged.
Setup runs by itself the first time farcli is used.
";

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let Some(name) = args.next() else {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let app_args: Vec<OsString> = args.collect();
    match name.to_str() {
        Some("--help" | "-h") => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some("--version" | "-V") => {
            println!("farcli {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some("setup") => return ExitCode::from(farcli::setup::run_as_command()),
        _ => {}
    }

    farcli::setup::run_before_first_app();
    let (profile, program) = AppProfile::for_command(&name);
    match farcli::run::run(profile, &program, &app_args) {
        // farcli ends with the Target app's own exit status.
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("farcli: cannot run {}: {error}", program.to_string_lossy());
            ExitCode::from(127)
        }
    }
}
