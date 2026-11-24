//! Lines of Farsi that stand under one another in a reply are told from
//! code: each line reads as a paragraph of its own, and code stays as Claude
//! Code drew it.
//!
//! These tests replay two recordings in which Claude Code draws a Markdown
//! text written for the purpose (`claude-classic-markdown-shapes.reply.md`),
//! with the colours a user's session has. No model wrote it: an earlier
//! session's transcript was copied, the reply in the copy replaced by hand,
//! and the copy started again with `--resume`, so that Claude Code drew it.
//!
//! What tells code, as Claude Code 2.1.322 draws it:
//!
//! - a word in one of the Terminal's numbered colours: the app gives those
//!   to the keywords, numbers, strings, comments and built-in names of a
//!   code block in a language it knows;
//! - a row drawn wholly in a colour: a code block with no language named is
//!   drawn in the colour of inline code, every word of it;
//! - a row drawn wholly dim: the name of a language the app does not know,
//!   which it draws above the code;
//! - a row that starts further in than text does.
//!
//! What does not: a code block in a language the app knows, or named
//! `text`, in which no word is a keyword, number, string, comment or
//! built-in name. Claude Code draws it exactly as it draws lines of prose,
//! and farcli reads it as lines of prose. The last test holds that.

mod claude_code;

use claude_code::{as_drawn, direction_of, laid_out};
use farcli::layout::Direction::RightToLeft;
use farcli::recording::{Event, Recording, Replay};
use farcli::screen::{row_text, Cell, Color, Row};
use std::collections::HashMap;
use std::path::PathBuf;

const CLASSIC: &str = "claude-classic-markdown-shapes";
const FULLSCREEN: &str = "claude-fullscreen-markdown-shapes";

/// The width of the area a reply's text is drawn in.
const WIDTH: usize = 98;

fn load(name: &str) -> Recording {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("recordings");
    Recording::load(folder.join(format!("{name}.recording"))).unwrap()
}

/// The reply as drawn and as shown through farcli, in the classic display
/// at the end of the session: the rows that scrolled off the top and then
/// the rows on screen.
struct Reply {
    drawn: Vec<Row>,
    shown: Vec<Row>,
}

impl Reply {
    fn of_the_classic_display() -> Reply {
        let recording = load(CLASSIC);
        let mut through_farcli = Replay::start(&recording, recording.profile());
        let mut alone = Replay::without_farcli(&recording);
        through_farcli.finish();
        alone.finish();
        let mut drawn = alone.terminal.scrollback();
        drawn.extend(alone.terminal.screen());
        let mut shown = through_farcli.terminal.scrollback();
        shown.extend(through_farcli.terminal.screen());
        Reply { drawn, shown }
    }

    /// Where Claude Code drew the line `logical` of the Markdown text, on a
    /// row of its own, starting `indent` columns into the reply's area.
    fn row_of(&self, logical: &str, indent: usize) -> usize {
        let wanted = format!("{}{}", " ".repeat(2 + indent), as_drawn(logical));
        self.drawn
            .iter()
            .position(|row| {
                let text = row_text(row);
                text == wanted || text.strip_prefix('●') == wanted.strip_prefix(' ')
            })
            .unwrap_or_else(|| panic!("no row drawn as {wanted:?}"))
    }

    /// The text of the row through farcli, without the two columns before
    /// the reply's area.
    fn shown(&self, row: usize) -> String {
        row_text(&self.shown[row]).chars().skip(2).collect()
    }

    fn is_as_drawn(&self, row: usize) -> bool {
        self.shown[row] == self.drawn[row]
    }

    /// Checks that the lines of a block, drawn one under another, are all
    /// exactly as Claude Code drew them.
    fn stays_as_drawn(&self, lines: &[&str]) {
        let first = self.row_of(lines[0], 0);
        for (below, line) in lines.iter().enumerate() {
            let row = first + below;
            assert_eq!(row_text(&self.drawn[row]), format!("  {}", as_drawn(line)));
            assert!(
                self.is_as_drawn(row),
                "{line}: shown as {:?}",
                self.shown(row)
            );
        }
    }

    /// Checks that each line reads as a paragraph of its own: as Layout
    /// shows it from the text that was written.
    fn reads_as_written(&self, lines: &[&str]) {
        let first = self.row_of(lines[0], 0);
        for (below, line) in lines.iter().enumerate() {
            let row = first + below;
            assert_eq!(row_text(&self.drawn[row]), format!("  {}", as_drawn(line)));
            assert_eq!(
                self.shown(row),
                laid_out(line, direction_of(line), WIDTH),
                "{line}"
            );
        }
    }
}

