//! What the Terminal sends farcli reaches the Target app's hidden terminal
//! in a form the hidden terminal hands on correctly: keys, pasted text with
//! the marks around it, and mouse reports.
//!
//! The bytes in these tests are what farcli was seen to read from Windows
//! with raw terminal input: from the console host of Windows Terminal 1.25,
//! and from the older one built into Windows 11 22H2. Every test goes
//! through the session.

use farcli::app_profile::AppProfile;
use farcli::hidden_terminal::HiddenTerminal;
use farcli::screen::Position;
use farcli::session::Session;
use farcli::Size;

const PASTE_STARTS: &str = "\x1b[200~";
const PASTE_ENDS: &str = "\x1b[201~";

/// The key `a` going down and coming up, and the Enter key likewise, in
/// Windows' own key report format.
const KEY_A: &str = "\x1b[65;30;97;1;0;1_\x1b[65;30;97;0;0;1_";
const KEY_ENTER: &str = "\x1b[13;28;13;1;0;1_\x1b[13;28;13;0;0;1_";

fn session_on(hidden_terminal: HiddenTerminal) -> Session {
    Session::start_on(
        hidden_terminal,
        AppProfile::plain(),
        Size { cols: 40, rows: 10 },
        Position { row: 0, col: 0 },
    )
}

/// Feeds the session each piece as one lot of what the Terminal sent, and
/// returns what was forwarded to the app after each.
fn forwarded(session: &mut Session, pieces: &[&str]) -> Vec<String> {
    pieces
        .iter()
        .map(|piece| String::from_utf8(forwarded_bytes(session, piece.as_bytes())).unwrap())
        .collect()
}

fn forwarded_bytes(session: &mut Session, piece: &[u8]) -> Vec<u8> {
    let reaction = session.user_typed(piece);
    assert_eq!(reaction.for_terminal, b"");
    reaction.for_app
}

/// How the console host built into Windows hands on a code its Terminal
/// sent, when keys were asked for in the Windows form: spelled out, each
/// character as a key going down that has no key number, and never coming
/// up.
fn spelled_out(code: &str) -> String {
    code.chars()
        .map(|character| format!("\x1b[0;0;{};1;0;1_", u32::from(character)))
        .collect()
}

/// The wheel turned one notch up over the tenth column of the fifth row, as
/// a Terminal reports it to an app that asked the way both Target apps do.
const WHEEL_UP: &str = "\x1b[<64;10;5M";

#[test]
fn a_mouse_report_reaches_the_hidden_terminal_as_the_terminal_wrote_it() {
    let mut session = session_on(HiddenTerminal::Shipped);

    let forwarded = forwarded(&mut session, &[WHEEL_UP, &spelled_out(WHEEL_UP)]);

    // Spelled out, it would reach an app that asked Windows for the mouse,
    // as Codex does, as the typed text `[<64;10;5M`.
    assert_eq!(forwarded, [WHEEL_UP, WHEEL_UP]);
}

#[test]
fn an_answer_from_the_terminal_reaches_the_hidden_terminal_in_the_form_it_came_in() {
    let mut session = session_on(HiddenTerminal::Shipped);
    // What kind of terminal it is, and its background colour.
    let answers = "\x1b[?61;4c\x1b]11;rgb:0c0c/0c0c/0c0c\x1b\\";
    let spelled = spelled_out(answers);

    let forwarded = forwarded(&mut session, &[answers, &spelled]);

    assert_eq!(forwarded, [answers, &spelled]);
}

