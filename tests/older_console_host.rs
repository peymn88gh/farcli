//! Pastes and the mouse wheel reach the Target app in a Terminal that sits
//! on an older console host than Windows Terminal's.
//!
//! Between a Terminal and farcli sits a console host: the Terminal's own, or
//! the one built into Windows. The built-in one of Windows 11 22H2 hands
//! farcli a paste mark or a mouse report spelled out, one key per character.
//! It is also the hidden terminal farcli falls back to. So in these tests
//! the stand-in Terminal is made to use it, and farcli runs once on the
//! hidden terminal it ships and once on the built-in one.
#![cfg(windows)]

mod stand_in;

use std::env;
use std::ffi::OsString;
use std::fs;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use farcli::app_runner::Host;
use farcli::hidden_terminal::HiddenTerminal;
use stand_in::{
    as_marked_paste, farcli, hand_to_app, Reads, PASTED, PASTE_ENDS, PASTE_STARTS, WHEEL_UP,
    WHEEL_UP_AS_WINDOWS_TELLS_IT,
};

/// Not a test of farcli: the stand-in Target app.
#[test]
fn stand_in_app() {
    stand_in::be_the_app_if_asked();
}

/// Which hidden terminal the `farcli` under test runs the app on.
#[derive(Clone, Copy, Debug)]
enum Inside {
    Shipped,
    BuiltIn,
}

const BOTH: [Inside; 2] = [Inside::Shipped, Inside::BuiltIn];

/// Makes the stand-in Terminal's console host the one built into Windows,
/// and the hidden terminal of every `farcli` started from now on `inside`.
/// The tests in this file change the environment, so they run one at a
/// time: each holds what this returns until it ends.
fn on_the_built_in_console_host(inside: Inside) -> MutexGuard<'static, ()> {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    static REAL_FOLDER: OnceLock<Option<OsString>> = OnceLock::new();
    let one_at_a_time = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);

    // The hidden terminal farcli ships is unpacked under the account's
    // local application data. With a file in the way there, whoever looks
    // falls back to the built-in one.
    let real_folder = REAL_FOLDER.get_or_init(|| env::var_os("LOCALAPPDATA"));
    let blocked = env::temp_dir().join("farcli-older-console-host-test");
    fs::create_dir_all(&blocked).unwrap();
    fs::write(blocked.join("farcli"), "in the way").unwrap();
    env::set_var("LOCALAPPDATA", &blocked);
    // This test program looks once and remembers.
    assert_eq!(Host::ready().hidden_terminal(), HiddenTerminal::BuiltIn);
    if let (Inside::Shipped, Some(real_folder)) = (inside, real_folder) {
        env::set_var("LOCALAPPDATA", real_folder);
    }
    one_at_a_time
}

/// Three lines a keyboard with English letters can type.
const THREE_LINES: &str = "first line\rsecond line, with (brackets)\rthird line";

#[test]
fn a_paste_of_several_lines_reaches_an_app_that_reads_what_the_terminal_sent_as_one_marked_paste() {
    for inside in BOTH {
        let _one_at_a_time = on_the_built_in_console_host(inside);

        let outcome = hand_to_app(
            "older-marked",
            Some(farcli()),
            Reads::Raw,
            false,
            &as_marked_paste(THREE_LINES),
        );

        assert_eq!(
            outcome.heard,
            format!("{PASTE_STARTS}{THREE_LINES}{PASTE_ENDS}"),
            "{inside:?}"
        );
    }
}

#[test]
fn the_same_paste_reaches_an_app_that_reads_keys_as_text_with_no_marks() {
    for inside in BOTH {
        let _one_at_a_time = on_the_built_in_console_host(inside);

        let outcome = hand_to_app(
            "older-unmarked",
            Some(farcli()),
            Reads::Keys,
            false,
            &as_marked_paste(THREE_LINES),
        );

        assert_eq!(outcome.heard, THREE_LINES, "{inside:?}");
    }
}

#[test]
fn pasted_farsi_with_half_spaces_reaches_either_kind_of_app_as_it_does_without_farcli() {
    // For a Half-space, Farsi punctuation and Persian digits this console
    // host makes up keys: Alt held down while the character's number is
    // typed.
    for inside in BOTH {
        for (reads, pasted) in [
            (Reads::Raw, format!("{PASTE_STARTS}{PASTED}{PASTE_ENDS}")),
            (Reads::Keys, PASTED.to_string()),
        ] {
            let _one_at_a_time = on_the_built_in_console_host(inside);

            let through_farcli = hand_to_app(
                "older-farsi",
                Some(farcli()),
                reads,
                false,
                &as_marked_paste(PASTED),
            );
            let without_farcli = hand_to_app(
                "older-farsi-straight",
                None,
                reads,
                false,
                &as_marked_paste(PASTED),
            );

            assert_eq!(without_farcli.heard, pasted, "{inside:?}");
            assert_eq!(through_farcli.heard, pasted, "{inside:?}");
        }
    }
}

#[test]
fn a_turn_of_the_wheel_reaches_either_kind_of_app() {
    for inside in BOTH {
        let _one_at_a_time = on_the_built_in_console_host(inside);

        let asked_the_terminal = hand_to_app(
            "older-wheel-raw",
            Some(farcli()),
            Reads::Raw,
            true,
            WHEEL_UP.as_bytes(),
        );
        let asked_windows = hand_to_app(
            "older-wheel-keys",
            Some(farcli()),
            Reads::Keys,
            true,
            WHEEL_UP.as_bytes(),
        );

        assert_eq!(asked_the_terminal.heard, WHEEL_UP, "{inside:?}");
        assert_eq!(
            asked_windows.heard, WHEEL_UP_AS_WINDOWS_TELLS_IT,
            "{inside:?}"
        );
    }
}
