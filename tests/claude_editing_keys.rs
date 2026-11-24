//! The editing keys in Claude Code's Input box: Left and Right move the
//! cursor the way it looks on screen, and Backspace and Delete remove the
//! character a reader of Farsi expects.
//!
//! Every test goes through the session and looks at three things: what the
//! Terminal shows, where its cursor is, and what the app was sent. The app
//! is a stand-in that keeps its message and draws its Input box as Claude
//! Code was seen to (`claude_code::input_box`): it moves its own cursor in
//! Logical order, and handles the keys farcli forwards, not the keys the
//! user pressed.

mod claude_code;

use claude_code::input_box::{ClaudeCode, Key};
use claude_code::{at, cells, laid_out};
use farcli::layout::Direction::{LeftToRight, RightToLeft};

/// The prompt mark and the no-break space the app draws after it.
const MARK: &str = "❯\u{a0}";

/// What Windows Terminal sends for `key`, which is also what the app is
/// sent when farcli forwards the key as it was pressed.
fn as_pressed(key: Key) -> String {
    key.sent_by_the_terminal()
}

/// The rows of a message as farcli should show them in a Terminal `cols`
/// wide: each row of `as_typed` laid out in the message's columns, right to
/// left.
fn right_to_left(as_typed: &[&str], cols: usize) -> Vec<String> {
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

/// The user presses `key` `times` over, the app handling each before the
/// next. Returns where the cursor was after each.
fn pressing(app: &mut ClaudeCode, key: Key, times: usize) -> Vec<(u16, u16)> {
    (0..times)
        .map(|_| {
            app.presses(key.clone());
            (app.cursor().row, app.cursor().col)
        })
        .collect()
}

/// The places on one row from `from` to `to`, one column at a time.
fn columns(row: u16, from: u16, to: u16) -> Vec<(u16, u16)> {
    if from <= to {
        (from..=to).map(|col| (row, col)).collect()
    } else {
        (to..=from).rev().map(|col| (row, col)).collect()
    }
}

#[test]
fn left_and_right_move_the_cursor_left_and_right_in_a_farsi_word() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام");
    // The cursor is on the empty cell left of the word, where the next
    // letter will appear.
    assert_eq!(app.cursor(), at(0, 35));

    let forwarded = app.presses(Key::Right);

    // It is on the last letter now. The app moves its cursor in Logical
    // order, so it was sent the key that goes back.
    assert_eq!(app.cursor(), at(0, 36));
    assert_eq!(forwarded, as_pressed(Key::Left));

    app.presses(Key::Right);
    assert_eq!(app.cursor(), at(0, 37));

    let forwarded = app.presses(Key::Left);

    assert_eq!(app.cursor(), at(0, 36));
    assert_eq!(forwarded, as_pressed(Key::Right));
}

#[test]
fn in_an_english_message_the_arrows_reach_the_app_as_they_were_pressed() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("hello world 42");

    for key in [Key::Left, Key::Left, Key::Left, Key::Right, Key::Left] {
        let forwarded = app.presses(key.clone());

        assert_eq!(forwarded, as_pressed(key));
        assert!(app.is_shown_as_drawn());
    }
    // Three to the left of the end, in the app's own column.
    assert_eq!(app.cursor(), at(0, 2 + 14 - 3));
    assert_eq!(app.character_under_the_cursor(), ' ');
}

#[test]
fn the_cursor_crosses_an_english_word_in_a_farsi_message_one_cell_at_a_time() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world خوبی");
    assert_eq!(app.shown(), right_to_left(&["سلام world خوبی"], 40));
    // On screen, from the left: an empty cell where the cursor is, «خوبی»,
    // a space, `world` reading from the left, a space, «سلام».
    let end = left_of(15, 40);
    assert_eq!(app.cursor(), at(0, end));

    // Right, to the start of the message at the right edge: one cell each
    // time, over Farsi and English alike.
    let went = pressing(&mut app, Key::Right, 15);
    assert_eq!(went, columns(0, end + 1, 39));

    // And Left, all the way back.
    let went = pressing(&mut app, Key::Left, 15);
    assert_eq!(went, columns(0, 38, end));
    assert_eq!(app.shown(), right_to_left(&["سلام world خوبی"], 40));
}

