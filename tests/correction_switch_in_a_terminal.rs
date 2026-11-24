//! F12 switches the correction off and on in the real `farcli`, running on
//! a hidden terminal with nobody at the keyboard.
//!
//! The test stands in for Windows Terminal: it writes to the hidden
//! terminal the two codes Windows Terminal sends for F12 going down and
//! coming up. They reach `farcli` through the console host it ships, which
//! is the one Windows Terminal 1.25 brings. `correction_switch.rs` holds
//! the rules of the key, tested through the session.
#![cfg(windows)]

mod in_a_terminal;

use in_a_terminal::{Folder, InATerminal, KEY_A, KEY_ENTER, KEY_F12};

/// A program that prints a line of Farsi and then waits for a line to be
/// typed, which it says back. farcli knows nothing about it, so it lays out
/// every row that holds Farsi.
const APP: &str = "@echo off\r\n\
     chcp 65001 >nul\r\n\
     echo سلام دنیا\r\n\
     set /p said=say: \r\n\
     echo heard %said%\r\n";

#[test]
fn f12_shows_what_the_app_drew_and_f12_again_shows_it_corrected_and_the_app_never_hears_of_it() {
    let folder = Folder::new("switch");
    folder.put("says-farsi.cmd", APP);
    let app = folder.0.join("says-farsi.cmd");
    let corrected = format!("{:>110}", "ﺎﯿﻧﺩ ﻡﺎﻠﺳ");
    let mut farcli = InATerminal::run(&folder, &[app.to_str().unwrap()]);

    farcli.wait_until("the app's question", |screen| screen[1] == "say:");
    assert_eq!(farcli.screen()[0], corrected);

    farcli.user_types(KEY_F12);
    farcli.wait_until("the row as the app drew it", |screen| {
        screen[0] == "سلام دنیا"
    });

    farcli.user_types(KEY_F12);
    farcli.wait_until("the row corrected again", |screen| screen[0] == corrected);

    // The app was given the other keys and nothing for F12.
    farcli.user_types(KEY_A);
    farcli.user_types(KEY_F12);
    farcli.user_types(KEY_A);
    farcli.user_types(KEY_F12);
    farcli.user_types(KEY_ENTER);
    let (shown, status, _) = farcli.end();
    assert_eq!(shown, [corrected.as_str(), "say: aa", "heard aa"]);
    assert_eq!(status, 0);
}
