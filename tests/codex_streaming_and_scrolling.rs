//! Agent output in Codex stays correct while it arrives in pieces, after it
//! has scrolled off the top of the inline display, and wherever the user
//! scrolls to in the default display.
//!
//! Codex writes only the cells that changed since its last frame, so a row
//! of a reply reaches farcli a few characters at a time, with jumps over
//! the cells in between. farcli works from the Hidden screen, where the
//! row is whole however it came.
//!
//! The Terminal in these tests is 30 columns wide: a reply's text is drawn
//! in the 28 columns from the third to the last.

mod codex;

use codex::{
    at_right_edge, coloured, from_the_right, input_box, on_row, reply_row, reply_start, screen,
    sent_row, sent_start, status_row, Run, COMMENT, WORD,
};

/// A row of a reply whose text, given as it sits on screen, ends at the
/// right edge of the reply's area.
fn from_right_edge(margin: &str, on_screen: &str) -> String {
    format!("{margin}{}", at_right_edge(28, on_screen))
}

/// A paragraph as Codex wraps it in 28 columns: the second row begins with
/// an English word, and read by itself would be laid out from the left.
const FIRST_ROW: &str = "این متن را بخوان و بعد دستور";
const SECOND_ROW: &str = "status را اجرا کن";

fn first_row_wanted(margin: &str) -> String {
    from_right_edge(margin, &from_the_right("ﺍﯾﻦ ﻣﺘﻦ ﺭﺍ ﺑﺨﻮﺍﻥ ﻭ ﺑﻌﺪ ﺩﺳﺘﻮﺭ"))
}

fn second_row_wanted() -> String {
    from_right_edge(
        "  ",
        &[from_the_right("ﺭﺍ ﺍﺟﺮﺍ ﮐﻦ").as_str(), " status"].concat(),
    )
}

#[test]
fn a_row_that_arrives_in_pieces_reads_as_if_it_had_been_written_whole() {
    let mut run = Run::start(30, 4);

    run.app_writes(on_row(0, &reply_start("فایل config")));
    assert_eq!(
        run.row(0),
        from_right_edge("• ", &["config ", from_the_right("ﻓﺎﯾﻞ").as_str()].concat())
    );

    // The rest of the file name, then a word two cells on: Codex jumps
    // over the space between them, which it emptied before.
    run.app_writes("\x1b[1;14H.json\x1b[1;20Hرا");
    run.app_writes("\x1b[1;23Hباز\x1b[1;27Hکن");

    assert_eq!(run.drawn_row(0), "• فایل config.json را باز کن");
    let whole = [
        from_the_right("ﺭﺍ ﺑﺎﺯ ﮐﻦ").as_str(),
        " config.json ",
        from_the_right("ﻓﺎﯾﻞ").as_str(),
    ]
    .concat();
    assert_eq!(run.row(0), from_right_edge("• ", &whole));
}

#[test]
fn a_paragraph_reads_correctly_while_it_grows_onto_a_second_row() {
    let mut run = Run::start(30, 6);

    run.app_writes(on_row(0, &reply_start("این متن را")));
    assert_eq!(
        run.row(0),
        from_right_edge("• ", &from_the_right("ﺍﯾﻦ ﻣﺘﻦ ﺭﺍ"))
    );

    // The first row is full and the next word is English.
    run.app_writes(on_row(0, &reply_start(FIRST_ROW)));
    run.app_writes(on_row(1, &reply_row("status")));
    assert_eq!(run.row(0), first_row_wanted("• "));
    assert_eq!(run.row(1), from_right_edge("  ", "status"));

    run.app_writes("\x1b[2;10Hرا\x1b[2;13Hاجرا\x1b[2;18Hکن");
    assert_eq!(run.row(1), second_row_wanted());
}

#[test]
fn the_part_being_written_reads_correctly_right_under_a_part_that_is_done() {
    let mut run = Run::start(30, 8);

    // While a reply arrives Codex draws the part still being written
    // right under the parts that are done, at times with no empty row
    // between: here the first words of a line of prose under a code block.
    run.app_writes(screen(&[
        reply_start("سلام دنیا"),
        String::new(),
        reply_row(&coloured("# پیام پاییزی", COMMENT)),
        reply_row(&coloured("print(1)", WORD)),
        reply_row("برگ افت"),
        String::new(),
        input_box(""),
        status_row(),
    ]));

    assert!(run.row_is_as_drawn(2));
    assert!(run.row_is_as_drawn(3));
    assert_eq!(
        run.row(4),
        from_right_edge("  ", &from_the_right("ﺑﺮﮒ ﺍﻓﺖ"))
    );
}

