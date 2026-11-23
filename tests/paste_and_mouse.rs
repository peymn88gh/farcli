//! What the user pastes, types and does with the mouse reaches the Target
//! app as it does without farcli.
//!
//! These tests run the real `farcli` on a hidden terminal with nobody at the
//! keyboard. The test stands in for Windows Terminal: it writes to the
//! hidden terminal what Windows Terminal writes for a paste, a key or a turn
//! of the mouse wheel. A stand-in Target app writes down everything it is
//! given, and the test compares that with what was sent.
#![cfg(windows)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Once};
use std::thread;
use std::time::{Duration, Instant};

use farcli::app_runner::RunningApp;
use farcli::fake_terminal::FakeTerminal;
use farcli::Size;
use windows_sys::Win32::System::Console::{
    GetStdHandle, ReadConsoleInputW, SetConsoleMode, ENABLE_EXTENDED_FLAGS, ENABLE_MOUSE_INPUT,
    ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_WINDOW_INPUT, INPUT_RECORD, KEY_EVENT, MOUSE_EVENT,
    STD_INPUT_HANDLE,
};

/// Several hundred characters of Farsi and English, with Half-spaces
/// (`\u{200c}`), Farsi punctuation and digits, and line breaks, which a
/// Terminal pastes as the Enter key (`\r`).
const PASTED: &str = "\
سلام! این متن را یک\u{200c}جا در farcli می\u{200c}چسبانیم تا ببینیم همان\u{200c}طور که هست به برنامه می\u{200c}رسد یا نه. \
The quick brown fox jumps over the lazy dog; 0123456789 and C:\\temp\\notes.txt [x] {y} (z).\r\
خط دوم: کتاب\u{200c}ها، «گیومه»، علامت سؤال؟ و عددهای ۱۲۳۴۵۶۷۸۹۰ هم باید سالم بمانند. \
Claude Code and Codex CLI both read keys one by one, so nothing may be lost, added or moved.\r\
و یک خط دیگر برای این\u{200c}که متن از چند صد نویسه بیشتر شود: می\u{200c}روم، نمی\u{200c}دانم، خانه\u{200c}ای.";

/// The marks a Terminal puts around a paste when the app has asked for them.
const PASTE_STARTS: &str = "\x1b[200~";
const PASTE_ENDS: &str = "\x1b[201~";

/// The key that tells the stand-in app to stop listening: Ctrl+D.
const END_OF_KEYS: u16 = 4;

/// How the stand-in app reads what it is given. The two Target apps differ.
#[derive(Clone, Copy, PartialEq)]
enum Reads {
    /// One key at a time, as Windows keeps them. Codex reads this way: it
    /// never sees paste marks, and the mouse comes as a record of its own.
    Keys,
    /// Whatever the Terminal sent, as characters. Claude Code reads this
    /// way: paste marks and mouse reports arrive as the Terminal wrote them.
    Raw,
}

/// Not a test of farcli: the stand-in Target app. The tests below start
/// this test program again with a file name after `keys-go-to=`. It asks for
/// paste marks as Claude Code and Codex do, and with `asks-for=mouse` for
/// mouse reports too. Then it listens, and on Ctrl+D writes every character
/// it was given to that file, with a line for each thing the mouse did.
#[test]
fn stand_in_app() {
    let given = |name: &str| env::args().find_map(|arg| arg.strip_prefix(name).map(String::from));
    let Some(file) = given("keys-go-to=").map(PathBuf::from) else {
        return;
    };
    let reads_raw = given("reads=").as_deref() == Some("raw");
    let wants_mouse = given("asks-for=").as_deref() == Some("mouse");

    let mut heard: Vec<u16> = Vec::new();
    // SAFETY: `INPUT_RECORD` is plain data, the buffer pointer and length
    // match, and a record's union is read only as its type says.
    unsafe {
        let keyboard = GetStdHandle(STD_INPUT_HANDLE);
        let mut mode = ENABLE_WINDOW_INPUT;
        let mut asks = String::from("\x1b[?2004h");
        match (reads_raw, wants_mouse) {
            (true, false) => mode |= ENABLE_VIRTUAL_TERMINAL_INPUT,
            (true, true) => {
                mode |= ENABLE_VIRTUAL_TERMINAL_INPUT;
                asks.push_str("\x1b[?1000h\x1b[?1006h");
            }
            // Codex asks Windows, not the Terminal.
            (false, true) => mode |= ENABLE_MOUSE_INPUT | ENABLE_EXTENDED_FLAGS,
            (false, false) => {}
        }
        assert!(SetConsoleMode(keyboard, mode) != 0);
        print!("{asks}");
        println!("ready for keys");
        let mut records: [INPUT_RECORD; 64] = mem::zeroed();
        'listening: loop {
            let mut count = 0u32;
            assert!(ReadConsoleInputW(keyboard, records.as_mut_ptr(), 64, &mut count) != 0);
            for record in &records[..count as usize] {
                match u32::from(record.EventType) {
                    KEY_EVENT => {
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
                    MOUSE_EVENT => {
                        let mouse = record.Event.MouseEvent;
                        let line = format!(
                            "[mouse column {} row {} buttons {:#x} kind {:#x}]",
                            mouse.dwMousePosition.X,
                            mouse.dwMousePosition.Y,
                            mouse.dwButtonState,
                            mouse.dwEventFlags
                        );
                        heard.extend(line.encode_utf16());
                    }
                    _ => {}
                }
            }
        }
    }
    fs::write(file, String::from_utf16_lossy(&heard)).unwrap();
}

