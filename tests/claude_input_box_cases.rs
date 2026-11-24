//! What the recordings of typing do not show about Claude Code's Input
//! box: the app drawing later than the keys come, a message the user did
//! not type, keys farcli does not follow, line breaks, and the screens on
//! which farcli must keep its hands off.
//!
//! Every test goes through the session. The app is a stand-in that draws
//! its Input box as Claude Code was seen to draw it (`claude_code::input_box`).

mod claude_code;

use std::path::PathBuf;

use claude_code::input_box::{ClaudeCode, Key};
use claude_code::{as_drawn, at, cells, laid_out};
use farcli::layout::Direction::{LeftToRight, RightToLeft};
use farcli::recording::{Recording, Replay};

/// The prompt mark and the no-break space the app draws after it.
const MARK: &str = "❯\u{a0}";

/// The rows of a message as farcli should show them in a Terminal `cols`
/// wide: each row of `as_typed` laid out in the message's columns, right to
/// left.
fn right_to_left(as_typed: &[String], cols: usize) -> Vec<String> {
    as_typed
        .iter()
        .enumerate()
        .map(|(row, typed)| {
            let margin = if row == 0 { MARK } else { "  " };
            format!("{margin}{}", laid_out(typed, RightToLeft, cols - 2))
        })
        .collect()
}

/// The column of the cursor after `typed` cells of a right-to-left row in a
/// Terminal `cols` wide.
fn left_of(typed: usize, cols: usize) -> u16 {
    (cols - 1 - typed) as u16
}

#[test]
fn the_stand_in_draws_the_input_box_as_claude_code_drew_it_in_the_recordings() {
    // The message of the recordings that wraps onto three rows, typed into
    // the stand-in in a Terminal of the same width.
    let message = "این یک پیام بلند است که باید در چند خط شکسته شود تا ببینیم جعبه\u{200c}ی \
                   ورودی با متن فارسی طولانی، کلمه\u{200c}های English و عدد 2026 چه می\u{200c}کند";
    let mut app = ClaudeCode::start(60, 12);
    app.types(message);

    let recording = Recording::load(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/recordings/claude-classic-typing.recording"),
    )
    .unwrap();
    let mut real = Replay::without_farcli(&recording);
    let last_row = "   دنکی\u{200c}م هچ 2026 ددع و";
    assert!(real.play_until(|real| real
        .terminal
        .screen_text()
        .iter()
        .any(|row| row == last_row)));
    let screen = real.terminal.screen_text();
    let first = screen.iter().position(|row| row.starts_with('❯')).unwrap();

    assert_eq!(app.drawn(), screen[first..first + 3]);
    // The real app's cursor is six rows further down, in the same column.
    let cursor = real.terminal.cursor();
    assert_eq!(
        app.run.without_farcli.cursor(),
        at(cursor.row - first as u16 + 3, cursor.col)
    );
}

#[test]
fn a_frame_drawn_before_the_app_has_handled_every_key_shows_the_message_as_far_as_drawn() {
    let mut app = ClaudeCode::start(40, 10);
    // A row that ends with an English word is one the screen does not
    // settle: the app draws «سلام w» as it draws «w سلام».
    app.types("سلام w");
    assert_eq!(app.shown(), right_to_left(&["سلام w".to_string()], 40));

    // Two more keys come while the app is busy, and it draws when it has
    // handled the first of them only.
    app.press_keys("or");
    app.handles(1);

    assert_eq!(app.shown(), right_to_left(&["سلام wo".to_string()], 40));
    assert_eq!(app.cursor(), at(0, left_of(7, 40)));

    app.handles(1);

    assert_eq!(app.shown(), right_to_left(&["سلام wor".to_string()], 40));
    assert_eq!(app.cursor(), at(0, left_of(8, 40)));
}

#[test]
fn keys_that_all_come_before_the_app_draws_once_are_all_in_the_message_it_draws() {
    let mut app = ClaudeCode::start(40, 10);

    app.press_keys("سلام world");
    app.handles(10);

    assert_eq!(app.shown(), right_to_left(&["سلام world".to_string()], 40));
    assert_eq!(app.cursor(), at(0, left_of(10, 40)));
}