#[test]
fn at_the_edge_between_farsi_and_english_the_app_is_sent_as_many_keys_as_the_step_takes() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world خوبی");
    // Onto the space left of `world`, which in Logical order is the space
    // after it.
    pressing(&mut app, Key::Right, 5);
    assert_eq!(app.character_under_the_cursor(), ' ');

    // One cell to the right is the `w`. In Logical order that is five
    // characters back.
    let forwarded = app.presses(Key::Right);

    assert_eq!(app.character_under_the_cursor(), 'w');
    assert_eq!(forwarded, as_pressed(Key::Left).repeat(5));

    // Inside the English word, right is onwards: the key as pressed.
    let forwarded = app.presses(Key::Right);

    assert_eq!(app.character_under_the_cursor(), 'o');
    assert_eq!(forwarded, as_pressed(Key::Right));

    // From the `d` to the space on its right, which in Logical order is
    // the space before the word: five characters back again.
    pressing(&mut app, Key::Right, 3);
    assert_eq!(app.character_under_the_cursor(), 'd');
    let column = app.cursor().col;

    let forwarded = app.presses(Key::Right);

    assert_eq!(app.cursor(), at(0, column + 1));
    assert_eq!(app.character_under_the_cursor(), ' ');
    assert_eq!(forwarded, as_pressed(Key::Left).repeat(5));

    // A letter typed there goes after «سلام», before the space.
    app.types("ت");
    assert_eq!(app.shown(), right_to_left(&["سلامت world خوبی"], 40));
}

#[test]
fn the_cursor_crosses_a_number_in_a_farsi_message_one_cell_at_a_time() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("قیمت 2500 تومان");
    let end = left_of(15, 40);

    let went = pressing(&mut app, Key::Right, 15);

    assert_eq!(went, columns(0, end + 1, 39));

    // Left again, onto the first digit of the number: it reads from the
    // left, so that is its left end.
    pressing(&mut app, Key::Left, 8);
    assert_eq!(app.character_under_the_cursor(), '2');
    assert_eq!(app.cursor(), at(0, end + 7));

    // A digit typed there goes before it.
    app.types("1");
    assert_eq!(app.shown(), right_to_left(&["قیمت 12500 تومان"], 40));
}

#[test]
fn the_cursor_crosses_a_farsi_word_in_an_english_message_one_cell_at_a_time() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("open فایل now");
    let shown = format!("{MARK}{}", laid_out("open فایل now", LeftToRight, 38));
    assert_eq!(app.shown(), std::slice::from_ref(&shown));
    // A left-to-right row: the cursor is after the text.
    assert_eq!(app.cursor(), at(0, 2 + 13));

    let went = pressing(&mut app, Key::Left, 13);
    assert_eq!(went, columns(0, 2 + 12, 2));

    let went = pressing(&mut app, Key::Right, 13);
    assert_eq!(went, columns(0, 3, 2 + 13));
    assert_eq!(app.shown(), [shown]);
}

#[test]
fn in_an_english_message_with_a_farsi_word_the_arrows_are_swapped_inside_that_word_only() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("open فایل now");

    // Over `now` and the space before it: the keys as pressed.
    for _ in 0..4 {
        assert_eq!(app.presses(Key::Left), as_pressed(Key::Left));
    }
    // One cell to the left is the last letter of «فایل» in Screen order,
    // which is its first: four characters back.
    assert_eq!(app.presses(Key::Left), as_pressed(Key::Left).repeat(4));
    // Inside the Farsi word, left is onwards.
    assert_eq!(app.presses(Key::Left), as_pressed(Key::Right));
}

