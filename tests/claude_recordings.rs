//! The recordings of real Claude Code replies in `tests/recordings`, played
//! through the session: every Farsi paragraph of a reply, and every line of
//! Farsi that stands on a row of its own, reads as Layout shows it from the
//! text the model wrote, in both of Claude Code's display modes, and
//! everything else on the screen stays as the app drew it, colours and all.
//!
//! What the model wrote is in `NAME.reply.md` beside each reply recording.
//! The recordings were made in a Terminal of 100 columns, where a reply's
//! text is drawn in the 98 columns from the third to the last, and with the
//! colours a user's session has.

mod claude_code;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

use claude_code::{as_drawn, direction_of, laid_out, wrapped};
use farcli::recording::{Event, Recording, Replay};
use farcli::screen::{row_text, Cell, Color, Row};

const CLASSIC: &str = "claude-classic-reply";
const FULLSCREEN: &str = "claude-fullscreen-reply";

/// An earlier session started again, in which Claude Code draws that
/// session's message and reply again and nothing is sent.
const DRAWN_AGAIN: &str = "claude-classic-resumed-reply";

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

/// What a row of a reply's prose belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prose {
    /// A paragraph, which Claude Code may wrap onto several rows.
    Paragraph,
    /// One of several lines the model separated by single line breaks, as
    /// in a poem. Each is a paragraph of its own.
    Line,
    /// A number standing alone before a part of the reply, such as «۳.».
    Label,
}

/// A row of a reply's prose.
struct ProseRow {
    /// The row as Claude Code drew it.
    drawn: String,
    /// The row as it should read: what Layout makes of the model's text.
    wanted: String,
    of: Prose,
}

/// Every row of the reply's prose, top to bottom: the blocks of the model's
/// text that are not a list, a table or a code block. Claude Code draws no
/// mark of the bold the model asked for. The reply mark stands before the
/// first row and two empty columns before every other.
fn prose_rows(name: &str) -> Vec<ProseRow> {
    let reply = fs::read_to_string(folder().join(format!("{name}.reply.md"))).unwrap();
    let reply = reply.replace("\r\n", "\n").replace("**", "");
    let mut rows = Vec::new();
    for block in reply.split("\n\n").map(str::trim_end) {
        if ["- ", "1. ", "|", "```"]
            .iter()
            .any(|start| block.starts_with(start))
        {
            continue;
        }
        let lines: Vec<&str> = block.lines().map(str::trim_end).collect();
        for line in &lines {
            let of = match line.contains(|c: char| c.is_alphabetic()) {
                false => Prose::Label,
                true if lines.len() > 1 => Prose::Line,
                true => Prose::Paragraph,
            };
            let direction = direction_of(line);
            for row in wrapped(line, WIDTH) {
                let margin = if rows.is_empty() { "● " } else { "  " };
                let on_screen = |text: String| format!("{margin}{text}").trim_end().to_string();
                rows.push(ProseRow {
                    drawn: on_screen(as_drawn(&row)),
                    wanted: on_screen(laid_out(&row, direction, WIDTH)),
                    of,
                });
            }
        }
    }
    rows
}

/// What a Terminal held at one moment of a recording, without farcli and
/// through it: the rows that scrolled off the top, then the rows on screen.
struct Moment {
    when: String,
    drawn: Vec<Row>,
    shown: Vec<Row>,
}

/// Plays a recording with and without farcli side by side, and takes what
/// the two Terminals hold each time the user is about to press a key, and at
/// the end.
fn moments(name: &str) -> Vec<Moment> {
    let recording = load(name);
    let mut moments = Vec::new();
    let mut through_farcli = Replay::start(&recording, recording.profile());
    let mut alone = Replay::without_farcli(&recording);
    let mut take = |through_farcli: &Replay, alone: &Replay, when: String| {
        let mut shown = through_farcli.terminal.scrollback();
        shown.extend(through_farcli.terminal.screen());
        let mut drawn = alone.terminal.scrollback();
        drawn.extend(alone.terminal.screen());
        assert_eq!(shown.len(), drawn.len(), "{name}, {when}");
        moments.push(Moment { when, drawn, shown });
    };
    for (index, (_, event)) in recording.events.iter().enumerate() {
        if matches!(event, Event::UserTyped(_)) {
            take(&through_farcli, &alone, format!("before event {index}"));
        }
        through_farcli.step();
        alone.step();
    }
    through_farcli.finish();
    alone.finish();
    take(&through_farcli, &alone, "at the end".to_string());
    moments
}

