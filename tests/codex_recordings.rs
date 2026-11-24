//! The recordings of real Codex replies in `tests/recordings`, played
//! through the session: every Farsi paragraph of a reply, every line of
//! Farsi that stands on a row of its own, every list item and every row of
//! a table reads as Layout shows it from the text the model wrote, in both
//! of Codex's display modes, while the reply arrives and after it.
//! Everything else on the screen stays as the app drew it, colours and all.
//!
//! What the model wrote is in `NAME.reply.md` beside each reply recording.
//! The recordings were made in a Terminal of 100 columns, where a reply's
//! text is drawn in the 98 columns from the third to the last, and with the
//! colours a user's session has.

mod codex;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

use codex::{cells, direction_of, from_the_right, laid_out, wrapped};
use farcli::layout::Direction::RightToLeft;
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

/// What a row of a reply belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prose {
    /// A paragraph, which Codex may wrap onto several rows.
    Paragraph,
    /// One of several lines the model separated by single line breaks, as
    /// in a poem. Each is a paragraph of its own.
    Line,
    /// A number standing alone before a part of the reply, such as «۳.».
    Label,
    /// An item of a list.
    ListItem,
    /// A row of a table that holds text: its header, or one of its rows.
    TableRow,
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

/// Every list item of the reply, top to bottom. Codex draws a bullet for
/// the model's dash, and an item inside an item four columns further in.
/// No item of the recorded replies is long enough to wrap.
fn list_rows(name: &str) -> Vec<ProseRow> {
    let reply = fs::read_to_string(folder().join(format!("{name}.reply.md"))).unwrap();
    let mut rows = Vec::new();
    for line in reply.lines().map(str::trim_end) {
        let text = line.trim_start();
        let depth = (line.len() - text.len()) / 2;
        let Some((marker, text)) = text.split_once(' ') else {
            continue;
        };
        let numbered = marker
            .strip_suffix('.')
            .is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()));
        if marker != "-" && !numbered {
            continue;
        }
        let marker = if marker == "-" { "•" } else { marker };
        let written = format!("{}{marker} {text}", " ".repeat(depth * 4));
        rows.push(ProseRow {
            drawn: format!("  {written}"),
            wanted: format!("  {}", laid_out(&written, direction_of(text), WIDTH)),
            first_of_its_paragraph: true,
            of: Prose::ListItem,
        });
    }
    rows
}

/// The rows of the reply's table that hold text, as Codex spaces them once
/// it has `body_rows` of the table's rows under the header: each cell as
/// wide as the widest cell of its column, with an empty column before it
/// and after it, and two more between one cell and the next.
fn table_rows_after(name: &str, body_rows: usize) -> Vec<ProseRow> {
    let (table, widths) = table_after(name, body_rows);
    table
        .iter()
        .map(|row| {
            let written: Vec<String> = row
                .iter()
                .zip(&widths)
                .map(|(text, width)| format!(" {text:<width$}", width = width - 1))
                .collect();
            let wanted: Vec<String> = written
                .iter()
                .zip(&widths)
                .map(|(cell, width)| {
                    let shown = laid_out(cell, direction_of(cell), *width);
                    format!("{shown:<width$}")
                })
                .collect();
            ProseRow {
                drawn: format!("  {}", written.join("  ").trim_end()),
                wanted: format!("  {}", wanted.join("  ").trim_end()),
                first_of_its_paragraph: true,
                of: Prose::TableRow,
            }
        })
        .collect()
}

/// The cells of the reply's table once Codex has `body_rows` of its rows
/// under the header, and how wide it draws each column then.
fn table_after(name: &str, body_rows: usize) -> (Vec<Vec<String>>, Vec<usize>) {
    let reply = fs::read_to_string(folder().join(format!("{name}.reply.md"))).unwrap();
    let table: Vec<Vec<String>> = reply
        .lines()
        .filter(|line| line.starts_with('|') && !line.starts_with("|---"))
        .take(1 + body_rows)
        .map(|line| {
            let line = line.trim_end().trim_matches('|');
            line.split('|')
                .map(|cell| cell.trim().to_string())
                .collect()
        })
        .collect();
    let columns = table.first().map_or(0, Vec::len);
    let widths: Vec<usize> = (0..columns)
        .map(|column| table.iter().map(|row| cells(&row[column])).max().unwrap() + 2)
        .collect();
    (table, widths)
}

