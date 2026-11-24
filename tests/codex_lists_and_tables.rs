//! Lists and tables in Codex's Agent output. In a Farsi list the bullet or
//! number is on the right of each item, items inside an item step in from
//! the right, and a wrapped item carries on under its own text. In a table
//! the text of each cell is joined and in order, while the columns and the
//! rules stay where Codex drew them. An English list or table is not
//! touched.
//!
//! Codex draws Logical order. The Terminal in these tests is 30 columns
//! wide: a reply's text is drawn in the 28 columns from the third to the
//! last.

mod codex;

use codex::{
    at_right_edge, bold, coloured, dim, from_the_right, input_box, on_row, reply_row, reply_start,
    screen, sent_start, status_row, Run, HEADING, MARKER,
};
use farcli::screen::Color;

/// A row of a reply whose text, given as it sits on screen, ends at the
/// right edge of the reply's area. Empty cells at the end of `on_screen`
/// are the columns between the text and that edge.
fn from_right_edge(on_screen: &str) -> String {
    format!("  {}", at_right_edge(28, on_screen))
        .trim_end()
        .to_string()
}

/// The text of a row with the empty cells at its right end left off.
fn row(run: &Run, row: usize) -> String {
    run.row(row).trim_end().to_string()
}

/// A list item as Codex draws it: `indent` empty columns, the marker in
/// the theme's colour, and the text.
fn item(indent: usize, marker: &str, text: &str) -> String {
    let marker = format!("{}{marker} ", " ".repeat(indent));
    reply_row(&format!("{}{text}", coloured(&marker, MARKER)))
}

/// A row an item's text wraps onto: under the text of its first row.
fn carried_on(indent: usize, text: &str) -> String {
    reply_row(&format!("{}{text}", " ".repeat(indent)))
}

/// A reply that says «سلام» and then holds `block`.
fn reply_with(block: &[String]) -> String {
    let mut rows = vec![reply_start("سلام"), String::new()];
    rows.extend(block.iter().cloned());
    screen(&rows)
}

#[test]
fn in_a_farsi_list_the_bullet_is_on_the_right_of_each_item() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[item(0, "•", "برگ"), item(0, "•", "باران")]));

    assert_eq!(run.drawn_row(2), "  • برگ");
    // Read from the right edge: the bullet, then the item's text.
    assert_eq!(
        run.row(2),
        from_right_edge(&[from_the_right("ﺑﺮﮒ").as_str(), " •"].concat())
    );
    assert_eq!(
        run.row(3),
        from_right_edge(&[from_the_right("ﺑﺎﺭﺍﻥ").as_str(), " •"].concat())
    );
}

#[test]
fn in_a_farsi_list_the_number_is_on_the_right_of_each_item() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[
        item(0, "1.", "قدم زدن"),
        item(0, "2.", "چای نوشیدن"),
    ]));

    // Read from the right edge: the number, its full stop, the text.
    assert_eq!(
        run.row(2),
        from_right_edge(&[from_the_right("ﻗﺪﻡ ﺯﺩﻥ").as_str(), " .1"].concat())
    );
    assert_eq!(
        run.row(3),
        from_right_edge(&[from_the_right("ﭼﺎﯼ ﻧﻮﺷﯿﺪﻥ").as_str(), " .2"].concat())
    );
}

#[test]
fn items_inside_a_farsi_item_step_in_from_the_right() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[
        item(0, "•", "باران"),
        item(4, "•", "نم\u{200c}نم"),
        item(4, "•", "رگبار"),
    ]));

    assert_eq!(run.drawn_row(3), "      • نم\u{200c}نم");
    assert_eq!(
        run.row(2),
        from_right_edge(&[from_the_right("ﺑﺎﺭﺍﻥ").as_str(), " •"].concat())
    );
    // Codex draws an item inside an item four columns further in. Here
    // those four columns are on the right.
    assert_eq!(
        run.row(3),
        from_right_edge(&[from_the_right("ﻧﻢﻧﻢ").as_str(), " •    "].concat())
    );
    assert_eq!(
        run.row(4),
        from_right_edge(&[from_the_right("ﺭﮔﺒﺎﺭ").as_str(), " •    "].concat())
    );
}