#[test]
fn at_either_end_of_the_message_an_arrow_that_points_out_of_it_moves_nothing() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام دنیا");
    let end = at(0, left_of(9, 40));
    assert_eq!(app.cursor(), end);

    // Left of the end of a Farsi message there is nothing. The app is sent
    // the key that goes on, which does nothing there: the key as pressed
    // would take its cursor back, to the right.
    let forwarded = app.presses(Key::Left);

    assert_eq!(app.cursor(), end);
    assert_eq!(forwarded, as_pressed(Key::Right));

    pressing(&mut app, Key::Right, 9);
    assert_eq!(app.cursor(), at(0, 39));

    let forwarded = app.presses(Key::Right);

    assert_eq!(app.cursor(), at(0, 39));
    assert_eq!(forwarded, as_pressed(Key::Left));
    assert_eq!(app.shown(), right_to_left(&["سلام دنیا"], 40));
}

#[test]
fn past_the_left_end_of_a_wrapped_row_the_cursor_goes_to_the_right_end_of_the_row_below() {
    let mut app = ClaudeCode::start(30, 12);
    app.types("یک دو سه چهار پنج شش هفت هشت");
    let rows = app.rows_as_typed();
    assert_eq!(rows.len(), 2);
    let (first, second) = (cells(&rows[0]), cells(&rows[1]));
    assert_eq!(app.cursor(), at(1, left_of(second, 30)));

    // Right, over the whole second row to its right end, where it starts.
    let went = pressing(&mut app, Key::Right, second);
    assert_eq!(went, columns(1, left_of(second, 30) + 1, 29));

    // One more: before the start of the second row comes the end of the
    // first, which is at its left end.
    app.presses(Key::Right);
    assert_eq!(app.cursor(), at(0, left_of(first, 30)));

    // On over the first row.
    app.presses(Key::Right);
    assert_eq!(app.cursor(), at(0, left_of(first, 30) + 1));

    // And back the same way: Left at the left end of the first row goes to
    // the right end of the second.
    let went = pressing(&mut app, Key::Left, 3);
    assert_eq!(went, [(0, left_of(first, 30)), (1, 29), (1, 28)]);
    let typed: Vec<&str> = rows.iter().map(String::as_str).collect();
    assert_eq!(app.shown(), right_to_left(&typed, 30));
}

#[test]
fn each_line_of_a_message_is_crossed_in_its_own_direction() {
    let mut app = ClaudeCode::start(40, 12);
    app.types("خط یک");
    app.presses(Key::LineBreak);
    app.types("line two");
    app.presses(Key::LineBreak);
    app.types("خط سه");
    assert_eq!(app.cursor(), at(2, left_of(5, 40)));

    // Right over the third line, which is Farsi, to its start at the right
    // edge.
    let went = pressing(&mut app, Key::Right, 5);
    assert_eq!(went, columns(2, left_of(5, 40) + 1, 39));

    // Before it comes the end of the English line, which is that line's
    // right end.
    app.presses(Key::Right);
    assert_eq!(app.cursor(), at(1, 2 + 8));

    // Left over the English line, the keys as pressed, to its start at the
    // left edge.
    for column in (2..2 + 8).rev() {
        assert_eq!(app.presses(Key::Left), as_pressed(Key::Left));
        assert_eq!(app.cursor(), at(1, column));
    }

    // Before it comes the end of the first line, which is Farsi: its left
    // end.
    app.presses(Key::Left);
    assert_eq!(app.cursor(), at(0, left_of(5, 40)));

    // Left of the end of a Farsi line is the line after it.
    app.presses(Key::Left);
    assert_eq!(app.cursor(), at(1, 2));

    // And Right at the right end of the English line goes on to the third.
    pressing(&mut app, Key::Right, 8);
    assert_eq!(app.cursor(), at(1, 2 + 8));
    app.presses(Key::Right);
    assert_eq!(app.cursor(), at(2, 39));
}

