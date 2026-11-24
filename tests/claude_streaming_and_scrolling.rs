//! Agent output in Claude Code stays correct while it arrives piece by
//! piece, after it has scrolled off the top of the classic display, and
//! wherever the user scrolls to in the fullscreen display. Colours and
//! styles stay on the letters they were given to.

mod claude_code;

use claude_code::{as_drawn, at, at_right_edge, from_the_right, laid_out, placed, wrapped, Run};
use farcli::layout::Direction::RightToLeft;
use farcli::screen::{Color, Style};

/// A paragraph that Claude Code wraps onto two rows in an area 46 columns
/// wide. The second row begins with an English word, so Claude Code draws
/// it left to right: read by itself, it would stay that way.
const TWO_ROWS: &str = "در پروژه فایل config.json را باز کن و مقدار timeout را روی 123 بگذار.";

/// The rows of `TWO_ROWS` as they should read in a Terminal 48 columns
/// wide: the reply's area, without the two columns before it.
fn two_rows_wanted() -> Vec<String> {
    wrapped(TWO_ROWS, 46)
        .iter()
        .map(|row| laid_out(row, RightToLeft, 46))
        .collect()
}

/// The bytes Claude Code writes for one row of a reply's text.
fn text_row(logical: &str) -> String {
    format!("{}\r\n", placed(2, &as_drawn(logical)))
}

#[test]
fn a_paragraph_reads_correctly_while_its_rows_arrive_one_by_one() {
    let rows = wrapped(TWO_ROWS, 46);
    let wanted = two_rows_wanted();
    let mut run = Run::start(48, 8);

    run.app_writes(format!("●{}", text_row(&rows[0])));
    assert_eq!(run.row(0), format!("● {}", wanted[0]));

    run.app_writes(text_row(&rows[1]));
    assert_eq!(run.row(0), format!("● {}", wanted[0]));
    assert_eq!(run.row(1), format!("  {}", wanted[1]));
}