/// The bytes a hidden terminal is given for `text` typed fast: for each
/// character one key going down and coming up again, in Windows' own key
/// report format, which is how Windows Terminal sends keys.
fn as_keys(text: &str) -> Vec<u8> {
    let mut keys = String::new();
    for unit in text.encode_utf16() {
        keys.push_str(&format!("\x1b[0;0;{unit};1;0;1_\x1b[0;0;{unit};0;0;1_"));
    }
    keys.into_bytes()
}

/// The bytes Windows Terminal writes for `text` pasted into an app that has
/// asked for paste marks: the text as it is, between the two marks.
fn as_marked_paste(text: &str) -> Vec<u8> {
    format!("{PASTE_STARTS}{text}{PASTE_ENDS}").into_bytes()
}

/// Makes every `farcli` these tests start run on a pretend machine where
/// Setup has already run, so that it leaves the real settings of whoever
/// runs the tests alone.
fn farcli() -> &'static Path {
    static PRETEND_MACHINE: Once = Once::new();
    PRETEND_MACHINE.call_once(|| {
        let machine = env::temp_dir().join("farcli-paste-and-mouse-machine");
        fs::create_dir_all(&machine).unwrap();
        fs::write(machine.join("setup-done"), "made by a test\n").unwrap();
        env::set_var("FARCLI_PRETEND_MACHINE", &machine);
    });
    Path::new(env!("CARGO_BIN_EXE_farcli"))
}

/// What came of handing a stand-in Terminal's input to the stand-in app.
struct Outcome {
    /// Every character the app was given, in order.
    heard: String,
    /// Every byte the stand-in Terminal was sent.
    terminal_received: Vec<u8>,
}

impl Outcome {
    fn terminal_was_asked_for(&self, mode: &str) -> bool {
        self.terminal_received
            .windows(mode.len())
            .any(|window| window == mode.as_bytes())
    }
}