#[test]
fn backspace_deletes_the_character_just_typed() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام دنیاا");

    let forwarded = app.presses(Key::Backspace);

    assert_eq!(forwarded, as_pressed(Key::Backspace));
    assert_eq!(app.shown(), right_to_left(&["سلام دنیا"], 40));
    assert_eq!(app.cursor(), at(0, left_of(9, 40)));

    // The same after an English word, whose last letter is at its right
    // end, away from the cursor.
    app.types(" worlds");

    let forwarded = app.presses(Key::Backspace);

    assert_eq!(forwarded, as_pressed(Key::Backspace));
    assert_eq!(app.shown(), right_to_left(&["سلام دنیا world"], 40));
    assert_eq!(app.cursor(), at(0, left_of(15, 40)));
}

#[test]
fn in_a_farsi_word_delete_removes_the_letter_under_the_cursor_and_backspace_the_one_on_its_right() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام");
    // Onto the «ا»: «م» is on its left, «ل» on its right.
    pressing(&mut app, Key::Right, 2);
    assert_eq!(app.cursor(), at(0, 37));

    let forwarded = app.presses(Key::Delete);

    // The letter after the cursor in reading order is the one it is on.
    assert_eq!(forwarded, as_pressed(Key::Delete));
    assert_eq!(app.shown(), right_to_left(&["سلم"], 40));
    assert_eq!(app.cursor(), at(0, 37));

    let forwarded = app.presses(Key::Backspace);

    // The letter before it in reading order is on its right.
    assert_eq!(forwarded, as_pressed(Key::Backspace));
    assert_eq!(app.shown(), right_to_left(&["سم"], 40));
    assert_eq!(app.cursor(), at(0, 38));
}

#[test]
fn in_an_english_word_of_a_farsi_message_delete_removes_the_letter_under_the_cursor_and_backspace_the_one_on_its_left(
) {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world");
    // Onto the `r`.
    pressing(&mut app, Key::Right, 3);
    assert_eq!(app.character_under_the_cursor(), 'r');

    app.presses(Key::Delete);

    assert_eq!(app.shown(), right_to_left(&["سلام wold"], 40));
    assert_eq!(app.character_under_the_cursor(), 'l');

    app.presses(Key::Backspace);

    assert_eq!(app.shown(), right_to_left(&["سلام wld"], 40));
    assert_eq!(app.character_under_the_cursor(), 'l');
}

#[test]
fn with_ctrl_held_the_arrows_go_a_word_to_the_side_they_point_to() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world خوبی");
    let end = left_of(15, 40);

    // A word to the right: the app is sent the key that goes back a word,
    // to the start of «خوبی».
    let forwarded = app.presses(Key::CtrlRight);

    assert_eq!(forwarded, as_pressed(Key::CtrlLeft));
    assert_eq!(app.cursor(), at(0, end + 4));

    // Again: to the start of `world`, which is its left end.
    app.presses(Key::CtrlRight);
    assert_eq!(app.character_under_the_cursor(), 'w');

    // A word to the left: to the end of `world` in reading order, which is
    // shown on the space left of it.
    let forwarded = app.presses(Key::CtrlLeft);

    assert_eq!(forwarded, as_pressed(Key::CtrlRight));
    assert_eq!(app.cursor(), at(0, end + 5));
    assert_eq!(app.shown(), right_to_left(&["سلام world خوبی"], 40));
}

#[test]
fn with_ctrl_held_the_arrows_reach_the_app_as_pressed_in_an_english_message() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("hello big world");

    assert_eq!(app.presses(Key::CtrlLeft), as_pressed(Key::CtrlLeft));
    assert_eq!(app.cursor(), at(0, 2 + 10));
    assert_eq!(app.presses(Key::CtrlLeft), as_pressed(Key::CtrlLeft));
    assert_eq!(app.presses(Key::CtrlRight), as_pressed(Key::CtrlRight));
    assert_eq!(app.cursor(), at(0, 2 + 9));
    assert!(app.is_shown_as_drawn());
}

