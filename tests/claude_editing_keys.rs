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

/// What the Terminal should show for the message the app holds: each row
/// the app wrapped it onto, laid out in the direction of its line. With no
/// message, the app's own hint.
fn as_the_app_holds_it(app: &ClaudeCode, cols: usize) -> Vec<String> {
    if app.holds_nothing() {
        return vec![format!("{MARK}{}", app.hint)];
    }
    let rows = app.rows_as_typed();
    let directions = app.directions();
    rows.iter()
        .zip(directions)
        .enumerate()
        .map(|(row, (typed, direction))| {
            let margin = if row == 0 { MARK } else { "  " };
            format!("{margin}{}", laid_out(typed, direction, cols - 2))
                .trim_end_matches(' ')
                .to_string()
        })
        .collect()
}

#[test]
fn after_any_sequence_of_editing_keys_the_text_shown_is_the_text_the_app_holds() {
    // Messages of Farsi and English words, numbers and punctuation, typed
    // key by key. Then keys at random: the arrows alone and with Ctrl,
    // Backspace, Delete, Home, End, letters, digits and spaces typed
    // wherever the cursor has got to, now and then a line break, a paste,
    // or a key that deletes a word or the rest of the line. Some come one
    // at a time, some several before the app draws.
    const WORDS: &[&str] = &[
        "abc",
        "to",
        "config.json",
        "سلام",
        "می\u{200c}شود",
        "پیام",
        "و",
        "12",
        "2.5",
        "۳۴",
        "10%",
        "x",
        "(abc)",
        "(پیام)",
        "[پیام]",
        "«سلام»",
    ];
    const TYPED: &[char] = &['ب', 'ی', 'ک', 'a', 'z', '7', ' ', '.', 'ن', 'e'];
    let mut state: u64 = 0x1234_5678_9abc_def1;
    let mut pick = |from: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % from as u64) as usize
    };
    let cols = 40;
    let position = |app: &ClaudeCode| (app.cursor().row, app.cursor().col);

    let mut app = ClaudeCode::start(cols as u16, 16);
    let (mut arrows, mut moved_a_cell, mut changed_row, mut right_to_left_rows) = (0, 0, 0, 0);
    for _ in 0..150 {
        let words: Vec<&str> = (0..1 + pick(10))
            .map(|_| WORDS[pick(WORDS.len())])
            .collect();
        let message = words.join(" ");
        app.types(&message);
        assert_eq!(app.shown(), as_the_app_holds_it(&app, cols), "{message:?}");

        // The keys so far, as a test would press them.
        let mut pressed: Vec<String> = Vec::new();
        let mut line_breaks = 0;
        for _ in 0..40 {
            let key = match pick(100) {
                0..=27 => Key::Left,
                28..=55 => Key::Right,
                56..=63 => Key::Backspace,
                64..=70 => Key::Delete,
                71..=74 => Key::CtrlLeft,
                75..=78 => Key::CtrlRight,
                79..=80 => Key::Home,
                81..=82 => Key::End,
                83 if line_breaks < 2 => {
                    line_breaks += 1;
                    Key::LineBreak
                }
                84 => Key::DeleteWord,
                85 => Key::DeleteToEnd,
                86 => Key::Paste("دو word".to_string()),
                _ => Key::Character(TYPED[pick(TYPED.len())]),
            };
            // One time in five, one or two more keys come before the app
            // draws.
            if pick(5) == 0 {
                let mut together = vec![key];
                for _ in 0..1 + pick(2) {
                    let key = [Key::Left, Key::Right, Key::Character('م'), Key::Backspace][pick(4)]
                        .clone();
                    together.push(key);
                }
                pressed.push(format!("{together:?} before the app draws"));
                let said = format!("{message:?}, then {}", pressed.join(", "));
                for key in &together {
                    app.press(key.clone());
                }
                // The app draws when it has handled the first of them, and
                // again when it has handled them all.
                app.handles(1);
                assert_eq!(
                    app.shown(),
                    as_the_app_holds_it(&app, cols),
                    "{said}, when the app had handled the first of the last"
                );
                app.handles(together.len() - 1);
                assert_eq!(app.shown(), as_the_app_holds_it(&app, cols), "{said}");
                continue;
            }
            pressed.push(format!("{key:?}"));
            let said = format!("{message:?}, then {}", pressed.join(", "));

            let before = position(&app);
            let forwarded = app.presses(key.clone());
            let after = position(&app);

            // The text shown is the text the app holds, row by row.
            assert_eq!(app.shown(), as_the_app_holds_it(&app, cols), "{said}");
            match key {
                Key::Left | Key::Right => {
                    // The cursor went one cell to the side the key points
                    // to, or to the row above or below, or it is at the
                    // edge of the first row or the last and stayed.
                    arrows += 1;
                    let one_cell = match key {
                        Key::Left => (before.0, before.1.wrapping_sub(1)),
                        _ => (before.0, before.1 + 1),
                    };
                    if after == one_cell {
                        moved_a_cell += 1;
                    } else if after.0 != before.0 {
                        assert_eq!(after.0.abs_diff(before.0), 1, "{said}");
                        changed_row += 1;
                    } else {
                        assert_eq!(after, before, "{said}");
                        let last = app.rows_as_typed().len() - 1;
                        assert!(after.0 == 0 || usize::from(after.0) == last, "{said}");
                    }
                }
                // With Ctrl held the app is sent one of the two, with Ctrl
                // held.
                Key::CtrlLeft | Key::CtrlRight => assert!(
                    [as_pressed(Key::CtrlLeft), as_pressed(Key::CtrlRight)].contains(&forwarded),
                    "{said}"
                ),
                // Every other key reaches the app as it was pressed.
                key => assert_eq!(forwarded, as_pressed(key), "{said}"),
            }
            right_to_left_rows += app
                .directions()
                .iter()
                .filter(|direction| **direction == RightToLeft)
                .count();
        }
        app.presses(Key::Enter);
    }
    // The arrows were pressed often, mostly moved the cursor one cell, and
    // now and then took it to another row; and many rows were right to left.
    assert!(arrows > 2000, "{arrows}");
    assert!(moved_a_cell > 1500, "{moved_a_cell}");
    assert!(changed_row > 30, "{changed_row}");
    assert!(right_to_left_rows > 3000, "{right_to_left_rows}");
}

