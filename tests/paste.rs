//! A paste reaches the Target app whole.
//!
//! A Terminal hands farcli a paste as a burst of keys: hundreds of them at
//! once. These tests run a stand-in Target app that writes down every
//! character it is given, feed it such a burst on a hidden terminal with
//! nobody at the keyboard, and compare what it heard with what was pasted.
#![cfg(windows)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use farcli::app_runner::RunningApp;
use farcli::fake_terminal::FakeTerminal;
use farcli::Size;
use windows_sys::Win32::System::Console::{
    GetStdHandle, ReadConsoleInputW, SetConsoleMode, ENABLE_WINDOW_INPUT, INPUT_RECORD, KEY_EVENT,
    STD_INPUT_HANDLE,
};

/// Several hundred characters of Farsi and English, with Half-spaces
/// (`\u{200c}`), Farsi punctuation and digits, and a line break, which a
/// Terminal pastes as the Enter key (`\r`).
const PASTED: &str = "\
سلام! این متن را یک\u{200c}جا در farcli می\u{200c}چسبانیم تا ببینیم همان\u{200c}طور که هست به برنامه می\u{200c}رسد یا نه. \
The quick brown fox jumps over the lazy dog; 0123456789 and C:\\temp\\notes.txt [x] {y} (z).\r\
خط دوم: کتاب\u{200c}ها، «گیومه»، علامت سؤال؟ و عددهای ۱۲۳۴۵۶۷۸۹۰ هم باید سالم بمانند. \
Claude Code and Codex CLI both read keys one by one, so nothing may be lost, added or moved.\r\
و یک خط دیگر برای این\u{200c}که متن از چند صد نویسه بیشتر شود: می\u{200c}روم، نمی\u{200c}دانم، خانه\u{200c}ای.";

/// The key that tells the stand-in app the paste is over: Ctrl+D.
const END_OF_KEYS: u16 = 4;

/// Not a test of farcli: the stand-in Target app. The tests below start
/// this test program again with a file name after `keys-go-to=`. It then
/// reads keys the way Claude Code and Codex do, one key record at a time,
/// and on Ctrl+D writes every character it was given to that file.
#[test]
fn stand_in_app() {
    let Some(file) = env::args().find_map(|arg| arg.strip_prefix("keys-go-to=").map(PathBuf::from))
    else {
        return;
    };
    let mut heard: Vec<u16> = Vec::new();
    // SAFETY: `INPUT_RECORD` is plain data, the buffer pointer and length
    // match, and a record's union is read only as its type says.
    unsafe {
        let keyboard = GetStdHandle(STD_INPUT_HANDLE);
        assert!(SetConsoleMode(keyboard, ENABLE_WINDOW_INPUT) != 0);
        println!("ready for keys");
        let mut records: [INPUT_RECORD; 64] = mem::zeroed();
        'listening: loop {
            let mut count = 0u32;
            assert!(ReadConsoleInputW(keyboard, records.as_mut_ptr(), 64, &mut count) != 0);
            for record in &records[..count as usize] {
                if u32::from(record.EventType) != KEY_EVENT {
                    continue;
                }
                let key = record.Event.KeyEvent;
                let character = key.uChar.UnicodeChar;
                if key.bKeyDown == 0 || character == 0 {
                    continue;
                }
                if character == END_OF_KEYS {
                    break 'listening;
                }
                heard.extend(std::iter::repeat_n(character, key.wRepeatCount.into()));
            }
        }
    }
    fs::write(file, String::from_utf16_lossy(&heard)).unwrap();
}

/// The bytes a hidden terminal is given for `text` typed or pasted: for
/// each character one key going down and coming up again, in Windows' own
/// key report format.
fn as_keys(text: &str) -> Vec<u8> {
    let mut keys = String::new();
    for unit in text.encode_utf16() {
        keys.push_str(&format!("\x1b[0;0;{unit};1;0;1_\x1b[0;0;{unit};0;0;1_"));
    }
    keys.into_bytes()
}

/// Starts the stand-in app on a hidden terminal, behind `in_front` if any
/// is given, hands the hidden terminal `pasted` all at once, and returns
/// the characters the app heard.
fn heard_by_app(test: &str, in_front: Option<&Path>, pasted: &str) -> String {
    let folder = env::temp_dir().join(format!("farcli-paste-test-{}-{test}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let heard = folder.join("heard.txt");

    let mut command: Vec<OsString> = in_front.iter().map(|program| program.into()).collect();
    command.push(env::current_exe().unwrap().into());
    command.push("stand_in_app".into());
    command.push(format!("keys-go-to={}", heard.display()).into());
    command.push("--exact".into());
    command.push("--nocapture".into());

    let size = Size { cols: 80, rows: 24 };
    let mut app = RunningApp::start(&command[0], &command[1..], size).unwrap();
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
    while !shows("ready for keys") {
        assert!(Instant::now() < give_up_at, "the app never started");
        thread::sleep(Duration::from_millis(20));
    }

    input.write_all(&as_keys(pasted)).unwrap();
    input
        .write_all(&as_keys(&String::from_utf16_lossy(&[END_OF_KEYS])))
        .unwrap();

    while !waiter.is_finished() {
        assert!(Instant::now() < give_up_at, "the app never ended");
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(waiter.join().unwrap().unwrap(), 0);
    reader.join().unwrap();
    let heard = fs::read_to_string(heard).unwrap();
    fs::remove_dir_all(&folder).unwrap();
    heard
}

#[test]
fn a_paste_through_farcli_reaches_the_app_exactly_as_pasted() {
    // On a pretend machine where Setup has already run, so that farcli
    // leaves the real settings of whoever runs the tests alone. No other
    // test in this file starts farcli, so nothing else reads this.
    let machine = env::temp_dir().join(format!("farcli-paste-machine-{}", std::process::id()));
    fs::create_dir_all(&machine).unwrap();
    fs::write(machine.join("setup-done"), "made by a test\n").unwrap();
    env::set_var("FARCLI_PRETEND_MACHINE", &machine);

    let heard = heard_by_app(
        "through-farcli",
        Some(Path::new(env!("CARGO_BIN_EXE_farcli"))),
        PASTED,
    );
    fs::remove_dir_all(&machine).unwrap();

    assert_eq!(heard, PASTED);
}

#[test]
fn keys_written_to_the_hidden_terminal_all_at_once_reach_the_app_in_order() {
    let heard = heard_by_app("straight", None, PASTED);

    assert_eq!(heard, PASTED);
}
