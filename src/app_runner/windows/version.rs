//! Learning a Target app's version at start without keeping the user
//! waiting.
//!
//! Asking the app is the plain way: it is run once with the arguments that
//! make it print its version and end. Measured on 2026-11-24, Claude Code
//! 2.1.322 answers in about 70 ms and Codex's own program in about 60 ms.
//! But npm installs Codex behind a batch file that starts `node`, which
//! starts Codex, and through that the answer takes 300 to 550 ms. So for a
//! program that npm installed the version is read from the package's own
//! description, which npm keeps beside the batch file. That takes no time
//! a person could notice, and is the number the app would have said.

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::setup::jsonc::{Found, Settings, Step};

use super::command_line;

/// How long an app is given to say its version. One that takes longer is
/// started without farcli knowing its version. On a busy machine the
/// answer was seen to take over a second, so the limit is well above the
/// usual tenth of a second: it is there for an app that never answers.
const TIME_TO_ANSWER: Duration = Duration::from_secs(3);

/// More than any batch file or package description that is read here holds.
const MOST_BYTES_READ: u64 = 1024 * 1024;

/// What `program` says its version is: what it prints when run with
/// `args`, or for a program that npm installed, the version of its
/// package. `None` when it cannot be found out soon enough.
pub fn version_said(program: &OsStr, args: &[&str]) -> Option<String> {
    let file = command_line::find_program(program).ok()?;
    if command_line::is_batch_file(&file) {
        if let Some(version) = version_of_npm_package(&file) {
            return Some(version);
        }
    }
    printed_within(&file, args, TIME_TO_ANSWER)
}

/// The version of the package whose program `batch_file` starts, when the
/// batch file is one that npm wrote. Such a file names the package's
/// script as `"%dp0%\node_modules\PACKAGE\SCRIPT"`, where `%dp0%` is the
/// folder the batch file is in, and the package's folder holds its
/// description in `package.json`.
fn version_of_npm_package(batch_file: &Path) -> Option<String> {
    const ITS_OWN_FOLDER: &str = "\"%dp0%\\";
    let text = String::from_utf8(read_small(batch_file)?).ok()?;
    let folder = batch_file.parent()?;
    let packages = folder.join("node_modules");
    text.match_indices(ITS_OWN_FOLDER).find_map(|(at, _)| {
        let named = &text[at + ITS_OWN_FOLDER.len()..];
        let script = folder.join(&named[..named.find('"')?]);
        // The script may be some folders down inside its package.
        script
            .ancestors()
            .skip(1)
            .take_while(|above| above.starts_with(&packages) && *above != packages)
            .find_map(|above| {
                let description = Settings::read(&read_small(&above.join("package.json"))?).ok()?;
                match description.find(&[Step::Key("version")]) {
                    Found::Text(version) => Some(version),
                    _ => None,
                }
            })
    })
}

/// The whole of a file that is not large.
fn read_small(file: &Path) -> Option<Vec<u8>> {
    let small = fs::metadata(file).ok()?.len() <= MOST_BYTES_READ;
    small.then(|| fs::read(file).ok()).flatten()
}

/// Runs `file` with `args` away from any terminal and returns what it
/// printed, or `None` when it has not ended within `limit`. It is then
/// ended, and whatever it started is left to end by itself.
fn printed_within(file: &Path, args: &[&str], limit: Duration) -> Option<String> {
    let mut asked = Command::new(file)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut answer = asked.stdout.take()?;
    let (said, heard) = mpsc::channel();
    thread::spawn(move || {
        let mut printed = Vec::new();
        if answer.read_to_end(&mut printed).is_ok() {
            let _ = said.send(printed);
        }
    });
    match heard.recv_timeout(limit) {
        Ok(printed) => {
            let _ = asked.wait();
            Some(String::from_utf8_lossy(&printed).into_owned())
        }
        Err(_) => {
            let _ = asked.kill();
            None
        }
    }
}