#[test]
fn a_farsi_item_that_wraps_carries_on_under_its_own_text_left_of_the_bullet() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[
        item(0, "•", "یک مورد بلند که به خط"),
        carried_on(2, "دوم می\u{200c}رود"),
        item(0, "•", "باد"),
    ]));

    assert_eq!(
        run.row(2),
        from_right_edge(&[from_the_right("ﯾﮏ ﻣﻮﺭﺩ ﺑﻠﻨﺪ ﮐﻪ ﺑﻪ ﺧﻂ").as_str(), " •"].concat())
    );
    // The second row ends two columns short of the right edge, under the
    // first row's text and not under its bullet.
    assert_eq!(
        run.row(3),
        from_right_edge(&[from_the_right("ﺩﻭﻡ ﻣﯽﺭﻭﺩ").as_str(), "  "].concat())
    );
    assert_eq!(
        run.row(4),
        from_right_edge(&[from_the_right("ﺑﺎﺩ").as_str(), " •"].concat())
    );
}

#[test]
fn a_wrapped_row_of_a_farsi_item_that_begins_with_an_english_word_keeps_the_items_direction() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[
        item(0, "1.", "برای دیدن تغییرها دستور"),
        carried_on(3, "status را بزن"),
    ]));

    let second = [from_the_right("ﺭﺍ ﺑﺰﻥ").as_str(), " status   "].concat();
    assert_eq!(run.row(3), from_right_edge(&second));
}

#[test]
fn the_marker_keeps_its_colour_where_it_is_shown() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[item(0, "•", "برگ")]));

    let shown = &run.terminal.screen()[2];
    assert_eq!(shown[29].character, '•');
    assert_eq!(shown[29].style.foreground, Color::Rgb(148, 226, 213));
    assert_eq!(shown[27].style.foreground, Color::Default);
}

#[test]
fn an_english_list_is_not_touched() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[
        item(0, "•", "leaves"),
        item(4, "•", "a long item that goes on"),
        carried_on(6, "to a second row"),
        item(0, "1.", "walk"),
    ]));

    for row in 2..6 {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
}

#[test]
fn an_item_that_begins_with_an_english_word_stays_on_the_left_with_its_farsi_joined() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[
        item(0, "•", "config.json را ببین"),
        item(0, "•", "برگ"),
    ]));

    // Each item is a paragraph, and takes its direction from its own
    // first letter.
    assert_eq!(
        row(&run, 2),
        ["  • config.json ", &from_the_right("ﺭﺍ ﺑﺒﯿﻦ")].concat()
    );
    assert_eq!(
        run.row(3),
        from_right_edge(&[from_the_right("ﺑﺮﮒ").as_str(), " •"].concat())
    );
}

#[test]
fn an_item_with_no_letter_in_it_goes_the_way_of_the_item_before_it() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[
        item(0, "1.", "برگ"),
        item(0, "2.", "2026"),
        item(0, "3.", "33 x 20"),
    ]));

    assert_eq!(run.row(3), from_right_edge("2026 .2"));
    // An item that holds an English letter and no Farsi is an English
    // item.
    assert!(run.row_is_as_drawn(4));
}

/// A table of three columns as Codex draws it: a bold, coloured header, a
/// heavy rule under it, a light rule between the rows, and no upright
/// bars. The text of each cell starts one column in from its rule.
fn table() -> Vec<String> {
    vec![
        reply_row(&bold(&coloured(" نام    اندازه     توضیح", HEADING))),
        reply_row(&dim("━━━━━  ━━━━━━━━━  ━━━━━━━")),
        reply_row(" برگ    33 x 20    زرد"),
        reply_row(&dim("─────  ─────────  ───────")),
        reply_row(" قاب    40 x 30    چوبی"),
    ]
}

