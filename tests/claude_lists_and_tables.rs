//! Lists and tables in Claude Code's Agent output. In a Farsi list the
//! bullet or number is on the right of each item, items inside an item
//! step in from the right, and a wrapped item carries on under its own
//! text. In a table the text of each cell is joined and in order, the
//! columns stand in the order of the table's borders, and the bars of each
//! row stand under those borders. An English list or table is not touched.
//!
//! Claude Code on Windows draws a list marker at the left of its item and
//! reorders the item's text after it, row by row. It reorders a table row
//! as one piece of text, bars included, so a row that begins with a Farsi
//! cell is drawn with its columns the other way round.
//!
//! The Terminal in these tests is 30 columns wide: a reply's text is drawn
//! in the 28 columns from the third to the last.

mod claude_code;

use claude_code::{as_drawn, at_right_edge, from_the_right, placed, Run};

/// A row of a reply whose text, given as it sits on screen, ends at the
/// right edge of the reply's area. Empty cells at the end of `on_screen`
/// are the columns between the text and that edge.
fn from_right_edge(on_screen: &str) -> String {
    format!("  {}", at_right_edge(28, on_screen))
        .trim_end()
        .to_string()
}

/// A list item as Claude Code draws it: `indent` empty columns, the
/// marker, and the item's text in the app's own Screen order.
fn item(indent: usize, marker: &str, text: &str) -> String {
    format!("{}{marker} {}", " ".repeat(indent), as_drawn(text))
}

/// A row an item's text wraps onto, drawn under the text of its first row.
fn carried_on(indent: usize, text: &str) -> String {
    format!("{}{}", " ".repeat(indent), as_drawn(text))
}

/// A row of a table: the app reorders the whole row, bars and all.
fn table_row(written: &str) -> String {
    as_drawn(written)
}

/// The bytes Claude Code writes for a reply that says «سلام» and then
/// holds `block`, whose rows are given as the app draws them.
fn reply_with(block: &[String]) -> String {
    let mut bytes = format!("●{}\r\n\r\n", placed(2, &as_drawn("سلام")));
    for row in block {
        bytes.push_str(&placed(2, row));
        bytes.push_str("\r\n");
    }
    bytes
}

#[test]
fn in_a_farsi_list_the_bullet_is_on_the_right_of_each_item() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[
        item(0, "-", "مورد اول"),
        item(0, "-", "مورد دوم"),
    ]));

    // Claude Code keeps the marker at the left and draws the text after
    // it from the right, unjoined.
    assert_eq!(run.drawn_row(2), "  - لوا دروم");
    // Read from the right edge: the bullet, then the item's text.
    assert_eq!(
        run.row(2),
        from_right_edge(&[from_the_right("ﻣﻮﺭﺩ ﺍﻭﻝ").as_str(), " -"].concat())
    );
    assert_eq!(
        run.row(3),
        from_right_edge(&[from_the_right("ﻣﻮﺭﺩ ﺩﻭﻡ").as_str(), " -"].concat())
    );
}

#[test]
fn in_a_farsi_list_the_number_is_on_the_right_of_each_item() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[
        item(0, "1.", "قدم اول"),
        item(0, "2.", "قدم دوم"),
    ]));

    assert_eq!(run.drawn_row(2), "  1. لوا مدق");
    // Read from the right edge: the number, its full stop, the text.
    assert_eq!(
        run.row(2),
        from_right_edge(&[from_the_right("ﻗﺪﻡ ﺍﻭﻝ").as_str(), " .1"].concat())
    );
    assert_eq!(
        run.row(3),
        from_right_edge(&[from_the_right("ﻗﺪﻡ ﺩﻭﻡ").as_str(), " .2"].concat())
    );
}

