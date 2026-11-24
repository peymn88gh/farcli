//! Agent output in Claude Code reads correctly through farcli: a Farsi
//! paragraph is joined, right to left and right-aligned, and everything that
//! is not a paragraph of Agent output stays as the app drew it.
//!
//! Claude Code on Windows reorders each row of Farsi itself, so every test
//! here feeds the session what Claude Code draws, never Logical order. The
//! cases are written by hand in the shape Claude Code 2.1.322 draws a
//! reply, and are the ones no recording shows. `claude_recordings.rs` and
//! `claude_lines_and_code.rs` replay the recordings of real sessions: code
//! blocks in colour, the message sent on its background, and a row with
//! the fullscreen display's button over it are tested there.

mod claude_code;

use claude_code::{
    as_drawn, at_right_edge, from_the_right, laid_out, reply, reply_of, wrapped, Run,
};
use farcli::layout::Direction;

#[test]
fn a_farsi_paragraph_in_a_reply_is_joined_right_to_left_and_right_aligned() {
    let mut run = Run::start(30, 6);

    run.app_writes(reply(&[&["سلام دنیا"]]));

    // The reply mark stays where the app drew it. The paragraph ends at the
    // right edge of the area the app drew it in, which is the Terminal's.
    assert_eq!(
        run.row(0),
        format!("●{}", at_right_edge(29, &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ")))
    );
}

#[test]
fn claude_codes_own_reordering_is_undone_before_a_row_is_laid_out() {
    // Claude Code draws this row in its own Screen order, unjoined.
    let logical = "اندازه 33 x 20 است";
    assert_eq!(as_drawn(logical), "تسا x 20 33 هزادنا");
    let mut run = Run::start(40, 6);

    run.app_writes(reply(&[&[logical]]));

    // What farcli shows is what Layout makes of the text the model wrote.
    assert_eq!(
        run.row(0),
        format!("● {}", laid_out(logical, Direction::RightToLeft, 38))
    );
    // Spelled out: joined letters from the right edge, and each number
    // with its digits in the order they were written.
    let on_screen = [
        from_the_right("ﺍﺳﺖ").as_str(),
        " x 20 33 ",
        from_the_right("ﺍﻧﺪﺍﺯﻩ").as_str(),
    ]
    .concat();
    assert_eq!(run.row(0), format!("●{}", at_right_edge(39, &on_screen)));
}

#[test]
fn every_row_of_a_wrapped_paragraph_reads_in_the_paragraphs_direction() {
    let paragraph = "در پروژه فایل config.json را باز کن و مقدار timeout را روی 123 بگذار.";
    // Claude Code wraps this before `timeout`, and draws that row left to
    // right because the row begins with an English word.
    let rows = wrapped(paragraph, 46);
    assert_eq!(
        rows,
        [
            "در پروژه فایل config.json را باز کن و مقدار ",
            "timeout را روی 123 بگذار."
        ]
    );
    assert_eq!(as_drawn(&rows[1]), "timeout راذگب 123 یور ار.");
    let mut run = Run::start(48, 6);

    run.app_writes(reply_of(&[paragraph], 48));

    // Both rows start at the right edge. The second begins with `timeout`
    // there, and its full stop ends it on the left.
    let first = [
        from_the_right("ﺭﺍ ﺑﺎﺯ ﮐﻦ ﻭ ﻣﻘﺪﺍﺭ").as_str(),
        " config.json ",
        from_the_right("ﺩﺭ ﭘﺮﻭﮊﻩ ﻓﺎﯾﻞ").as_str(),
    ]
    .concat();
    let second = [
        ".",
        from_the_right("ﺑﮕﺬﺍﺭ").as_str(),
        " 123 ",
        from_the_right("ﺭﺍ ﺭﻭﯼ").as_str(),
        " timeout",
    ]
    .concat();
    assert_eq!(run.row(0), format!("●{}", at_right_edge(47, &first)));
    assert_eq!(run.row(1), at_right_edge(48, &second));
}

#[test]
fn a_file_name_and_a_number_in_a_farsi_sentence_read_left_to_right_in_their_place() {
    let mut run = Run::start(52, 6);

    run.app_writes(reply(&[&["فایل config.json را باز کن و 123 خط اضافه کن"]]));

    let on_screen = [
        from_the_right(" ﺧﻂ ﺍﺿﺎﻓﻪ ﮐﻦ").as_str(),
        "123",
        from_the_right(" ﺭﺍ ﺑﺎﺯ ﮐﻦ ﻭ ").as_str(),
        "config.json",
        from_the_right("ﻓﺎﯾﻞ ").as_str(),
    ]
    .concat();
    assert_eq!(run.row(0), format!("●{}", at_right_edge(51, &on_screen)));
}

#[test]
fn a_paragraph_that_starts_with_an_english_word_stays_left_to_right_with_its_farsi_joined() {
    let mut run = Run::start(50, 6);

    run.app_writes(reply(&[&["The word سلام means hello (درود)."]]));

    let on_screen = [
        "● The word ",
        from_the_right("ﺳﻠﺎﻡ").as_str(),
        " means hello (",
        from_the_right("ﺩﺭﻭﺩ").as_str(),
        ").",
    ]
    .concat();
    assert_eq!(run.row(0), on_screen);
}

#[test]
fn any_row_whose_order_the_screen_settles_is_laid_out_from_the_text_that_was_written() {
    // Rows of English and Farsi words, punctuation and words in brackets,
    // in any order, each drawn the way Claude Code draws it. Numbers are
    // left out, and so are rows with an English letter at the left end and
    // a Farsi one at the right: those are the rows the screen does not
    // settle (see `claude_doubtful_rows.rs`).
    const WORDS: &[&str] = &[
        "abc",
        "to",
        "config.json",
        "سلام",
        "می\u{200c}شود",
        "پیام",
        "و",
        "-",
        "(abc)",
        "(پیام)",
        "(abc پیام)",
        "(سلام to)",
        "[پیام]",
        "«سلام»",
        "\"abc\"",
    ];
    // What may follow a word, with no space between.
    const MARKS: &[&str] = &["", "", "", "", ".", "،", ":", "!"];
    let is_english = |c: char| c.is_ascii_alphabetic();
    let is_farsi = |c: char| ('\u{600}'..='\u{6ff}').contains(&c);
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut pick = |from: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % from as u64) as usize
    };

    let mut checked = 0;
    let mut wrong: Vec<String> = Vec::new();
    for _ in 0..6000 {
        let words: Vec<String> = (0..1 + pick(6))
            .map(|_| format!("{}{}", WORDS[pick(WORDS.len())], MARKS[pick(MARKS.len())]))
            .collect();
        let line = words.join(" ");
        let line = line.as_str();
        if claude_code::cells(line) > 28 {
            continue;
        }
        let drawn = as_drawn(line);
        let letters: Vec<char> = drawn
            .chars()
            .filter(|c| is_english(*c) || is_farsi(*c))
            .collect();
        let (Some(leftmost), Some(rightmost)) = (letters.first(), letters.last()) else {
            continue;
        };
        if !letters.iter().any(|c| is_farsi(*c)) || (is_english(*leftmost) && is_farsi(*rightmost))
        {
            continue;
        }
        // A row is not taken for a list item or a table.
        if drawn.starts_with(['-', '│']) {
            continue;
        }
        let mut run = Run::start(30, 3);

        run.app_writes(reply(&[&[line]]));

        let direction = claude_code::direction_of(line);
        let wanted = format!("● {}", laid_out(line, direction, 28));
        if run.row(0).trim_end() != wanted.trim_end() {
            wrong.push(format!(
                "{line:?} drawn as {drawn:?} shown as {:?}",
                run.row(0)
            ));
        }
        checked += 1;
    }
    assert!(checked > 1000, "{checked}");
    assert!(
        wrong.is_empty(),
        "{} of {checked} wrong: {:#?}",
        wrong.len(),
        &wrong[..wrong.len().min(12)]
    );
}