#[test]
fn with_shift_held_the_arrows_move_the_cursor_as_they_do_alone() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام");

    // The app takes an arrow with Shift for the arrow, so farcli does too,
    // and Shift stays held on the key the app is sent.
    let forwarded = app.presses(Key::ShiftRight);

    assert_eq!(forwarded, as_pressed(Key::ShiftLeft));
    assert_eq!(app.cursor(), at(0, 36));

    let forwarded = app.presses(Key::ShiftLeft);

    assert_eq!(forwarded, as_pressed(Key::ShiftRight));
    assert_eq!(app.cursor(), at(0, 35));
}

#[test]
fn in_a_box_shown_as_the_app_drew_it_the_arrows_reach_the_app_as_pressed() {
    let mut app = ClaudeCode::start(40, 10);
    // A message the user did not type, whose order the screen does not
    // settle: farcli shows it as drawn, in the app's own order.
    app.press(Key::Up);
    app.holds("سلام world");
    assert!(app.is_shown_as_drawn());

    for key in [
        Key::Left,
        Key::Left,
        Key::Right,
        Key::CtrlLeft,
        Key::ShiftRight,
    ] {
        let forwarded = app.presses(key.clone());

        assert_eq!(forwarded, as_pressed(key));
        assert!(app.is_shown_as_drawn());
    }
}

#[test]
fn in_the_box_that_runs_a_command_the_arrows_reach_the_app_as_pressed() {
    let mut app = ClaudeCode::start(40, 10);
    app.mark = "\x1b[38;2;253;93;177m!\x1b[39m ".to_string();
    app.types("echo سلام");

    for key in [Key::Left, Key::Right, Key::Left, Key::Left] {
        let forwarded = app.presses(key.clone());

        assert_eq!(forwarded, as_pressed(key));
        assert!(app.is_shown_as_drawn());
    }
}

#[test]
fn in_an_empty_box_the_arrows_reach_the_app_as_pressed() {
    let mut app = ClaudeCode::start(40, 10);
    // The app's own hint, in Farsi.
    app.hint = claude_code::as_drawn("یک آزمون بنویس");
    app.draws();

    for key in [Key::Left, Key::Right] {
        assert_eq!(app.presses(key.clone()), as_pressed(key));
    }

    // And after a message is sent, when the box is empty again.
    app.types("سلام");
    app.presses(Key::Enter);

    for key in [Key::Left, Key::Right] {
        assert_eq!(app.presses(key.clone()), as_pressed(key));
    }
}

#[test]
fn while_the_app_asks_something_else_the_arrows_reach_it_as_pressed() {
    let mut app = ClaudeCode::start(40, 12);
    app.types("سلام دنیا");
    pressing(&mut app, Key::Right, 3);

    // The app puts a question of its own where the box was. Left and Right
    // may choose between its answers.
    app.run.app_writes(format!(
        "\x1b[?2026h\x1b[3;1H\x1b[J{}\r\n Do you want to proceed?\r\n ❯ 1. Yes\r\n   2. No\x1b[?2026l",
        "─".repeat(40)
    ));
    assert!(app.is_shown_as_drawn());

    for key in [Key::Left, Key::Right, Key::Right, Key::CtrlLeft] {
        assert_eq!(app.press(key.clone()), as_pressed(key));
    }
    app.takes_the_keys_for_something_else();

    // The box is back, the cursor where it was, and the arrows mean sides
    // of the screen again.
    app.draws();
    assert_eq!(app.cursor(), at(0, left_of(9, 40) + 3));

    app.presses(Key::Right);

    assert_eq!(app.cursor(), at(0, left_of(9, 40) + 4));
}

