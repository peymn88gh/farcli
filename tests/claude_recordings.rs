//! The recordings of real Claude Code sessions in `tests/recordings`,
//! played through the session: every Farsi paragraph of a reply reads as
//! Layout shows it from the text the model wrote, in both of Claude Code's
//! display modes, and everything else on the screen stays as the app drew
//! it.
//!
//! What the model wrote is in `NAME.reply.md` beside each reply recording.
//! The recordings were made in a Terminal of 100 columns, where a reply's
//! text is drawn in the 98 columns from the third to the last.

mod claude_code;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

use claude_code::{as_drawn, direction_of, laid_out, wrapped};
use farcli::recording::{Event, Recording, Replay};

const CLASSIC: &str = "claude-classic-reply";
const FULLSCREEN: &str = "claude-fullscreen-reply";

/// Sessions in which Farsi is typed into the Input box and nothing is sent.
const TYPING: [&str; 3] = [
    "claude-classic-typing",
    "claude-fullscreen-typing",
    "claude-classic-input-probes",
];

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
        // This includes the message the user sent. In the fullscreen
        // display, scrolled up, its last row is the first thing under the
        // header, with its first rows out of sight. These recordings were
        // made with colours switched off, so that row has no background
        // to tell it from the end of a paragraph of a reply: farcli knows
        // it from having shown the whole message before.
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

/// Every pair of a row as the app drew it and the same row as farcli showed
/// it, over all the screens of a recording at the moments the user is about
/// to press a key and at the end, with the rows that scrolled off the top.
fn rows_drawn_and_shown(name: &str) -> BTreeSet<(String, String)> {
    let recording = load(name);
    let mut pairs = BTreeSet::new();
    let mut through_farcli = Replay::start(&recording, recording.profile());
    let mut alone = Replay::without_farcli(&recording);
    let mut collect = |through_farcli: &Replay, alone: &Replay| {
        let mut shown = through_farcli.terminal.scrollback_text();
        shown.extend(through_farcli.terminal.screen_text());
        let mut drawn = alone.terminal.scrollback_text();
        drawn.extend(alone.terminal.screen_text());
        pairs.extend(drawn.into_iter().zip(shown));
    };
    for (_, event) in &recording.events {
        if matches!(event, Event::UserTyped(_)) {
            collect(&through_farcli, &alone);
        }
        through_farcli.step();
        alone.step();
    }
    through_farcli.finish();
    alone.finish();
    collect(&through_farcli, &alone);
    pairs
}

#[test]
fn of_the_thirty_rows_of_farsi_in_the_two_replies_the_eight_paragraph_rows_read_as_written() {
    // What this ticket corrects and what it leaves. The paragraphs of a
    // reply are laid out from the text the model wrote. List items and
    // table rows wait for their own ticket: they are shown as Claude Code
    // drew them, reordered by the app and unjoined.
    let mut as_the_model_wrote = 0;
    let mut as_the_app_drew = 0;
    for name in [CLASSIC, FULLSCREEN] {
        let prose = prose_rows(name);
        let mut list_items = BTreeSet::new();
        let mut table_rows = BTreeSet::new();
        let mut paragraph_rows = BTreeSet::new();
        for (drawn, shown) in rows_drawn_and_shown(name) {
            if !drawn.contains(is_farsi) {
                continue;
            }
            let text = drawn.trim_start();
            if let Some(row) = prose.iter().find(|row| row.drawn == drawn) {
                assert_eq!(shown, row.wanted, "{name}");
                paragraph_rows.insert(drawn);
            } else if text.starts_with("- ") || text.starts_with("1. ") || text.starts_with("2. ") {
                assert_eq!(shown, drawn, "{name}");
                // The fullscreen display draws a button over one list
                // item while the user has scrolled up.
                let item = drawn.split("  Jump to bottom").next().unwrap();
                list_items.insert(item.trim_end().to_string());
            } else if text.starts_with('│') && !text.contains("Jump to bottom") {
                assert_eq!(shown, drawn, "{name}");
                table_rows.insert(drawn);
            }
        }
        // Two paragraphs of two rows, a list of five items and one of two,
        // and a table of a header and three rows.
        assert_eq!(
            (paragraph_rows.len(), list_items.len(), table_rows.len()),
            (4, 7, 4),
            "{name}"
        );
        as_the_model_wrote += paragraph_rows.len();
        as_the_app_drew += list_items.len() + table_rows.len();
    }
    assert_eq!((as_the_model_wrote, as_the_app_drew), (8, 22));
}

#[test]
fn the_code_block_of_a_reply_stays_as_claude_code_drew_it() {
    for name in [CLASSIC, FULLSCREEN] {
        let pairs = rows_drawn_and_shown(name);
        for line in [
            "  nums = [1, 2, 3]",
            "  total = sum(nums)",
            "  print(total)",
        ] {
            assert!(
                pairs.contains(&(line.to_string(), line.to_string())),
                "{name}: {line}"
            );
        }
    }
}

#[test]
fn while_farsi_is_typed_into_the_input_box_the_whole_screen_stays_as_claude_code_drew_it() {
    // The Input box is the app's own until the ticket that makes typing in
    // it right. Nothing in these sessions is Agent output, so farcli has
    // nothing to change: every cell and the cursor are as the app left
    // them, each time the user is about to press a key.
    for name in TYPING {
        let recording = load(name);
        let mut through_farcli = Replay::start(&recording, recording.profile());
        let mut alone = Replay::without_farcli(&recording);
        let mut keys = 0;
        for (index, (_, event)) in recording.events.iter().enumerate() {
            if matches!(event, Event::UserTyped(_)) {
                keys += 1;
                assert_eq!(
                    through_farcli.terminal.screen(),
                    alone.terminal.screen(),
                    "{name}: before event {index}"
                );
                assert_eq!(
                    through_farcli.terminal.cursor(),
                    alone.terminal.cursor(),
                    "{name}: before event {index}"
                );
            }
            through_farcli.step();
            alone.step();
        }
        assert!(keys > 50, "{name}");
    }
}
