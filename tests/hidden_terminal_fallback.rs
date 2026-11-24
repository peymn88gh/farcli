//! When the hidden terminal farcli ships cannot be made ready, farcli says
//! so in one line and runs the Target app on the one built into Windows.
//!
//! The test runs the real `farcli` program on a hidden terminal, in an
//! account whose local application data cannot take the files.
//!
//! This file holds one test because it sets environment variables for the
//! programs it starts.
#![cfg(windows)]

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Read, Write};
use std::thread;

use farcli::app_runner::RunningApp;
use farcli::fake_terminal::FakeTerminal;
use farcli::hidden_terminal::HiddenTerminal;
use farcli::Size;

#[test]
fn farcli_says_in_one_line_that_it_fell_back_and_still_runs_the_app() {
    let folder = env::temp_dir().join(format!("farcli-fallback-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    // Setup has already run on this pretend machine.
    let machine = folder.join("machine");
    fs::create_dir_all(&machine).unwrap();
    fs::write(machine.join("setup-done"), "made by a test\n").unwrap();
    // A file lies where farcli's folder would go.
    let local = folder.join("local");
    fs::create_dir_all(&local).unwrap();
    fs::write(local.join("farcli"), "in the way").unwrap();
    env::set_var("FARCLI_PRETEND_MACHINE", &machine);
    env::set_var("LOCALAPPDATA", &local);

    let size = Size {
        cols: 400,
        rows: 20,
    };
    let args = ["cmd", "/d", "/c", "echo the app ran& exit 3"].map(OsString::from);
    let mut farcli =
        RunningApp::start(OsStr::new(env!("CARGO_BIN_EXE_farcli")), &args, size).unwrap();
    // The stand-in for the Terminal could not unpack the files either.
    assert_eq!(farcli.hidden_terminal(), HiddenTerminal::BuiltIn);
    let mut output = farcli.take_output();
    let mut input = farcli.take_input();
    let waiter = thread::spawn(move || farcli.wait());
    let mut terminal = FakeTerminal::new(size);
    let mut buffer = [0u8; 65536];
    while let Ok(count @ 1..) = output.read(&mut buffer) {
        let bytes = &buffer[..count];
        terminal.receive(bytes);
        if bytes.windows(4).any(|window| window == b"\x1b[6n") {
            let cursor = terminal.cursor();
            let answer = format!("\x1b[{};{}R", cursor.row + 1, cursor.col + 1);
            input.write_all(answer.as_bytes()).unwrap();
        }
    }
    let status = waiter.join().unwrap().unwrap();
    fs::remove_dir_all(&folder).unwrap();

    let shown = terminal.screen_text();
    assert!(
        shown[0].starts_with("farcli: using the hidden terminal built into Windows"),
        "{shown:?}"
    );
    assert!(shown[0].contains("Half-space"), "{shown:?}");
    // The reason, which names what was in the way.
    assert!(shown[0].contains("farcli"), "{shown:?}");
    // One line, and then the app as usual.
    assert_eq!(shown[1], "the app ran", "{shown:?}");
    assert_eq!(status, 3);
}