#[test]
fn while_the_app_hides_the_cursor_the_arrows_reach_it_as_pressed() {
    let mut app = ClaudeCode::start(40, 10);
    app.shows_cursor = false;
    app.types("سلام");

    // With no cursor in the box there is no saying the keys are for it.
    for key in [Key::Left, Key::Right] {
        assert_eq!(app.presses(key.clone()), as_pressed(key));
    }
}

#[test]
fn every_other_key_reaches_the_app_as_it_was_pressed() {
    let mut app = ClaudeCode::start(40, 12);
    app.types("سلام world خوبی");
    pressing(&mut app, Key::Right, 7);

    // Keys of the stand-in, and then keys as Windows Terminal sends them:
    // Tab, Shift and Tab, F5, Page Up, Insert, Ctrl with A, E, B, F, R and
    // C, Alt with B and F, a turn of the mouse wheel, the Terminal's
    // answer to a question of the app's.
    let keys = [
        Key::Up,
        Key::Down,
        Key::Home,
        Key::End,
        Key::Escape,
        Key::Delete,
        Key::Backspace,
        Key::Character('ب'),
        Key::Character('x'),
        Key::Character(' '),
        Key::Character('\u{200c}'),
        Key::DeleteWord,
        Key::DeleteToEnd,
        Key::LineBreak,
        Key::Paste("متن pasted".to_string()),
    ];
    let mut sent: Vec<String> = keys.iter().map(Key::sent_by_the_terminal).collect();
    let key = |number: u32, scan: u32, character: u32, held: u32| {
        format!(
            "\x1b[{number};{scan};{character};1;{held};1_\x1b[{number};{scan};{character};0;{held};1_"
        )
    };
    let with = |holder: &str, keys: String| match holder {
        "ctrl" => format!("\x1b[17;29;0;1;8;1_{keys}\x1b[17;29;0;0;0;1_"),
        "alt" => format!("\x1b[18;56;0;1;2;1_{keys}\x1b[18;56;0;0;0;1_"),
        _ => format!("\x1b[16;42;0;1;16;1_{keys}\x1b[16;42;0;0;0;1_"),
    };
    sent.extend([
        key(9, 15, 9, 0),
        with("shift", key(9, 15, 9, 16)),
        key(116, 63, 0, 0),
        key(33, 73, 0, 256),
        key(45, 82, 0, 256),
        with("ctrl", key(65, 30, 1, 8)),
        with("ctrl", key(69, 18, 5, 8)),
        with("ctrl", key(66, 48, 2, 8)),
        with("ctrl", key(70, 33, 6, 8)),
        with("ctrl", key(82, 19, 18, 8)),
        with("ctrl", key(67, 46, 3, 8)),
        with("alt", key(66, 48, 98, 2)),
        with("alt", key(70, 33, 102, 2)),
        with("ctrl", key(38, 72, 0, 256 | 8)),
        with("shift", key(36, 71, 0, 256 | 16)),
        "\x1b[<64;10;5M".to_string(),
        "\x1b[?61c".to_string(),
        "\x1b[I".to_string(),
    ]);

    for sent in sent {
        let forwarded = app.press_as(&sent);
        app.handles(1);

        assert_eq!(forwarded, sent);
    }
}

#[test]
fn an_arrow_key_held_down_moves_the_cursor_again_each_time_it_repeats() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام دنیا");
    let end = left_of(9, 40);
    let (down, up) = ("\x1b[39;77;0;1;256;1_", "\x1b[39;77;0;0;256;1_");

    // Right goes down three times and comes up once.
    for column in end + 1..=end + 3 {
        let forwarded = app.press_as(down);
        app.handles(1);

        // Each time the app is sent Left, going down and coming up.
        assert_eq!(forwarded, as_pressed(Key::Left));
        assert_eq!(app.cursor(), at(0, column));
    }
    // So when Right comes up there is nothing left to send.
    assert_eq!(app.press_as(up), "");
    app.handles(1);

    // A key that says it repeated three times is three presses.
    let forwarded = app.press_as("\x1b[39;77;0;1;256;3_");
    app.handles(1);

    assert_eq!(forwarded, as_pressed(Key::Left).repeat(3));
    assert_eq!(app.cursor(), at(0, end + 6));
    assert_eq!(app.press_as(up), "");
}

