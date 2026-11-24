//! Rows of Claude Code's whose Logical order the screen cannot settle, and
//! the reading farcli picks for them.
//!
//! Claude Code draws a row that begins with an English word and ends in
//! Farsi exactly as it draws a row that begins in Farsi and ends with the
//! English word. And it draws a number beside an English word the same
//! whichever of the two was written first. farcli guesses the likelier
//! reading from the paragraph the row is in (the decision on ticket #3).
//! Each test here shows two texts that Claude Code draws alike, and which
//! of them farcli takes the row to be. A wrong guess moves a word or a
//! number within its row; it does not scramble the row.

mod claude_code;

use claude_code::{
    as_drawn, at_right_edge, from_the_right, laid_out, reply, reply_of, wrapped, Run,
};
use farcli::layout::Direction::RightToLeft;

/// The rows of a reply with one paragraph, as shown in a Terminal `cols`
/// wide, without the two columns before the reply's text.
fn shown(paragraph: &str, cols: u16) -> Vec<String> {
    let mut run = Run::start(cols, 8);
    run.app_writes(reply_of(&[paragraph], usize::from(cols)));
    let rows = wrapped(paragraph, usize::from(cols) - 2).len();
    run.terminal.screen_text()[..rows]
        .iter()
        .map(|row| row.chars().skip(2).collect())
        .collect()
}

#[test]
fn a_wrapped_row_with_the_wraps_space_on_its_left_was_drawn_right_to_left() {
    let paragraph = "در این پروژه تنظیمات را در فایل config.json نگه می‌داریم و بعد آن را می‌خوانیم.";
    let rows = wrapped(paragraph, 45);
    // The first row ends with the file name, and the wrap left its space
    // after it. A row that began with the file name would look the same
    // but for that space, which Claude Code draws at the row's end: on the
    // left of a row drawn right to left.
    assert_eq!(rows[0], "در این پروژه تنظیمات را در فایل config.json ");
    assert_eq!(
        as_drawn(rows[0].trim_end()),
        as_drawn("config.json در این پروژه تنظیمات را در فایل")
    );
    assert!(as_drawn(&rows[0]).starts_with(" config.json"));

    let shown = shown(paragraph, 47);

    // The file name stays at the end of the row, on the left.
    assert_eq!(shown[0], laid_out(&rows[0], RightToLeft, 45));
    assert!(
        shown[0].trim_start().starts_with("config.json "),
        "{shown:?}"
    );
    assert_eq!(shown[1], laid_out(&rows[1], RightToLeft, 45));
}

#[test]
fn a_wrapped_row_with_room_for_the_wraps_space_and_none_on_its_left_was_drawn_left_to_right() {
    let paragraph = "تنظیمات این برنامه را همیشه در فایل config.json نگه می‌داریم تا هر کس بتواند آن را بخواند و تغییر دهد.";
    let rows = wrapped(paragraph, 41);
    // The second row begins with the file name. It is neither the first
    // row nor the last, and it holds no full stop: only the missing space
    // on its left tells it from a row that ends with the file name.
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1], "config.json نگه می‌داریم تا هر کس بتواند ");
    assert_eq!(
        as_drawn(rows[1].trim_end()),
        as_drawn("نگه می‌داریم تا هر کس بتواند config.json")
    );

    let shown = shown(paragraph, 43);

    // The file name begins the row, at the right edge.
    assert_eq!(shown[1], laid_out(&rows[1], RightToLeft, 41));
    assert!(shown[1].ends_with(" config.json"), "{shown:?}");
    assert_eq!(shown[0], laid_out(&rows[0], RightToLeft, 41));
    assert_eq!(shown[2], laid_out(&rows[2], RightToLeft, 41));
}

#[test]
fn a_last_row_with_a_full_stop_at_its_right_end_was_drawn_left_to_right() {
    let paragraph = "در پروژه فایل config.json را باز کن و مقدار timeout را روی 123 بگذار.";
    let rows = wrapped(paragraph, 46);
    // A full stop ends a row and does not begin one. Claude Code draws it
    // at the right end of a row drawn left to right.
    assert_eq!(rows[1], "timeout را روی 123 بگذار.");
    assert_eq!(as_drawn(&rows[1]), as_drawn(".را روی 123 بگذار timeout"));

    let shown = shown(paragraph, 48);

    assert_eq!(shown[1], laid_out(&rows[1], RightToLeft, 46));
    assert!(shown[1].ends_with(" timeout"), "{shown:?}");
}

