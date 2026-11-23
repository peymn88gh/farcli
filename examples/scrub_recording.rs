//! Developer tool: takes personal details out of recordings before they go
//! into the public repo, and checks that none are left.
//!
//! A recording is the byte stream of a real screen. It holds the user's
//! name (the hidden terminal sends a window title with the Target app's
//! full path) and whatever else the app showed: folders, an account name.
//!
//! ```text
//! cargo run --example scrub_recording -- --names FILE [--check] RECORDING...
//! ```
//!
//! The names file lists what must not appear, one per line, each followed
//! by a tab and what to put in its place:
//!
//! ```text
//! jane.doe<TAB>some.user
//! ```
//!
//! The replacement must have as many characters as the original, so that
//! every cell on screen stays where it was and the recording plays the
//! same. A line with no tab is only searched for. Letters match in either
//! case. Keep the names file outside the repo: it is the list of secrets.
//!
//! Each recording is rewritten in place, unless `--check` is given, and
//! then searched twice: in its bytes, and on the screen after every event
//! when played. The second search finds a name that arrived in two pieces.
//! The tool ends with status 1 if anything is found.

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::process::ExitCode;

use farcli::app_profile::AppProfile;
use farcli::recording::{Event, Recorder, Recording, Replay};

struct Name {
    private: String,
    replacement: Option<String>,
}

fn main() -> ExitCode {
    let mut names_file = None;
    let mut check_only = false;
    let mut recordings = Vec::new();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--names" => names_file = args.next(),
            "--check" => check_only = true,
            _ => recordings.push(arg),
        }
    }
    let (Some(names_file), false) = (names_file, recordings.is_empty()) else {
        eprintln!("usage: scrub_recording --names FILE [--check] RECORDING...");
        return ExitCode::from(2);
    };
    let names = match read_names(&names_file) {
        Ok(names) => names,
        Err(what) => {
            eprintln!("{names_file}: {what}");
            return ExitCode::from(2);
        }
    };

    let mut found_any = false;
    for file in &recordings {
        let mut recording = match Recording::load(file) {
            Ok(recording) => recording,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        };
        if !check_only {
            let replaced = scrub(&mut recording, &names);
            if let Err(error) = save(&recording, file) {
                eprintln!("{file}: {error}");
                return ExitCode::FAILURE;
            }
            println!("{file}: {replaced} replaced");
        }
        let found = left_in(&recording, &names);
        for (index, place) in &found {
            // The name itself is not printed: this output may be pasted.
            println!("{file}: name on line {index} of the names file found {place}");
        }
        if found.is_empty() {
            println!("{file}: clean");
        }
        found_any |= !found.is_empty();
    }
    if found_any {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn read_names(file: &str) -> Result<Vec<Name>, String> {
    let text = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
    let mut names = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            // Keep the count in step with the file's line numbers.
            names.push(Name {
                private: String::new(),
                replacement: None,
            });
            continue;
        }
        let (private, replacement) = match line.split_once('\t') {
            Some((private, replacement)) => (private, Some(replacement.to_string())),
            None => (line, None),
        };
        if let Some(replacement) = &replacement {
            // The same bytes too, so that plain letters replace plain letters.
            if replacement.chars().count() != private.chars().count()
                || replacement.len() != private.len()
            {
                return Err(format!(
                    "line {}: the replacement must be as long as what it replaces",
                    index + 1
                ));
            }
        }
        names.push(Name {
            private: private.to_string(),
            replacement,
        });
    }
    Ok(names)
}

/// Replaces every name that has a replacement, in everything the recording
/// holds. Returns how many were replaced.
fn scrub(recording: &mut Recording, names: &[Name]) -> usize {
    let mut replaced = 0;
    for name in names {
        let Some(replacement) = &name.replacement else {
            continue;
        };
        let about = &mut recording.about;
        let mut texts = vec![&mut about.app, &mut about.app_version, &mut about.system];
        for (noted, value) in &mut about.notes {
            texts.push(noted);
            texts.push(value);
        }
        for text in texts {
            let mut bytes = std::mem::take(text).into_bytes();
            replaced += replace(&mut bytes, name.private.as_bytes(), replacement.as_bytes());
            *text = String::from_utf8_lossy(&bytes).into_owned();
        }
        for (_, event) in &mut recording.events {
            if let Event::AppWrote(bytes) | Event::UserTyped(bytes) = event {
                replaced += replace(bytes, name.private.as_bytes(), replacement.as_bytes());
            }
        }
    }
    replaced
}

/// Where each name still shows, as (line of the names file, place).
fn left_in(recording: &Recording, names: &[Name]) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut saved = Vec::new();
    write(recording, &mut saved).expect("writing to memory cannot fail");
    let mut replay = Replay::start(recording, AppProfile::plain());
    let mut frames: Vec<String> = Vec::new();
    loop {
        let mut frame = replay.terminal.screen_text().join("\n");
        frame.push('\n');
        if frames.last() != Some(&frame) {
            frames.push(frame);
        }
        if replay.step().is_none() {
            break;
        }
    }
    frames.push(replay.terminal.scrollback_text().join("\n"));
    for (index, name) in names.iter().enumerate() {
        if name.private.is_empty() {
            continue;
        }
        let wanted = name.private.as_bytes();
        if find(&saved, wanted, 0).is_some() {
            found.push((index + 1, "in the file's bytes".to_string()));
        }
        if frames
            .iter()
            .any(|frame| find(frame.as_bytes(), wanted, 0).is_some())
        {
            found.push((index + 1, "on the screen when played".to_string()));
        }
    }
    found
}

fn save(recording: &Recording, file: &str) -> std::io::Result<()> {
    write(recording, BufWriter::new(File::create(file)?))
}

fn write(recording: &Recording, file: impl std::io::Write) -> std::io::Result<()> {
    let mut recorder = Recorder::start(file, &recording.about)?;
    for (milliseconds, event) in &recording.events {
        recorder.record(*milliseconds, event)?;
    }
    Ok(())
}

/// Replaces every `private` in `bytes`, whatever the case of its letters.
fn replace(bytes: &mut [u8], private: &[u8], replacement: &[u8]) -> usize {
    let mut replaced = 0;
    let mut from = 0;
    while let Some(at) = find(bytes, private, from) {
        bytes[at..at + private.len()].copy_from_slice(replacement);
        from = at + private.len();
        replaced += 1;
    }
    replaced
}

fn find(bytes: &[u8], wanted: &[u8], from: usize) -> Option<usize> {
    if wanted.is_empty() || bytes.len() < wanted.len() {
        return None;
    }
    (from..=bytes.len() - wanted.len())
        .find(|&at| bytes[at..at + wanted.len()].eq_ignore_ascii_case(wanted))
}