#[test]
fn items_inside_a_farsi_item_step_in_from_the_right() {
    let mut run = Run::start(30, 10);

    // Claude Code draws an item inside an item under the text of the item
    // it is in: two columns in under a bullet, three under a number.
    run.app_writes(reply_with(&[
        item(0, "-", "مورد دوم"),
        item(2, "-", "زیرمورد الف"),
        item(0, "2.", "قدم دوم"),
        item(3, "-", "زیرمورد"),
    ]));

    assert_eq!(run.drawn_row(3), "    - فلا درومریز");
    assert_eq!(
        run.row(3),
        from_right_edge(&[from_the_right("ﺯﯾﺮﻣﻮﺭﺩ ﺍﻟﻒ").as_str(), " -  "].concat())
    );
    assert_eq!(
        run.row(5),
        from_right_edge(&[from_the_right("ﺯﯾﺮﻣﻮﺭﺩ").as_str(), " -   "].concat())
    );
    // The items they are in start at the right edge.
    assert_eq!(
        run.row(2),
        from_right_edge(&[from_the_right("ﻣﻮﺭﺩ ﺩﻭﻡ").as_str(), " -"].concat())
    );
    assert_eq!(
        run.row(4),
        from_right_edge(&[from_the_right("ﻗﺪﻡ ﺩﻭﻡ").as_str(), " .2"].concat())
    );
}

#[test]
fn a_farsi_item_that_wraps_carries_on_under_its_own_text_left_of_the_bullet() {
    let mut run = Run::start(30, 8);

    // The item's text is drawn in the 26 columns after the marker. The
    // wrap left its space on the end of the first row, which the app draws
    // at the left of that row's text.
    run.app_writes(reply_with(&[
        item(0, "-", "یک مورد بلند که به خط دوم "),
        carried_on(2, "می\u{200c}رود و تمام"),
        item(0, "-", "مورد دوم"),
    ]));

    assert_eq!(run.drawn_row(2), "  -  مود طخ هب هک دنلب دروم کی");
    assert_eq!(
        run.row(2),
        from_right_edge(&[" ", &from_the_right("ﯾﮏ ﻣﻮﺭﺩ ﺑﻠﻨﺪ ﮐﻪ ﺑﻪ ﺧﻂ ﺩﻭﻡ"), " -"].concat())
    );
    // The second row ends two columns short of the right edge, under the
    // first row's text and not under its bullet.
    assert_eq!(
        run.row(3),
        from_right_edge(&[from_the_right("ﻣﯽﺭﻭﺩ ﻭ ﺗﻤﺎﻡ").as_str(), "  "].concat())
    );
    assert_eq!(
        run.row(4),
        from_right_edge(&[from_the_right("ﻣﻮﺭﺩ ﺩﻭﻡ").as_str(), " -"].concat())
    );
}

#[test]
fn a_wrapped_row_of_a_farsi_item_that_begins_with_an_english_word_keeps_the_items_direction() {
    let mut run = Run::start(30, 8);

    // The second row begins with an English word, so the app drew it left
    // to right: the full stop at its right end tells that it does not end
    // with that word.
    run.app_writes(reply_with(&[
        item(0, "1.", "برای دیدن تغییرها دستور "),
        carried_on(3, "status را ببین."),
    ]));

    assert_eq!(run.drawn_row(3), "     status نیبب ار.");
    let second = [".", &from_the_right("ﺭﺍ ﺑﺒﯿﻦ"), " status   "].concat();
    assert_eq!(run.row(3), from_right_edge(&second));
}

#[test]
fn an_english_list_is_not_touched() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[
        item(0, "-", "leaves"),
        item(2, "-", "a long item that goes "),
        carried_on(4, "to a second row"),
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
        item(0, "-", "config.json را ببین."),
        item(0, "-", "مورد دوم"),
    ]));

    // Each item is a paragraph, and takes its direction from its own
    // first letter.
    assert_eq!(
        run.row(2),
        ["  - config.json ", &from_the_right("ﺭﺍ ﺑﺒﯿﻦ"), "."].concat()
    );
    assert_eq!(
        run.row(3),
        from_right_edge(&[from_the_right("ﻣﻮﺭﺩ ﺩﻭﻡ").as_str(), " -"].concat())
    );
}

