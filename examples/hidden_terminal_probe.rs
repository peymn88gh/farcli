//! Developer tool: runs a program on a hidden terminal with nobody at the
//! keyboard, plays scripted keys and resizes into it, and prints what a
//! Terminal would show. It stands in for a person at a Terminal when
//! checking the real `farcli` binary:
//!
//! ```text
//! cargo run --example hidden_terminal_probe -- [OPTIONS] -- PROGRAM [ARGS]...
//!
//!   --size COLSxROWS        size of the stand-in Terminal (80x24 unless given)
//!   --shown TEXT            what the Terminal shows before the program starts
//!   --do STEP               a step to play; may be given many times
//!   --steps FILE            steps to play, one per line (`#` starts a comment)
//!   --give-up-after MS      stop waiting for the program (20000 unless given)
//!   --raw                   also print every byte the program sends, as it comes
//! ```
//!
//! Steps run in order: `wait:MILLISECONDS`, `type:TEXT`, `hex:BYTES` (for
//! example `hex:1b5b41`, written as they are), `resize:COLSxROWS`, `screen:`
//! (print what the Terminal shows now), `timed:TEXT` (type, and print how
//! long the program took to send anything back), `keys:TEXT` (type one
//! character at a time, as a person does), `press:KEY` (one key that types
//! no character: `left`, `right`, `up`, `down`, `home`, `end`, `delete`,
//! `backspace`, `page-up`, `page-down`, `enter`, `tab`, `escape`, or a letter
//! with Ctrl held, as `ctrl+c`), `paste:TEXT` (as Windows Terminal pastes it:
//! the text as
//! it is, all at once, and between the marks `ESC[200~` and `ESC[201~` if
//! the program has asked for them), `quiet:MILLISECONDS`
//! (wait until the program has sent nothing for that long), `until:TEXT` (wait
//! until the Terminal shows TEXT), `need:TEXT` (stop at once, playing no
//! further step, unless the Terminal shows TEXT now: put it before a step
//! that must not be played on the wrong screen, such as the Enter that sends
//! a message). In TEXT, `\u{200c}` stands for the
//! character with that number, so a Half-space can be seen in a steps file.
//! After the last step the probe waits for the program to end.
//!
//! `type:`, `keys:` and `press:` write what Windows Terminal sends for the
//! keys: to a program that asked for keys in the Windows form, as `farcli`
//! does, a code for each key going down and coming up.