#[test]
fn what_is_cut_anywhere_reaches_the_hidden_terminal_in_whole_codes() {
    // Windows hands farcli at most 256 characters at a time, so a burst of
    // keys or a large paste is cut wherever that falls. The hidden terminal
    // takes a code that arrives in two writes for typed text.
    let codes = [
        "\x1b[65;30;97;1;0;1_",
        "\x1b[65;30;97;0;0;1_",
        PASTE_STARTS,
        "س",
        "\u{200c}",
        "x",
        "\x1b[13;28;13;1;0;1_",
        "\x1b[13;28;13;0;0;1_",
        "😀",
        WHEEL_UP,
        PASTE_ENDS,
        "\x1b]11;rgb:0c0c/0c0c/0c0c\x1b\\",
        "\x1b[A",
        "\x1bOP",
    ];
    let sent = codes.concat();
    let ends_of_codes: Vec<usize> = codes
        .iter()
        .scan(0, |end, code| {
            *end += code.len();
            Some(*end)
        })
        .collect();

    for cut in 1..sent.len() {
        let mut session = session_on(HiddenTerminal::Shipped);

        let first = forwarded_bytes(&mut session, &sent.as_bytes()[..cut]);
        let second = forwarded_bytes(&mut session, &sent.as_bytes()[cut..]);

        assert!(
            first.is_empty() || ends_of_codes.contains(&first.len()),
            "cut after {cut} bytes, {} were forwarded",
            first.len()
        );
        assert_eq!([first, second].concat(), sent.as_bytes(), "cut at {cut}");
    }
}

#[test]
fn an_escape_key_sent_as_the_character_that_starts_every_code_is_forwarded_when_nothing_follows() {
    let mut session = session_on(HiddenTerminal::Shipped);
    assert!(!session.holding_input());

    // A Terminal that does not send keys in the Windows form sends this
    // for the Escape key. It may as well be the start of a code.
    let at_first = session.user_typed(b"\x1b");

    assert_eq!(at_first.for_app, b"");
    assert!(session.holding_input());

    // The session has no clock: whoever drives it says when the wait is
    // over.
    let released = session.release_held_input();

    assert_eq!(released.for_app, b"\x1b");
    assert!(!session.holding_input());
    assert_eq!(forwarded(&mut session, &["[A"]), ["[A"]);
}

#[test]
fn the_start_of_a_spelled_out_code_is_forwarded_as_keys_when_the_rest_never_comes() {
    let mut session = session_on(HiddenTerminal::Shipped);
    let start = spelled_out("\x1b[20");

    let at_first = session.user_typed(start.as_bytes());

    assert_eq!(at_first.for_app, b"");
    assert!(session.holding_input());

    let released = session.release_held_input();

    assert_eq!(released.for_app, start.as_bytes());
    assert!(!session.holding_input());
}

#[test]
fn pasted_farsi_reaches_the_hidden_terminal_built_into_windows_as_keys() {
    let mut session = session_on(HiddenTerminal::BuiltIn);

    // A letter, a Half-space, and a character that takes two keys.
    let forwarded = forwarded(&mut session, &["\x1b[200~xس\u{200c}😀\x1b[201~"]);

    // Given a plain Half-space, the built-in hidden terminal makes up a key
    // sequence for it (Alt held down while its number is typed), which
    // Codex drops. A key that carries the character arrives as typed.
    assert_eq!(
        forwarded,
        ["\x1b[200~x\
          \x1b[0;0;1587;1;0;1_\x1b[0;0;1587;0;0;1_\
          \x1b[0;0;8204;1;0;1_\x1b[0;0;8204;0;0;1_\
          \x1b[0;0;55357;1;0;1_\x1b[0;0;55357;0;0;1_\
          \x1b[0;0;56832;1;0;1_\x1b[0;0;56832;0;0;1_\
          \x1b[201~"]
    );
}

/// The Alt key going down and coming up with nothing typed.
const ALT_DOWN: &str = "\x1b[18;56;0;1;2;1_";
const ALT_UP: &str = "\x1b[18;56;0;0;0;1_";