/// Starts the stand-in app on a hidden terminal, behind `in_front` if any
/// is given, writes `sent` to the hidden terminal all at once as a Terminal
/// does, and returns what the app heard.
fn hand_to_app(
    test: &str,
    in_front: Option<&Path>,
    reads: Reads,
    wants_mouse: bool,
    sent: &[u8],
) -> Outcome {
    let folder = env::temp_dir().join(format!("farcli-paste-test-{}-{test}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let heard = folder.join("heard.txt");

    let mut command: Vec<OsString> = in_front.iter().map(|program| program.into()).collect();
    command.push(env::current_exe().unwrap().into());
    command.push("stand_in_app".into());
    command.push(format!("keys-go-to={}", heard.display()).into());
    if reads == Reads::Raw {
        command.push("reads=raw".into());
    }
    if wants_mouse {
        command.push("asks-for=mouse".into());
    }
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
            let mut received = Vec::new();
            let mut buffer = [0u8; 65536];
            while let Ok(count @ 1..) = output.read(&mut buffer) {
                let bytes = &buffer[..count];
                received.extend_from_slice(bytes);
                terminal.lock().unwrap().receive(bytes);
                // The hidden terminal asks where the cursor is before it
                // starts the program, and what kind of terminal it is on.
                if bytes.windows(4).any(|window| window == b"\x1b[6n") {
                    answers.write_all(b"\x1b[1;1R").unwrap();
                }
                if bytes.windows(3).any(|window| window == b"\x1b[c") {
                    answers.write_all(b"\x1b[?61c").unwrap();
                }
            }
            received
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

    input.write_all(sent).unwrap();
    input
        .write_all(&as_keys(&String::from_utf16_lossy(&[END_OF_KEYS])))
        .unwrap();

    while !waiter.is_finished() {
        assert!(Instant::now() < give_up_at, "the app never ended");
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(waiter.join().unwrap().unwrap(), 0);
    let terminal_received = reader.join().unwrap();
    let heard = fs::read_to_string(heard).unwrap();
    fs::remove_dir_all(&folder).unwrap();
    Outcome {
        heard,
        terminal_received,
    }
}

#[test]
fn a_paste_of_several_lines_reaches_an_app_that_reads_what_the_terminal_sent_as_one_marked_paste() {
    let outcome = hand_to_app(
        "marked",
        Some(farcli()),
        Reads::Raw,
        false,
        &as_marked_paste(PASTED),
    );

    // Between the marks a line break is text, not the key that sends.
    assert_eq!(outcome.heard, format!("{PASTE_STARTS}{PASTED}{PASTE_ENDS}"));
}

#[test]
fn the_same_paste_reaches_an_app_that_reads_keys_as_it_does_without_farcli() {
    let through_farcli = hand_to_app(
        "unmarked",
        Some(farcli()),
        Reads::Keys,
        false,
        &as_marked_paste(PASTED),
    );
    let without_farcli = hand_to_app(
        "unmarked-straight",
        None,
        Reads::Keys,
        false,
        &as_marked_paste(PASTED),
    );

    // Windows keeps the marks from an app that reads keys. It gets the
    // text, and a line break as the Enter key.
    assert_eq!(through_farcli.heard, PASTED);
    assert_eq!(without_farcli.heard, PASTED);
}

/// A paste of more than 5 KiB, the size from which Windows Terminal asks
/// before pasting. Every line is numbered, so a piece that is lost, doubled
/// or moved shows.
fn large_paste() -> String {
    let lines: Vec<&str> = PASTED.split('\r').collect();
    let mut paste = String::new();
    for number in 1.. {
        if paste.len() > 6 * 1024 {
            break;
        }
        let line = lines[number % lines.len()];
        paste.push_str(&format!("{number}: {line}\r"));
    }
    paste
}

#[test]
fn a_paste_large_enough_for_the_terminal_to_ask_first_reaches_either_kind_of_app_whole() {
    let paste = large_paste();

    let read_raw = hand_to_app(
        "large-raw",
        Some(farcli()),
        Reads::Raw,
        false,
        &as_marked_paste(&paste),
    );
    let read_as_keys = hand_to_app(
        "large-keys",
        Some(farcli()),
        Reads::Keys,
        false,
        &as_marked_paste(&paste),
    );

    assert_eq!(read_raw.heard, format!("{PASTE_STARTS}{paste}{PASTE_ENDS}"));
    assert_eq!(read_as_keys.heard, paste);
}

/// What Windows Terminal sends when the wheel is turned one notch up with
/// the pointer on the tenth column of the fifth row, to an app that asked
/// for mouse reports in the form both Target apps ask for.
const WHEEL_UP: &str = "\x1b[<64;10;5M";

/// Every way an app can ask a Terminal for mouse reports.
const MOUSE_REQUESTS: [&str; 9] = [
    "\x1b[?9h",
    "\x1b[?1000h",
    "\x1b[?1001h",
    "\x1b[?1002h",
    "\x1b[?1003h",
    "\x1b[?1005h",
    "\x1b[?1006h",
    "\x1b[?1015h",
    "\x1b[?1016h",
];

#[test]
fn a_turn_of_the_wheel_reaches_an_app_that_asked_the_terminal_for_mouse_reports() {
    let outcome = hand_to_app(
        "wheel-raw",
        Some(farcli()),
        Reads::Raw,
        true,
        WHEEL_UP.as_bytes(),
    );

    // Claude Code's fullscreen display asks like this and scrolls on what
    // comes back.
    assert!(outcome.terminal_was_asked_for("\x1b[?1000h"));
    assert!(outcome.terminal_was_asked_for("\x1b[?1006h"));
    assert_eq!(outcome.heard, WHEEL_UP);
}

#[test]
fn a_turn_of_the_wheel_reaches_an_app_that_asked_windows_for_the_mouse() {
    let outcome = hand_to_app(
        "wheel-keys",
        Some(farcli()),
        Reads::Keys,
        true,
        WHEEL_UP.as_bytes(),
    );

    // Codex asks Windows, and the hidden terminal asks the Terminal for it.
    assert!(outcome.terminal_was_asked_for("\x1b[?1003h"));
    assert!(outcome.terminal_was_asked_for("\x1b[?1006h"));
    // Windows counts columns and rows from nought. Kind 4 is the wheel, and
    // a turn away from the user has the top bit of `buttons` clear.
    assert_eq!(
        outcome.heard,
        "[mouse column 9 row 4 buttons 0x800000 kind 0x4]"
    );
}

#[test]
fn an_app_that_did_not_ask_for_the_mouse_leaves_it_to_the_terminal() {
    let outcome = hand_to_app("no-mouse", Some(farcli()), Reads::Raw, false, b"x");

    // Nothing asked the Terminal for mouse reports, so selecting with the
    // mouse and scrolling back with the wheel stay the Terminal's own.
    for request in MOUSE_REQUESTS {
        assert!(!outcome.terminal_was_asked_for(request), "{request:?}");
    }
    assert_eq!(outcome.heard, "x");
}

#[test]
fn keys_typed_fast_through_farcli_reach_the_app_as_the_same_keys() {
    let outcome = hand_to_app(
        "typed-fast",
        Some(farcli()),
        Reads::Keys,
        false,
        &as_keys(PASTED),
    );

    assert_eq!(outcome.heard, PASTED);
}

#[test]
fn keys_written_to_the_hidden_terminal_all_at_once_reach_the_app_in_order() {
    let outcome = hand_to_app("straight", None, Reads::Keys, false, &as_keys(PASTED));

    assert_eq!(outcome.heard, PASTED);
}
