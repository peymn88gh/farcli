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
//!   --styles       under each row, print the colours and styles of its
//!                  cells: `[first-last what]` for each stretch of columns
//!                  drawn in anything but the Terminal's own colours
//!   --frames       print the screen after every event that changed it, to
//!                  watch a reply arrive
//!   --from MS      with --frames, start printing at this many milliseconds
//!   --to MS        with --frames, stop printing after this many
//!   --farcli       show what farcli makes of it: play the recording through
//!                  a session with the App profile of the recorded app
//! ```
//!
//! Without `--at-keys` only the screen at the end is printed. Without
//! `--farcli` the Terminal is sent exactly what the Target app wrote.

use std::env;
use std::process::ExitCode;

use farcli::recording::{Event, Recording, Replay};
use farcli::screen::{row_text, Cell, Color, Style, Underline};

fn main() -> ExitCode {
    let mut at_keys = false;
    let mut scrollback = false;
    let mut numbers = false;
    let mut styles = false;
    let mut through_farcli = false;
    let mut frames = false;
    let mut from = 0;
    let mut to = u64::MAX;
    let mut file = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut number = || args.next().and_then(|value| value.parse().ok());
        match arg.as_str() {
            "--frames" => frames = true,
            "--from" => from = number().unwrap_or(from),
            "--to" => to = number().unwrap_or(to),
            "--at-keys" => at_keys = true,
            "--scrollback" => scrollback = true,
            "--numbers" => numbers = true,
            "--styles" => styles = true,
            "--farcli" => through_farcli = true,
            _ => file = Some(arg),
        }
    }
    let Some(file) = file else {
        eprintln!(
            "usage: show_recording [--at-keys] [--frames [--from MS] [--to MS]] [--scrollback] [--numbers] [--styles] [--farcli] FILE"
        );
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

    let print_row = |row: &[Cell]| {
        println!("|{}", visible(&row_text(row), numbers));
        if styles {
            let stretches = styled_stretches(row);
            if !stretches.is_empty() {
                println!("    {}", stretches.join(" "));
            }
        }
    };
    let show = |title: &str, replay: &Replay| {
        if scrollback {
            println!("--- scrolled off the top ---");
            for row in replay.terminal.scrollback() {
                print_row(&row);
            }
        }
        println!("--- {title} ---");
        for row in replay.terminal.screen() {
            print_row(&row);
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
        let before = frames.then(|| replay.terminal.screen());
        replay.step();
        if (from..=to).contains(milliseconds)
            && before.is_some_and(|rows| rows != replay.terminal.screen())
        {
            show(
                &format!("after event {index}, at {milliseconds} ms"),
                &replay,
            );
        }
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

/// Each stretch of a row's cells that share a style other than the
/// Terminal's own, as `[first-last what]`, columns counted from 0. Empty
/// cells in the Terminal's own colours are left out.
fn styled_stretches(row: &[Cell]) -> Vec<String> {
    let plain = Style::default();
    let mut stretches: Vec<(usize, usize, &Style)> = Vec::new();
    for (col, cell) in row.iter().enumerate() {
        if cell.is_left_alone() || cell.style == plain {
            continue;
        }
        match stretches.last_mut() {
            Some((_, last, style)) if *last + 1 == col && **style == cell.style => *last = col,
            _ => stretches.push((col, col, &cell.style)),
        }
    }
    stretches
        .into_iter()
        .map(|(first, last, style)| format!("[{first}-{last} {}]", described(style)))
        .collect()
}

/// A style in words: the foreground colour, `on` the background colour,
/// then whatever else is set.
fn described(style: &Style) -> String {
    let color = |color: Color| match color {
        Color::Default => "default".to_string(),
        Color::Palette(number) => format!("colour{number}"),
        Color::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
    };
    let mut parts = Vec::new();
    if style.foreground != Color::Default {
        parts.push(color(style.foreground));
    }
    if style.background != Color::Default {
        parts.push(format!("on {}", color(style.background)));
    }
    for (set, name) in [
        (style.bold, "bold"),
        (style.dim, "dim"),
        (style.italic, "italic"),
        (style.inverse, "inverse"),
        (style.hidden, "hidden"),
        (style.strikeout, "strikeout"),
        (style.underline != Underline::None, "underline"),
        (style.link.is_some(), "link"),
    ] {
        if set {
            parts.push(name.to_string());
        }
    }
    parts.join(" ")
}