#[test]
fn a_row_with_a_full_stop_at_its_left_end_was_drawn_right_to_left() {
    // The whole paragraph is one row that ends with a command and a full
    // stop. Claude Code draws the stop at the left end. A row that began
    // with «‎.git» would look the same.
    let paragraph = "از دستور git status.";
    assert_eq!(as_drawn(paragraph), ".git status روتسد زا");
    assert_eq!(as_drawn(paragraph), as_drawn(".git status از دستور"));

    let shown = shown(paragraph, 40);

    let on_screen = [".git status ", from_the_right("ﺍﺯ ﺩﺳﺘﻮﺭ").as_str()].concat();
    assert_eq!(shown[0], at_right_edge(38, &on_screen));
}

#[test]
fn a_row_that_gives_no_sign_is_taken_to_run_the_way_its_paragraph_does() {
    let paragraph = "در این آزمایش یک متن بلند فارسی نوشته‌ایم و تنظیمات این برنامه را همیشه در فایل config.json نگه می‌داریم.";
    let rows = wrapped(paragraph, 47);
    // The second row fills its area to the last column, so the wrap left
    // no space on it, and it holds no full stop. It ends with the file
    // name; a row that began with the file name would be drawn the same.
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1], "تنظیمات این برنامه را همیشه در فایل config.json");
    assert_eq!(
        as_drawn(&rows[1]),
        as_drawn("config.json تنظیمات این برنامه را همیشه در فایل")
    );

    let shown = shown(paragraph, 49);

    // farcli takes it as a row drawn right to left, like its paragraph:
    // the file name ends the row, on the left. Had the model written the
    // row beginning with the file name, this would be the wrong reading.
    assert_eq!(shown[1], laid_out(&rows[1], RightToLeft, 47));
    assert!(shown[1].starts_with("config.json "), "{shown:?}");
}

#[test]
fn a_one_row_paragraph_that_gives_no_sign_is_taken_to_begin_with_its_farsi() {
    // No full stop and no second row. The two readings differ only in
    // which edge the row starts from: every word is in the same place
    // relative to the others.
    let paragraph = "برای اجرا بزن git status";
    assert_eq!(as_drawn(paragraph), as_drawn("git status برای اجرا بزن"));

    let shown = shown(paragraph, 40);

    let on_screen = ["git status ", from_the_right("ﺑﺮﺍﯼ ﺍﺟﺮﺍ ﺑﺰﻥ").as_str()].concat();
    assert_eq!(shown[0], at_right_edge(38, &on_screen));
}

#[test]
fn a_number_right_after_an_english_word_is_read_with_that_word() {
    // The second row begins with an English word, so Claude Code draws it
    // left to right, with the number between the word and the Farsi. It
    // draws «Python را نصب کن 3.» the same way: there the number ends the
    // Farsi.
    let first = "برای این کار از ";
    let second = "Python 3 را نصب کن.";
    assert_eq!(as_drawn(second), "Python 3 نک بصن ار.");
    assert_eq!(as_drawn(second), as_drawn("Python را نصب کن 3."));
    let mut run = Run::start(22, 6);

    run.app_writes(reply(&[&[first, second]]));

    // farcli reads the number with the word it stands beside. Laid out in
    // the paragraph's direction, «Python 3» begins the row at the right.
    let on_screen = [".", from_the_right("ﺭﺍ ﻧﺼﺐ ﮐﻦ").as_str(), " Python 3"].concat();
    assert_eq!(run.row(1), at_right_edge(22, &on_screen));
    assert_eq!(
        run.row(1),
        format!("  {}", laid_out(second, RightToLeft, 20))
    );
}

