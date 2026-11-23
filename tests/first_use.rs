//! Setup runs by itself the first time farcli is used in a Terminal.
//!
//! The test runs the real `farcli` program on a hidden terminal, the way a
//! person would run it in a Terminal, and reads what that Terminal would
//! show. `FARCLI_PRETEND_MACHINE` names a temporary folder that stands in
//! for the machine, so no real settings, fonts or registry are read or
//! changed.
//!
//! This file holds one test because it sets an environment variable for the
//! programs it starts.
#![cfg(windows)]

mod pretend_pc;

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use farcli::app_runner::RunningApp;
use farcli::fake_terminal::FakeTerminal;
use farcli::Size;
use pretend_pc::{assert_same_file, fixture, Pc};

/// Runs `farcli` with `args` in a Terminal with nobody at the keyboard.
/// Returns everything the Terminal showed, top to bottom, and the exit
/// status.
fn run_farcli_in_a_terminal(args: &[&str]) -> (String, i32) {
    let size = Size {
        cols: 200,
        rows: 50,
    };
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let mut farcli =
        RunningApp::start(OsStr::new(env!("CARGO_BIN_EXE_farcli")), &args, size).unwrap();
    let mut output = farcli.take_output();
    let mut input = farcli.take_input();
    let terminal = Arc::new(Mutex::new(FakeTerminal::new(size)));
    let reader = {
        let terminal = Arc::clone(&terminal);
        thread::spawn(move || {
            let mut buffer = [0u8; 65536];
            while let Ok(count @ 1..) = output.read(&mut buffer) {
                let bytes = &buffer[..count];
                let mut terminal = terminal.lock().unwrap();
                terminal.receive(bytes);
                // A real Terminal answers when asked where its cursor is,
                // and the hidden terminal asks before it starts the program.
                if bytes.windows(4).any(|window| window == b"\x1b[6n") {
                    let cursor = terminal.cursor();
                    let answer = format!("\x1b[{};{}R", cursor.row + 1, cursor.col + 1);
                    let _ = input.write_all(answer.as_bytes());
                }
            }
        })
    };
    let farcli = Arc::new(farcli);
    let waiter = {
        let farcli = Arc::clone(&farcli);
        thread::spawn(move || farcli.wait())
    };
    let give_up_at = Instant::now() + Duration::from_secs(30);
    while !waiter.is_finished() {
        assert!(Instant::now() < give_up_at, "farcli did not end");
        thread::sleep(Duration::from_millis(20));
    }
    let status = waiter.join().unwrap().unwrap();
    drop(farcli);
    reader.join().unwrap();
    let terminal = terminal.lock().unwrap();
    let mut shown = terminal.scrollback_text();
    shown.extend(terminal.screen_text());
    (shown.join("\n"), status)
}

#[test]
fn setup_runs_by_itself_the_first_time_farcli_is_used_and_not_the_second() {
    let pc = Pc::new().with_windows_terminal("windows-terminal/no-font.json");
    std::env::set_var("FARCLI_PRETEND_MACHINE", &pc.root);
    let app = ["cmd", "/d", "/c", "echo the app ran& exit 3"];

    let (first, first_status) = run_farcli_in_a_terminal(&app);
    let (second, second_status) = run_farcli_in_a_terminal(&app);

    // The first time: Setup's report, then the app as usual.
    assert!(first.contains("Cascadia Mono, Vazir Code WOL"), "{first}");
    assert!(first.contains("farcli-backup"), "{first}");
    assert!(first.contains("the app ran"), "{first}");
    assert_eq!(first_status, 3);
    assert_same_file(
        &pc.read("windows-terminal/settings.json"),
        &fixture("windows-terminal/no-font.after.json"),
    );
    // The second time: only the app.
    assert!(!second.contains("Vazir"), "{second}");
    assert!(second.contains("the app ran"), "{second}");
    assert_eq!(second_status, 3);
}