/// The styles a row's characters are drawn in, whatever their order.
fn styles(row: &[Cell]) -> Vec<String> {
    let mut styles: Vec<String> = row
        .iter()
        .filter(|cell| !cell.is_left_alone() && cell.character != ' ')
        .map(|cell| format!("{:?}", cell.style))
        .collect();
    styles.sort();
    styles
}

/// Checks every moment of a recording: each row of the reply's prose reads
/// as wanted and keeps the styles its characters had, and every other row
/// is exactly as drawn, in every cell. Returns how often each prose row was
/// seen.
fn check_recording(name: &str) -> Vec<usize> {
    let prose = prose_rows(name);
    let wanted: HashMap<&str, usize> = prose
        .iter()
        .enumerate()
        .map(|(at, row)| (row.drawn.as_str(), at))
        .collect();
    let mut times_seen = vec![0; prose.len()];
    for moment in moments(name) {
        for (row, (shown, drawn)) in moment.shown.iter().zip(&moment.drawn).enumerate() {
            let what = format!("{name}, {}, row {row}", moment.when);
            let drawn_text = row_text(drawn);
            if let Some(at) = wanted.get(drawn_text.as_str()) {
                assert_eq!(row_text(shown), prose[*at].wanted, "{what}");
                assert_eq!(styles(shown), styles(drawn), "{what}");
                times_seen[*at] += 1;
                continue;
            }
            // This includes the message the user sent, which Claude Code
            // draws on a background of its own, and the rows the fullscreen
            // display draws a button over.
            assert!(
                shown == drawn,
                "{what}:\n shown {:?}\n drawn {drawn_text:?}",
                row_text(shown)
            );
        }
    }
    times_seen
}

/// Every pair of a row as the app drew it and the same row as farcli showed
/// it, over all the moments of a recording.
fn rows_drawn_and_shown(name: &str) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    for moment in moments(name) {
        for (drawn, shown) in moment.drawn.iter().zip(&moment.shown) {
            pairs.insert((row_text(drawn), row_text(shown)));
        }
    }
    pairs
}

