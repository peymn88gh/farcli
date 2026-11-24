//! The rules Layout adds to the Unicode reordering rules, because a Farsi
//! reader does not want what those rules alone give:
//!
//! - numbers joined by a mark keep the order they were written in;
//! - a word with no Farsi in it, such as a path or a dotted name, stays in
//!   one piece;
//! - a number written in Farsi digits with no letter beside it, such as the
//!   label «۳.», sits on the right.
//!
//! Every test goes through the session: it feeds the bytes a program wrote
//! and checks what a Terminal shows. The joined letters are written as text
//! that reads from the right, and `from_the_right` turns that into Screen
//! order.

use farcli::app_profile::AppProfile;
use farcli::fake_terminal::FakeTerminal;
use farcli::session::Session;
use farcli::Size;

/// The first row of a Terminal `cols` wide after a program farcli knows
/// nothing about wrote `line`.
fn shown(cols: u16, line: &str) -> String {
    let mut terminal = FakeTerminal::new(Size { cols, rows: 3 });
    let mut session = Session::start(AppProfile::plain(), terminal.size(), terminal.cursor());
    let reaction = session.app_wrote(format!("{line}\r\n").as_bytes());
    terminal.receive(&reaction.for_terminal);
    terminal.screen_text().remove(0)
}

/// Text that reads from the right, as it sits on a screen from the left.
fn from_the_right(text: &str) -> String {
    text.chars().rev().collect()
}

/// A row `cols` wide with `text` against its right edge.
fn at_right_edge(cols: usize, text: &str) -> String {
    format!("{text:>cols$}")
}

/// What a Terminal `cols` wide shows for a Farsi sentence made of the
/// pieces of `sentence` in the order they are read: each Farsi piece is
/// given joined, and every other piece reads left to right where the
/// sentence reaches it.
fn farsi_sentence(cols: usize, sentence: &[&str]) -> String {
    let on_screen: String = sentence
        .iter()
        .rev()
        .map(|piece| match piece.chars().any(|c| c > '\u{2000}') {
            true => from_the_right(piece),
            false => piece.to_string(),
        })
        .collect();
    at_right_edge(cols, &on_screen)
}

#[test]
fn a_size_in_a_farsi_sentence_keeps_its_numbers_in_the_order_they_were_written() {
    // By the Unicode rules alone the letter x takes the second number with
    // it and the first is left behind: `x 20 33`.
    let row = shown(40, "اندازه 33 x 20 است");

    assert_eq!(row, farsi_sentence(40, &["ﺍﻧﺪﺍﺯﻩ ", "33 x 20", " ﺍﺳﺖ"]));
}

#[test]
fn a_size_keeps_its_order_whatever_mark_stands_between_its_numbers() {
    for size in [
        "33 × 20",
        "33 * 20",
        "33 X 20",
        "33x20",
        "128 x 32 x 3",
        "۳۳ x ۲۰",
    ] {
        let row = shown(40, &format!("اندازه {size} است"));

        assert_eq!(row, farsi_sentence(40, &["ﺍﻧﺪﺍﺯﻩ ", size, " ﺍﺳﺖ"]));
    }
}

#[test]
fn a_size_in_brackets_keeps_its_order_and_its_brackets() {
    let row = shown(40, "اندازه (33 x 20) است");

    assert_eq!(row, farsi_sentence(40, &["ﺍﻧﺪﺍﺯﻩ ", "(33 x 20)", " ﺍﺳﺖ"]));
}

#[test]
fn a_date_written_with_dashes_keeps_its_order() {
    // By the Unicode rules alone a dash between numbers that follow Farsi
    // letters does not hold them together: `23-11-2026`.
    let row = shown(40, "تاریخ 2026-11-23 است");

    assert_eq!(row, farsi_sentence(40, &["ﺗﺎﺭﯾﺦ ", "2026-11-23", " ﺍﺳﺖ"]));
}

#[test]
fn numbers_with_only_spaces_or_farsi_between_them_are_read_from_the_right_as_before() {
    // «از 10 تا 20»: the reader meets 10 first, on the right.
    let row = shown(40, "از 10 تا 20 و 30 40");

    assert_eq!(
        row,
        farsi_sentence(40, &["ﺍﺯ ", "10", " ﺗﺎ ", "20", " ﻭ ", "30", " ", "40"])
    );
}

#[test]
fn a_size_in_an_english_line_that_holds_farsi_keeps_its_order() {
    // By the Unicode rules alone the first number goes with the Farsi word
    // before it, to that word's left: `33 مالس x 20`.
    let row = shown(40, "Size سلام 33 x 20");

    assert_eq!(row, ["Size ", &from_the_right("ﺳﻠﺎﻡ"), " 33 x 20"].concat());
}

#[test]
fn a_path_in_a_farsi_sentence_stays_in_one_piece() {
    // By the Unicode rules alone the marks a path begins with go to its
    // other end: `src/main.rs/.`.
    let row = shown(40, "فایل ./src/main.rs را ببین");

    assert_eq!(
        row,
        farsi_sentence(40, &["ﻓﺎﯾﻞ ", "./src/main.rs", " ﺭﺍ ﺑﺒﯿﻦ"])
    );
}

#[test]
fn a_name_that_begins_or_ends_with_a_mark_stays_in_one_piece() {
    for name in [
        ".gitignore",
        "--help",
        "~/notes",
        "src/",
        "$HOME",
        "%TEMP%",
        "@types/node",
        "C++",
        "main()",
        "items[0]",
        "*.rs",
        "_private",
        "./2026/notes.md",
    ] {
        let row = shown(40, &format!("فایل {name} را ببین"));

        assert_eq!(row, farsi_sentence(40, &["ﻓﺎﯾﻞ ", name, " ﺭﺍ ﺑﺒﯿﻦ"]));
    }
}

#[test]
fn the_mark_that_ends_a_sentence_after_an_english_word_still_ends_the_sentence() {
    // The full stop belongs to the sentence, not to the name: it is the
    // last thing read, at the left.
    let row = shown(40, "فایل .gitignore.");

    assert_eq!(row, farsi_sentence(40, &["ﻓﺎﯾﻞ ", ".gitignore", "."]));

    // So does a comma, and a bracket that closes round the name.
    let row = shown(40, "فایل (main.rs)، را ببین");

    assert_eq!(
        row,
        farsi_sentence(40, &["ﻓﺎﯾﻞ ", "(main.rs)", "،", " ﺭﺍ ﺑﺒﯿﻦ"])
    );
}

#[test]
fn a_number_label_in_farsi_digits_sits_on_the_right() {
    // Read from the right: the number, then its full stop.
    assert_eq!(shown(20, "۳."), at_right_edge(20, ".۳"));
    assert_eq!(shown(20, "۱۲)"), at_right_edge(20, "(۱۲"));
}

#[test]
fn a_number_in_english_digits_with_no_letter_beside_it_stays_on_the_left() {
    assert_eq!(shown(20, "3.").trim_end(), "3.");
    assert_eq!(shown(20, "2026-11-23").trim_end(), "2026-11-23");
}