#[test]
fn the_part_being_written_reads_correctly_while_the_replys_mark_is_out_of_sight() {
    let mut run = Run::start(30, 8);

    // Codex takes a finished paragraph out of the part being written one
    // frame before it draws it among the parts that are done. For that
    // frame the second paragraph stands under the message sent, with the
    // reply's mark nowhere on the screen.
    run.app_writes(screen(&[
        sent_start("پیام کاربر"),
        sent_row(""),
        String::new(),
        reply_row("سلام دنیا"),
        String::new(),
        input_box(""),
        status_row(),
    ]));

    assert_eq!(
        run.row(3),
        from_right_edge("  ", &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ"))
    );
    assert!(run.row_is_as_drawn(0));
}

/// The bytes Codex writes in its inline display to put finished rows above
/// its Input box, which starts on the row `box_row` (counted from 0): the
/// rows above the box scroll up, and those pushed off the top are the
/// Terminal's to keep for scrolling back.
fn inserted_above(box_row: usize, rows: &[String]) -> String {
    let mut bytes = format!("\x1b[?2026h\x1b[1;{box_row}r\x1b[{box_row};1H");
    for row in rows {
        bytes.push_str(&format!("\r\n\x1b[0m\x1b[K{row}\x1b[0m"));
    }
    bytes.push_str("\x1b[r\x1b[?2026l");
    bytes
}

/// The rows found by scrolling back, without the empty rows that were above
/// the conversation when it began.
fn scrolled_off(run: &Run) -> Vec<String> {
    let rows = run.terminal.scrollback_text();
    rows.into_iter().skip_while(String::is_empty).collect()
}

#[test]
fn rows_that_scroll_off_the_top_of_the_inline_display_are_found_corrected_by_scrolling_back() {
    let mut run = Run::start(30, 6);
    run.app_writes(on_row(4, &input_box("")));
    run.app_writes(on_row(5, &status_row()));

    run.app_writes(inserted_above(
        4,
        &[
            reply_start(FIRST_ROW),
            reply_row(SECOND_ROW),
            String::new(),
            reply_row("سلام دنیا"),
        ],
    ));
    assert_eq!(run.row(0), first_row_wanted("• "));
    assert_eq!(run.row(1), second_row_wanted());

    // One more row is put above the box. The top of the screen now cuts
    // through the paragraph: its first row has scrolled off, and the
    // second, which begins with an English word, still reads as part of
    // it.
    run.app_writes(inserted_above(4, &[String::new()]));
    assert_eq!(scrolled_off(&run), [first_row_wanted("• ")]);
    assert_eq!(run.row(0), second_row_wanted());

    run.app_writes(inserted_above(4, &[reply_row("خط دوم")]));
    assert_eq!(
        scrolled_off(&run),
        [first_row_wanted("• "), second_row_wanted()]
    );
    assert_eq!(
        run.row(1),
        from_right_edge("  ", &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ"))
    );
    assert_eq!(run.row(3), from_right_edge("  ", &from_the_right("ﺧﻂ ﺩﻭﻡ")));
    // The Input box and the row under it never moved.
    assert!(run.row_is_as_drawn(4));
    assert!(run.row_is_as_drawn(5));
}

/// The row Codex's default display lays over the top row once the message
/// sent has scrolled out of sight: its start, cut short.
fn start_of_the_message() -> String {
    sent_start("پیام کا…")
}

#[test]
fn in_the_default_display_a_paragraph_reads_the_same_with_its_first_row_out_of_sight() {
    let mut run = Run::start(30, 6);
    run.app_writes("\x1b[?1049h");

    // The whole paragraph is in sight while it arrives.
    run.app_writes(screen(&[
        sent_start("پیام کاربر"),
        String::new(),
        reply_start(FIRST_ROW),
        reply_row(SECOND_ROW),
        input_box(""),
        status_row(),
    ]));
    assert_eq!(run.row(3), second_row_wanted());

    // More of the reply arrives and the conversation moves up under the
    // start of the message, which stays on the top row. The paragraph's
    // second row is right under it.
    run.app_writes(screen(&[
        start_of_the_message(),
        reply_row(SECOND_ROW),
        String::new(),
        reply_row("سلام دنیا"),
        input_box(""),
        status_row(),
    ]));

    assert!(run.row_is_as_drawn(0));
    assert_eq!(run.row(1), second_row_wanted());
    assert_eq!(
        run.row(3),
        from_right_edge("  ", &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ"))
    );
}

#[test]
fn in_the_default_display_the_end_of_a_paragraph_never_seen_whole_is_read_by_its_farsi_rows() {
    let mut run = Run::start(30, 6);
    run.app_writes("\x1b[?1049h");

    // A session started again, scrolled to a place where the top row
    // covers the first row of a paragraph. Of the two rows in sight, the
    // first fills its 28 cells and begins with an English word, and the
    // one it wraps onto begins with a Farsi word: a Farsi paragraph far
    // more often than an English one.
    run.app_writes(screen(&[
        start_of_the_message(),
        reply_row("status را اجرا کن و بعد فایل"),
        reply_row("را ببین"),
        String::new(),
        input_box(""),
        status_row(),
    ]));

    let filled = [from_the_right("ﺭﺍ ﺍﺟﺮﺍ ﮐﻦ ﻭ ﺑﻌﺪ ﻓﺎﯾﻞ").as_str(), " status"].concat();
    assert_eq!(run.row(1), format!("  {filled}"));
    assert_eq!(run.row(1).chars().count(), 30);
    assert_eq!(
        run.row(2),
        from_right_edge("  ", &from_the_right("ﺭﺍ ﺑﺒﯿﻦ"))
    );
}