/// The rule Codex draws under the header of the reply's table once it has
/// `body_rows` of the table's rows: a stretch for each column.
fn table_rule_after(name: &str, body_rows: usize) -> String {
    let (_, widths) = table_after(name, body_rows);
    let stretches: Vec<String> = widths.iter().map(|width| "━".repeat(*width)).collect();
    format!("  {}", stretches.join("  "))
}

/// The rows of the reply's finished table that hold text.
fn table_rows(name: &str) -> Vec<ProseRow> {
    table_rows_after(name, 3)
}

/// Every row of the reply that farcli lays out: its prose, its list items
/// and the rows of its table.
fn rows_laid_out(name: &str) -> Vec<ProseRow> {
    let mut rows = prose_rows(name);
    rows.extend(list_rows(name));
    rows.extend(table_rows(name));
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

/// Checks every moment of a recording: each row of the reply's prose, of
/// its lists and of its table, whole or still being written, reads as
/// wanted and keeps the styles its characters had, and every other row is
/// exactly as drawn, in every cell. Returns how often each of those rows
/// was seen whole on a settled screen.
fn check_recording(name: &str) -> Vec<usize> {
    // While the table arrives Codex draws it again with each row the model
    // adds, and its columns grow. The rule under its header tells how many
    // rows it has.
    let at_each_stage: Vec<(String, Vec<ProseRow>)> = (0..=3)
        .map(|body_rows| {
            let mut rows = prose_rows(name);
            rows.extend(list_rows(name));
            rows.extend(table_rows_after(name, body_rows));
            (table_rule_after(name, body_rows), rows)
        })
        .collect();
    let mut times_seen = vec![0; at_each_stage[3].1.len()];
    for moment in moments(name) {
        let stage = (0..3)
            .rev()
            .find(|stage| {
                let rule = &at_each_stage[*stage].0;
                moment.drawn.iter().any(|row| row_text(row) == *rule)
            })
            .unwrap_or(3);
        let rows = &at_each_stage[stage].1;
        let whole = if stage == 3 { rows.len() } else { 0 };
        for (row, (shown, drawn)) in moment.shown.iter().zip(&moment.drawn).enumerate() {
            let what = format!("{name}, {}, row {row}", moment.when);
            let drawn_text = row_text(drawn);
            if let Some((at, wanted)) = wanted(rows, &drawn_text) {
                let shown_text = row_text(shown);
                if has_no_direction_yet(&moment, row) {
                    let (margin, text) = drawn_text.split_at(2);
                    let from_the_right = format!("{margin}{}", laid_out(text, RightToLeft, WIDTH));
                    assert!(
                        shown_text == drawn_text || shown_text == from_the_right,
                        "{what}: {shown_text:?}"
                    );
                } else {
                    assert_eq!(shown_text, wanted, "{what}");
                }
                assert_eq!(styles(shown), styles(drawn), "{what}");
                if moment.settled && at < whole && drawn_text == rows[at].drawn {
                    times_seen[at] += 1;
                }
                continue;
            }
            // This includes the message the user sent, the Input box, the
            // rules of the table and the code block.
            assert!(
                shown == drawn,
                "{what}:\n shown {:?}\n drawn {drawn_text:?}",
                row_text(shown)
            );
        }
    }
    times_seen
}

/// True for a row that holds no letter yet while the reply is still
/// arriving, such as «۵» or a bullet whose text has not come. Such a row
/// has no direction of its own. Where it is shown depends on what stands
/// right above it at that moment: for a frame Codex draws the first
/// characters of a new part against the last row of the part before, and
/// a list item goes the way of the item above it.
fn has_no_direction_yet(moment: &Moment, row: usize) -> bool {
    !moment.settled && !row_text(&moment.drawn[row]).contains(char::is_alphabetic)
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
    // a poem of three lines, seven list items, and a table of a header and
    // three rows.
    for name in [DEFAULT, INLINE] {
        let ever_drawn: BTreeSet<String> = rows_drawn_and_shown(name)
            .into_iter()
            .map(|(drawn, _)| drawn)
            .collect();
        assert_eq!(prose_rows(name).len(), 10, "{name}");
        assert_eq!(list_rows(name).len(), 7, "{name}");
        assert_eq!(table_rows(name).len(), 4, "{name}");
        for row in rows_laid_out(name) {
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
    // Counted here: the frames in which a row of the reply's prose or a
    // list item was on screen unfinished, and read as the start of what
    // the model wrote.
    for name in [DEFAULT, INLINE] {
        let prose = rows_laid_out(name);
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
        // Two paragraphs, three lines of a poem and two list items, each
        // seen at several lengths on its way.
        assert!(unfinished.len() >= 18, "{name}: {}", unfinished.len());
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
fn of_the_forty_six_rows_of_farsi_in_the_two_replies_forty_two_read_as_the_model_wrote_them() {
    // What farcli corrects and what it leaves. The paragraphs of a reply,
    // lines of Farsi separated by single line breaks, numbers that stand
    // alone as labels, list items and the rows of a table are laid out
    // from the text the model wrote. Code is shown as Codex drew it, in
    // Logical order and unjoined, for good.
    let mut as_the_model_wrote = 0;
    let mut as_the_app_drew = 0;
    for name in [DEFAULT, INLINE] {
        let laid_out = rows_laid_out(name);
        let mut rows: HashMap<&str, BTreeSet<String>> = HashMap::new();
        for (drawn, shown) in rows_drawn_and_shown(name) {
            if !drawn.contains(is_farsi) {
                continue;
            }
            let text = drawn.trim_start();
            let what = if let Some(row) = laid_out.iter().find(|row| row.drawn == drawn) {
                assert_eq!(shown, row.wanted, "{name}");
                assert_ne!(shown, drawn, "{name}");
                match row.of {
                    Prose::Paragraph => "paragraph rows",
                    Prose::Line => "lines of the poem",
                    Prose::Label => "labels",
                    Prose::ListItem => "list items",
                    Prose::TableRow => "table rows",
                }
            } else {
                // Everything else is as drawn: also the message sent, the
                // Input box and the row under it that names the session.
                assert_eq!(shown, drawn, "{name}");
                if text.starts_with("# ") || text.starts_with("message = ") {
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
        // A number alone on its row, written in Farsi digits.
        assert_eq!(count("labels"), 4, "{name}");
        // A list of five items and one of two, a table of a header and
        // three rows, and the comment and the string in the code block.
        assert_eq!(count("list items"), 7, "{name}");
        assert_eq!(count("table rows"), 4, "{name}");
        assert_eq!(count("rows of code"), 2, "{name}");
        as_the_model_wrote += count("paragraph rows")
            + count("lines of the poem")
            + count("labels")
            + count("list items")
            + count("table rows");
        as_the_app_drew += count("rows of code");
    }
    assert_eq!((as_the_model_wrote, as_the_app_drew), (42, 4));
}

#[test]
fn the_lists_and_the_table_of_a_reply_spelled_out() {
    // The rows above are checked against what Layout makes of the model's
    // text. Here some of them are written out by hand, each as Codex drew
    // it and as it should be shown.
    for name in [DEFAULT, INLINE] {
        let shown = |drawn: &str| row_text(&row_drawn_as(name, drawn).1);
        let from_right_edge =
            |on_screen: String| format!("{on_screen:>100}").trim_end().to_string();

        // «- باران», the item inside it «- رگبار», and «2. چای نوشیدن»: the
        // marker on the right, the inner item four columns in from there.
        assert_eq!(
            shown("  • باران"),
            from_right_edge(format!("{} •", from_the_right("ﺑﺎﺭﺍﻥ")))
        );
        assert_eq!(
            shown("      • رگبار"),
            from_right_edge(format!("{} •    ", from_the_right("ﺭﮔﺒﺎﺭ")))
        );
        assert_eq!(
            shown("  2. چای نوشیدن"),
            from_right_edge(format!("{} .2", from_the_right("ﭼﺎﯼ ﻧﻮﺷﯿﺪﻥ")))
        );

        // «| برگ | زرد | 33 x 20 |»: the columns where Codex drew them,
        // under the same rule, each Farsi cell read from the right inside
        // its own column, and the size as the model wrote it.
        let rule = "  ─────  ────────  ─────────";
        assert_eq!(
            shown("   برگ    زرد       33 x 20"),
            format!(
                "   {}       {}    33 x 20",
                from_the_right("ﺑﺮﮒ"),
                from_the_right("ﺯﺭﺩ")
            )
        );
        assert_eq!(shown(rule), rule);
    }
}

#[test]
fn a_size_in_a_sentence_of_the_reply_keeps_the_order_of_its_numbers() {
    // The model wrote «... (33 x 20) ...» in its second paragraph. By the
    // Unicode rules alone it would be shown as `(x 20 33)`.
    for name in [DEFAULT, INLINE] {
        let row = prose_rows(name)
            .into_iter()
            .find(|row| row.drawn.contains("(33 x 20)"))
            .expect("the row with the size");
        let (_, shown) = row_drawn_as(name, &row.drawn);

        let shown = row_text(&shown);
        assert!(shown.contains(" (33 x 20) "), "{shown}");
    }
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
