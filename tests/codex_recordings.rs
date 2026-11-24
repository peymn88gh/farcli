//! The recordings of real Codex replies in `tests/recordings`, played
//! through the session: every Farsi paragraph of a reply, and every line of
//! Farsi that stands on a row of its own, reads as Layout shows it from the
//! text the model wrote, in both of Codex's display modes, while the reply
//! arrives and after it. Everything else on the screen stays as the app
//! drew it, colours and all.
//!
//! What the model wrote is in `NAME.reply.md` beside each reply recording.
//! The recordings were made in a Terminal of 100 columns, where a reply's
//! text is drawn in the 98 columns from the third to the last, and with the
//! colours a user's session has.

mod codex;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use codex::{direction_of, laid_out, wrapped};
use farcli::recording::{Event, Recording, Replay};
use farcli::screen::{row_text, Cell, Row};

/// Codex's default display, which draws on the alternate screen.
const DEFAULT: &str = "codex-reply";

/// Codex started with `--no-alt-screen`: the conversation scrolls off the
/// top of the main screen.
const INLINE: &str = "codex-inline-reply";

/// A session in which Farsi is typed into the Input box and nothing is sent.
const TYPING: &str = "codex-typing";

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
    /// A paragraph, which Codex may wrap onto several rows.
    Paragraph,
    /// One of several lines the model separated by single line breaks, as
    /// in a poem. Each is a paragraph of its own.
    Line,
    /// A number standing alone before a part of the reply, such as «۳.».
    Label,
}

/// A row of a reply's prose.
struct ProseRow {
    /// The row as Codex drew it: Logical order, the reply mark before the
    /// reply's first row and two empty columns before every other.
    drawn: String,
    /// The row as it should read: what Layout makes of the model's text.
    wanted: String,
    /// The row is the first of its paragraph.
    first_of_its_paragraph: bool,
    of: Prose,
}

/// Every row of the reply's prose, top to bottom: the lines of the model's
/// text that are not in a list, a table or a code block.
fn prose_rows(name: &str) -> Vec<ProseRow> {
    let reply = fs::read_to_string(folder().join(format!("{name}.reply.md"))).unwrap();
    let reply = reply.replace("\r\n", "\n");
    let mut rows = Vec::new();
    let mut in_code = false;
    let mut lines_of_block = 0;
    for line in reply.lines().map(str::trim_end) {
        if line.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        let text = line.trim_start();
        let numbered = text
            .split_once(". ")
            .is_some_and(|(number, _)| number.chars().all(|c| c.is_ascii_digit()));
        if line.is_empty() {
            lines_of_block = 0;
        }
        if in_code || line.is_empty() || text.starts_with("- ") || text.starts_with('|') || numbered
        {
            continue;
        }
        lines_of_block += 1;
        let of = match line.contains(|c: char| c.is_alphabetic()) {
            false => Prose::Label,
            true if lines_of_block > 1 => Prose::Line,
            true => Prose::Paragraph,
        };
        let direction = direction_of(line);
        for (at, row) in wrapped(line, WIDTH).into_iter().enumerate() {
            let margin = if rows.is_empty() { "• " } else { "  " };
            rows.push(ProseRow {
                drawn: format!("{margin}{row}"),
                wanted: format!("{margin}{}", laid_out(&row, direction, WIDTH))
                    .trim_end()
                    .to_string(),
                first_of_its_paragraph: at == 0,
                of,
            });
        }
    }
    rows
}

/// How a row that Codex drew should read, when it is a row of the reply's
/// prose or the start of one, still being written. `None` for any other
/// row, which should read as drawn.
fn wanted(prose: &[ProseRow], drawn: &str) -> Option<(usize, String)> {
    if let Some(at) = prose.iter().position(|row| row.drawn == drawn) {
        return Some((at, prose[at].wanted.clone()));
    }
    // While a reply arrives, its last row grows from the left, because
    // Codex draws Logical order. Until its first letter has come, a
    // paragraph has no direction of its own yet.
    let (at, row) = prose
        .iter()
        .enumerate()
        .find(|(_, row)| drawn.chars().count() > 2 && row.drawn.starts_with(drawn))?;
    let (margin, text) = drawn.split_at(drawn.char_indices().nth(2)?.0);
    let direction = match row.first_of_its_paragraph {
        true => direction_of(text),
        false => direction_of(first_row_of_paragraph(prose, at)),
    };
    Some((
        at,
        format!("{margin}{}", laid_out(text, direction, WIDTH))
            .trim_end()
            .to_string(),
    ))
}

/// The first row of the paragraph that the row at `at` belongs to, without
/// its margin: the row whose first letter gives the Paragraph direction.
fn first_row_of_paragraph(prose: &[ProseRow], at: usize) -> &str {
    let first = (0..=at)
        .rev()
        .find(|row| prose[*row].first_of_its_paragraph)
        .unwrap_or(at);
    let drawn = &prose[first].drawn;
    &drawn[drawn.char_indices().nth(2).map_or(0, |(at, _)| at)..]
}

