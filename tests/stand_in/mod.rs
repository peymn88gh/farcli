//! Shared by the tests that run the real `farcli` on a hidden terminal with
//! nobody at the keyboard: a stand-in Terminal, which writes to the hidden
//! terminal what a Terminal writes for a paste, a key or a turn of the mouse
//! wheel, and a stand-in Target app, which writes down everything it is
//! given.
//!
//! The stand-in app is the test program itself, started again. A test file
//! that uses this module must hold a test named `stand_in_app` that calls
//! `stand_in::be_the_app_if_asked()`.
#![allow(dead_code)]

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
pub const PASTED: &str = "\
سلام! این متن را یک\u{200c}جا در farcli می\u{200c}چسبانیم تا ببینیم همان\u{200c}طور که هست به برنامه می\u{200c}رسد یا نه. \
The quick brown fox jumps over the lazy dog; 0123456789 and C:\\temp\\notes.txt [x] {y} (z).\r\
خط دوم: کتاب\u{200c}ها، «گیومه»، علامت سؤال؟ و عددهای ۱۲۳۴۵۶۷۸۹۰ هم باید سالم بمانند. \
Claude Code and Codex CLI both read keys one by one, so nothing may be lost, added or moved.\r\
و یک خط دیگر برای این\u{200c}که متن از چند صد نویسه بیشتر شود: می\u{200c}روم، نمی\u{200c}دانم، خانه\u{200c}ای.";

/// The marks a Terminal puts around a paste when the app has asked for them.
pub const PASTE_STARTS: &str = "\x1b[200~";
pub const PASTE_ENDS: &str = "\x1b[201~";

/// What Windows Terminal sends when the wheel is turned one notch up with
/// the pointer on the tenth column of the fifth row, to an app that asked
/// for mouse reports in the form both Target apps ask for.
pub const WHEEL_UP: &str = "\x1b[<64;10;5M";

/// What the stand-in app writes down for that turn of the wheel when it
/// asked Windows for the mouse. Windows counts columns and rows from
/// nought. Kind 4 is the wheel, and a turn away from the user has the top
/// bit of `buttons` clear.
pub const WHEEL_UP_AS_WINDOWS_TELLS_IT: &str = "[mouse column 9 row 4 buttons 0x800000 kind 0x4]";

/// The key that tells the stand-in app to stop listening: Ctrl+D.
const END_OF_KEYS: u16 = 4;

/// Windows' number for the Alt key.
const ALT_KEY: u16 = 18;

/// How the stand-in app reads what it is given. The two Target apps differ.
#[derive(Clone, Copy, PartialEq)]
pub enum Reads {
    /// One key at a time, as Windows keeps them. Codex reads this way: it
    /// never sees paste marks, and the mouse comes as a record of its own.
    Keys,
    /// Whatever the Terminal sent, as characters. Claude Code reads this
    /// way: paste marks and mouse reports arrive as the Terminal wrote them.
    Raw,
}