/// The row a reply's one-row paragraph «این یک متن است» is shown on, in a
/// Terminal 40 columns wide.
fn the_paragraph_row() -> String {
    format!("●{}", at_right_edge(39, &from_the_right("ﺍﯾﻦ ﯾﮏ ﻣﺘﻦ ﺍﺳﺖ")))
}

// The next two tests are of Claude Code run with its colours switched off
// (`NO_COLOR`), which no recording shows. With colours, it is the colours
// that tell code: `claude_recordings.rs` and `claude_lines_and_code.rs`
// replay that, and the third test below shows the turn from one to the
// other.

#[test]
fn code_without_colours_is_told_by_its_indentation_and_stays_as_drawn() {
    let mut run = Run::start(40, 8);

    run.app_writes(reply(&[&["این یک متن است"]]));
    run.app_writes("\x1b[3Gdef greet():\r\n");
    run.app_writes(format!("\x1b[7G{}\r\n", as_drawn("# سلام می‌کند")));
    run.app_writes(format!("\x1b[7G{}\r\n", as_drawn("print(\"سلام دنیا\")")));

    assert_eq!(run.row(0), the_paragraph_row());
    for row in 2..5 {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
}

#[test]
fn with_colours_switched_off_lines_under_one_another_may_be_code_and_stay_as_drawn() {
    let mut run = Run::start(40, 8);

    // Three short rows one under another, with no indentation, drawn by an
    // app that draws nothing in colour: lines of code, or lines of a poem.
    // A paragraph would have been wrapped only where the next word did not
    // fit. Without the app's colours farcli cannot tell which this is, so
    // it shows it as the app drew it.
    run.app_writes(reply(&[
        &["این یک متن است"],
        &["# محاسبه مجموع", "total = 5", "# پایان"],
    ]));

    assert_eq!(run.row(0), the_paragraph_row());
    for row in 2..5 {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
}

#[test]
fn once_the_app_has_drawn_in_colour_plain_lines_under_one_another_are_lines_of_prose() {
    let mut run = Run::start(40, 10);

    // The same three rows from an app that draws in colour: its reply mark
    // is white, as Claude Code's is. Had the rows been code in a language
    // Claude Code knows, the comments would be green. Each line that holds
    // Farsi reads as a paragraph of its own, and the line with none stays
    // where it is.
    let in_colour = reply(&[
        &["این یک متن است"],
        &["# محاسبه مجموع", "total = 5", "# پایان"],
    ])
    .replacen('●', "\x1b[38;2;255;255;255m●\x1b[39m", 1);
    run.app_writes(in_colour);

    assert_eq!(run.row(0), the_paragraph_row());
    assert_eq!(
        run.row(2),
        format!(
            "  {}",
            laid_out("# محاسبه مجموع", Direction::RightToLeft, 38)
        )
    );
    assert!(run.row_is_as_drawn(3), "{}", run.row(3));
    assert_eq!(
        run.row(4),
        format!("  {}", laid_out("# پایان", Direction::RightToLeft, 38))
    );
}

/// A border of the Input box, as wide as the Terminal.
fn rule(cols: usize) -> String {
    format!("\x1b[38;2;136;136;136m{}\x1b[39m\r\n", "─".repeat(cols))
}

#[test]
fn the_sent_message_and_the_status_line_stay_exactly_as_drawn() {
    let mut run = Run::start(40, 12);
    let sent = as_drawn("سلام، یک متن بنویس");
    let typed = as_drawn("فایل config.json را ببین");

    // The message the user sent, on a background of its own that fills
    // the row. Then the reply, the line under it, the Input box with Farsi
    // in it, and the status line.
    run.app_writes(format!(
        "\x1b[48;2;55;55;55m❯ {sent}{}\x1b[49m\r\n\r\n",
        " ".repeat(40 - 2 - sent.chars().count())
    ));
    run.app_writes(reply(&[&["این یک متن است"]]));
    run.app_writes("\x1b[38;2;153;153;153m✻ Brewed for 6s\x1b[39m\r\n\r\n");
    run.app_writes(rule(40));
    run.app_writes(format!("❯\u{a0}{typed}\r\n"));
    run.app_writes(rule(40));
    run.app_writes(format!("  ⏵⏵ auto mode on · {}", as_drawn("پوشه پروژه")));

    assert_eq!(run.row(2), the_paragraph_row());
    assert_eq!(run.drawn_row(0), format!("❯ {sent}"));
    assert_eq!(run.drawn_row(7), format!("❯\u{a0}{typed}"));
    for row in [0, 1, 3, 4, 5, 6, 8, 9, 10, 11] {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
    // Besides the reply, only the message in the Input box is laid out.
    // `tests/claude_input_box.rs` is about that.
    assert_eq!(
        run.row(7),
        format!(
            "❯\u{a0}{}",
            laid_out("فایل config.json را ببین", Direction::RightToLeft, 38)
        )
    );
    assert_eq!(run.terminal.cursor(), run.without_farcli.cursor());
}

#[test]
fn a_permission_prompt_and_a_menu_stay_exactly_as_drawn() {
    let mut run = Run::start(40, 14);

    // Claude Code asks before it runs a command: a border, the command,
    // the question and the choices, in place of the Input box.
    run.app_writes(reply(&[&["این یک متن است"]]));
    run.app_writes(rule(40));
    run.app_writes(" Bash command\r\n\r\n");
    run.app_writes(format!("   {}\r\n", as_drawn("echo \"سلام دنیا\"")));
    run.app_writes(format!("   {}\r\n\r\n", as_drawn("چاپ یک پیام")));
    run.app_writes(" Do you want to proceed?\r\n");
    run.app_writes(" ❯ 1. Yes\r\n   2. No\r\n");

    assert_eq!(run.row(0), the_paragraph_row());
    for row in 1..14 {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
}

#[test]
fn what_a_tool_gave_back_is_not_agent_output_and_stays_as_drawn() {
    let mut run = Run::start(40, 8);

    // A tool call under the reply mark, and what the tool printed under
    // the mark Claude Code puts before it.
    run.app_writes("●\x1b[3GBash(cat notes.txt)\r\n");
    run.app_writes(format!("\x1b[3G⎿\x1b[6G{}\r\n", as_drawn("این یک متن است")));
    run.app_writes(format!("\x1b[6G{}\r\n\r\n", as_drawn("خط دوم فایل")));
    run.app_writes(reply(&[&["این یک متن است"]]));

    for row in 0..3 {
        assert!(run.row_is_as_drawn(row), "row {row}: {}", run.row(row));
    }
    assert_eq!(run.row(4), the_paragraph_row());
}