/// What a Terminal held at one moment of a recording, without farcli and
/// through it: the rows that scrolled off the top, then the rows on screen.
struct Moment {
    when: String,
    /// The user is about to press a key, or the recording is over: the
    /// screen has stood still for a while.
    settled: bool,
    drawn: Vec<Row>,
    shown: Vec<Row>,
}

/// Plays a recording with and without farcli side by side, and takes what
/// the two Terminals hold after every frame Codex completed, each time the
/// user is about to press a key, and at the end.
fn moments(name: &str) -> Vec<Moment> {
    let recording = load(name);
    let mut moments = Vec::new();
    let mut through_farcli = Replay::start(&recording, recording.profile());
    let mut alone = Replay::without_farcli(&recording);
    let mut take = |through_farcli: &Replay, alone: &Replay, when: String, settled: bool| {
        let mut shown = through_farcli.terminal.scrollback();
        shown.extend(through_farcli.terminal.screen());
        let mut drawn = alone.terminal.scrollback();
        drawn.extend(alone.terminal.screen());
        assert_eq!(shown.len(), drawn.len(), "{name}, {when}");
        moments.push(Moment {
            when,
            settled,
            drawn,
            shown,
        });
    };
    for (index, (_, event)) in recording.events.iter().enumerate() {
        if matches!(event, Event::UserTyped(_)) && !through_farcli.holding_frame() {
            take(
                &through_farcli,
                &alone,
                format!("before event {index}"),
                true,
            );
        }
        through_farcli.step();
        alone.step();
        // Between the marks of a frame the Terminal without farcli is
        // ahead: it shows the frame half drawn.
        if matches!(event, Event::AppWrote(_)) && !through_farcli.holding_frame() {
            take(
                &through_farcli,
                &alone,
                format!("after event {index}"),
                false,
            );
        }
    }
    through_farcli.finish();
    alone.finish();
    take(&through_farcli, &alone, "at the end".to_string(), true);
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

/// Checks every moment of a recording: each row of the reply's prose, whole
/// or still being written, reads as wanted and keeps the styles its
/// characters had, and every other row is exactly as drawn, in every cell.
/// Returns how often each prose row was seen whole on a settled screen.
fn check_recording(name: &str) -> Vec<usize> {
    let prose = prose_rows(name);
    let mut times_seen = vec![0; prose.len()];
    for moment in moments(name) {
        for (row, (shown, drawn)) in moment.shown.iter().zip(&moment.drawn).enumerate() {
            let what = format!("{name}, {}, row {row}", moment.when);
            let drawn_text = row_text(drawn);
            if let Some((at, wanted)) = wanted(&prose, &drawn_text) {
                assert_eq!(row_text(shown), wanted, "{what}");
                assert_eq!(styles(shown), styles(drawn), "{what}");
                if moment.settled && drawn_text == prose[at].drawn {
                    times_seen[at] += 1;
                }
                continue;
            }
            // This includes the message the user sent, the Input box, the
            // lists, the table and the code block.
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
/// it, over the settled moments of a recording.
fn rows_drawn_and_shown(name: &str) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    for moment in moments(name).into_iter().filter(|moment| moment.settled) {
        for (drawn, shown) in moment.drawn.iter().zip(&moment.shown) {
            pairs.insert((row_text(drawn), row_text(shown)));
        }
    }
    pairs
}

#[test]
fn the_rows_worked_out_from_what_the_model_wrote_are_the_rows_codex_drew() {
    // The tests below find a reply's prose on the screen by working out,
    // from the model's text, how Codex wrapped and drew it. This holds
    // that working-out against the recordings themselves.
    //
    // Each reply: a paragraph of one row, one of two rows, four labels and
    // a poem of three lines.
    for name in [DEFAULT, INLINE] {
        let ever_drawn: BTreeSet<String> = rows_drawn_and_shown(name)
            .into_iter()
            .map(|(drawn, _)| drawn)
            .collect();
        let prose = prose_rows(name);
        assert_eq!(prose.len(), 10, "{name}");
        for row in prose {
            assert!(ever_drawn.contains(&row.drawn), "{name}: {}", row.drawn);
        }
    }
}

#[test]
fn in_the_default_display_every_paragraph_of_the_reply_reads_as_the_model_wrote_it() {
    let times_seen = check_recording(DEFAULT);

    // The reply is longer than the screen. Its first paragraphs come into
    // sight when the user scrolls up with Page Up.
    assert!(times_seen.iter().all(|times| *times >= 1), "{times_seen:?}");
}

#[test]
fn in_the_inline_display_every_paragraph_of_the_reply_reads_as_the_model_wrote_it() {
    let times_seen = check_recording(INLINE);

    // The reply's first rows scroll off the top, and are found by
    // scrolling back.
    assert!(times_seen.iter().all(|times| *times >= 1), "{times_seen:?}");
}