#[test]
fn a_character_typed_by_its_number_reaches_the_hidden_terminal_as_the_character() {
    let mut session = session_on(HiddenTerminal::Shipped);
    // For a pasted character the keyboard has no key for, the console host
    // built into Windows makes up keys: Alt goes down, the character's
    // number is typed on the number pad (6 and 3 here), and the character
    // itself, a Half-space, comes as Alt is let go.
    let made_up_keys = format!(
        "{ALT_DOWN}\x1b[102;77;0;1;2;1_\x1b[102;77;0;0;2;1_\
         \x1b[99;81;0;1;2;1_\x1b[99;81;0;0;2;1_\x1b[18;56;8204;0;0;1_"
    );

    let forwarded = forwarded(&mut session, &[&made_up_keys]);

    // Given those keys, the hidden terminal farcli ships hands an app that
    // reads what the Terminal sent, as Claude Code does, nothing at all.
    assert_eq!(forwarded, ["\u{200c}"]);
}

#[test]
fn the_alt_key_held_for_a_shortcut_reaches_the_hidden_terminal_as_it_came() {
    let mut session = session_on(HiddenTerminal::Shipped);
    // Alt and the Left arrow.
    let left = "\x1b[37;75;0;1;258;1_\x1b[37;75;0;0;258;1_";

    let forwarded = forwarded(&mut session, &[ALT_DOWN, left, ALT_UP]);

    // Alt going down may start a character typed by its number. The next
    // key tells.
    assert_eq!(forwarded, ["", &format!("{ALT_DOWN}{left}"), ALT_UP]);
}

#[test]
fn a_paste_from_windows_terminal_reaches_the_hidden_terminal_as_it_came() {
    let mut session = session_on(HiddenTerminal::Shipped);
    // The marks and the text come as they are. A line break in the text
    // comes as the Enter key.
    let paste = format!("{PASTE_STARTS}می\u{200c}روم{KEY_ENTER}second line{PASTE_ENDS}");

    let forwarded = forwarded(&mut session, &[&paste]);

    assert_eq!(forwarded, [paste]);
}

#[test]
fn paste_marks_that_an_older_console_host_spells_out_reach_the_hidden_terminal_as_the_marks() {
    let mut session = session_on(HiddenTerminal::Shipped);
    let paste = format!(
        "{}{KEY_A}{KEY_ENTER}{KEY_A}{}",
        spelled_out(PASTE_STARTS),
        spelled_out(PASTE_ENDS)
    );

    let forwarded = forwarded(&mut session, &[&paste]);

    // Left spelled out, an app that reads keys would be handed `[200~` as
    // typed text.
    assert_eq!(
        forwarded,
        [format!(
            "{PASTE_STARTS}{KEY_A}{KEY_ENTER}{KEY_A}{PASTE_ENDS}"
        )]
    );
}

/// A key that carries nothing, coming up: no key number and no character.
const NOTHING_COMING_UP: &str = "\x1b[0;0;0;0;0;1_";

#[test]
fn an_app_on_the_hidden_terminal_built_into_windows_can_be_woken_after_what_it_was_sent() {
    let mut session = session_on(HiddenTerminal::BuiltIn);

    // The built-in hidden terminal can take keys for an app at the moment
    // the app, having found none, goes to wait for more, and then leave it
    // waiting with those keys in hand until something else arrives.
    let forwarded = forwarded(&mut session, &[KEY_A]);

    assert_eq!(forwarded, [KEY_A]);
    assert!(session.app_can_be_left_asleep_on_input());

    // The session has no clock: whoever drives it says when the app has
    // had the time to go to sleep.
    let wake = session.wake_app();

    // Something else arrives, which no app takes for a key.
    assert_eq!(wake.for_app, NOTHING_COMING_UP.as_bytes());
    assert_eq!(wake.for_terminal, b"");
}

#[test]
fn an_app_on_the_hidden_terminal_farcli_ships_is_sent_nothing_it_was_not_sent() {
    let mut session = session_on(HiddenTerminal::Shipped);

    let forwarded = forwarded(&mut session, &[KEY_A]);

    // The shipped hidden terminal wakes an app for every key it takes.
    assert_eq!(forwarded, [KEY_A]);
    assert!(!session.app_can_be_left_asleep_on_input());
    assert_eq!(session.wake_app().for_app, b"");
}