use std::env;
use std::ffi::OsString;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{Read, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use farcli::app_runner::RunningApp;
use farcli::fake_terminal::FakeTerminal;
use farcli::Size;

enum Step {
    Wait(u64),
    Bytes(Vec<u8>),
    Type(String),
    Press(Key),
    Resize(Size),
    Screen,
    Timed(Vec<u8>),
    KeyByKey(String),
    Paste(String),
    Quiet(u64),
    Until(String),
    Need(String),
}

/// The longest `quiet:` and `until:` wait for.
const LONGEST_WAIT: Duration = Duration::from_secs(180);

/// The pause between two characters of `keys:`.
const BETWEEN_KEYS: Duration = Duration::from_millis(70);

/// What a program writes to ask the Terminal for its text colour and its
/// background colour, and what the stand-in Terminal answers.
const TERMINAL_COLOURS: [(&[u8], &[u8]); 2] = [
    (b"\x1b]10;?", b"\x1b]10;rgb:cccc/cccc/cccc\x1b\\"),
    (b"\x1b]11;?", b"\x1b]11;rgb:0c0c/0c0c/0c0c\x1b\\"),
];

/// Keeps a probed `farcli` from running Setup on the real settings of
/// whoever is probing: unless `FARCLI_PRETEND_MACHINE` is already set, it
/// is pointed at a pretend machine on which Setup has already run.
fn keep_setup_away_from_real_settings() {
    if env::var_os("FARCLI_PRETEND_MACHINE").is_some() {
        return;
    }
    let machine = env::temp_dir().join("farcli-probe-machine");
    std::fs::create_dir_all(&machine).expect("could not make a pretend machine");
    std::fs::write(machine.join("setup-done"), "made by the probe\n")
        .expect("could not make a pretend machine");
    env::set_var("FARCLI_PRETEND_MACHINE", machine);
}

fn main() -> ExitCode {
    keep_setup_away_from_real_settings();
    let mut size = Size { cols: 80, rows: 24 };
    let mut shown = String::new();
    let mut steps = Vec::new();
    let mut give_up_after = 20_000u64;
    let mut print_raw = false;
    let mut args = env::args_os().skip(1);
    let mut command: Vec<OsString> = Vec::new();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--") => {
                command = args.collect();
                break;
            }
            Some("--size") => size = parse_size(&next_text(&mut args)),
            Some("--shown") => shown = next_text(&mut args),
            Some("--do") => steps.push(parse_step(&next_text(&mut args))),
            Some("--steps") => {
                let file = next_text(&mut args);
                let listed = std::fs::read_to_string(&file).expect("could not read the steps");
                steps.extend(
                    listed
                        .lines()
                        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
                        .map(parse_step),
                );
            }
            Some("--give-up-after") => {
                give_up_after = next_text(&mut args).parse().expect("milliseconds")
            }
            Some("--raw") => print_raw = true,
            _ => panic!("unknown option {arg:?}"),
        }
    }
    let (program, program_args) = command.split_first().expect("no program given after --");

    let mut terminal = FakeTerminal::new(size);
    terminal.receive(shown.replace("\\n", "\r\n").as_bytes());
    let terminal = Arc::new(Mutex::new(terminal));

    let mut app = RunningApp::start(program, program_args, size).expect("could not start");
    let mut output = app.take_output();
    let mut input = app.take_input();
    let app = Arc::new(app);

    let pieces = Arc::new(AtomicU64::new(0));
    let last_piece = Arc::new(Mutex::new(Instant::now()));
    let wants_paste_marks = Arc::new(AtomicBool::new(false));
    let wants_windows_keys = Arc::new(AtomicBool::new(false));
    let reader = {
        let terminal = Arc::clone(&terminal);
        let wants_paste_marks = Arc::clone(&wants_paste_marks);
        let wants_windows_keys = Arc::clone(&wants_windows_keys);
        let mut answers = input.try_clone().expect("could not share the input");
        let pieces = Arc::clone(&pieces);
        let last_piece = Arc::clone(&last_piece);
        thread::spawn(move || {
            let mut all = Vec::new();
            let mut buffer = [0u8; 65536];
            loop {
                let count = match output.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => count,
                };
                let bytes = &buffer[..count];
                if print_raw {
                    println!("{}", escaped(bytes));
                }
                all.extend_from_slice(bytes);
                // The program asks for marks around a paste, or for keys in
                // the Windows form, or stops asking. The last of the two it
                // wrote counts, and it may have been cut by the end of a
                // piece.
                let lately = &all[all.len().saturating_sub(bytes.len() + 8)..];
                let last = |code: &[u8]| lately.windows(code.len()).rposition(|at| at == code);
                for (wanted, asks, stops) in [
                    (&wants_paste_marks, b"\x1b[?2004h", b"\x1b[?2004l"),
                    (&wants_windows_keys, b"\x1b[?9001h", b"\x1b[?9001l"),
                ] {
                    match (last(asks), last(stops)) {
                        (Some(asked), stopped) if Some(asked) > stopped => {
                            wanted.store(true, Ordering::SeqCst)
                        }
                        (_, Some(_)) => wanted.store(false, Ordering::SeqCst),
                        _ => {}
                    }
                }
                pieces.fetch_add(1, Ordering::SeqCst);
                *last_piece.lock().unwrap() = Instant::now();
                let mut terminal = terminal.lock().unwrap();
                terminal.receive(bytes);
                // A real Terminal answers when asked where its cursor is,
                // and the hidden terminal asks before it starts the program.
                if bytes.windows(4).any(|window| window == b"\x1b[6n") {
                    let cursor = terminal.cursor();
                    // In one write: the hidden terminal takes a code that
                    // arrives in pieces for separate key presses.
                    let answer = format!("\x1b[{};{}R", cursor.row + 1, cursor.col + 1);
                    let _ = answers.write_all(answer.as_bytes());
                }
                // It also answers when asked what kind of terminal it is.
                // The hidden terminal farcli ships asks that as it starts
                // and waits a second for the answer; programs ask it too.
                if bytes.windows(3).any(|window| window == b"\x1b[c") {
                    let _ = answers.write_all(b"\x1b[?61c");
                }
                // And when asked for its own text and background colours.
                // Windows Terminal answers both (seen in the program files
                // of version 1.25), and Codex draws its Input box and the
                // messages sent on a background of their own only when it
                // has the answer. These are the colours of Windows
                // Terminal's default colour scheme.
                for (question, answer) in TERMINAL_COLOURS {
                    if bytes
                        .windows(question.len())
                        .any(|window| window == question)
                    {
                        let _ = answers.write_all(answer);
                    }
                }
            }
            all
        })
    };
    let waiter = {
        let app = Arc::clone(&app);
        thread::spawn(move || app.wait())
    };

    for step in steps {
        match step {
            Step::Wait(milliseconds) => thread::sleep(Duration::from_millis(milliseconds)),
            Step::Bytes(bytes) => {
                let _ = input.write_all(&bytes);
            }
            Step::Type(text) => {
                let windows_keys = wants_windows_keys.load(Ordering::SeqCst);
                let _ = input.write_all(&typed(&text, windows_keys));
            }
            Step::Press(key) => {
                let windows_keys = wants_windows_keys.load(Ordering::SeqCst);
                let _ = input.write_all(&key.pressed(windows_keys));
            }
            Step::Resize(size) => {
                terminal.lock().unwrap().resize(size);
                app.resize(size).expect("could not resize");
            }
            Step::Screen => print_screen("screen now", &terminal.lock().unwrap()),
            Step::KeyByKey(text) => {
                for character in text.chars() {
                    let windows_keys = wants_windows_keys.load(Ordering::SeqCst);
                    let _ = input.write_all(&typed(&character.to_string(), windows_keys));
                    thread::sleep(BETWEEN_KEYS);
                }
            }
            Step::Paste(text) => {
                // A Terminal marks a paste only for a program that asked.
                let marked = wants_paste_marks.load(Ordering::SeqCst);
                let mut paste = Vec::new();
                if marked {
                    paste.extend(b"\x1b[200~");
                }
                paste.extend(text.as_bytes());
                if marked {
                    paste.extend(b"\x1b[201~");
                }
                let _ = input.write_all(&paste);
            }
            Step::Quiet(milliseconds) => {
                let quiet = Duration::from_millis(milliseconds);
                let started = Instant::now();
                while last_piece.lock().unwrap().elapsed() < quiet {
                    if started.elapsed() > LONGEST_WAIT {
                        println!("--- it never went quiet ---");
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
            }
            Step::Until(wanted) => {
                let started = Instant::now();
                let shows = |terminal: &FakeTerminal| {
                    terminal
                        .screen_text()
                        .iter()
                        .any(|row| row.contains(&wanted))
                };
                while !shows(&terminal.lock().unwrap()) {
                    if started.elapsed() > LONGEST_WAIT {
                        println!("--- {wanted:?} never showed ---");
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
            }
            Step::Need(wanted) => {
                let terminal = terminal.lock().unwrap();
                if !terminal
                    .screen_text()
                    .iter()
                    .any(|row| row.contains(&wanted))
                {
                    print_screen("screen when stopping", &terminal);
                    println!("--- stopped: {wanted:?} is not on screen ---");
                    // Leaving closes the hidden terminal, which ends the program.
                    std::process::exit(3);
                }
            }
            Step::Timed(bytes) => {
                let before = pieces.load(Ordering::SeqCst);
                let pressed = Instant::now();
                let _ = input.write_all(&bytes);
                while pieces.load(Ordering::SeqCst) == before
                    && pressed.elapsed() < Duration::from_secs(2)
                {
                    thread::yield_now();
                }
                println!(
                    "--- answered after {} microseconds ---",
                    pressed.elapsed().as_micros()
                );
            }
        }
    }

    let give_up_at = Instant::now() + Duration::from_millis(give_up_after);
    while !waiter.is_finished() {
        if Instant::now() > give_up_at {
            print_screen("screen when giving up", &terminal.lock().unwrap());
            println!("--- gave up: the program was still running ---");
            // Leaving closes the hidden terminal, which ends the program.
            std::process::exit(2);
        }
        thread::sleep(Duration::from_millis(20));
    }
    let status = waiter.join().unwrap().expect("could not wait");
    let all = reader.join().unwrap();
    println!("--- the program sent {} bytes ---", all.len());
    print_screen("screen at the end", &terminal.lock().unwrap());
    println!("--- exit status {status} ---");
    ExitCode::SUCCESS
}

fn print_screen(title: &str, terminal: &FakeTerminal) {
    let scrollback = terminal.scrollback_text();
    if !scrollback.is_empty() {
        println!("--- scrolled off the top ---");
        for row in scrollback {
            println!("|{row}");
        }
    }
    println!("--- {title} ---");
    for row in terminal.screen_text() {
        println!("|{row}");
    }
    // Changes whenever a character, colour or style anywhere on screen does.
    let mut fingerprint = DefaultHasher::new();
    format!("{:?}", terminal.screen()).hash(&mut fingerprint);
    println!("--- fingerprint {:016x} ---", fingerprint.finish());
    let cursor = terminal.cursor();
    println!(
        "--- cursor at row {} column {}, {} ---",
        cursor.row,
        cursor.col,
        if terminal.cursor_visible() {
            "visible"
        } else {
            "hidden"
        }
    );
}

fn next_text(args: &mut impl Iterator<Item = OsString>) -> String {
    args.next()
        .and_then(|value| value.into_string().ok())
        .expect("an option is missing its value")
}

fn parse_size(text: &str) -> Size {
    let (cols, rows) = text.split_once('x').expect("a size looks like 80x24");
    Size {
        cols: cols.parse().expect("columns must be a number"),
        rows: rows.parse().expect("rows must be a number"),
    }
}

fn parse_step(text: &str) -> Step {
    let (kind, value) = text.split_once(':').expect("a step looks like kind:value");
    match kind {
        "wait" => Step::Wait(value.parse().expect("milliseconds must be a number")),
        "type" => Step::Type(with_numbered_characters(value)),
        "press" => Step::Press(Key::named(value)),
        "keys" => Step::KeyByKey(with_numbered_characters(value)),
        "paste" => Step::Paste(with_numbered_characters(value)),
        "quiet" => Step::Quiet(value.parse().expect("milliseconds must be a number")),
        "until" => Step::Until(with_numbered_characters(value)),
        "need" => Step::Need(with_numbered_characters(value)),
        "hex" => Step::Bytes(
            (0..value.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&value[at..at + 2], 16).expect("bad hex"))
                .collect(),
        ),
        "resize" => Step::Resize(parse_size(value)),
        "screen" => Step::Screen,
        "timed" => Step::Timed(value.as_bytes().to_vec()),
        _ => panic!("unknown step {kind}"),
    }
}

/// What to write to the hidden terminal for `text` typed on a keyboard.
///
/// Windows Terminal sends a program that asked for keys in the Windows form
/// (`ESC[?9001h`, which `farcli` asks) a code for every key going down and
/// coming up: `ESC [ key ; scan code ; character ; down ; modifiers ;
/// repeats _`. With `windows_keys` the English characters of `text` are
/// written that way, as the keys of a US keyboard. Without it they are
/// written as they are, which is what a Terminal sends a program that did
/// not ask.
///
/// Any other character is always written as a key code that carries the
/// character and no key number. Written plain, the hidden terminal would
/// make up a key sequence for it (Alt held down while its number is typed),
/// which programs treat differently from a key of the user's keyboard
/// layout.
fn typed(text: &str, windows_keys: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    for character in text.chars() {
        match us_key(character) {
            Some((key, scan_code, shifted)) if windows_keys => {
                let modifiers = if shifted { SHIFT_HELD } else { 0 };
                if shifted {
                    key_code(&mut bytes, SHIFT_KEY, 0, true, SHIFT_HELD);
                }
                let key = (key, scan_code);
                key_code(&mut bytes, key, character as u16, true, modifiers);
                key_code(&mut bytes, key, character as u16, false, modifiers);
                if shifted {
                    key_code(&mut bytes, SHIFT_KEY, 0, false, 0);
                }
            }
            _ if character.is_ascii() => bytes.push(character as u8),
            _ => {
                for unit in character.encode_utf16(&mut [0; 2]) {
                    key_code(&mut bytes, (0, 0), *unit, true, 0);
                    key_code(&mut bytes, (0, 0), *unit, false, 0);
                }
            }
        }
    }
    bytes
}

/// Windows' number and the keyboard's own number for the left Shift key and
/// the left Ctrl key, and the modifier bits that say one of them is held.
const SHIFT_KEY: (u16, u16) = (16, 42);
const CTRL_KEY: (u16, u16) = (17, 29);
const SHIFT_HELD: u32 = 0x10;
const CTRL_HELD: u32 = 0x08;
/// The modifier bit of a key that is not on the main block of the keyboard:
/// the arrows and the keys above them.
const OFF_THE_MAIN_BLOCK: u32 = 0x100;

/// Adds the code for one key going down or coming up, in the Windows form.
fn key_code(bytes: &mut Vec<u8>, key: (u16, u16), character: u16, down: bool, modifiers: u32) {
    let (key, scan_code) = key;
    let down = u8::from(down);
    bytes.extend(format!("\x1b[{key};{scan_code};{character};{down};{modifiers};1_").into_bytes());
}

/// The key of a US keyboard that types an English character: Windows'
/// number for it, the keyboard's own number, and whether Shift is held.
fn us_key(character: char) -> Option<(u16, u16, bool)> {
    // Each row of the keyboard: the keyboard's number of its first key, what
    // the keys type, what they type with Shift, and Windows' numbers.
    const ROWS: [(u16, &str, &str, &[u16]); 5] = [
        (
            2,
            "1234567890-=",
            "!@#$%^&*()_+",
            &[49, 50, 51, 52, 53, 54, 55, 56, 57, 48, 189, 187],
        ),
        (
            16,
            "qwertyuiop[]",
            "QWERTYUIOP{}",
            &[81, 87, 69, 82, 84, 89, 85, 73, 79, 80, 219, 221],
        ),
        (
            30,
            "asdfghjkl;'`",
            "ASDFGHJKL:\"~",
            &[65, 83, 68, 70, 71, 72, 74, 75, 76, 186, 222, 192],
        ),
        (
            43,
            "\\zxcvbnm,./",
            "|ZXCVBNM<>?",
            &[220, 90, 88, 67, 86, 66, 78, 77, 188, 190, 191],
        ),
        (57, " ", "", &[32]),
    ];
    for (first, plain, with_shift, keys) in ROWS {
        for (typed, shifted) in [(plain, false), (with_shift, true)] {
            if let Some(at) = typed.chars().position(|c| c == character) {
                return Some((keys[at], first + at as u16, shifted));
            }
        }
    }
    None
}

/// A key that types no character of its own, or a letter with Ctrl held:
/// what `press:` takes.
struct Key {
    /// Windows' number for the key and the keyboard's own.
    key: (u16, u16),
    character: u16,
    modifiers: u32,
    /// What a Terminal sends for it to a program that did not ask for keys
    /// in the Windows form.
    plain: Vec<u8>,
}

impl Key {
    fn named(name: &str) -> Key {
        if let Some(letter) = name.strip_prefix("ctrl+") {
            let letter = letter.chars().next().expect("ctrl+ needs a letter");
            let (key, scan_code, _) = us_key(letter).expect("ctrl+ needs an English letter");
            let character = letter.to_ascii_uppercase() as u16 & 0x1f;
            return Key {
                key: (key, scan_code),
                character,
                modifiers: CTRL_HELD,
                plain: vec![character as u8],
            };
        }
        let (key, character, modifiers, plain): (_, u16, _, &[u8]) = match name {
            "left" => ((37, 75), 0, OFF_THE_MAIN_BLOCK, b"\x1b[D"),
            "right" => ((39, 77), 0, OFF_THE_MAIN_BLOCK, b"\x1b[C"),
            "up" => ((38, 72), 0, OFF_THE_MAIN_BLOCK, b"\x1b[A"),
            "down" => ((40, 80), 0, OFF_THE_MAIN_BLOCK, b"\x1b[B"),
            "home" => ((36, 71), 0, OFF_THE_MAIN_BLOCK, b"\x1b[H"),
            "end" => ((35, 79), 0, OFF_THE_MAIN_BLOCK, b"\x1b[F"),
            "delete" => ((46, 83), 0, OFF_THE_MAIN_BLOCK, b"\x1b[3~"),
            "page-up" => ((33, 73), 0, OFF_THE_MAIN_BLOCK, b"\x1b[5~"),
            "page-down" => ((34, 81), 0, OFF_THE_MAIN_BLOCK, b"\x1b[6~"),
            "backspace" => ((8, 14), 8, 0, b"\x7f"),
            "tab" => ((9, 15), 9, 0, b"\t"),
            "enter" => ((13, 28), 13, 0, b"\r"),
            "escape" => ((27, 1), 27, 0, b"\x1b"),
            _ => panic!("unknown key {name}"),
        };
        Key {
            key,
            character,
            modifiers,
            plain: plain.to_vec(),
        }
    }

    /// What Windows Terminal sends when the key is pressed and let go.
    fn pressed(&self, windows_keys: bool) -> Vec<u8> {
        if !windows_keys {
            return self.plain.clone();
        }
        let mut bytes = Vec::new();
        let with_ctrl = self.modifiers & CTRL_HELD != 0;
        if with_ctrl {
            key_code(&mut bytes, CTRL_KEY, 0, true, CTRL_HELD);
        }
        key_code(&mut bytes, self.key, self.character, true, self.modifiers);
        key_code(&mut bytes, self.key, self.character, false, self.modifiers);
        if with_ctrl {
            key_code(&mut bytes, CTRL_KEY, 0, false, 0);
        }
        bytes
    }
}

/// Replaces each `\u{NUMBER}` in `text` by the character with that number.
fn with_numbered_characters(text: &str) -> String {
    let mut result = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("\\u{") {
        let (before, numbered) = rest.split_at(at);
        let end = numbered.find('}').expect("a numbered character ends in }");
        let number = u32::from_str_radix(&numbered[3..end], 16).expect("bad character number");
        result.push_str(before);
        result.push(char::from_u32(number).expect("no such character"));
        rest = &numbered[end + 1..];
    }
    result.push_str(rest);
    result
}

/// Shows control bytes as visible text, one line of output per line feed.
fn escaped(bytes: &[u8]) -> String {
    let mut text = String::new();
    for character in String::from_utf8_lossy(bytes).chars() {
        match character {
            '\x1b' => text.push_str("\\e"),
            '\r' => text.push_str("\\r"),
            '\n' => text.push_str("\\n\n"),
            control if control.is_control() => {
                text.push_str(&format!("\\x{:02x}", control as u32));
            }
            other => text.push(other),
        }
    }
    text
}
