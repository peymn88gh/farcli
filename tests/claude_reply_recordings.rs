//! The recordings of real Claude Code replies in `tests/recordings`, played
//! through the session: every Farsi paragraph of a reply reads as Layout
//! shows it from the text the model wrote, in both of Claude Code's display
//! modes, and everything else on the screen stays as the app drew it.
//!
//! What the model wrote is in `NAME.reply.md` beside each recording. The
//! recordings were made in a Terminal of 100 columns, where a reply's text
//! is drawn in the 98 columns from the third to the last.

mod claude_code;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use claude_code::{as_drawn, direction_of, laid_out, wrapped};
use farcli::layout::Direction;
use farcli::recording::{Event, Recording, Replay};

const CLASSIC: &str = "claude-classic-reply";
const FULLSCREEN: &str = "claude-fullscreen-reply";

/// The width of the area a reply's text is drawn in.
const WIDTH: usize = 98;

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("recordings")
}

fn load(name: &str) -> Recording {
    Recording::load(folder().join(format!("{name}.recording"))).unwrap()
}

fn is_farsi(character: char) -> bool {
    ('\u{600}'..='\u{6ff}').contains(&character)
}

/// The paragraphs of prose the model wrote: everything before the first
/// list of its reply.
fn paragraphs(name: &str) -> Vec<String> {
    let reply = fs::read_to_string(folder().join(format!("{name}.reply.md"))).unwrap();
    reply
        .replace("\r\n", "\n")
        .split("\n\n")
        .take_while(|block| !block.starts_with("- "))
        .map(str::to_string)
        .collect()
}

/// A row of a reply's prose.
struct ProseRow {
    /// The row as Claude Code drew it.
    drawn: String,
    /// The row as it should read: what Layout makes of the model's text.
    wanted: String,
}

/// Every row of the reply's prose, top to bottom. The reply mark stands
/// before the first row and two empty columns before every other.
fn prose_rows(name: &str) -> Vec<ProseRow> {
    let mut rows = Vec::new();
    for paragraph in paragraphs(name) {
        let direction = direction_of(&paragraph);
        for line in wrapped(&paragraph, WIDTH) {
            let margin = if rows.is_empty() { "● " } else { "  " };
            let on_screen = |text: String| format!("{margin}{text}").trim_end().to_string();
            rows.push(ProseRow {
                drawn: on_screen(as_drawn(&line)),
                wanted: on_screen(laid_out(&line, direction, WIDTH)),
            });
        }
    }
    rows
}

/// The last row of the message sent in both recordings, in Logical order.
/// The message is wrapped in 97 columns.
const LAST_ROW_SENT: &str =
    "جدول با سه ستون (نام، توضیح، تعداد) و سه ردیف. ۵. یک بلوک کد Python سه\u{200c}خطی.";

/// Checks one screen shown through farcli against the same screen as the
/// app drew it: each row of the reply's prose reads as wanted, and every
/// other row is exactly as drawn. Returns the prose rows it saw.
fn check_screen(
    name: &str,
    when: &str,
    shown: &[String],
    drawn: &[String],
    prose: &[ProseRow],
) -> Vec<usize> {
    let wanted: HashMap<&str, usize> = prose
        .iter()
        .enumerate()
        .map(|(at, row)| (row.drawn.as_str(), at))
        .collect();
    let mut seen = Vec::new();
    assert_eq!(shown.len(), drawn.len(), "{name}, {when}");
    for (row, (shown_row, drawn_row)) in shown.iter().zip(drawn).enumerate() {
        let what = format!("{name}, {when}, row {row}");
        if let Some(at) = wanted.get(drawn_row.as_str()) {
            assert_eq!(shown_row, &prose[*at].wanted, "{what}");
            seen.push(*at);
            continue;
        }
        // These recordings were made with colours switched off. With
        // colours a sent message has a background of its own and is told
        // apart from a reply wherever it is. Without them, the end of a
        // sent message at the top of the fullscreen display, with its
        // first rows scrolled out of sight, looks like the end of a
        // paragraph of a reply, and is laid out as one.
        let end_of_sent_message = format!("  {}", as_drawn(LAST_ROW_SENT));
        let its_start_is_in_sight = drawn[..row].iter().any(|row| row.starts_with('❯'));
        if *drawn_row == end_of_sent_message && !its_start_is_in_sight {
            let as_a_paragraph = laid_out(LAST_ROW_SENT, Direction::RightToLeft, WIDTH);
            assert_eq!(shown_row, &format!("  {as_a_paragraph}"), "{what}");
            continue;
        }
        assert_eq!(shown_row, drawn_row, "{what}");
    }
    seen
}

/// Plays a recording with and without farcli side by side, and checks the
/// screen each time the user is about to press a key and at the end, with
/// the rows that scrolled off the top. Returns how often each prose row was
/// seen.
fn check_recording(name: &str) -> Vec<usize> {
    let recording = load(name);
    let prose = prose_rows(name);
    let mut times_seen = vec![0; prose.len()];
    let mut through_farcli = Replay::start(&recording, recording.profile());
    let mut alone = Replay::without_farcli(&recording);
    let mut check = |through_farcli: &Replay, alone: &Replay, when: String| {
        let mut shown = through_farcli.terminal.scrollback_text();
        shown.extend(through_farcli.terminal.screen_text());
        let mut drawn = alone.terminal.scrollback_text();
        drawn.extend(alone.terminal.screen_text());
        for at in check_screen(name, &when, &shown, &drawn, &prose) {
            times_seen[at] += 1;
        }
    };
    for (index, (_, event)) in recording.events.iter().enumerate() {
        if matches!(event, Event::UserTyped(_)) {
            check(&through_farcli, &alone, format!("before event {index}"));
        }
        through_farcli.step();
        alone.step();
    }
    through_farcli.finish();
    alone.finish();
    check(&through_farcli, &alone, "at the end".to_string());
    times_seen
}