#[test]
fn lines_separated_by_single_line_breaks_each_read_as_a_paragraph_of_their_own() {
    let reply = Reply::of_the_classic_display();

    // An address: three lines one under another, in the Terminal's own
    // colours. Each starts at the right edge, and the numbers of the last
    // keep their order.
    reply.reads_as_written(&["نشانی من این است:", "تهران، خیابان آزادی", "پلاک 12، واحد 3"]);
    let last = reply.row_of("پلاک 12، واحد 3", 0);
    assert_eq!(reply.shown(last).chars().count(), WIDTH);
    assert!(reply.shown(last).trim_start().starts_with("3 "));

    // The same for a line break the writer asked for with two spaces.
    reply.reads_as_written(&[
        "این پاراگراف دو خط دارد که با دو فاصله جدا شده‌اند.",
        "خط دوم همان پاراگراف است.",
    ]);
}

#[test]
fn a_line_that_begins_with_an_english_word_is_a_paragraph_by_itself() {
    let reply = Reply::of_the_classic_display();
    let poem = [
        "باران می‌بارد، کوچه خیس است",
        "Python را دوست دارم و می‌نویسم",
        "دلم برای تو تنگ است",
    ];
    let first = reply.row_of(poem[0], 0);

    assert_eq!(reply.shown(first), laid_out(poem[0], RightToLeft, WIDTH));
    assert_eq!(
        reply.shown(first + 2),
        laid_out(poem[2], RightToLeft, WIDTH)
    );
    // The middle line is one row that begins with an English word and ends
    // in Farsi. Claude Code draws a row that ends with the English word the
    // same way, and nothing on the row tells which it is: it is read as
    // beginning with its Farsi, like any paragraph of one such row (see
    // `claude_doubtful_rows.rs`). So it starts at the right edge like the
    // lines around it, with `Python` at its left end.
    assert_eq!(as_drawn(poem[1]), as_drawn("را دوست دارم و می‌نویسم Python"));
    assert_eq!(
        reply.shown(first + 1),
        laid_out("را دوست دارم و می‌نویسم Python", RightToLeft, WIDTH)
    );
}

#[test]
fn a_code_block_with_no_language_is_drawn_wholly_in_a_colour_and_stays_as_drawn() {
    let reply = Reply::of_the_classic_display();
    let lines = ["سلام دنیا", "این یک متن نمونه است", "name = علی"];

    // Every word of the block is in the colour Claude Code gives inline
    // code, which is not one of the Terminal's numbered colours.
    let first = reply.row_of(lines[0], 0);
    for row in &reply.drawn[first..first + 3] {
        let mut words = row.iter().filter(|cell| cell.character != ' ');
        assert!(words.all(|cell| matches!(cell.style.foreground, Color::Rgb(..))));
    }
    reply.stays_as_drawn(&lines);

    // And a block of one line, which starts with a Farsi letter.
    reply.stays_as_drawn(&["سلام دنیا، این کد است"]);
}

#[test]
fn a_code_block_whose_words_have_numbered_colours_stays_as_drawn() {
    let reply = Reply::of_the_classic_display();

    // `echo` is a built-in name of the shell, and coloured. The line under
    // it holds no coloured word and starts with a Farsi letter: it is code
    // because it stands in the same block.
    reply.stays_as_drawn(&["echo سلام دنیا", "پوشه=خانه"]);
    // Lines with no Farsi in them are what Layout would make of them
    // anyway.
    reply.stays_as_drawn(&["git add .", "git commit", "npm install"]);
    // A block with an indented line, and a string in Farsi.
    let first = reply.row_of("def greet():", 0);
    assert_eq!(
        row_text(&reply.drawn[first + 1]),
        format!("      {}", as_drawn("print(\"سلام\")"))
    );
    assert!(reply.is_as_drawn(first) && reply.is_as_drawn(first + 1));
}

#[test]
fn a_code_block_in_a_language_claude_code_does_not_know_stays_as_drawn_under_its_name() {
    let reply = Reply::of_the_classic_display();

    // Claude Code draws the name of the language above the code, dim, and
    // the code in the Terminal's own colours.
    let name = reply.row_of("foolang", 0);
    assert!(reply.drawn[name]
        .iter()
        .filter(|cell| cell.character != ' ')
        .all(|cell| cell.style.dim));

    reply.stays_as_drawn(&["foolang", "اجرا کن برنامه", "تمام شد"]);
}

