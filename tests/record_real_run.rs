//! The real `farcli` program writes a recording of its session when asked
//! to, and that recording plays back what the session showed.
//!
//! The test runs `farcli claude` on a hidden terminal, with a stand-in
//! `claude` first on the path, and types into it the way a user would.
#![cfg(windows)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use farcli::app_profile::AppProfile;
use farcli::app_runner::RunningApp;
use farcli::fake_terminal::FakeTerminal;
use farcli::recording::{Event, Recording, Replay};
use farcli::screen::Position;
use farcli::Size;

/// A stand-in Target app: it tells its version when asked, and otherwise
/// asks for a line and says it back.
const STAND_IN_CLAUDE: &str = "\
@echo off\r
if \"%1\"==\"--version\" (\r
  echo 9.8.7 ^(Claude Code^)\r
  exit /b 0\r
)\r
set /p said=say: \r
echo heard %said%\r
";

#[test]
fn farcli_records_its_session_to_the_file_it_is_told_to() {
    let folder = env::temp_dir().join(format!("farcli-record-test-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("claude.cmd"), STAND_IN_CLAUDE).unwrap();
    let file = folder.join("session.farcli-recording");
    let mut path = OsString::from(&folder);
    path.push(";");
    path.push(env::var_os("PATH").unwrap());
    // This test is alone in its file, so nothing else reads these.
    env::set_var("PATH", path);
    env::set_var("FARCLI_RECORD", &file);

    let size = Size { cols: 40, rows: 10 };
    let farcli = OsString::from(env!("CARGO_BIN_EXE_farcli"));
    let mut app = RunningApp::start(&farcli, &[OsString::from("claude")], size).unwrap();
    let mut output = app.take_output();
    let mut input = app.take_input();
    let app = Arc::new(app);
    let terminal = Arc::new(Mutex::new(FakeTerminal::new(size)));
    let reader = {
        let terminal = Arc::clone(&terminal);
        let mut answers = input.try_clone().unwrap();
        thread::spawn(move || {
            let mut buffer = [0u8; 65536];
            while let Ok(count @ 1..) = output.read(&mut buffer) {
                let bytes = &buffer[..count];
                terminal.lock().unwrap().receive(bytes);
                // The hidden terminal asks where the cursor is before it
                // starts the program.
                if bytes.windows(4).any(|window| window == b"\x1b[6n") {
                    answers.write_all(b"\x1b[1;1R").unwrap();
                }
            }
        })
    };
    let waiter = {
        let app = Arc::clone(&app);
        thread::spawn(move || app.wait())
    };

    let shows = |wanted: &str| {
        let terminal = terminal.lock().unwrap();
        terminal
            .screen_text()
            .iter()
            .any(|row| row.contains(wanted))
    };
    let give_up_at = Instant::now() + Duration::from_secs(60);
    while !shows("say:") {
        assert!(Instant::now() < give_up_at, "the app never asked");
        thread::sleep(Duration::from_millis(20));
    }
    input.write_all(b"hi\r").unwrap();
    while !waiter.is_finished() {
        assert!(Instant::now() < give_up_at, "farcli never ended");
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(waiter.join().unwrap().unwrap(), 0);
    reader.join().unwrap();
    assert!(shows("heard hi"));

    let recording = Recording::load(&file).unwrap();
    fs::remove_dir_all(&folder).unwrap();

    assert_eq!(recording.about.app, "claude");
    assert_eq!(recording.about.app_version, "9.8.7 (Claude Code)");
    assert_eq!(recording.about.system, "Windows native");
    assert_eq!(recording.about.size, size);
    assert_eq!(recording.about.cursor, Position { row: 0, col: 0 });
    assert!(recording
        .note("windows-version")
        .is_some_and(|version| version.contains("Windows")));
    assert_eq!(recording.note("command"), Some("claude"));

    let mut replay = Replay::start(&recording, AppProfile::plain());
    replay.finish();
    assert_eq!(replay.terminal.screen_text()[..2], ["say: hi", "heard hi"]);
    // What the app received is what the user typed, as farcli received it.
    let typed: Vec<u8> = recording
        .events
        .iter()
        .filter_map(|(_, event)| match event {
            Event::UserTyped(bytes) => Some(bytes.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(!typed.is_empty());
    assert!(replay.app_received.ends_with(&typed));
}
