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
//!   --give-up-after MS      stop waiting for the program (20000 unless given)
//!   --raw                   also print every byte the program sends, as it comes
//! ```
//!
//! Steps run in order: `wait:MILLISECONDS`, `type:TEXT`, `hex:BYTES` (for
//! example `hex:1b5b41`), `resize:COLSxROWS`, `screen:` (print what the
//! Terminal shows now), `timed:TEXT` (type, and print how long the program
//! took to send anything back). After the last step the probe waits for the
//! program to end.

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
}

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
    let reader = {
        let terminal = Arc::clone(&terminal);
        let mut answers = input.try_clone().expect("could not share the input");
        let pieces = Arc::clone(&pieces);
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
        "type" => Step::Keys(value.as_bytes().to_vec()),
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