#[test]
fn a_paragraph_with_a_link_in_it_reads_as_written_and_the_link_stays_on_its_word() {
    let reply = Reply::of_the_classic_display();
    // Claude Code draws none of the marks of the Markdown: only the words.
    let written = "متن با کد درون‌خطی و متن پررنگ و متن کج و پیوند در یک جمله.";
    let row = reply.row_of(written, 0);
    // The link is in one of the Terminal's numbered colours, as the words
    // of a code block are.
    let is_link = |cell: &&Cell| cell.style.link.is_some();
    let link: Vec<&Cell> = reply.drawn[row].iter().filter(is_link).collect();
    assert_eq!(link.len(), "پیوند".chars().count());
    assert!(link
        .iter()
        .all(|cell| matches!(cell.style.foreground, Color::Palette(_))));

    assert_eq!(reply.shown(row), laid_out(written, RightToLeft, WIDTH));
    // Each style is still on as many characters as before: the link, the
    // colour of inline code, bold and italic.
    let styles = |row: &Row| {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for cell in row.iter().filter(|cell| cell.character != ' ') {
            *counts.entry(format!("{:?}", cell.style)).or_default() += 1;
        }
        counts
    };
    assert_eq!(styles(&reply.shown[row]), styles(&reply.drawn[row]));
    assert_eq!(styles(&reply.drawn[row]).len(), 5);
    // And each is on the words it was given to: the joined letters of
    // those words, in Screen order.
    let letters_of = |words: &str| -> String {
        laid_out(words, RightToLeft, WIDTH)
            .chars()
            .filter(|c| !matches!(c, ' ' | '\u{200c}'))
            .collect()
    };
    let letters_drawn = |with: &dyn Fn(&Cell) -> bool| -> String {
        reply.shown[row]
            .iter()
            .filter(|cell| cell.character != ' ' && with(cell))
            .map(|cell| cell.character)
            .collect()
    };
    assert_eq!(
        letters_drawn(&|cell| cell.style.link.is_some()),
        letters_of("پیوند")
    );
    assert_eq!(
        letters_drawn(&|cell| cell.style.italic),
        letters_of("متن کج")
    );
    assert_eq!(
        letters_drawn(&|cell| cell.style.bold),
        letters_of("متن پررنگ")
    );
    assert_eq!(
        letters_drawn(&|cell| matches!(cell.style.foreground, Color::Rgb(..))),
        letters_of("کد درون‌خطی")
    );
    assert_eq!(letters_of("پیوند"), "ﺪﻧﻮﯿﭘ");
}

#[test]
fn a_heading_reads_as_written_and_stays_bold() {
    let reply = Reply::of_the_classic_display();
    let row = reply.row_of("یک عنوان فارسی", 0);

    assert_eq!(
        reply.shown(row),
        laid_out("یک عنوان فارسی", RightToLeft, WIDTH)
    );
    assert!(reply.shown[row]
        .iter()
        .filter(|cell| cell.character != ' ')
        .all(|cell| cell.style.bold));
}

#[test]
fn a_quotation_stays_as_drawn_with_its_bar() {
    // Claude Code draws a bar before each line of a quotation and keeps it
    // out of the reordering, as it does a list marker. Until quotations are
    // laid out with their bar, they stay as drawn.
    let reply = Reply::of_the_classic_display();
    let first = (0..reply.drawn.len())
        .find(|row| row_text(&reply.drawn[*row]).starts_with("  ▎ "))
        .expect("a quotation");

    for row in [first, first + 1] {
        assert!(row_text(&reply.drawn[row]).starts_with("  ▎ "));
        assert!(reply.is_as_drawn(row), "{:?}", reply.shown(row));
    }
}

#[test]
fn list_items_stay_as_drawn_with_the_rows_they_wrap_onto() {
    // Lists wait for their own ticket. Claude Code draws the rows an item
    // wraps onto under the item's text, further in than a paragraph's rows.
    let reply = Reply::of_the_classic_display();
    let items: Vec<usize> = (0..reply.drawn.len())
        .filter(|row| {
            let text = row_text(&reply.drawn[*row]);
            let text = text.trim_start();
            ["- ", "1. ", "2. "]
                .iter()
                .any(|mark| text.starts_with(mark))
        })
        .collect();

    assert_eq!(items.len(), 5);
    for item in items {
        assert!(reply.is_as_drawn(item), "{:?}", reply.shown(item));
    }
    let wrapped_onto: Vec<usize> = (0..reply.drawn.len())
        .filter(|row| {
            let text = row_text(&reply.drawn[*row]);
            text.starts_with("    .") || text.starts_with("     .") || text.starts_with("       .")
        })
        .collect();
    assert_eq!(wrapped_onto.len(), 3);
    for row in wrapped_onto {
        assert!(reply.is_as_drawn(row), "{:?}", reply.shown(row));
    }
}

