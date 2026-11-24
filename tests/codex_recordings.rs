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

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

use codex::{direction_of, laid_out, wrapped};
use farcli::recording::{Event, Recording, Replay};
use farcli::screen::{row_text, Cell, Color, Row};

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

#[test]
fn the_reply_reads_correctly_in_every_frame_while_it_arrives() {
    // Codex shows a reply as the model writes it, a few words a frame.
    // Counted here: the frames in which a row of the reply's prose was on
    // screen unfinished, and read as the start of what the model wrote.
    for name in [DEFAULT, INLINE] {
        let prose = prose_rows(name);
        let mut unfinished = BTreeSet::new();
        for moment in moments(name) {
            for (shown, drawn) in moment.shown.iter().zip(&moment.drawn) {
                let drawn = row_text(drawn);
                let Some((at, wanted)) = wanted(&prose, &drawn) else {
                    continue;
                };
                // A number alone, such as «۳», reads the same either way.
                let holds_a_farsi_letter = drawn.chars().any(|c| is_farsi(c) && c.is_alphabetic());
                if drawn != prose[at].drawn && holds_a_farsi_letter {
                    assert_eq!(row_text(shown), wanted, "{name}, {}", moment.when);
                    assert_ne!(row_text(shown), drawn, "{name}, {}", moment.when);
                    unfinished.insert(drawn);
                }
            }
        }
        // Two paragraphs and three lines of a poem, each seen at several
        // lengths on its way.
        assert!(unfinished.len() >= 15, "{name}: {}", unfinished.len());
    }
}

#[test]
fn a_wrapped_row_that_begins_with_an_english_word_keeps_its_paragraphs_direction() {
    // The model wrote «... سپس دستور git status را اجرا کنید ...» and Codex
    // wrapped it between `git` and `status`. The second row begins with an
    // English word and still reads from the right edge: `status` first, at
    // the right, and the full stop last, at the left.
    let prose = prose_rows(DEFAULT);
    let row = prose
        .iter()
        .find(|row| row.drawn.starts_with("  status "))
        .expect("the row that begins with `status`");
    let (_, shown) = row_drawn_as(DEFAULT, &row.drawn);

    let shown = row_text(&shown);
    assert_eq!(shown.chars().count(), 100, "{shown}");
    assert!(shown.ends_with(" status"), "{shown}");
    assert!(shown.trim_start().starts_with('.'), "{shown}");
}

#[test]
fn of_the_forty_six_rows_of_farsi_in_the_two_replies_twelve_read_as_the_model_wrote_them() {
    // What farcli corrects so far and what it leaves. The paragraphs of a
    // reply, and lines of Farsi separated by single line breaks, are laid
    // out from the text the model wrote. List items and table rows wait for
    // their own ticket: they are shown as Codex drew them, in Logical order
    // and unjoined. So is code, for good.
    let mut as_the_model_wrote = 0;
    let mut as_the_app_drew = 0;
    let mut the_same_either_way = 0;
    for name in [DEFAULT, INLINE] {
        let prose = prose_rows(name);
        let mut rows: HashMap<&str, BTreeSet<String>> = HashMap::new();
        for (drawn, shown) in rows_drawn_and_shown(name) {
            if !drawn.contains(is_farsi) {
                continue;
            }
            let text = drawn.trim_start();
            let margin = drawn.len() - text.len();
            let what = if let Some(row) = prose.iter().find(|row| row.drawn == drawn) {
                assert_eq!(shown, row.wanted, "{name}");
                match row.of {
                    Prose::Paragraph => "paragraph rows",
                    Prose::Line => "lines of the poem",
                    Prose::Label => "labels",
                }
            } else {
                // Everything else is as drawn: also the message sent, the
                // Input box and the row under it that names the session.
                assert_eq!(shown, drawn, "{name}");
                if margin >= 2 && ["• ", "1. ", "2. "].iter().any(|m| text.starts_with(m)) {
                    "list items"
                } else if margin == 3 && text.contains("    ") {
                    "table rows"
                } else if text.starts_with("# ") || text.starts_with("message = ") {
                    "rows of code"
                } else {
                    continue;
                }
            };
            rows.entry(what).or_default().insert(drawn);
        }
        let count = |what: &str| rows.get(what).map_or(0, BTreeSet::len);
        // A paragraph of one row, one of two rows, and a poem of three
        // lines.
        assert_eq!(count("paragraph rows"), 3, "{name}");
        assert_eq!(count("lines of the poem"), 3, "{name}");
        // A number alone on its row, such as «۳.», holds no letter:
        // Layout leaves it where Codex drew it.
        assert_eq!(count("labels"), 4, "{name}");
        // A list of five items and one of two, a table of a header and
        // three rows, and the comment and the string in the code block.
        assert_eq!(count("list items"), 7, "{name}");
        assert_eq!(count("table rows"), 4, "{name}");
        assert_eq!(count("rows of code"), 2, "{name}");
        as_the_model_wrote += count("paragraph rows") + count("lines of the poem");
        as_the_app_drew += count("list items") + count("table rows") + count("rows of code");
        the_same_either_way += count("labels");
    }
    assert_eq!(
        (as_the_model_wrote, as_the_app_drew, the_same_either_way),
        (12, 26, 8)
    );
}

