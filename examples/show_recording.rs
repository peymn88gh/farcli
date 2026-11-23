//! Developer tool: plays a recording and prints what the Terminal shows, so
//! a recording can be read without running the Target app again.
//!
//! ```text
//! cargo run --example show_recording -- [OPTIONS] FILE
//!
//!   --at-keys      print the screen each time the user is about to type
//!                  after the app has drawn something
//!   --scrollback   also print the rows that scrolled off the top
//!   --numbers      show characters nobody can see as `<200c>`
//!   --farcli       show what farcli makes of it: play the recording through
//!                  a session with the App profile of the recorded app
//! ```
//!
//! Without `--at-keys` only the screen at the end is printed. Without
//! `--farcli` the Terminal is sent exactly what the Target app wrote.

use std::env;
use std::process::ExitCode;

use farcli::recording::{Event, Recording, Replay};

fn main() -> ExitCode {
    let mut at_keys = false;
    let mut scrollback = false;
    let mut numbers = false;
    let mut through_farcli = false;
    let mut file = None;
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--at-keys" => at_keys = true,
            "--scrollback" => scrollback = true,
            "--numbers" => numbers = true,
            "--farcli" => through_farcli = true,
            _ => file = Some(arg),
        }
    }
    let Some(file) = file else {
        eprintln!("usage: show_recording [--at-keys] [--scrollback] [--numbers] [--farcli] FILE");
        return ExitCode::from(2);
    };
    let recording = match Recording::load(&file) {
        Ok(recording) => recording,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let about = &recording.about;
    println!("app: {} {}", about.app, about.app_version);
    println!("system: {}", about.system);
    println!("size: {}x{}", about.size.cols, about.size.rows);
    for (name, value) in &about.notes {
        println!("{name}: {value}");
    }

    let show = |title: &str, replay: &Replay| {
        if scrollback {
            println!("--- scrolled off the top ---");
            for row in replay.terminal.scrollback_text() {
                println!("|{}", visible(&row, numbers));
            }
        }
        println!("--- {title} ---");
        for row in replay.terminal.screen_text() {
            println!("|{}", visible(&row, numbers));
        }
        let cursor = replay.terminal.cursor();
        println!(
            "--- cursor at row {} column {}, {} ---",
            cursor.row,
            cursor.col,
            if replay.terminal.cursor_visible() {
                "visible"
            } else {
                "hidden"
            }
        );
    };

    let mut replay = if through_farcli {
        Replay::start(&recording, recording.profile())
    } else {
        Replay::without_farcli(&recording)
    };
    let mut drawn_since_keys = false;
    for (index, (milliseconds, event)) in recording.events.iter().enumerate() {
        if at_keys && drawn_since_keys && matches!(event, Event::UserTyped(_)) {
            show(
                &format!("before event {index}, at {milliseconds} ms"),
                &replay,
            );
            drawn_since_keys = false;
        }
        if matches!(event, Event::AppWrote(_)) {
            drawn_since_keys = true;
        }
        replay.step();
    }
    replay.finish();
    show("at the end", &replay);
    ExitCode::SUCCESS
}

/// The row as printed: with `numbers`, a character nobody can see is
/// replaced by its number.
fn visible(row: &str, numbers: bool) -> String {
    if !numbers {
        return row.to_string();
    }
    let mut text = String::new();
    for character in row.chars() {
        // Joiners and direction marks, then accents and Arabic vowel marks.
        let unseen = matches!(
            character,
            '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{300}'..='\u{36f}'
                | '\u{610}'..='\u{61a}'
                | '\u{64b}'..='\u{65f}'
                | '\u{670}'
        );
        if unseen {
            text.push_str(&format!("<{:04x}>", character as u32));
        } else {
            text.push(character);
        }
    }
    text
}