#[test]
fn a_row_of_farsi_reads_correctly_while_it_arrives_word_by_word() {
    let mut run = Run::start(30, 4);

    // Claude Code draws a row again each time it grows: the newest word is
    // on the left, where its own reordering puts it.
    run.app_writes(format!("●{}", placed(2, &as_drawn("سلام"))));
    assert_eq!(
        run.row(0),
        format!("●{}", at_right_edge(29, &from_the_right("ﺳﻠﺎﻡ")))
    );

    run.app_writes(format!("\r\x1b[2K●{}", placed(2, &as_drawn("سلام دنیا"))));
    assert_eq!(
        run.row(0),
        format!("●{}", at_right_edge(29, &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ")))
    );
}

#[test]
fn rows_that_scroll_off_the_top_are_found_corrected_when_scrolling_back() {
    let rows = wrapped(TWO_ROWS, 46);
    let wanted = two_rows_wanted();
    let mut run = Run::start(48, 5);
    let short = format!("  {}", laid_out("سلام دنیا", RightToLeft, 46));

    // The classic display: the reply is written downwards and its first
    // rows leave the screen for good.
    run.app_writes(format!("●{}", text_row(&rows[0])));
    run.app_writes(text_row(&rows[1]));
    run.app_writes("\r\n");
    run.app_writes(text_row("سلام دنیا"));
    run.app_writes("\r\n");
    // The top of the screen now cuts through the paragraph: its first row
    // has scrolled off, and the second still reads as part of it.
    assert_eq!(run.terminal.scrollback_text(), [format!("● {}", wanted[0])]);
    assert_eq!(run.row(0), format!("  {}", wanted[1]));

    run.app_writes(text_row("سلام دنیا"));
    run.app_writes("\r\n");
    assert_eq!(
        run.terminal.scrollback_text(),
        [
            format!("● {}", wanted[0]),
            format!("  {}", wanted[1]),
            String::new()
        ]
    );
    assert_eq!(run.row(0), short);
    assert_eq!(run.row(2), short);
}

#[test]
fn a_reply_written_in_one_piece_that_is_longer_than_the_screen_is_corrected_from_its_first_row() {
    let rows = wrapped(TWO_ROWS, 46);
    let wanted = two_rows_wanted();
    let mut run = Run::start(48, 4);

    // This is how the classic display writes a finished reply on Windows:
    // its first rows are on screen for no frame at all.
    let mut reply = format!("●{}{}\r\n", text_row(&rows[0]), text_row(&rows[1]));
    for _ in 0..3 {
        reply.push_str(&text_row("سلام دنیا"));
        reply.push_str("\r\n");
    }
    run.app_writes(reply);

    let scrolled_off = run.terminal.scrollback_text();
    assert_eq!(scrolled_off[0], format!("● {}", wanted[0]));
    assert_eq!(scrolled_off[1], format!("  {}", wanted[1]));
}

#[test]
fn the_lines_of_a_poem_read_correctly_while_they_scroll_off_the_top_one_by_one() {
    // Four lines under one another, each a paragraph of its own. The app
    // draws in colour (its reply mark is white), so plain lines are prose.
    // The third begins with an English word and ends with a full stop, so
    // it runs left to right; the lines around it do not follow it.
    let lines = [
        "باران می‌بارد، کوچه خیس است",
        "برگ زرد روی شاخه می‌لرزد",
        "Python را دوست دارم.",
        "دلم برای تو تنگ است",
    ];
    let wanted: Vec<String> = lines
        .iter()
        .map(|line| laid_out(line, claude_code::direction_of(line), 38))
        .collect();
    assert!(wanted[2].starts_with("Python "), "{wanted:?}");
    let mut run = Run::start(40, 4);

    // The classic display: every row written pushes the rows above it up,
    // and the top of the screen cuts between the lines.
    run.app_writes("\x1b[38;2;255;255;255m●\x1b[39m");
    for (written, line) in lines.iter().enumerate() {
        run.app_writes(text_row(line));
        let scrolled_off = run.terminal.scrollback_text();
        let shown = [scrolled_off, run.terminal.screen_text()].concat();
        for (row, wanted) in wanted[..=written].iter().enumerate() {
            let margin = if row == 0 { "● " } else { "  " };
            assert_eq!(
                shown[row],
                format!("{margin}{wanted}"),
                "row {row} after line {written}"
            );
        }
    }
    assert_eq!(run.terminal.scrollback_text().len(), 1);
    run.app_writes("\r\n\r\n");
    assert_eq!(
        run.terminal.scrollback_text(),
        [
            format!("● {}", wanted[0]),
            format!("  {}", wanted[1]),
            format!("  {}", wanted[2])
        ]
    );
    assert_eq!(run.row(0), format!("  {}", wanted[3]));
}

/// The bytes that draw `rows` from the top of the fullscreen display, the
/// way Claude Code draws the conversation again after the user scrolls.
fn fullscreen_frame(rows: &[String]) -> String {
    let mut bytes = String::new();
    for (row, text) in rows.iter().enumerate() {
        bytes.push_str(&format!("\x1b[{};1H\x1b[2K{text}", row + 1));
    }
    bytes
}

#[test]
fn in_the_fullscreen_display_a_paragraph_reads_the_same_when_its_first_row_is_scrolled_out_of_sight(
) {
    let rows = wrapped(TWO_ROWS, 46);
    let wanted = two_rows_wanted();
    let mut run = Run::start(48, 6);
    let first = format!("●{}", placed(2, &as_drawn(&rows[0])));
    let second = placed(2, &as_drawn(&rows[1]));
    let empty = String::new();

    run.app_writes("\x1b[?1049h");
    run.app_writes(fullscreen_frame(&[
        empty.clone(),
        first,
        second.clone(),
        empty.clone(),
    ]));
    assert_eq!(run.row(1), format!("● {}", wanted[0]));
    assert_eq!(run.row(2), format!("  {}", wanted[1]));

    // Scrolled down by two rows: the paragraph's second row is the first
    // thing on screen. By itself it is a row that begins with an English
    // word; farcli has seen the paragraph it belongs to.
    run.app_writes(fullscreen_frame(&[
        second,
        empty.clone(),
        empty.clone(),
        empty,
    ]));
    assert_eq!(run.row(0), format!("  {}", wanted[1]));
}

#[test]
fn in_the_fullscreen_display_the_end_of_a_paragraph_never_seen_before_is_read_from_its_rows() {
    // The user scrolls up into a long reply: the first thing on screen is
    // the end of a paragraph whose first rows are still out of sight.
    let paragraph = "در این آزمایش یک متن بلند فارسی نوشته‌ایم و timeout را روی 123 می‌گذاریم تا نتیجه را ببینیم.";
    let rows = wrapped(paragraph, 46);
    assert_eq!(rows.len(), 3);
    assert!(rows[1].starts_with("timeout"), "{rows:?}");
    let mut run = Run::start(48, 6);

    // No mark of a reply is in sight. The Input box under the conversation,
    // which this display always shows, is what tells that the screen is
    // Claude Code's: rows that only look like part of a reply are left as
    // drawn on a screen that shows nothing of the app's own.
    let border = "─".repeat(48);
    run.app_writes("\x1b[?1049h");
    run.app_writes(fullscreen_frame(&[
        placed(2, &as_drawn(&rows[1])),
        placed(2, &as_drawn(&rows[2])),
        String::new(),
        border.clone(),
        "❯ ".to_string(),
        border,
    ]));

    // The last row is plainly drawn right to left, so the paragraph is
    // taken to be a Farsi one, and the row above it reads that way too.
    assert_eq!(
        run.row(0),
        format!("  {}", laid_out(&rows[1], RightToLeft, 46))
    );
    assert_eq!(
        run.row(1),
        format!("  {}", laid_out(&rows[2], RightToLeft, 46))
    );
}

#[test]
fn in_the_fullscreen_display_lines_left_as_drawn_stay_so_when_only_the_last_of_them_is_in_sight() {
    // Three lines from an app with its colours switched off, which may be
    // code (see `with_colours_switched_off_lines_under_one_another_may_be_
    // code_and_stay_as_drawn` in `claude_agent_output.rs`). Scrolled until
    // only the last one is on screen, it looks like a one-row paragraph.
    let lines: Vec<String> = ["# محاسبه مجموع", "total = 5", "# پایان"]
        .iter()
        .map(|line| placed(2, &as_drawn(line)))
        .collect();
    let empty = String::new();
    let mut run = Run::start(40, 6);

    run.app_writes("\x1b[?1049h");
    run.app_writes(fullscreen_frame(&[
        format!("●{}", placed(2, &as_drawn("سلام دنیا"))),
        empty.clone(),
        lines[0].clone(),
        lines[1].clone(),
        lines[2].clone(),
    ]));
    for row in 2..5 {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }

    run.app_writes(fullscreen_frame(&[
        lines[2].clone(),
        empty.clone(),
        empty.clone(),
        empty.clone(),
        empty,
    ]));
    assert!(run.row_is_as_drawn(0), "{}", run.row(0));
}

#[test]
fn the_colour_of_an_english_word_in_a_farsi_row_stays_on_that_word() {
    let mut run = Run::start(40, 4);

    // «فایل config.json را ببین», with the file name in the colour Claude
    // Code gives inline code and the last word bold. Claude Code draws the
    // row from the right, so the bold word is written first, on the left.
    // (`claude_lines_and_code.rs` replays a recorded row whose styled words
    // are Farsi. No recording has a coloured English word in a Farsi row.)
    run.app_writes(format!(
        "●\x1b[3G\x1b[1m{}\x1b[22m {} \x1b[38;2;177;185;249mconfig.json\x1b[39m {}\r\n",
        as_drawn("ببین"),
        as_drawn("را"),
        as_drawn("فایل"),
    ));

    let on_screen = [
        from_the_right("ﺑﺒﯿﻦ").as_str(),
        " ",
        from_the_right("ﺭﺍ").as_str(),
        " config.json ",
        from_the_right("ﻓﺎﯾﻞ").as_str(),
    ]
    .concat();
    assert_eq!(run.row(0), format!("●{}", at_right_edge(39, &on_screen)));
    // The row ends at column 39. From the right: «فایل», a space, the file
    // name, a space, «را», a space and the bold word.
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    let code = Style {
        foreground: Color::Rgb(177, 185, 249),
        ..Style::default()
    };
    let style = |col: u16| run.terminal.cell(at(0, col)).style;
    for col in 36..40 {
        assert_eq!(style(col), Style::default(), "column {col}");
    }
    assert_eq!(run.terminal.cell(at(0, 24)).character, 'c');
    for col in 24..35 {
        assert_eq!(style(col), code, "column {col}");
    }
    for col in 20..24 {
        assert_eq!(style(col), Style::default(), "column {col}");
    }
    assert_eq!(run.terminal.cell(at(0, 19)).character, 'ﺑ');
    for col in 16..20 {
        assert_eq!(style(col), bold, "column {col}");
    }
    assert_eq!(style(15), Style::default());
}