#[test]
fn a_message_the_user_did_not_type_is_laid_out_when_the_screen_settles_its_order() {
    let mut app = ClaudeCode::start(40, 10);

    // An earlier message called back with the Up key: farcli has seen none
    // of its characters typed. The row begins and ends in Farsi, so only
    // one order of it gives what the app drew.
    app.press(Key::Up);
    app.holds("فایل config.json را ببین");

    assert_eq!(
        app.shown(),
        right_to_left(&["فایل config.json را ببین".to_string()], 40)
    );
    assert_eq!(app.cursor(), at(0, left_of(24, 40)));
}

#[test]
fn a_message_the_user_did_not_type_whose_order_the_screen_does_not_settle_is_shown_as_drawn() {
    let mut app = ClaudeCode::start(40, 10);

    // «سلام world» called back. The app draws it exactly as it would draw
    // « world سلام», and farcli saw neither typed.
    app.press(Key::Up);
    app.holds("سلام world");

    assert_eq!(app.drawn(), [format!("{MARK} world مالس")]);
    assert!(app.is_shown_as_drawn());

    // One Farsi letter after it and the row both begins and ends in Farsi:
    // now it has one order only.
    app.types(" خ");

    assert_eq!(app.shown(), right_to_left(&["سلام world خ".to_string()], 40));
    assert_eq!(app.cursor(), at(0, left_of(12, 40)));
}

#[test]
fn a_message_the_app_changes_by_itself_is_never_shown_as_it_was_before() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام wor");
    assert_eq!(app.shown(), right_to_left(&["سلام wor".to_string()], 40));

    // The app completes the word. No key farcli saw explains the new text,
    // and the screen does not settle its order.
    app.press(Key::Down);
    app.holds("سلام workspace");

    assert!(app.is_shown_as_drawn());
}

#[test]
fn a_letter_typed_after_a_key_that_moved_the_cursor_goes_where_the_app_put_the_cursor() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world");

    // The Home key: farcli does not work out where it takes the cursor. It
    // sees where the app draws it.
    app.presses(Key::Home);
    assert_eq!(app.cursor(), at(0, 39));
    assert_eq!(app.character_under_the_cursor(), 'ﺳ');

    app.types("و ");

    assert_eq!(app.shown(), right_to_left(&["و سلام world".to_string()], 40));
    assert_eq!(app.cursor(), at(0, 37));
}

#[test]
fn a_letter_typed_after_a_key_that_did_nothing_goes_where_the_cursor_was() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world");

    // The End key at the end of the message: the app has nothing to draw.
    app.press(Key::End);
    app.handles(1);
    app.types("s");

    assert_eq!(app.shown(), right_to_left(&["سلام worlds".to_string()], 40));
}

#[test]
fn backspace_and_delete_leave_the_rest_of_the_message_in_order() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام 33 x 20s");

    app.presses(Key::Backspace);

    assert_eq!(app.shown(), right_to_left(&["سلام 33 x 20".to_string()], 40));

    // The cursor is on the empty cell at the left end of the row, and the
    // «x» is the character on its right: «x 20» reads from the left.
    app.presses(Key::Right);
    assert_eq!(app.character_under_the_cursor(), 'x');

    app.presses(Key::Delete);

    assert_eq!(app.shown(), right_to_left(&["سلام 33  20".to_string()], 40));
}

#[test]
fn the_cursor_is_on_the_right_digit_of_a_number_beside_an_english_word() {
    let mut app = ClaudeCode::start(40, 10);
    // The app draws «33 x 20» as it draws «x 20 33» and «33 20 x». Only the
    // keys tell which digit the app's cursor is before.
    app.types("اندازه 33 x 20 است");
    // From the left end of the row, to the right over «است», a space,
    // «x 20» and a space.
    for _ in 0..10 {
        app.presses(Key::Right);
    }

    // Before the first «3» of «33»: it is under the cursor, and the second
    // «3» is to its right.
    assert_eq!(app.character_under_the_cursor(), '3');
    let cursor = app.run.terminal.cursor();
    let right = app.run.terminal.cell(at(cursor.row, cursor.col + 1));
    assert_eq!(right.character, '3');
    let left = app.run.terminal.cell(at(cursor.row, cursor.col - 1));
    assert_eq!(left.character, ' ');
    assert_eq!(
        app.shown(),
        right_to_left(&["اندازه 33 x 20 است".to_string()], 40)
    );
}