#[test]
fn in_the_fullscreen_display_every_row_reads_as_in_the_classic_display_wherever_it_is_scrolled_to()
{
    // What each row of the reply reads as in the classic display, where the
    // whole reply is read from its first row down.
    let classic = Reply::of_the_classic_display();
    let mut reads_as: HashMap<String, String> = HashMap::new();
    for (drawn, shown) in classic.drawn.iter().zip(&classic.shown) {
        reads_as.insert(row_text(drawn), row_text(shown));
    }

    // In the fullscreen display the user turns the mouse wheel up through
    // the reply and down again, then presses Page Up and Page Down. The top
    // of the screen cuts through every part of the reply in turn.
    let recording = load(FULLSCREEN);
    let mut through_farcli = Replay::start(&recording, recording.profile());
    let mut alone = Replay::without_farcli(&recording);
    // What can read otherwise: rows of code with nothing on them that tells
    // code, while the rows of their block that do tell are above the top of
    // the screen and have never been in sight. In this recording that
    // happens to the code under the name of a language Claude Code does not
    // know. The first time the user scrolls up to it, its lines come into
    // sight before the name above them does, and until then they look like
    // lines of prose and are read so.
    let name_of_the_language = "  foolang".to_string();
    let written = ["اجرا کن برنامه", "تمام شد"];
    let under_the_name = written.map(|line| format!("  {}", as_drawn(line)));
    let mut moments = 0;
    let mut corrected = 0;
    let mut read_as_prose_under_a_name_out_of_sight = 0;
    let mut name_was_in_sight = false;
    for (index, (_, event)) in recording.events.iter().enumerate() {
        if matches!(event, Event::UserTyped(_)) {
            moments += 1;
            let drawn = alone.terminal.screen();
            let shown = through_farcli.terminal.screen();
            name_was_in_sight |= drawn
                .iter()
                .any(|row| row_text(row) == name_of_the_language);
            for (row, (drawn, shown)) in drawn.iter().zip(&shown).enumerate() {
                let what = format!("before event {index}, row {row}");
                let drawn_text = row_text(drawn);
                match reads_as.get(&drawn_text) {
                    Some(_) if under_the_name.contains(&drawn_text) && !name_was_in_sight => {
                        let line = under_the_name.iter().position(|line| *line == drawn_text);
                        assert_eq!(
                            row_text(shown),
                            format!("  {}", laid_out(written[line.unwrap()], RightToLeft, WIDTH)),
                            "{what}"
                        );
                        read_as_prose_under_a_name_out_of_sight += 1;
                    }
                    Some(wanted) => {
                        assert_eq!(&row_text(shown), wanted, "{what}");
                        corrected += usize::from(wanted != &drawn_text);
                    }
                    // The header row, a row with the button for jumping to
                    // the bottom over it, the Input box and the status line.
                    None => assert!(shown == drawn, "{what}: {drawn_text:?}"),
                }
            }
        }
        through_farcli.step();
        alone.step();
    }
    assert!(moments > 80, "{moments}");
    assert!(corrected > 500, "{corrected}");
    // Once the name has been in sight the lines under it stay as drawn,
    // wherever the user scrolls to.
    assert!(name_was_in_sight);
    assert!(
        (1..=4).contains(&read_as_prose_under_a_name_out_of_sight),
        "{read_as_prose_under_a_name_out_of_sight}"
    );
}

#[test]
fn code_with_no_word_that_claude_code_colours_is_read_as_lines_of_prose() {
    // The limit of telling code by its colours. In a block of a language
    // Claude Code knows, a line with no keyword, number, string, comment or
    // built-in name is drawn in the Terminal's own colours, and so is every
    // line of a block named `text`. When no line of a block has a coloured
    // word, nothing tells the block from lines of prose, and each line is
    // read as a paragraph of its own. Code holds its Farsi in strings and
    // comments, which are coloured, so this takes a block like these two:
    // names in Farsi and nothing else, or plain text.
    let reply = Reply::of_the_classic_display();

    // ```python
    reply.reads_as_written(&["نتیجه = محاسبه(ورودی)", "ذخیره(نتیجه)"]);
    // ```text
    reply.reads_as_written(&["خط اول متن", "خط دوم متن"]);
}
