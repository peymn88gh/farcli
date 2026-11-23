//! Recording a real run: when the environment variable `FARCLI_RECORD` names
//! a file, everything the session is fed is also written there.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::time::Instant;

use crate::app_profile::AppProfile;
use crate::app_runner;
use crate::hidden_terminal::HiddenTerminal;
use crate::recording::{About, Event, Recorder};
use crate::screen::Position;
use crate::Size;

/// The environment variable that names the file to record to.
const RECORD_TO: &str = "FARCLI_RECORD";

/// The recording of this run, if the user asked for one.
pub struct Tape {
    recorder: Option<(Recorder<File>, Instant)>,
}

impl Tape {
    /// Starts the recording if `FARCLI_RECORD` is set. Asking the Target app
    /// for its version runs the app once, so this comes before the app is
    /// started on the hidden terminal.
    pub fn start(
        profile: &AppProfile,
        program: &OsStr,
        args: &[OsString],
        size: Size,
        cursor: Position,
        hidden_terminal: HiddenTerminal,
    ) -> io::Result<Tape> {
        let Some(file) = env::var_os(RECORD_TO).filter(|file| !file.is_empty()) else {
            return Ok(Tape { recorder: None });
        };
        // A farcli started inside this session must not write to the same file.
        env::remove_var(RECORD_TO);

        let app_version = profile
            .version_args()
            .and_then(|version_args| app_runner::printed_by(program, version_args).ok())
            .map(|printed| printed.trim().to_string())
            .filter(|version| !version.is_empty())
            .unwrap_or_else(|| "unknown".to_string());
        let mut command = program.to_string_lossy().into_owned();
        for arg in args {
            command.push(' ');
            command.push_str(&arg.to_string_lossy());
        }
        let mut notes = vec![("command".to_string(), command)];
        notes.extend(system_notes());
        // What a Target app writes reaches farcli through the hidden
        // terminal, and the two hidden terminals pass it on differently.
        notes.push(("hidden-terminal".to_string(), hidden_terminal.name()));
        let about = About {
            app: program.to_string_lossy().into_owned(),
            app_version,
            system: SYSTEM.to_string(),
            size,
            cursor,
            notes,
        };
        let recorder = Recorder::start(File::create(&file)?, &about).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("cannot record to {}: {error}", file.to_string_lossy()),
            )
        })?;
        Ok(Tape {
            recorder: Some((recorder, Instant::now())),
        })
    }

    /// Adds something that just happened. `event` is only worked out when
    /// there is a recording. A recording that can no longer be written is
    /// given up without disturbing the session.
    pub fn add(&mut self, event: impl FnOnce() -> Event) {
        if let Some((recorder, started)) = &mut self.recorder {
            let milliseconds = started.elapsed().as_millis() as u64;
            if recorder.record(milliseconds, &event()).is_err() {
                self.recorder = None;
            }
        }
    }
}

#[cfg(windows)]
const SYSTEM: &str = "Windows native";
#[cfg(not(windows))]
const SYSTEM: &str = std::env::consts::OS;

/// What else identifies the system: the hidden terminal built into Windows
/// draws again what a Target app wrote, and that drawing differs between
/// Windows builds.
#[cfg(windows)]
fn system_notes() -> Vec<(String, String)> {
    let mut notes = Vec::new();
    if let Ok(printed) = app_runner::printed_by(OsStr::new("cmd"), &["/d", "/c", "ver"]) {
        notes.push(("windows-version".to_string(), printed.trim().to_string()));
    }
    notes
}

#[cfg(not(windows))]
fn system_notes() -> Vec<(String, String)> {
    Vec::new()
}