#[test]
fn each_line_of_a_message_with_line_breaks_has_its_own_direction() {
    let mut app = ClaudeCode::start(40, 12);

    app.types("خط یک");
    app.presses(Key::LineBreak);
    app.types("line two (x).");
    app.presses(Key::LineBreak);
    app.types("خط سه");

    // The English line between the two Farsi lines is a paragraph of its
    // own: it stays as the app drew it, full stop at its right end.
    assert_eq!(
        app.shown(),
        [
            format!("{MARK}{}", laid_out("خط یک", RightToLeft, 38)),
            "  line two (x).".to_string(),
            format!("  {}", laid_out("خط سه", RightToLeft, 38)),
        ]
    );
    assert_eq!(app.cursor(), at(2, left_of(5, 40)));
}

#[test]
fn a_line_that_begins_with_an_english_word_and_holds_farsi_is_joined_and_stays_at_the_left() {
    let mut app = ClaudeCode::start(40, 12);

    app.types("سلام");
    app.presses(Key::LineBreak);
    app.types("hello دنیا");

    assert_eq!(
        app.shown(),
        [
            format!("{MARK}{}", laid_out("سلام", RightToLeft, 38)),
            format!("  {}", laid_out("hello دنیا", LeftToRight, 38)),
        ]
    );
    // After the ten characters of the line, as the app has it.
    assert_eq!(app.cursor(), at(1, 12));
}

#[test]
fn a_pasted_message_of_several_lines_reads_as_it_was_pasted() {
    let mut app = ClaudeCode::start(40, 12);

    // Windows Terminal ends each pasted line with a carriage return.
    app.presses(Key::Paste(
        "سلام دنیا\rhello world\rعدد 33 x 20 است".to_string(),
    ));

    assert_eq!(
        app.shown(),
        [
            format!("{MARK}{}", laid_out("سلام دنیا", RightToLeft, 38)),
            "  hello world".to_string(),
            format!("  {}", laid_out("عدد 33 x 20 است", RightToLeft, 38)),
        ]
    );
    assert_eq!(app.cursor(), at(2, left_of(15, 40)));
    assert!(app
        .sent_to_app
        .contains("\x1b[200~سلام دنیا\rhello world\rعدد 33 x 20 است\x1b[201~"));
}

#[test]
fn a_message_wraps_again_in_a_narrower_terminal_and_still_reads_as_typed() {
    let mut app = ClaudeCode::start(60, 12);
    app.types("این یک پیام بلند است که باید در چند خط شکسته شود و کلمه English هم دارد");
    assert_eq!(app.rows_as_typed().len(), 2);
    assert_eq!(app.shown(), right_to_left(&app.rows_as_typed(), 60));

    app.is_resized(34, 12);

    assert_eq!(app.rows_as_typed().len(), 3);
    assert_eq!(app.shown(), right_to_left(&app.rows_as_typed(), 34));
}

#[test]
fn the_cursor_on_the_space_the_app_wrapped_at_is_at_the_left_end_of_the_row_above() {
    let mut app = ClaudeCode::start(30, 12);
    app.types("یک دو سه چهار پنج شش هفت هشت");
    let rows = app.rows_as_typed();
    assert_eq!(rows.len(), 2);
    let second = cells(&rows[1]);

    // To the right end of the second row, where it starts, and one more:
    // the app's cursor is on the space it wrapped at, which it draws at
    // the end of the first row.
    for _ in 0..=second {
        app.presses(Key::Right);
    }

    assert_eq!(app.shown(), right_to_left(&rows, 30));
    assert_eq!(app.cursor(), at(0, left_of(cells(&rows[0]), 30)));
}

#[test]
fn characters_that_come_with_no_key_are_followed_like_keys() {
    let mut app = ClaudeCode::start(40, 10);

    // A Terminal that does not send keys in the Windows form sends the
    // characters themselves, and its own codes for the arrow keys.
    for character in "سلام worlds".chars() {
        app.press_as(&character.to_string());
        app.handles(1);
    }
    // To the right over «world», onto the «s».
    for _ in 0..6 {
        app.press_as("\x1b[C");
        app.handles(1);
    }
    app.press_as("\x7f");
    app.handles(1);

    assert_eq!(app.shown(), right_to_left(&["سلام worls".to_string()], 40));
    assert_eq!(app.character_under_the_cursor(), 's');
}