/// Not a test of farcli: the stand-in Target app. `hand_to_app` starts the
/// test program again with a file name after `keys-go-to=`. It asks for
/// paste marks as Claude Code and Codex do, and with `asks-for=mouse` for
/// mouse reports too. Then it listens, and on Ctrl+D writes every character
/// it was given to that file, with a note for each thing the mouse did.
///
/// It waits inside the call that reads keys, as a plain program does. The
/// hidden terminal built into Windows can leave such a program waiting with
/// keys in hand. With `ends=when-woken` the app listens, not for Ctrl+D,
/// but until a key that carries nothing comes up after it has heard
/// something: what farcli wakes an app with.
pub fn be_the_app_if_asked() {
    let given = |name: &str| env::args().find_map(|arg| arg.strip_prefix(name).map(String::from));
    let Some(file) = given("keys-go-to=").map(PathBuf::from) else {
        return;
    };
    let reads_raw = given("reads=").as_deref() == Some("raw");
    let wants_mouse = given("asks-for=").as_deref() == Some("mouse");
    let ends_when_woken = given("ends=").as_deref() == Some("when-woken");

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
                        let carries_nothing = key.wVirtualKeyCode == 0 && character == 0;
                        if ends_when_woken
                            && key.bKeyDown == 0
                            && carries_nothing
                            && !heard.is_empty()
                        {
                            break 'listening;
                        }
                        // A character typed as its number with Alt held
                        // down comes when Alt is let go.
                        let typed = key.bKeyDown != 0 || key.wVirtualKeyCode == ALT_KEY;
                        if !typed || character == 0 {
                            continue;
                        }
                        if character == END_OF_KEYS && !ends_when_woken {
                            break 'listening;
                        }
                        heard.extend(std::iter::repeat_n(character, key.wRepeatCount.into()));
                    }
                    MOUSE_EVENT => {
                        let mouse = record.Event.MouseEvent;
                        let note = format!(
                            "[mouse column {} row {} buttons {:#x} kind {:#x}]",
                            mouse.dwMousePosition.X,
                            mouse.dwMousePosition.Y,
                            mouse.dwButtonState,
                            mouse.dwEventFlags
                        );
                        heard.extend(note.encode_utf16());
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
pub fn as_keys(text: &str) -> Vec<u8> {
    let mut keys = String::new();
    for unit in text.encode_utf16() {
        keys.push_str(&format!("\x1b[0;0;{unit};1;0;1_\x1b[0;0;{unit};0;0;1_"));
    }
    keys.into_bytes()
}

/// The bytes Windows Terminal writes for `text` pasted into an app that has
/// asked for paste marks: the text as it is, between the two marks.
pub fn as_marked_paste(text: &str) -> Vec<u8> {
    format!("{PASTE_STARTS}{text}{PASTE_ENDS}").into_bytes()
}

/// The `farcli` program under test. Every `farcli` these tests start runs
/// on a pretend machine where Setup has already run, so that it leaves the
/// real settings of whoever runs the tests alone.
pub fn farcli() -> &'static Path {
    static PRETEND_MACHINE: Once = Once::new();
    PRETEND_MACHINE.call_once(|| {
        let machine = env::temp_dir().join("farcli-stand-in-machine");
        fs::create_dir_all(&machine).unwrap();
        fs::write(machine.join("setup-done"), "made by a test\n").unwrap();
        env::set_var("FARCLI_PRETEND_MACHINE", &machine);
    });
    Path::new(env!("CARGO_BIN_EXE_farcli"))
}

/// What came of handing a stand-in Terminal's input to the stand-in app.
pub struct Outcome {
    /// Every character the app was given, in order.
    pub heard: String,
    /// Every byte the stand-in Terminal was sent.
    pub terminal_received: Vec<u8>,
}

impl Outcome {
    pub fn terminal_was_asked_for(&self, mode: &str) -> bool {
        self.terminal_received
            .windows(mode.len())
            .any(|window| window == mode.as_bytes())
    }
}

/// Starts the stand-in app on a hidden terminal, behind `in_front` if any
/// is given, writes `sent` to the hidden terminal all at once as a Terminal
/// does, then the key that ends the app, and returns what the app heard.
pub fn hand_to_app(
    test: &str,
    in_front: Option<&Path>,
    reads: Reads,
    wants_mouse: bool,
    sent: &[u8],
) -> Outcome {
    hand_to_app_that_ends(Ends::OnCtrlD, test, in_front, reads, wants_mouse, sent)
}

/// The same, but after `sent` the Terminal sends nothing more: the app
/// listens until it is woken with a key that carries nothing.
pub fn hand_to_app_until_woken(
    test: &str,
    in_front: Option<&Path>,
    reads: Reads,
    sent: &[u8],
) -> Outcome {
    hand_to_app_that_ends(Ends::WhenWoken, test, in_front, reads, false, sent)
}

/// What makes the stand-in app stop listening.
#[derive(Clone, Copy, PartialEq)]
enum Ends {
    OnCtrlD,
    WhenWoken,
}

fn hand_to_app_that_ends(
    ends: Ends,
    test: &str,
    in_front: Option<&Path>,
    reads: Reads,
    wants_mouse: bool,
    sent: &[u8],
) -> Outcome {
    let folder = env::temp_dir().join(format!("farcli-stand-in-{}-{test}", std::process::id()));
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
    if ends == Ends::WhenWoken {
        command.push("ends=when-woken".into());
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
    if ends == Ends::OnCtrlD {
        input
            .write_all(&as_keys(&String::from_utf16_lossy(&[END_OF_KEYS])))
            .unwrap();
    }

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
