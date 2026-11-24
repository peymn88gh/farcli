//! Agent output in Codex: the paragraphs of a reply are joined, laid out in
//! the Paragraph direction and right-aligned in the area Codex drew them
//! in. Codex draws Logical order and joins nothing, so there is nothing to
//! undo. Everything that is not a paragraph of a reply stays as drawn.
//!
//! The Terminal in these tests is 30 columns wide: a reply's text is drawn
//! in the 28 columns from the third to the last.

mod codex;

use codex::{
    at_right_edge, bold, coloured, dim, from_the_right, input_box, italic, on_row, reply_row,
    reply_start, screen, sent_row, sent_start, status_row, Run, COMMENT, HEADING, MARKER, STRING,
    WORD,
};
use farcli::screen::Color;

/// A row of a reply whose text, given as it sits on screen, ends at the
/// right edge of the reply's area.
fn from_right_edge(margin: &str, on_screen: &str) -> String {
    format!("{margin}{}", at_right_edge(28, on_screen))
}

fn rows(rows: &[&str]) -> Vec<String> {
    rows.iter().map(|row| row.to_string()).collect()
}

#[test]
fn a_farsi_paragraph_is_joined_and_laid_out_from_the_right_edge_of_the_replys_area() {
    let mut run = Run::start(30, 4);

    run.app_writes(on_row(0, &reply_start("سلام دنیا")));

    // The reply mark stays in the first column. The text is drawn in the
    // columns from the third to the last, and starts at the last.
    assert_eq!(run.drawn_row(0), "• سلام دنیا");
    assert_eq!(
        run.row(0),
        from_right_edge("• ", &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ"))
    );
}

#[test]
fn every_row_of_a_wrapped_paragraph_keeps_the_paragraphs_direction() {
    let mut run = Run::start(30, 4);

    // Codex wrapped the paragraph before `status`, which did not fit after
    // the 28 cells of the first row. Read by itself, the second row begins
    // with an English word.
    run.app_writes(screen(&[
        reply_start("این متن را بخوان و بعد دستور"),
        reply_row("status را اجرا کن"),
    ]));

    assert_eq!(
        run.row(0),
        from_right_edge("• ", &from_the_right("ﺍﯾﻦ ﻣﺘﻦ ﺭﺍ ﺑﺨﻮﺍﻥ ﻭ ﺑﻌﺪ ﺩﺳﺘﻮﺭ"))
    );
    let second = [from_the_right("ﺭﺍ ﺍﺟﺮﺍ ﮐﻦ").as_str(), " status"].concat();
    assert_eq!(run.row(1), from_right_edge("  ", &second));
}

#[test]
fn a_paragraph_that_begins_with_an_english_word_is_laid_out_from_the_left() {
    let mut run = Run::start(30, 4);

    run.app_writes(on_row(0, &reply_start("Codex یک ابزار است")));

    assert_eq!(
        run.row(0),
        format!("• Codex {}", from_the_right("ﯾﮏ ﺍﺑﺰﺍﺭ ﺍﺳﺖ"))
    );
}

#[test]
fn a_file_name_and_a_number_in_a_farsi_sentence_read_left_to_right_in_their_place() {
    let mut run = Run::start(30, 4);

    run.app_writes(screen(&[
        reply_start("فایل config.json را باز کن"),
        String::new(),
        reply_row("مقدار 42 است"),
    ]));

    let with_a_file_name = [
        from_the_right("ﺭﺍ ﺑﺎﺯ ﮐﻦ").as_str(),
        " config.json ",
        from_the_right("ﻓﺎﯾﻞ").as_str(),
    ]
    .concat();
    assert_eq!(run.row(0), from_right_edge("• ", &with_a_file_name));
    let with_a_number = [
        from_the_right("ﺍﺳﺖ").as_str(),
        " 42 ",
        from_the_right("ﻣﻘﺪﺍﺭ").as_str(),
    ]
    .concat();
    assert_eq!(run.row(2), from_right_edge("  ", &with_a_number));
}

#[test]
fn a_code_block_stays_as_drawn_and_the_paragraphs_around_it_are_laid_out() {
    let mut run = Run::start(30, 8);

    // Codex colours every word of a code block in a language it knows.
    run.app_writes(screen(&[
        reply_start("سلام دنیا"),
        String::new(),
        reply_row(&coloured("# پیام پاییزی", COMMENT)),
        reply_row(&[coloured("print(", WORD), coloured("\"سلام\"", STRING)].concat()),
        String::new(),
        reply_row("خط دوم"),
    ]));

    assert_eq!(run.drawn_row(2), "  # پیام پاییزی");
    assert!(run.row_is_as_drawn(2));
    assert!(run.row_is_as_drawn(3));
    assert_eq!(
        run.row(0),
        from_right_edge("• ", &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ"))
    );
    assert_eq!(run.row(5), from_right_edge("  ", &from_the_right("ﺧﻂ ﺩﻭﻡ")));
}

#[test]
fn code_with_no_colours_is_told_by_the_rows_that_start_further_in() {
    let mut run = Run::start(30, 8);

    // A block with no language named gets no colour from Codex. Its
    // indentation is its own.
    run.app_writes(screen(&[
        reply_start("سلام دنیا"),
        String::new(),
        reply_row("اگر x:"),
        reply_row("    print(\"سلام\")"),
        String::new(),
        input_box(""),
        status_row(),
    ]));

    assert!(run.row_is_as_drawn(2));
    assert!(run.row_is_as_drawn(3));
    assert_ne!(run.row(0), run.drawn_row(0));
}

#[test]
fn a_code_block_with_no_language_whose_lines_all_start_at_the_edge_reads_as_lines_of_prose() {
    // Known to be wrong, and kept: nothing on Codex's screen tells this
    // block from a poem. Each line is laid out as a paragraph of its own,
    // so a line that begins with a Farsi letter moves to the right edge.
    // A line that begins with an English letter keeps every English
    // character where it was.
    let mut run = Run::start(30, 8);

    run.app_writes(screen(&[
        reply_start("سلام دنیا"),
        String::new(),
        reply_row("# توضیح"),
        reply_row("echo \"سلام\""),
        String::new(),
        input_box(""),
        status_row(),
    ]));

    assert_eq!(
        run.row(2),
        from_right_edge("  ", &[from_the_right("ﺗﻮﺿﯿﺢ").as_str(), " #"].concat())
    );
    assert_eq!(run.row(3), format!("  echo \"{}\"", from_the_right("ﺳﻠﺎﻡ")));
}

#[test]
fn lines_under_one_another_are_each_a_paragraph_with_a_direction_of_its_own() {
    let mut run = Run::start(30, 8);

    // The model broke the lines itself: the first row left room for the
    // first word of the second, so Codex did not wrap there.
    run.app_writes(screen(&[
        reply_start("Python را دوست دارم"),
        reply_row("سلام دنیا"),
        String::new(),
        input_box(""),
        status_row(),
    ]));

    assert_eq!(
        run.row(0),
        format!("• Python {}", from_the_right("ﺭﺍ ﺩﻭﺳﺖ ﺩﺍﺭﻡ"))
    );
    assert_eq!(
        run.row(1),
        from_right_edge("  ", &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ"))
    );
}

#[test]
fn lines_under_one_another_stay_as_drawn_when_codex_draws_no_colours() {
    // With its colours switched off Codex gives a code block no colour
    // either, so several lines under one another may be code.
    let mut run = Run::start(30, 8);

    run.app_writes(screen(&[
        reply_start("Python را دوست دارم"),
        reply_row("سلام دنیا"),
        String::new(),
        reply_row("خط دوم"),
    ]));

    assert!(run.row_is_as_drawn(0));
    assert!(run.row_is_as_drawn(1));
    // A paragraph by itself is still a paragraph.
    assert_eq!(run.row(3), from_right_edge("  ", &from_the_right("ﺧﻂ ﺩﻭﻡ")));
}

#[test]
fn a_heading_is_laid_out_like_a_paragraph() {
    let mut run = Run::start(30, 4);

    // Codex draws a heading with its marks, bold and in a colour from end
    // to end. A row of code in one colour is never bold from end to end.
    run.app_writes(screen(&[
        reply_start("سلام دنیا"),
        String::new(),
        reply_row(&bold(&coloured("## عنوان بخش", HEADING))),
    ]));

    assert_eq!(
        run.row(2),
        from_right_edge(
            "  ",
            &[from_the_right("ﻋﻨﻮﺍﻥ ﺑﺨﺶ").as_str(), " ##"].concat()
        )
    );
}

#[test]
fn colours_and_styles_stay_on_the_characters_they_were_given_to() {
    let mut run = Run::start(30, 4);

    // A file name in the colour Codex gives inline code, and a bold word.
    run.app_writes(on_row(
        0,
        &reply_start(&format!(
            "فایل {} را {}",
            coloured("a.rs", MARKER),
            bold("ببین")
        )),
    ));

    let on_screen = [
        from_the_right("ﺑﺒﯿﻦ").as_str(),
        " ",
        from_the_right("ﺭﺍ").as_str(),
        " a.rs ",
        from_the_right("ﻓﺎﯾﻞ").as_str(),
    ]
    .concat();
    assert_eq!(run.row(0), from_right_edge("• ", &on_screen));
    let row = &run.terminal.screen()[0];
    let coloured: Vec<usize> = (0..30)
        .filter(|col| row[*col].style.foreground != Color::Default)
        .collect();
    let bold: Vec<usize> = (0..30).filter(|col| row[*col].style.bold).collect();
    let dim: Vec<usize> = (0..30).filter(|col| row[*col].style.dim).collect();
    // The text is 17 cells long and ends at column 29: «ببین» is its
    // last word, at the left, and the file name is four cells after «را».
    assert_eq!(bold, [13, 14, 15, 16]);
    assert_eq!(coloured, [21, 22, 23, 24]);
    // The reply mark and the cell after it are dim, as Codex drew them.
    assert_eq!(dim, [0, 1]);
}

#[test]
fn codexs_own_interface_stays_exactly_as_drawn() {
    let mut run = Run::start(40, 16);
    let green_mark = "\x1b[1;32m•\x1b[22;39m";
    let working = format!(
        "• {}{}",
        coloured("در حال فکر کردن", (108, 108, 108)),
        dim(" (3s • esc to interrupt)")
    );

    run.app_writes(screen(&[
        // The folder the session runs in, under the app's name.
        format!("  {}", bold(">_ OpenAI Codex")),
        format!("     {}", dim("C:\\کار\\پوشه")),
        String::new(),
        sent_start("پیام کاربر"),
        sent_row("خط دوم"),
        String::new(),
        // A command the model ran, and what it printed.
        format!(
            "{green_mark} {} {}",
            bold("Ran"),
            coloured("echo سلام", WORD)
        ),
        format!("  {}", dim("└ سلام")),
        String::new(),
        reply_start("سلام دنیا"),
        String::new(),
        // The line that says Codex is working, with a title in Farsi.
        working,
        String::new(),
        input_box("متن فارسی"),
        format!("  {}", coloured("GPT low · پیام کاربر", (246, 226, 183))),
    ]));

    // The one paragraph of a reply on the screen is laid out.
    assert_eq!(
        run.row(9),
        format!("• {}", at_right_edge(38, &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ")))
    );
    for row in (0..16).filter(|row| *row != 9) {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
    assert_eq!(run.terminal.cursor(), run.without_farcli.cursor());
}

#[test]
fn what_codex_draws_under_the_input_box_stays_as_drawn() {
    let mut run = Run::start(30, 8);

    // A list of files to choose from, in plain text from the third column
    // like a reply's.
    run.app_writes(screen(&[
        reply_start("سلام دنیا"),
        String::new(),
        input_box("@پو"),
        reply_row("پوشه/متن.txt"),
        reply_row("پوشه دوم"),
    ]));

    assert_ne!(run.row(0), run.drawn_row(0));
    for row in 2..5 {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
}

#[test]
fn a_tool_call_whose_mark_looks_like_a_replys_stays_as_drawn() {
    let mut run = Run::start(40, 8);

    // With its moving marks switched off, Codex draws a dim mark before a
    // command that is still running, as before a reply. After the mark
    // comes the name of the call in bold, and nothing in the plain style
    // of a reply's text.
    run.app_writes(screen(&[
        format!(
            "{}{} {}",
            dim("• "),
            bold("Running"),
            coloured("echo سلام", WORD)
        ),
        String::new(),
        format!("{}{}", dim("• "), bold("Explored")),
        format!("  {}", dim("└ Read پوشه/a.rs")),
        String::new(),
        // A reply that begins with a bold word goes on in plain text.
        format!("{}{} سلام دنیا", dim("• "), bold("Note:")),
    ]));

    for row in 0..4 {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
    assert_eq!(
        run.row(5),
        format!("• Note: {}", from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ"))
    );
}

#[test]
fn parts_of_a_reply_that_are_not_surely_paragraphs_stay_as_drawn() {
    let mut run = Run::start(30, 12);

    run.app_writes(screen(&[
        reply_start("سلام دنیا"),
        String::new(),
        // A table the model has not finished writing: Codex shows its
        // text until the table can be drawn.
        reply_row("| نام | مقدار |"),
        String::new(),
        // A quotation.
        reply_row("> یک نقل قول"),
        String::new(),
        // A list item and a row of a table.
        reply_row(&format!("{}برگ", coloured("• ", MARKER))),
        String::new(),
        reply_row(" نام    مقدار"),
        reply_row(&dim("━━━━━  ━━━━━━")),
        reply_row(" برگ    42"),
        String::new(),
    ]));
    // The model's thinking, which Codex draws dim and slanted after the
    // same mark as a reply.
    run.app_writes(on_row(
        11,
        &format!("{}{}", dim("• "), dim(&italic("در حال فکر کردن"))),
    ));

    assert_ne!(run.row(0), run.drawn_row(0));
    for row in 1..12 {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
}

#[test]
fn a_sent_message_drawn_with_no_background_is_told_by_its_mark() {
    // Codex gives the message sent a background only when the Terminal
    // has told it its own colours.
    let mut run = Run::start(30, 8);

    run.app_writes(screen(&rows(&[
        "\x1b[1;2m› \x1b[22mپیام کاربر",
        "  خط دوم",
        "",
        &reply_start("سلام دنیا"),
    ])));

    assert!(run.row_is_as_drawn(0));
    assert!(run.row_is_as_drawn(1));
    assert_eq!(
        run.row(3),
        from_right_edge("• ", &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ"))
    );
}