#[test]
fn after_a_message_is_sent_the_next_one_is_followed_from_an_empty_box() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world");

    app.presses(Key::Enter);

    // The box shows the app's own hint again, and nothing of it is touched.
    assert!(app.is_shown_as_drawn());

    app.types("فایل main.rs");

    assert_eq!(
        app.shown(),
        right_to_left(&["فایل main.rs".to_string()], 40)
    );
}

#[test]
fn keys_pressed_while_the_app_asks_something_else_do_nothing_to_the_message() {
    let mut app = ClaudeCode::start(40, 12);
    app.types("سلام world");

    // The app puts a question of its own where the box was. The user picks
    // an answer with keys that are not for the message.
    app.run.app_writes(format!(
        "\x1b[?2026h\x1b[3;1H\x1b[J{}\r\n Do you want to proceed?\r\n ❯ 1. Yes\r\n   2. No\x1b[?2026l",
        "─".repeat(40)
    ));
    assert!(app.is_shown_as_drawn());
    app.press(Key::Down);
    app.press(Key::Backspace);
    app.press(Key::Character('y'));
    app.takes_the_keys_for_something_else();

    // The box is back with the message as it was, and typing goes on.
    app.draws();
    assert_eq!(app.shown(), right_to_left(&["سلام world".to_string()], 40));

    app.types("s");

    assert_eq!(app.shown(), right_to_left(&["سلام worlds".to_string()], 40));
}

#[test]
fn the_box_in_the_mode_that_runs_a_command_stays_as_the_app_drew_it() {
    let mut app = ClaudeCode::start(40, 10);
    // An exclamation mark turns the box into one that runs a command: the
    // mark changes and the exclamation mark is not part of the text. A
    // command is always left to right.
    app.mark = "\x1b[38;2;253;93;177m!\x1b[39m ".to_string();

    app.types("echo سلام");

    assert_eq!(app.drawn(), [format!("! echo {}", as_drawn("سلام"))]);
    assert!(app.is_shown_as_drawn());
}

#[test]
fn a_hint_the_app_shows_in_an_empty_box_stays_as_the_app_drew_it() {
    let mut app = ClaudeCode::start(40, 10);
    // What the app suggests typing next, in Farsi, drawn dim. It is not a
    // message: no key typed it.
    app.hint = as_drawn("یک آزمون بنویس");

    app.draws();

    assert_eq!(
        app.drawn(),
        [format!("{MARK}{}", as_drawn("یک آزمون بنویس"))]
    );
    assert!(app.is_shown_as_drawn());
}

#[test]
fn the_message_is_laid_out_while_the_app_hides_the_cursor() {
    let mut app = ClaudeCode::start(40, 10);
    app.shows_cursor = false;

    app.types("سلام world");

    assert_eq!(app.shown(), right_to_left(&["سلام world".to_string()], 40));
    assert!(!app.run.terminal.cursor_visible());
}

#[test]
fn a_key_that_deletes_the_word_before_the_cursor_leaves_the_rest_in_order() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world foo");

    // farcli does not know what Ctrl+W deletes. It sees that what the app
    // draws next is the message without what stood before the cursor.
    app.presses(Key::DeleteWord);

    assert_eq!(app.shown(), right_to_left(&["سلام world".to_string()], 40));
    assert_eq!(app.cursor(), at(0, left_of(11, 40)));

    app.types("bar");

    assert_eq!(
        app.shown(),
        right_to_left(&["سلام world bar".to_string()], 40)
    );
}

#[test]
fn a_key_that_deletes_to_the_end_of_the_line_leaves_the_rest_in_order() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world foo");
    // To the right over «world», onto the space before «foo».
    for _ in 0..6 {
        app.presses(Key::Right);
    }

    app.presses(Key::DeleteToEnd);

    assert_eq!(app.shown(), right_to_left(&["سلام world".to_string()], 40));
    assert_eq!(app.cursor(), at(0, left_of(10, 40)));
}