#[test]
fn an_arrow_key_held_down_in_an_english_message_is_forwarded_as_it_came() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("hello");
    let (down, up) = ("\x1b[37;75;0;1;256;1_", "\x1b[37;75;0;0;256;1_");

    for sent in [down, down, down, up, "\x1b[37;75;0;1;256;2_", up] {
        assert_eq!(app.press_as(sent), sent);
    }
    app.handles(6);
    assert_eq!(app.cursor(), at(0, 2));
}

#[test]
fn arrows_pressed_before_the_app_has_drawn_the_ones_before_them_still_go_one_cell_each() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world خوبی");
    let end = left_of(15, 40);

    // Seven times Right while the app is busy: over «خوبی», the space, and
    // into `world`. Each is worked out from where the ones before it will
    // have left the cursor.
    for _ in 0..7 {
        app.press(Key::Right);
    }
    app.handles(7);

    assert_eq!(app.cursor(), at(0, end + 7));
    assert_eq!(app.character_under_the_cursor(), 'o');

    // The app draws when it has handled only some of them.
    for _ in 0..6 {
        app.press(Key::Left);
    }
    app.handles(2);
    assert_eq!(app.cursor(), at(0, end + 5));
    app.handles(4);
    assert_eq!(app.cursor(), at(0, end + 1));
}

#[test]
fn an_arrow_pressed_right_after_a_letter_the_app_has_not_drawn_yet_counts_that_letter() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world");

    // Two letters and Right, all before the app draws. One cell right of
    // the end is the `w`, and with the two letters that is seven
    // characters back.
    app.press_keys("ly");
    let forwarded = app.press(Key::Right);
    app.handles(3);

    assert_eq!(forwarded, as_pressed(Key::Left).repeat(7));
    assert_eq!(app.shown(), right_to_left(&["سلام worldly"], 40));
    assert_eq!(app.character_under_the_cursor(), 'w');

    // A Farsi letter and Left: the letter goes before the `w`, and left
    // of the `w` is then the space after the word in reading order, at
    // the end of the message.
    app.press_keys("و");
    let forwarded = app.press(Key::Left);
    app.handles(2);

    assert_eq!(forwarded, as_pressed(Key::Right).repeat(7));
    assert_eq!(app.shown(), right_to_left(&["سلام وworldly"], 40));
    assert_eq!(app.cursor(), at(0, left_of(13, 40)));
}

#[test]
fn the_arrow_codes_of_a_terminal_that_sends_no_windows_keys_are_swapped_like_the_keys() {
    let mut app = ClaudeCode::start(40, 10);
    for character in "سلام world".chars() {
        app.press_as(&character.to_string());
        app.handles(1);
    }
    let end = left_of(10, 40);

    // Right, onto the `w`: five characters back.
    let forwarded = app.press_as("\x1b[C");
    app.handles(1);

    assert_eq!(forwarded, "\x1b[D".repeat(5));
    assert_eq!(app.cursor(), at(0, end + 1));

    // Left, back to the end.
    let forwarded = app.press_as("\x1b[D");
    app.handles(1);

    assert_eq!(forwarded, "\x1b[C".repeat(5));
    assert_eq!(app.cursor(), at(0, end));

    // With Ctrl held: a word to the right is a word back.
    let forwarded = app.press_as("\x1b[1;5C");
    app.handles(1);

    assert_eq!(forwarded, "\x1b[1;5D");
    assert_eq!(app.cursor(), at(0, end + 1));
}