#[test]
fn the_rows_worked_out_from_what_the_model_wrote_are_the_rows_claude_code_drew() {
    // The tests below find a reply's prose on the screen by working out,
    // from the model's text, how Claude Code wrapped and drew it. This
    // holds that working-out against the recordings themselves.
    //
    // The classic reply: two paragraphs of two rows, four labels and a poem
    // of three lines. The fullscreen reply: the same without the labels.
    // The earlier reply drawn again: two paragraphs of two rows.
    for (name, rows) in [(CLASSIC, 11), (FULLSCREEN, 7), (DRAWN_AGAIN, 4)] {
        let ever_drawn: BTreeSet<String> = rows_drawn_and_shown(name)
            .into_iter()
            .map(|(drawn, _)| drawn)
            .collect();
        let prose = prose_rows(name);
        assert_eq!(prose.len(), rows, "{name}");
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

    // The paragraphs come into sight when the user scrolls up, and are in
    // sight before each of the Page Up and Page Down keys that follow.
    assert!(times_seen.iter().all(|times| *times >= 2), "{times_seen:?}");
}

#[test]
fn a_reply_drawn_again_in_a_session_started_again_reads_as_the_model_wrote_it() {
    let times_seen = check_recording(DRAWN_AGAIN);

    assert!(times_seen.iter().all(|times| *times >= 1), "{times_seen:?}");
}

#[test]
fn a_wrapped_row_that_begins_with_an_english_word_keeps_its_paragraphs_direction() {
    // The model wrote «... مقدار timeout را روی 123 میلی‌ثانیه بگذار و
    // اندازه‌ی آیکون را (33 x 20) نگه دار.» and Claude Code wrapped it
    // before `timeout`. It drew that row left to right, because the row
    // begins with an English word. The screen alone does not say whether
    // the row begins with `timeout` or ends with it: the full stop at its
    // right end is what tells.
    let recording = load(DRAWN_AGAIN);
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
fn of_the_forty_two_rows_of_farsi_in_the_two_replies_fourteen_read_as_the_model_wrote_them() {
    // What farcli corrects so far and what it leaves. The paragraphs of a
    // reply, and lines of Farsi separated by single line breaks, are laid
    // out from the text the model wrote. List items and table rows wait for
    // their own ticket: they are shown as Claude Code drew them, reordered
    // by the app and unjoined. So is code, for good.
    let mut as_the_model_wrote = 0;
    let mut as_the_app_drew = 0;
    let mut the_same_either_way = 0;
    for (name, labels) in [(CLASSIC, 4), (FULLSCREEN, 0)] {
        let prose = prose_rows(name);
        let mut rows: HashMap<&str, BTreeSet<String>> = HashMap::new();
        for (drawn, shown) in rows_drawn_and_shown(name) {
            if !drawn.contains(is_farsi) {
                continue;
            }
            // The fullscreen display draws a button over a row while the
            // user has scrolled up. Such a row is shown as drawn, and is
            // counted where it shows without the button.
            if drawn.contains("Jump to bottom") {
                assert_eq!(shown, drawn, "{name}");
                continue;
            }
            let text = drawn.trim_start();
            let what = if let Some(row) = prose.iter().find(|row| row.drawn == drawn) {
                assert_eq!(shown, row.wanted, "{name}");
                match row.of {
                    Prose::Paragraph => "paragraph rows",
                    Prose::Line => "lines of the poem",
                    Prose::Label => "labels",
                }
            } else {
                let what = if ["- ", "1. ", "2. "]
                    .iter()
                    .any(|mark| text.starts_with(mark))
                {
                    "list items"
                } else if text.starts_with('│') {
                    "table rows"
                } else if text.ends_with(" #") {
                    "comments in code"
                } else {
                    // The message sent, in the Input box and above the reply.
                    continue;
                };
                assert_eq!(shown, drawn, "{name}");
                what
            };
            rows.entry(what).or_default().insert(drawn);
        }
        let count = |what: &str| rows.get(what).map_or(0, BTreeSet::len);
        // Two paragraphs of two rows and a poem of three lines.
        assert_eq!(count("paragraph rows"), 4, "{name}");
        assert_eq!(count("lines of the poem"), 3, "{name}");
        // A number alone on its row holds no letter: Layout leaves it
        // where Claude Code drew it.
        assert_eq!(count("labels"), labels, "{name}");
        // A list of five items and one of two, a table of a header and
        // three rows, and the Farsi comment that starts the code block.
        assert_eq!(count("list items"), 7, "{name}");
        assert_eq!(count("table rows"), 4, "{name}");
        assert_eq!(count("comments in code"), 1, "{name}");
        as_the_model_wrote += count("paragraph rows") + count("lines of the poem");
        as_the_app_drew += count("list items") + count("table rows") + count("comments in code");
        the_same_either_way += count("labels");
    }
    assert_eq!(
        (as_the_model_wrote, as_the_app_drew, the_same_either_way),
        (14, 24, 4)
    );
}

/// The rows of a recording, as drawn and as shown, whose text as drawn is
/// `text`, from the last moment that holds one.
fn row_drawn_as(name: &str, text: &str) -> (Row, Row) {
    moments(name)
        .into_iter()
        .rev()
        .find_map(|moment| {
            let at = moment.drawn.iter().position(|row| row_text(row) == text)?;
            Some((moment.drawn[at].clone(), moment.shown[at].clone()))
        })
        .unwrap_or_else(|| panic!("{name}: no row drawn as {text:?}"))
}

fn numbered_colours(row: &[Cell]) -> usize {
    row.iter()
        .filter(|cell| matches!(cell.style.foreground, Color::Palette(_)))
        .count()
}

#[test]
fn the_code_block_of_a_reply_stays_as_claude_code_drew_it_colours_and_all() {
    // Claude Code gives the words of a code block the Terminal's numbered
    // colours, and reorders the Farsi of a comment as it does on any row:
    // the comment is drawn from the right, its mark last.
    for (name, comment) in [
        (CLASSIC, "# این برنامه یک عدد را چاپ می‌کند"),
        (FULLSCREEN, "# این خط یک کامنت فارسی است"),
    ] {
        let comment = format!("  {}", as_drawn(comment));
        assert!(comment.ends_with(" #"), "{comment}");
        for (line, coloured) in [(comment.as_str(), 20), ("  x = 123", 3), ("  print(x)", 5)] {
            let (drawn, shown) = row_drawn_as(name, line);
            assert!(numbered_colours(&drawn) >= coloured, "{name}: {line}");
            assert!(shown == drawn, "{name}: {line}");
        }
    }
}

#[test]
fn the_message_sent_stays_as_drawn_on_its_background_in_both_displays() {
    // Claude Code draws the message the user sent on a background of its
    // own that fills each row, mark included. That background is what
    // tells its rows from a reply's when the mark is out of sight.
    let on_a_background = |row: &[Cell]| {
        let background = row[0].style.background;
        background != Color::Default && row.iter().all(|cell| cell.style.background == background)
    };
    // The last row of the message, which begins neither with the prompt
    // mark nor with a Farsi letter at the left: «... خط و بدون خط خالی بین
    // مصرع‌ها.»
    let last_row = format!("  {}", as_drawn("خط و بدون خط خالی بین مصرع‌ها."));

    // The classic display: found by scrolling back.
    let (drawn, shown) = row_drawn_as(CLASSIC, &last_row);
    assert!(on_a_background(&drawn));
    assert!(shown == drawn);

    // The fullscreen display, scrolled up: a header row with the start of
    // the message, cut short, and under it the last row of the message.
    // Its first rows are out of sight.
    let under_the_header: Vec<Moment> = moments(FULLSCREEN)
        .into_iter()
        .filter(|moment| {
            row_text(&moment.drawn[0]).starts_with('…') && row_text(&moment.drawn[1]) == last_row
        })
        .collect();
    assert!(!under_the_header.is_empty());
    for moment in under_the_header {
        for row in 0..2 {
            assert!(on_a_background(&moment.drawn[row]), "{}", moment.when);
            assert!(moment.shown[row] == moment.drawn[row], "{}", moment.when);
        }
    }
}

#[test]
fn a_row_with_a_button_drawn_over_it_stays_as_drawn_while_the_rows_around_it_are_corrected() {
    // While the user has scrolled up, the fullscreen display draws a button
    // for jumping to the bottom over the last row of the conversation in
    // sight. In this moment that row is the last line of the poem: «دلم
    // برای تو، پاییز من، تنگ است».
    let prose = prose_rows(FULLSCREEN);
    let poem: Vec<&ProseRow> = prose.iter().filter(|row| row.of == Prose::Line).collect();
    let covered: Vec<(Moment, usize)> = moments(FULLSCREEN)
        .into_iter()
        .filter_map(|moment| {
            let row = moment.drawn.iter().position(|row| {
                let text = row_text(row);
                text.starts_with(&poem[2].drawn) && text.contains("Jump to bottom")
            })?;
            Some((moment, row))
        })
        .collect();
    assert!(!covered.is_empty());

    for (moment, row) in covered {
        assert!(moment.shown[row] == moment.drawn[row], "{}", moment.when);
        // The two lines above it are whole, and read as written.
        for (above, line) in [(2, &poem[0]), (1, &poem[1])] {
            assert_eq!(row_text(&moment.drawn[row - above]), line.drawn);
            assert_eq!(row_text(&moment.shown[row - above]), line.wanted);
        }
    }
}

#[test]
fn bold_stays_on_the_characters_it_was_given_to() {
    // The model began its first paragraph with a bold «۱.». Claude Code
    // draws the paragraph from the right, so the two bold cells are the
    // last two of the row, and they still are when farcli has laid the row
    // out: the number, then the full stop to its left.
    let prose = prose_rows(CLASSIC);
    let (drawn, shown) = row_drawn_as(CLASSIC, &prose[0].drawn);

    for row in [&drawn, &shown] {
        let bold: Vec<usize> = (0..row.len()).filter(|col| row[*col].style.bold).collect();
        assert_eq!(bold, [98, 99]);
        assert_eq!((row[98].character, row[99].character), ('.', '۱'));
    }
    assert_ne!(row_text(&shown), row_text(&drawn));
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