#[test]
fn any_message_the_user_did_not_type_is_shown_as_it_was_written_or_as_the_app_drew_it() {
    // Messages of English and Farsi words, numbers, punctuation and words
    // in brackets, in any order, each put into the box by the app itself.
    // With no key to go by, farcli may lay a message out only when the
    // screen leaves one order for it. It must never show another order
    // than the one the message was written in.
    const WORDS: &[&str] = &[
        "abc",
        "to",
        "config.json",
        "سلام",
        "می\u{200c}شود",
        "پیام",
        "و",
        "-",
        "12",
        "2.5",
        "۳۴",
        "10%",
        "۱۰٪",
        "$5",
        "12-34",
        "5/6",
        "2026-11-23",
        "+2",
        "#1",
        "A1",
        "x",
        "(2.5%)",
        "(abc)",
        "(پیام)",
        "(12)",
        "(abc پیام)",
        "(سلام to)",
        "[پیام]",
        "«سلام»",
        "\"abc\"",
    ];
    // What may follow a word, with no space between.
    const MARKS: &[&str] = &["", "", "", "", ".", "،", ":", "!", "%"];
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut pick = |from: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % from as u64) as usize
    };

    let mut app = ClaudeCode::start(40, 10);
    let (mut laid_out_from_the_right, mut as_drawn_with_farsi) = (0, 0);
    let mut wrong: Vec<String> = Vec::new();
    for _ in 0..2500 {
        let words: Vec<String> = (0..1 + pick(6))
            .map(|_| format!("{}{}", WORDS[pick(WORDS.len())], MARKS[pick(MARKS.len())]))
            .collect();
        let message = words.join(" ");
        if cells(&message) > 34 {
            continue;
        }

        app.press(Key::Up);
        app.holds(&message);

        let direction = claude_code::direction_of(&message);
        let as_written = format!("{MARK}{}", laid_out(&message, direction, 38));
        let shown = &app.shown()[0];
        if app.is_shown_as_drawn() {
            as_drawn_with_farsi += usize::from(*shown != as_written);
        } else if *shown == as_written {
            laid_out_from_the_right += usize::from(direction == RightToLeft);
        } else {
            wrong.push(format!("{message:?} is shown as {shown:?}"));
        }
    }

    assert!(
        wrong.is_empty(),
        "{} wrong:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    // Both happen often: many messages have one order only, and many do not.
    assert!(laid_out_from_the_right > 300, "{laid_out_from_the_right}");
    assert!(as_drawn_with_farsi > 300, "{as_drawn_with_farsi}");
}

#[test]
fn any_message_typed_key_by_key_reads_as_typed_after_every_key() {
    // The same kinds of messages, typed. With the keys to go by, every one
    // of them is laid out, on every row it wraps onto, after every key.
    const WORDS: &[&str] = &[
        "abc",
        "to",
        "config.json",
        "سلام",
        "می\u{200c}شود",
        "پیام",
        "و",
        "-",
        "12",
        "2.5",
        "۳۴",
        "10%",
        "$5",
        "12-34",
        "5/6",
        "+2",
        "x",
        "(2.5%)",
        "(abc)",
        "(پیام)",
        "(abc پیام)",
        "(سلام to)",
        "[پیام]",
        "«سلام»",
    ];
    const MARKS: &[&str] = &["", "", "", "", ".", "،", ":", "!", "%"];
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut pick = |from: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % from as u64) as usize
    };

    let mut app = ClaudeCode::start(40, 12);
    let (mut keys, mut wrapped) = (0, 0);
    for _ in 0..120 {
        let words: Vec<String> = (0..1 + pick(12))
            .map(|_| format!("{}{}", WORDS[pick(WORDS.len())], MARKS[pick(MARKS.len())]))
            .collect();
        let message = words.join(" ");

        for character in message.chars() {
            app.presses(Key::Character(character));
            keys += 1;

            // The message takes the direction of its first letter, once it
            // has one.
            let so_far: String = message.chars().take(keys).collect();
            let direction = claude_code::direction_of(&so_far);
            let as_typed: Vec<String> = app
                .rows_as_typed()
                .iter()
                .enumerate()
                .map(|(row, typed)| {
                    let margin = if row == 0 { MARK } else { "  " };
                    format!("{margin}{}", laid_out(typed, direction, 38))
                        .trim_end()
                        .to_string()
                })
                .collect();
            assert_eq!(app.shown(), as_typed, "{message:?} after {so_far:?}");
            wrapped += usize::from(as_typed.len() > 1);
        }
        keys = 0;
        app.presses(Key::Enter);
    }
    assert!(wrapped > 300, "{wrapped}");
}