/// A row of that table as it should read, given the cells of its three
/// columns as they sit on screen.
fn cells(first: &str, second: &str, third: &str) -> String {
    format!("  {first}  {second}  {third}")
        .trim_end()
        .to_string()
}

#[test]
fn the_text_of_each_table_cell_is_joined_and_in_order_inside_its_own_column() {
    let mut run = Run::start(30, 10);

    run.app_writes(reply_with(&table()));

    assert_eq!(run.drawn_row(4), "   برگ    33 x 20    زرد");
    // The columns are where Codex drew them, the first at the left. Each
    // Farsi cell is joined and read from the right inside its column, and
    // ends one column in from the right end of its rule.
    assert_eq!(
        run.row(2),
        cells(
            &format!(" {} ", from_the_right("ﻧﺎﻡ")),
            &format!("  {} ", from_the_right("ﺍﻧﺪﺍﺯﻩ")),
            &format!(" {} ", from_the_right("ﺗﻮﺿﯿﺢ")),
        )
    );
    assert_eq!(
        run.row(4),
        cells(
            &format!(" {} ", from_the_right("ﺑﺮﮒ")),
            " 33 x 20 ",
            &format!("   {} ", from_the_right("ﺯﺭﺩ")),
        )
    );
    assert_eq!(
        run.row(6),
        cells(
            &format!(" {} ", from_the_right("ﻗﺎﺏ")),
            " 40 x 30 ",
            &format!("  {} ", from_the_right("ﭼﻮﺑﯽ")),
        )
    );
    // The rules are untouched.
    assert!(run.row_is_as_drawn(3));
    assert!(run.row_is_as_drawn(5));
}

#[test]
fn the_header_of_a_table_keeps_its_style() {
    let mut run = Run::start(30, 10);

    run.app_writes(reply_with(&table()));

    let header = &run.terminal.screen()[2];
    let letters: Vec<_> = header.iter().filter(|c| c.character != ' ').collect();
    assert_eq!(letters.len(), 14);
    assert!(letters.iter().all(|cell| cell.style.bold));
    assert!(letters
        .iter()
        .all(|cell| cell.style.foreground == Color::Rgb(249, 226, 175)));
}

#[test]
fn a_size_inside_a_farsi_cell_keeps_the_order_of_its_numbers() {
    let mut run = Run::start(30, 10);

    run.app_writes(reply_with(&[
        reply_row(&bold(&coloured(" نام    اندازه", HEADING))),
        reply_row(&dim("━━━━━  ━━━━━━━━━━━━━━━━")),
        reply_row(" برگ    اندازه 33 x 20"),
    ]));

    let second = [" 33 x 20 ", &from_the_right("ﺍﻧﺪﺍﺯﻩ"), " "].concat();
    assert_eq!(
        run.row(4),
        format!("   {}   {second}", from_the_right("ﺑﺮﮒ")).trim_end()
    );
}

#[test]
fn an_english_table_is_not_touched() {
    let mut run = Run::start(30, 10);

    run.app_writes(reply_with(&[
        reply_row(&bold(&coloured(" Name   Size", HEADING))),
        reply_row(&dim("━━━━━━  ━━━━━━━━━")),
        reply_row(" leaf   33 x 20"),
    ]));

    for row in 2..5 {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
}

#[test]
fn a_row_under_a_table_that_does_not_sit_in_its_columns_stays_as_drawn() {
    // While a reply arrives, Codex draws the first words of the next part
    // right under the table for a frame.
    let mut run = Run::start(30, 10);
    let mut rows = table();
    rows.push(reply_row("۵"));

    run.app_writes(reply_with(&rows));

    assert!(run.row_is_as_drawn(7), "{}", run.row(7));
    assert_ne!(run.row(6), run.drawn_row(6));
}