#[test]
fn the_rows_worked_out_from_what_the_model_wrote_are_the_rows_claude_code_drew() {
    // The tests below find a reply's prose on the screen by working out,
    // from the model's text, how Claude Code wrapped and drew it. This
    // holds that working-out against the recordings themselves.
    for name in [CLASSIC, FULLSCREEN] {
        let recording = load(name);
        let mut ever_drawn: Vec<String> = Vec::new();
        let mut replay = Replay::without_farcli(&recording);
        loop {
            ever_drawn.extend(replay.terminal.screen_text());
            if replay.step().is_none() {
                break;
            }
        }
        ever_drawn.extend(replay.terminal.scrollback_text());

        let prose = prose_rows(name);
        assert_eq!(prose.len(), 4, "{name}");
        for row in prose {
            assert!(ever_drawn.contains(&row.drawn), "{name}: {}", row.drawn);
        }
    }
}

#[test]
fn in_the_classic_display_every_paragraph_of_the_reply_reads_as_the_model_wrote_it() {
    let times_seen = check_recording(CLASSIC);

    // The reply is written in one piece and is longer than the screen: its
    // paragraphs are found by scrolling back, at the end.
    assert!(times_seen.iter().all(|times| *times >= 1), "{times_seen:?}");
}

#[test]
fn in_the_fullscreen_display_every_paragraph_of_the_reply_reads_as_the_model_wrote_it() {
    let times_seen = check_recording(FULLSCREEN);

    // The paragraphs come into sight when the user scrolls up, twice.
    assert!(times_seen.iter().all(|times| *times >= 2), "{times_seen:?}");
}

#[test]
fn a_wrapped_row_that_begins_with_an_english_word_keeps_its_paragraphs_direction() {
    // The model wrote «... مقدار timeout را روی 123 میلی‌ثانیه بگذار و
    // اندازه‌ی آیکون را (33 x 20) نگه دار.» and Claude Code wrapped it
    // before `timeout`. It drew that row left to right, because the row
    // begins with an English word. The screen alone does not say whether
    // the row begins with `timeout` or ends with it: the full stop at its
    // right end is what tells.
    let recording = load(CLASSIC);
    let mut replay = Replay::start(&recording, recording.profile());
    replay.finish();

    let rows = replay.terminal.scrollback_text();
    let row = rows
        .iter()
        .find(|row| row.contains("timeout"))
        .expect("the row with `timeout`");
    // Read from the right edge: timeout, then the Farsi, the number, and
    // the full stop last, at the left.
    assert!(row.ends_with(" timeout"), "{row}");
    assert!(row.trim_start().starts_with('.'), "{row}");
    assert_eq!(row.chars().count(), 100, "{row}");
}

#[test]
fn the_lists_tables_and_code_of_a_reply_stay_as_claude_code_drew_them_for_now() {
    // Of the rows of a reply that hold Farsi, the paragraphs are laid out
    // again. List items and table rows wait for their own ticket: they are
    // shown as the app drew them, reordered by the app and unjoined.
    for name in [CLASSIC, FULLSCREEN] {
        let recording = load(name);
        let mut through_farcli = Replay::start(&recording, recording.profile());
        let mut alone = Replay::without_farcli(&recording);
        let reply_is_shown = |replay: &Replay| {
            replay
                .terminal
                .screen_text()
                .iter()
                .any(|row| row.starts_with("  print(total)"))
        };
        assert!(alone.play_until(reply_is_shown), "{name}");
        while !reply_is_shown(&through_farcli) {
            assert!(through_farcli.step().is_some(), "{name}");
        }

        let shown = through_farcli.terminal.screen_text();
        let drawn = alone.terminal.screen_text();
        let farsi_rows = |starts_with: &dyn Fn(&str) -> bool| -> Vec<usize> {
            (0..drawn.len())
                .filter(|row| drawn[*row].contains(is_farsi))
                .filter(|row| starts_with(drawn[*row].trim_start()))
                .collect()
        };
        let list_items = farsi_rows(&|row| {
            row.starts_with("- ") || row.starts_with(|c: char| c.is_ascii_digit())
        });
        let table_rows = farsi_rows(&|row| row.starts_with('│'));
        // Classic: the first rows of the list have scrolled off already.
        assert!(list_items.len() >= 4, "{name}: {list_items:?}");
        assert_eq!(table_rows.len(), 4, "{name}");
        for row in list_items.into_iter().chain(table_rows) {
            assert_eq!(shown[row], drawn[row], "{name}, row {row}");
        }
        let code = drawn
            .iter()
            .position(|row| row == "  nums = [1, 2, 3]")
            .unwrap();
        assert_eq!(shown[code..code + 3], drawn[code..code + 3], "{name}");
    }
}