#[test]
fn a_key_that_deletes_right_after_a_word_jump_the_app_has_not_drawn_yet_is_still_followed() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world foo");

    // A word to the right and Backspace, both before the app draws: where
    // a word jump leaves the cursor is the app's business, so farcli has
    // to find where the character went from.
    app.press(Key::CtrlRight);
    app.press(Key::Backspace);
    app.handles(2);

    assert_eq!(app.shown(), right_to_left(&["سلام worldfoo"], 40));
    assert_eq!(app.character_under_the_cursor(), 'f');

    // The same with Home and a letter typed, and the app drawing when it
    // has handled the letter but not the two that follow it.
    app.press(Key::Home);
    app.press_keys("بله");
    app.handles(2);

    assert_eq!(app.shown(), right_to_left(&["بسلام worldfoo"], 40));

    app.handles(2);

    assert_eq!(app.shown(), right_to_left(&["بلهسلام worldfoo"], 40));
    assert_eq!(app.cursor(), at(0, left_of(3, 40)));
}

#[test]
fn backspace_at_the_start_of_a_line_joins_it_to_the_line_before_only_when_the_app_has_drawn_that() {
    let mut app = ClaudeCode::start(40, 12);
    app.types("abc");
    app.presses(Key::LineBreak);
    app.types("می");
    let two_lines = [
        format!("{MARK}abc"),
        format!("  {}", laid_out("می", RightToLeft, 38)),
    ];
    assert_eq!(app.shown(), two_lines);
    // To the start of the second line, at the right edge.
    pressing(&mut app, Key::Right, 2);
    assert_eq!(app.cursor(), at(1, 39));

    // Backspace there deletes the line break. The app draws once more
    // before it has handled the key: two rows still, and two lines. A row
    // this short is not one the app would have wrapped.
    let forwarded = app.press(Key::Backspace);
    app.draws();

    assert_eq!(forwarded, as_pressed(Key::Backspace));
    assert_eq!(app.shown(), two_lines);
    assert_eq!(app.cursor(), at(1, 39));

    // Now it has: one line, which begins in English.
    app.handles(1);

    assert_eq!(
        app.shown(),
        [format!("{MARK}{}", laid_out("abcمی", LeftToRight, 38))]
    );
    // The cursor is before the «م», which is the right one of the two.
    assert_eq!(app.cursor(), at(0, 2 + 4));
}

#[test]
fn at_the_edge_of_the_first_row_an_arrow_that_points_out_of_it_moves_nothing_wherever_the_message_starts(
) {
    let mut app = ClaudeCode::start(40, 10);
    // A Farsi message that starts with a number: the number reads from the
    // left, so the cell at the right edge is its last digit, and the start
    // of the message is one cell further left.
    app.types("12 سلام");
    assert_eq!(app.shown(), right_to_left(&["12 سلام"], 40));
    pressing(&mut app, Key::Right, 7);
    assert_eq!(app.cursor(), at(0, 39));
    assert_eq!(app.character_under_the_cursor(), '2');

    // There is nothing to the right of it. The app's own key for going
    // back would take its cursor to the `1`, to the left: it is sent
    // nothing.
    let forwarded = app.presses(Key::Right);

    assert_eq!(forwarded, "");
    assert_eq!(app.cursor(), at(0, 39));

    // Left goes to the `1`, which is the start of the message.
    let forwarded = app.presses(Key::Left);

    assert_eq!(forwarded, as_pressed(Key::Left));
    assert_eq!(app.cursor(), at(0, 38));
    assert_eq!(app.character_under_the_cursor(), '1');
}