/// The rows of a recording, as drawn and as shown, whose text as drawn is
/// `text`, from the last settled moment that holds one.
fn row_drawn_as(name: &str, text: &str) -> (Row, Row) {
    moments(name)
        .into_iter()
        .rev()
        .filter(|moment| moment.settled)
        .find_map(|moment| {
            let at = moment.drawn.iter().position(|row| row_text(row) == text)?;
            Some((moment.drawn[at].clone(), moment.shown[at].clone()))
        })
        .unwrap_or_else(|| panic!("{name}: no row drawn as {text:?}"))
}

#[test]
fn the_code_block_of_a_reply_stays_as_codex_drew_it_colours_and_all() {
    // Codex colours every word of the block, and draws the Farsi of the
    // comment and of the string in Logical order like everything else.
    for name in [DEFAULT, INLINE] {
        for line in [
            "  # پیام پاییزی",
            "  message = \"سلام پاییز\"",
            "  print(message)",
        ] {
            let (drawn, shown) = row_drawn_as(name, line);
            let characters: Vec<&Cell> = drawn.iter().filter(|c| c.character != ' ').collect();
            assert!(characters.len() >= 11, "{name}: {line}");
            assert!(
                characters
                    .iter()
                    .all(|cell| matches!(cell.style.foreground, Color::Rgb(..))),
                "{name}: {line}"
            );
            assert!(shown == drawn, "{name}: {line}");
        }
    }
}

#[test]
fn the_message_sent_stays_as_drawn_on_its_background_in_both_displays() {
    let on_a_background = |row: &[Cell]| {
        let background = row[0].style.background;
        background != Color::Default && row.iter().all(|cell| cell.style.background == background)
    };
    // The last row of the message, which carries no mark: «... خط و بدون
    // خط خالی بین مصرع‌ها.»
    let last_row = "  خط و بدون خط خالی بین مصرع\u{200c}ها.";
    for name in [DEFAULT, INLINE] {
        let (drawn, shown) = row_drawn_as(name, last_row);
        assert!(on_a_background(&drawn), "{name}");
        assert!(shown == drawn, "{name}");
    }

    // The default display, once the message has scrolled out of sight:
    // its start stays on the top row, cut short, and the reply's rows
    // under it are corrected.
    let prose = prose_rows(DEFAULT);
    let under_the_start: Vec<Moment> = moments(DEFAULT)
        .into_iter()
        .filter(|moment| {
            let top = row_text(&moment.drawn[0]);
            moment.settled && top.starts_with("› ") && top.ends_with('…')
        })
        .collect();
    assert!(under_the_start.len() >= 4);
    for moment in under_the_start {
        assert!(on_a_background(&moment.drawn[0]), "{}", moment.when);
        assert!(moment.shown[0] == moment.drawn[0], "{}", moment.when);
        let corrected = moment
            .drawn
            .iter()
            .zip(&moment.shown)
            .any(|(drawn, shown)| {
                let drawn = row_text(drawn);
                let row = prose.iter().find(|row| row.drawn == drawn);
                row.is_some_and(|row| row.of != Prose::Label && row_text(shown) == row.wanted)
            });
        assert!(corrected, "{}", moment.when);
    }
}

#[test]
fn while_farsi_is_typed_into_the_input_box_the_whole_screen_stays_as_codex_drew_it() {
    // The Input box is the app's own until the ticket that makes typing in
    // it right. Nothing in this session is Agent output, so farcli has
    // nothing to change: every cell and the cursor are as the app left
    // them, each time the user is about to press a key.
    let recording = load(TYPING);
    let mut through_farcli = Replay::start(&recording, recording.profile());
    let mut alone = Replay::without_farcli(&recording);
    let mut keys = 0;
    for (index, (_, event)) in recording.events.iter().enumerate() {
        if matches!(event, Event::UserTyped(_)) && !through_farcli.holding_frame() {
            keys += 1;
            assert_eq!(
                through_farcli.terminal.screen(),
                alone.terminal.screen(),
                "before event {index}"
            );
            assert_eq!(
                through_farcli.terminal.cursor(),
                alone.terminal.cursor(),
                "before event {index}"
            );
        }
        through_farcli.step();
        alone.step();
    }
    assert!(keys > 50);
}