#[test]
fn a_lone_x_with_two_numbers_after_it_is_read_as_a_size() {
    // «33 x 20», «x 20 33» and «33 20 x» are all drawn `x 20 33` in a row
    // that runs right to left. farcli takes the reading a person would
    // write, the size, and shows its numbers in the order of that reading.
    let readings = [
        "اندازه 33 x 20 است",
        "اندازه x 20 33 است",
        "اندازه 33 20 x است",
    ];
    for reading in readings {
        assert_eq!(as_drawn(reading), "تسا x 20 33 هزادنا");
    }

    let shown = shown(readings[0], 40);

    let on_screen = [
        from_the_right("ﺍﺳﺖ").as_str(),
        " 33 x 20 ",
        from_the_right("ﺍﻧﺪﺍﺯﻩ").as_str(),
    ]
    .concat();
    assert_eq!(shown[0], at_right_edge(38, &on_screen));
    // Had the model written the letter first and both numbers after it,
    // this would be the wrong reading.
    assert_ne!(shown[0], laid_out(readings[1], RightToLeft, 38));
}

#[test]
fn a_size_of_three_numbers_and_a_size_in_brackets_are_read_as_sizes() {
    for (written, size) in [
        ("اندازه 128 x 32 x 3 است", " 128 x 32 x 3 "),
        ("اندازه (33 x 20) است", " (33 x 20) "),
        ("اندازه 33 X 20 است", " 33 X 20 "),
    ] {
        let shown = shown(written, 40);

        let on_screen = [
            from_the_right("ﺍﺳﺖ").as_str(),
            size,
            from_the_right("ﺍﻧﺪﺍﺯﻩ").as_str(),
        ]
        .concat();
        assert_eq!(shown[0], at_right_edge(38, &on_screen), "{written}");
    }
}

#[test]
fn a_size_written_with_a_multiplication_sign_or_a_date_leaves_no_doubt() {
    // Claude Code draws these with their numbers in the opposite order,
    // and only one text gives that picture.
    for (written, drawn, piece) in [
        ("اندازه 33 × 20 است", "تسا 20 × 33 هزادنا", " 33 × 20 "),
        (
            "تاریخ 2026-11-23 است",
            "تسا 23-11-2026 خیرات",
            " 2026-11-23 ",
        ),
    ] {
        assert_eq!(as_drawn(written), drawn);

        let shown = shown(written, 40);

        let first_word = match written.starts_with("اندازه") {
            true => "ﺍﻧﺪﺍﺯﻩ",
            false => "ﺗﺎﺭﯾﺦ",
        };
        let on_screen = [
            from_the_right("ﺍﺳﺖ").as_str(),
            piece,
            from_the_right(first_word).as_str(),
        ]
        .concat();
        assert_eq!(shown[0], at_right_edge(38, &on_screen), "{written}");
    }
}

#[test]
fn a_size_in_a_farsi_row_of_an_english_paragraph_is_read_as_a_size_too() {
    // The paragraph begins in English, so it runs left to right, and its
    // second row begins in Farsi, so Claude Code drew that row right to
    // left.
    let first = "The picture is small: ";
    let written = "اندازه 33 x 20 است";
    assert_eq!(as_drawn(written), "تسا x 20 33 هزادنا");
    let mut run = Run::start(26, 6);

    run.app_writes(reply(&[&[first, written]]));

    // Laid out left to right like its paragraph, each stretch of Farsi
    // reading from the right in its place.
    let on_screen = [
        from_the_right("ﺍﻧﺪﺍﺯﻩ").as_str(),
        " 33 x 20 ",
        from_the_right("ﺍﺳﺖ").as_str(),
    ]
    .concat();
    assert_eq!(run.row(1), format!("  {on_screen}"));
}

#[test]
fn a_path_and_a_dotted_name_inside_a_farsi_row_are_shown_in_one_piece() {
    // Claude Code draws the marks a path begins with at its other end.
    // Only one text gives that picture, so nothing is guessed here: the
    // row is turned back, and Layout keeps the path whole.
    for (written, drawn, name) in [
        (
            "فایل ./src/main.rs را ببین",
            "نیبب ار src/main.rs/. لیاف",
            " ./src/main.rs ",
        ),
        (
            "فایل .gitignore را ببین",
            "نیبب ار gitignore. لیاف",
            " .gitignore ",
        ),
    ] {
        assert_eq!(as_drawn(written), drawn);

        let shown = shown(written, 40);

        let on_screen = [
            from_the_right("ﺭﺍ ﺑﺒﯿﻦ").as_str(),
            name,
            from_the_right("ﻓﺎﯾﻞ").as_str(),
        ]
        .concat();
        assert_eq!(shown[0], at_right_edge(38, &on_screen), "{written}");
    }
}