#[test]
fn an_item_with_no_letter_in_it_goes_the_way_of_the_item_before_it() {
    let mut run = Run::start(30, 8);

    run.app_writes(reply_with(&[
        item(0, "1.", "قدم اول"),
        item(0, "2.", "2026"),
    ]));

    assert_eq!(run.drawn_row(3), "  2. 2026");
    assert_eq!(run.row(3), from_right_edge("2026 .2"));
}

/// A table of two columns as Claude Code draws it, given as it was
/// written: the header's cells centred, the other cells one column in
/// from the bar on their left.
fn table() -> Vec<String> {
    vec![
        "┌───────┬─────────┐".to_string(),
        table_row("│  نام  │ اندازه  │"),
        "├───────┼─────────┤".to_string(),
        table_row("│ لوگو  │ 33 x 20 │"),
        "├───────┼─────────┤".to_string(),
        table_row("│ آیکون │ 10 x 20 │"),
        "└───────┴─────────┘".to_string(),
    ]
}

#[test]
fn the_text_of_each_table_cell_is_joined_and_in_order_under_the_tables_borders() {
    let mut run = Run::start(30, 12);

    run.app_writes(reply_with(&table()));

    // Claude Code drew each row from the right, so its first column is
    // under the last column of the borders, the bar between the two is
    // not under the joint above it, and the size reads `x 20 33`.
    assert_eq!(run.drawn_row(2), "  ┌───────┬─────────┐");
    assert_eq!(run.drawn_row(3), "  │  هزادنا │  مان  │");
    assert_eq!(run.drawn_row(5), "  │ x 20 33 │  وگول │");

    // The columns stand in the order of the borders, the first at the
    // left, and each bar is under its joint. Each Farsi cell is joined and
    // read from the right inside its column.
    assert_eq!(
        run.row(3),
        format!(
            "  │  {}  │  {} │",
            from_the_right("ﻧﺎﻡ"),
            from_the_right("ﺍﻧﺪﺍﺯﻩ")
        )
    );
    assert_eq!(
        run.row(5),
        format!("  │  {} │ 33 x 20 │", from_the_right("ﻟﻮﮔﻮ"))
    );
    assert_eq!(
        run.row(7),
        format!("  │ {} │ 10 x 20 │", from_the_right("ﺁﯾﮑﻮﻥ"))
    );
    // The borders are untouched.
    for border in [2, 4, 6, 8] {
        assert!(run.row_is_as_drawn(border), "row {border}");
    }
}

#[test]
fn a_table_row_that_begins_with_an_english_cell_keeps_its_columns_and_has_its_farsi_joined() {
    let mut run = Run::start(30, 12);

    // The row begins with an English letter, so the app drew it left to
    // right: its columns are in place, and only the Farsi word is turned.
    run.app_writes(reply_with(&[
        "┌──────┬───────┐".to_string(),
        table_row("│ logo │ لوگو  │"),
        "└──────┴───────┘".to_string(),
    ]));

    assert_eq!(run.drawn_row(3), "  │ logo │ وگول  │");
    // Joined, and read from the right inside its cell like every Farsi
    // cell.
    assert_eq!(
        run.row(3),
        format!("  │ logo │  {} │", from_the_right("ﻟﻮﮔﻮ"))
    );
}

#[test]
fn an_english_table_is_not_touched() {
    let mut run = Run::start(30, 12);

    run.app_writes(reply_with(&[
        "┌──────┬─────────┐".to_string(),
        "│ Name │  Size   │".to_string(),
        "├──────┼─────────┤".to_string(),
        "│ logo │ 33 x 20 │".to_string(),
        "└──────┴─────────┘".to_string(),
    ]));

    for row in 2..7 {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
}

#[test]
fn a_table_row_whose_bars_cannot_be_put_under_the_borders_stays_as_drawn() {
    let mut run = Run::start(30, 12);

    // A row with a cell too many for the borders around it: nothing farcli
    // knows of draws this, so it does not guess.
    run.app_writes(reply_with(&[
        "┌───────┬─────────┐".to_string(),
        table_row("│ لوگو  │ 33 │ 20  │"),
        "└───────┴─────────┘".to_string(),
    ]));

    assert!(run.row_is_as_drawn(3), "{}", run.row(3));
}
