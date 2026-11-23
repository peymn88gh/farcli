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
//! example `hex:1b5b41`), `resize:COLSxROWS`, `screen:` (print what the
//! Terminal shows now), `timed:TEXT` (type, and print how long the program
//! took to send anything back), `keys:TEXT` (type one character at a time, as
//! a person does), `paste:TEXT` (as a Terminal pastes it, between the marks
//! `ESC[200~` and `ESC[201~`, a few characters at a time), `quiet:MILLISECONDS`
//! (wait until the program has sent nothing for that long), `until:TEXT` (wait
//! until the Terminal shows TEXT). In TEXT, `\u{200c}` stands for the
//! character with that number, so a Half-space can be seen in a steps file.
//! After the last step the probe waits for the program to end.

use std::env;
use std::ffi::OsString;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{Read, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use farcli::app_runner::RunningApp;
use farcli::fake_terminal::FakeTerminal;
use farcli::Size;

enum Step {
    Wait(u64),
    Keys(Vec<u8>),
    Resize(Size),
    Screen,
    Timed(Vec<u8>),
    KeyByKey(String),
    Paste(String),
    Quiet(u64),
    Until(String),
}

/// The longest `quiet:` and `until:` wait for.
const LONGEST_WAIT: Duration = Duration::from_secs(180);

/// The pause between two characters of `keys:`.
const BETWEEN_KEYS: Duration = Duration::from_millis(70);

/// How many characters of a paste are written at once, and the pause
/// before the next lot. A hidden terminal takes a key code that arrives in
/// two pieces for separate keys, and a paste written all at once reaches
/// the program under test faster than it passes the keys on.
const PASTE_LOT: usize = 4;
const BETWEEN_PASTE_LOTS: Duration = Duration::from_millis(30);

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
    let reader = {
        let terminal = Arc::clone(&terminal);
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
            Step::Keys(bytes) => {
                let _ = input.write_all(&bytes);
            }
            Step::Resize(size) => {
                terminal.lock().unwrap().resize(size);
                app.resize(size).expect("could not resize");
            }
            Step::Screen => print_screen("screen now", &terminal.lock().unwrap()),
            Step::KeyByKey(text) => {
                for character in text.chars() {
                    let _ = input.write_all(&typed(&character.to_string()));
                    thread::sleep(BETWEEN_KEYS);
                }
            }
            Step::Paste(text) => {
                let _ = input.write_all(b"\x1b[200~");
                thread::sleep(BETWEEN_PASTE_LOTS);
                let characters: Vec<char> = text.chars().collect();
                for lot in characters.chunks(PASTE_LOT) {
                    let lot: String = lot.iter().collect();
                    let _ = input.write_all(&typed(&lot));
                    thread::sleep(BETWEEN_PASTE_LOTS);
                }
                let _ = input.write_all(b"\x1b[201~");
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
        "type" => Step::Keys(typed(&with_numbered_characters(value))),
        "keys" => Step::KeyByKey(with_numbered_characters(value)),
        "paste" => Step::Paste(with_numbered_characters(value)),
        "quiet" => Step::Quiet(value.parse().expect("milliseconds must be a number")),
        "until" => Step::Until(with_numbered_characters(value)),
        "hex" => Step::Keys(
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
/// Plain English characters are written as they are, and the hidden
/// terminal finds the key for each. For any other character it would make
/// up a different key sequence (Alt held down while its number is typed),
/// which programs treat differently from a key of the user's keyboard
/// layout. So such a character is written as the key code a keyboard layout
/// sends: one key going down and up that carries the character.
fn typed(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for character in text.chars() {
        if character.is_ascii() {
            bytes.push(character as u8);
            continue;
        }
        for unit in character.encode_utf16(&mut [0; 2]) {
            bytes.extend(format!("\x1b[0;0;{unit};1;0;1_\x1b[0;0;{unit};0;0;1_").into_bytes());
        }
    }
    bytes
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