#[test]
fn a_key_that_deletes_is_not_taken_to_be_handled_while_the_app_has_drawn_only_the_jump_before_it() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world");
    app.presses(Key::Home);
    assert_eq!(app.cursor(), at(0, 39));

    // A word to the left and Backspace, and the app draws when it has
    // handled the first of the two only.
    app.press(Key::CtrlLeft);
    app.press(Key::Backspace);
    app.handles(1);

    assert_eq!(app.shown(), right_to_left(&["سلام world"], 40));
    assert_eq!(app.cursor(), at(0, 35));

    // Then the other. The row ends with an English word, so the screen
    // alone would not say what order it is in.
    app.handles(1);

    assert_eq!(app.shown(), right_to_left(&["سلا world"], 40));
    assert_eq!(app.cursor(), at(0, 36));
}

#[test]
fn an_arrow_pressed_right_after_a_key_that_deletes_a_word_does_not_lose_the_message() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world foo");

    // The key that deletes a word and Right, and the app draws when it
    // has handled the first of the two only. The row ends with an English
    // word, so the screen alone would not say what order it is in.
    app.press(Key::DeleteWord);
    app.press(Key::Right);
    app.handles(1);

    assert_eq!(app.shown(), right_to_left(&["سلام world"], 40));
    assert_eq!(app.cursor(), at(0, left_of(11, 40)));

    app.handles(1);

    assert_eq!(app.shown(), right_to_left(&["سلام world"], 40));
    assert_eq!(app.cursor(), at(0, left_of(10, 40)));
}

#[test]
fn a_key_that_deletes_a_space_nobody_can_see_does_not_lose_the_message_later() {
    let mut app = ClaudeCode::start(40, 10);
    // A space at the end of the message is drawn only up to the cursor.
    // With the cursor before it, the key that deletes the rest of the line
    // changes nothing on screen: farcli cannot know the space is gone.
    app.types("سلام world ");
    app.presses(Key::Home);
    app.presses(Key::End);
    app.presses(Key::Right);
    app.presses(Key::DeleteToEnd);
    assert_eq!(app.shown(), right_to_left(&["سلام world"], 40));
    let end = at(0, left_of(10, 40));
    assert_eq!(app.cursor(), end);

    // Left of the end there is nothing.
    app.presses(Key::Left);
    assert_eq!(app.cursor(), end);

    // Right, Left and Backspace before the app draws, and it draws when it
    // has handled the first of them only, and again when it has handled
    // them all. The row ends with an English word: the screen alone would
    // not say what order it is in.
    app.press(Key::Right);
    app.press(Key::Left);
    app.press(Key::Backspace);
    app.handles(1);

    assert_eq!(app.shown(), right_to_left(&["سلام world"], 40));

    app.handles(2);

    assert_eq!(app.shown(), right_to_left(&["سلام worl"], 40));
}

#[test]
fn while_the_correction_is_switched_off_every_arrow_reaches_the_app_as_pressed() {
    let mut app = ClaudeCode::start(40, 10);
    app.types("سلام world");
    let end = left_of(10, 40);
    // Onto the `r`.
    pressing(&mut app, Key::Right, 3);
    assert_eq!(app.character_under_the_cursor(), 'r');

    // F12 switches the correction off. The Terminal shows the box as the
    // app drew it, with the cursor where the app has it: the user now sees
    // a cursor that moves in Logical order.
    let f12 = "\x1b[123;88;0;1;0;1_\x1b[123;88;0;0;0;1_";
    assert_eq!(app.press_as(f12), "");
    app.handles(1);
    assert!(app.is_shown_as_drawn());

    for key in [
        Key::Left,
        Key::Left,
        Key::Right,
        Key::CtrlLeft,
        Key::ShiftRight,
        Key::Left,
    ] {
        let forwarded = app.presses(key.clone());

        assert_eq!(forwarded, as_pressed(key));
        assert!(app.is_shown_as_drawn());
    }
    // The keys of a Terminal that sends no Windows keys, too.
    for sent in ["\x1b[C", "\x1b[D", "\x1b[1;5C", "\x1b[1;5D"] {
        let forwarded = app.press_as(sent);
        app.handles(1);

        assert_eq!(forwarded, sent);
    }

    // On again: the text field, with the cursor where those keys left it,
    // which is before the `w`. The arrows mean sides of the screen again:
    // left of the `w` is the end of the message, five characters on.
    assert_eq!(app.press_as(f12), "");
    app.handles(1);
    assert_eq!(app.shown(), right_to_left(&["سلام world"], 40));
    assert_eq!(app.cursor(), at(0, end + 1));

    let forwarded = app.presses(Key::Left);

    assert_eq!(forwarded, as_pressed(Key::Right).repeat(5));
    assert_eq!(app.cursor(), at(0, end));
}
