//! A program run on the hidden terminal farcli ships has its Farsi arrive
//! where the program put it, Half-spaces and vowel marks included.
//!
//! These tests start a real program on a real hidden terminal and join it
//! to a session, the way `farcli` itself does, with a stand-in for the
//! Terminal. The hidden terminal built into Windows fails each of them: it
//! gives a Half-space and a vowel mark a cell of their own.
#![cfg(all(windows, target_arch = "x86_64"))]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

use farcli::app_profile::AppProfile;
use farcli::app_runner::RunningApp;
use farcli::fake_terminal::FakeTerminal;
use farcli::hidden_terminal::HiddenTerminal;
use farcli::session::Session;
use farcli::Size;

const SIZE: Size = Size { cols: 40, rows: 12 };

/// Runs a batch file holding `lines` on the hidden terminal, through a
/// session, and returns what a Terminal then shows.
fn shown_when_a_program_writes(name: &str, lines: &[String]) -> Vec<String> {
    let folder = env::temp_dir().join(format!("farcli-{name}-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let script: PathBuf = folder.join("writes.cmd");
    // The command prompt reads the file as UTF-8 once told to.
    let mut text = String::from("@echo off\r\nchcp 65001 >nul\r\n");
    for line in lines {
        text.push_str(&format!("echo {line}\r\n"));
    }
    fs::write(&script, text).unwrap();

    let shown = shown_when_run(script.as_os_str(), &[]);
    fs::remove_dir_all(&folder).unwrap();
    shown.screen_text()
}

/// Runs `program` on the hidden terminal, through a session, and returns
/// the stand-in Terminal as the program left it.
fn shown_when_run(program: &std::ffi::OsStr, args: &[OsString]) -> FakeTerminal {
    let mut app = RunningApp::start(program, args, SIZE).unwrap();
    assert_eq!(app.hidden_terminal(), HiddenTerminal::Shipped);
    let mut output = app.take_output();
    let mut input = app.take_input();
    let mut terminal = FakeTerminal::new(SIZE);
    // Shown as the program drew it: farcli lays out only what it knows to
    // be a paragraph, and that is not what is tested here.
    let mut session = Session::start_on(
        app.hidden_terminal(),
        AppProfile::claude_code(),
        SIZE,
        terminal.cursor(),
    );
    let waiter = std::thread::spawn(move || app.wait());
    let mut buffer = [0u8; 65536];
    while let Ok(count @ 1..) = output.read(&mut buffer) {
        let reaction = session.app_wrote(&buffer[..count]);
        terminal.receive(&reaction.for_terminal);
        if !reaction.for_app.is_empty() {
            let _ = input.write_all(&reaction.for_app);
        }
    }
    assert_eq!(waiter.join().unwrap().unwrap(), 0);
    terminal
}

/// Farsi as a Target app may write it, with the cells a reader counts.
const COUNTED: [(&str, &str, usize); 11] = [
    ("a word", "سلام", 4),
    ("a Half-space", "می\u{200c}خواهم", 7),
    ("two Half-spaces", "می\u{200c}خ\u{200c}ه", 4),
    ("a vowel mark", "س\u{64e}ب", 2),
    ("a doubled vowel mark at the end", "لطفا\u{64b}", 4),
    ("two marks on one letter", "ک\u{650}ت\u{651}اب", 4),
    ("a small alef above", "عیسی\u{670}", 4),
    ("a hamza above", "خانه\u{654}", 4),
    ("lam and alef", "سلام لا", 7),
    ("letters already joined", "\u{fedf}\u{fe8e}\u{fee3}", 3),
    ("a direction mark", "a\u{200f}b", 2),
];

#[test]
fn the_hidden_screen_counts_the_cells_of_farsi_as_the_hidden_terminal_does() {
    // The program writes each sample at the start of a row, asks the hidden
    // terminal where its cursor then is, and writes `=` and the answer.
    // Where the `=` lands is the Hidden screen's own count.
    let samples: Vec<String> = COUNTED
        .iter()
        .map(|(_, text, _)| {
            let characters: Vec<String> = text
                .chars()
                .map(|character| (character as u32).to_string())
                .collect();
            format!("@({})", characters.join(","))
        })
        .collect();
    let script = format!(
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
         [Console]::Write([char]13 + [char]10); \
         foreach ($sample in @({})) {{ \
           [Console]::Write(-join ($sample | ForEach-Object {{ [char]$_ }})); \
           $column = [Console]::CursorLeft; \
           [Console]::Write('=' + $column + [char]13 + [char]10) }}",
        samples.join(",")
    );

    let terminal = shown_when_run(
        "powershell".as_ref(),
        &["-NoProfile", "-NonInteractive", "-Command", &script].map(OsString::from),
    );

    let rows: Vec<_> = terminal
        .screen()
        .into_iter()
        .filter(|row| row.iter().any(|cell| cell.character == '='))
        .collect();
    assert_eq!(rows.len(), COUNTED.len(), "{:?}", terminal.screen_text());
    for ((name, _, cells), row) in COUNTED.iter().zip(&rows) {
        let text = farcli::screen::row_text(row);
        let said_by_hidden_terminal: usize = text.rsplit('=').next().unwrap().parse().unwrap();
        let on_hidden_screen = row.iter().position(|cell| cell.character == '=').unwrap();
        assert_eq!(said_by_hidden_terminal, *cells, "{name}: {text:?}");
        assert_eq!(on_hidden_screen, *cells, "{name}: {text:?}");
    }
}

/// Moves the cursor to a column of its row, counted from 1.
fn to_column(column: u16) -> String {
    format!("\x1b[{column}G")
}

#[test]
fn a_border_placed_by_column_stays_in_its_column_after_a_half_space_or_a_vowel_mark() {
    let shown = shown_when_a_program_writes(
        "border",
        &[
            format!("abcdefg{}^|", to_column(10)),
            format!("می\u{200c}خواهم{}^|", to_column(10)),
            format!("a می\u{200c}رویم{}^|", to_column(10)),
            format!("س\u{64e}ب{}^|", to_column(10)),
            format!("لطفا\u{64b} ک\u{650}تاب{}^|", to_column(20)),
        ],
    );

    // Column 10 is nine cells in. «می‌خواهم» fills seven of them, «سَب» two.
    assert_eq!(shown[0], "abcdefg  |");
    assert_eq!(shown[1], "می\u{200c}خواهم  |");
    assert_eq!(shown[2], "a می\u{200c}رویم |");
    assert_eq!(shown[3], "س\u{64e}ب       |");
    // Column 20 is nineteen cells in, and «لطفاً کِتاب» fills nine.
    assert_eq!(shown[4], "لطفا\u{64b} ک\u{650}تاب          |");
}

#[test]
fn text_written_one_row_up_lands_on_that_row_after_a_full_row_with_a_half_space() {
    // A row exactly as wide as the Terminal by the program's count, which
    // gives the Half-space no cell.
    let full_row = format!("می\u{200c}خواهم{}", "x".repeat(usize::from(SIZE.cols) - 7));

    let shown = shown_when_a_program_writes(
        "full-row",
        &[
            full_row.clone(),
            "second line".to_string(),
            "\x1b[1Aover".to_string(),
        ],
    );

    assert_eq!(shown[0], full_row);
    assert_eq!(shown[1], "overnd line");
    assert_eq!(shown[2], "");
}
