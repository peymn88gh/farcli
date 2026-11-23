//! What the user pastes, types and does with the mouse reaches the Target
//! app as it does without farcli.
//!
//! These tests run the real `farcli` on a hidden terminal with nobody at the
//! keyboard. The test stands in for Windows Terminal: it writes to the
//! hidden terminal what Windows Terminal writes for a paste, a key or a turn
//! of the mouse wheel. A stand-in Target app writes down everything it is
//! given, and the test compares that with what was sent.
#![cfg(windows)]

mod stand_in;

use stand_in::{
    as_keys, as_marked_paste, farcli, hand_to_app, Reads, PASTED, PASTE_ENDS, PASTE_STARTS,
    WHEEL_UP, WHEEL_UP_AS_WINDOWS_TELLS_IT,
};

/// Not a test of farcli: the stand-in Target app.
#[test]
fn stand_in_app() {
    stand_in::be_the_app_if_asked();
}

#[test]
fn a_paste_of_several_lines_reaches_an_app_that_reads_what_the_terminal_sent_as_one_marked_paste() {
    let outcome = hand_to_app(
        "marked",
        Some(farcli()),
        Reads::Raw,
        false,
        &as_marked_paste(PASTED),
    );

    // Between the marks a line break is text, not the key that sends.
    assert_eq!(outcome.heard, format!("{PASTE_STARTS}{PASTED}{PASTE_ENDS}"));
}

#[test]
fn the_same_paste_reaches_an_app_that_reads_keys_as_it_does_without_farcli() {
    let through_farcli = hand_to_app(
        "unmarked",
        Some(farcli()),
        Reads::Keys,
        false,
        &as_marked_paste(PASTED),
    );
    let without_farcli = hand_to_app(
        "unmarked-straight",
        None,
        Reads::Keys,
        false,
        &as_marked_paste(PASTED),
    );

    // Windows keeps the marks from an app that reads keys. It gets the
    // text, and a line break as the Enter key.
    assert_eq!(through_farcli.heard, PASTED);
    assert_eq!(without_farcli.heard, PASTED);
}

/// A paste of more than 5 KiB, the size from which Windows Terminal asks
/// before pasting. Every line is numbered, so a piece that is lost, doubled
/// or moved shows.
fn large_paste() -> String {
    let lines: Vec<&str> = PASTED.split('\r').collect();
    let mut paste = String::new();
    for number in 1.. {
        if paste.len() > 6 * 1024 {
            break;
        }
        let line = lines[number % lines.len()];
        paste.push_str(&format!("{number}: {line}\r"));
    }
    paste
}

#[test]
fn a_paste_large_enough_for_the_terminal_to_ask_first_reaches_either_kind_of_app_whole() {
    let paste = large_paste();

    let read_raw = hand_to_app(
        "large-raw",
        Some(farcli()),
        Reads::Raw,
        false,
        &as_marked_paste(&paste),
    );
    let read_as_keys = hand_to_app(
        "large-keys",
        Some(farcli()),
        Reads::Keys,
        false,
        &as_marked_paste(&paste),
    );

    assert_eq!(read_raw.heard, format!("{PASTE_STARTS}{paste}{PASTE_ENDS}"));
    assert_eq!(read_as_keys.heard, paste);
}

/// Every way an app can ask a Terminal for mouse reports.
const MOUSE_REQUESTS: [&str; 9] = [
    "\x1b[?9h",
    "\x1b[?1000h",
    "\x1b[?1001h",
    "\x1b[?1002h",
    "\x1b[?1003h",
    "\x1b[?1005h",
    "\x1b[?1006h",
    "\x1b[?1015h",
    "\x1b[?1016h",
];

#[test]
fn a_turn_of_the_wheel_reaches_an_app_that_asked_the_terminal_for_mouse_reports() {
    let outcome = hand_to_app(
        "wheel-raw",
        Some(farcli()),
        Reads::Raw,
        true,
        WHEEL_UP.as_bytes(),
    );

    // Claude Code's fullscreen display asks like this and scrolls on what
    // comes back.
    assert!(outcome.terminal_was_asked_for("\x1b[?1000h"));
    assert!(outcome.terminal_was_asked_for("\x1b[?1006h"));
    assert_eq!(outcome.heard, WHEEL_UP);
}

#[test]
fn a_turn_of_the_wheel_reaches_an_app_that_asked_windows_for_the_mouse() {
    let outcome = hand_to_app(
        "wheel-keys",
        Some(farcli()),
        Reads::Keys,
        true,
        WHEEL_UP.as_bytes(),
    );

    // Codex asks Windows, and the hidden terminal asks the Terminal for it.
    assert!(outcome.terminal_was_asked_for("\x1b[?1003h"));
    assert!(outcome.terminal_was_asked_for("\x1b[?1006h"));
    assert_eq!(outcome.heard, WHEEL_UP_AS_WINDOWS_TELLS_IT);
}

#[test]
fn an_app_that_did_not_ask_for_the_mouse_leaves_it_to_the_terminal() {
    let outcome = hand_to_app("no-mouse", Some(farcli()), Reads::Raw, false, b"x");

    // Nothing asked the Terminal for mouse reports, so selecting with the
    // mouse and scrolling back with the wheel stay the Terminal's own.
    for request in MOUSE_REQUESTS {
        assert!(!outcome.terminal_was_asked_for(request), "{request:?}");
    }
    assert_eq!(outcome.heard, "x");
}

#[test]
fn keys_typed_fast_through_farcli_reach_either_kind_of_app_as_the_same_keys() {
    // Typing has no marks around it, however fast it comes: each line break
    // is the Enter key.
    let read_as_keys = hand_to_app(
        "typed-fast",
        Some(farcli()),
        Reads::Keys,
        false,
        &as_keys(PASTED),
    );
    let read_raw = hand_to_app(
        "typed-fast-raw",
        Some(farcli()),
        Reads::Raw,
        false,
        &as_keys(PASTED),
    );

    assert_eq!(read_as_keys.heard, PASTED);
    assert_eq!(read_raw.heard, PASTED);
}

#[test]
fn keys_written_to_the_hidden_terminal_all_at_once_reach_the_app_in_order() {
    let outcome = hand_to_app("straight", None, Reads::Keys, false, &as_keys(PASTED));

    assert_eq!(outcome.heard, PASTED);
}
