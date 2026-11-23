//! The hidden terminal a Target app runs on says things of its own, besides
//! passing on what the app wrote. farcli answers what is meant for it and
//! keeps it from the Terminal.
//!
//! The bytes in these tests are what Microsoft's newer hidden terminal (the
//! one farcli ships) was seen to write. Every test goes through the session.

use farcli::app_profile::AppProfile;
use farcli::fake_terminal::FakeTerminal;
use farcli::hidden_terminal::HiddenTerminal;
use farcli::session::{Reaction, Session};
use farcli::Size;

/// What the shipped hidden terminal writes before the app's first output:
/// its window is shown, where is the cursor, what kind of terminal is this,
/// report focus, and send keys in the Windows form.
const AT_START: &str = "\x1b[1t\x1b[6n\x1b[c\x1b[?1004h\x1b[?9001h";

/// What it writes after the app has ended.
const AT_END: &str = "\x1b[?1004l\x1b[?9001l";

struct Run {
    session: Session,
    terminal: FakeTerminal,
    app_received: Vec<u8>,
    terminal_received: Vec<u8>,
}

impl Run {
    /// Starts on `hidden_terminal`, in a Terminal that already shows three
    /// rows of an earlier command.
    fn start_on(hidden_terminal: HiddenTerminal) -> Run {
        let mut terminal = FakeTerminal::new(Size { cols: 30, rows: 8 });
        terminal.receive(b"PS> one\r\ntwo\r\nPS> farcli app\r\n");
        let session = Session::start_on(
            hidden_terminal,
            AppProfile::plain(),
            terminal.size(),
            terminal.cursor(),
        );
        Run {
            session,
            terminal,
            app_received: Vec::new(),
            terminal_received: Vec::new(),
        }
    }

    fn app_writes(&mut self, bytes: impl AsRef<[u8]>) {
        let Reaction {
            for_terminal,
            for_app,
        } = self.session.app_wrote(bytes.as_ref());
        self.terminal.receive(&for_terminal);
        self.terminal_received.extend(for_terminal);
        self.app_received.extend(for_app);
    }

    fn terminal_received(&self, wanted: &str) -> bool {
        self.terminal_received
            .windows(wanted.len())
            .any(|window| window == wanted.as_bytes())
    }
}

#[test]
fn the_shipped_hidden_terminal_is_answered_at_once_when_it_starts() {
    let mut run = Run::start_on(HiddenTerminal::Shipped);

    run.app_writes(AT_START);

    // Where the cursor is, then what kind of terminal farcli is: one with
    // no extras. Until both arrive the hidden terminal holds the app back.
    assert_eq!(run.app_received, b"\x1b[4;1R\x1b[?61c");
}

#[test]
fn what_the_shipped_hidden_terminal_asks_of_farcli_never_reaches_the_terminal() {
    let mut run = Run::start_on(HiddenTerminal::Shipped);

    run.app_writes(AT_START);
    run.app_writes("\x1b[4;1Hhello");
    run.app_writes(AT_END);

    // Its own window being shown, its question and the form it wants keys
    // in are between it and farcli.
    assert!(!run.terminal_received("\x1b[1t"));
    assert!(!run.terminal_received("\x1b[c"));
    assert!(!run.terminal_received("\x1b[?9001h"));
    assert!(!run.terminal_received("\x1b[?9001l"));
    assert_eq!(run.terminal.screen_text()[3], "hello");
}

#[test]
fn the_apps_own_question_about_the_terminal_still_goes_to_the_terminal() {
    let mut run = Run::start_on(HiddenTerminal::Shipped);
    run.app_writes(AT_START);
    run.app_received.clear();

    // The shipped hidden terminal passes the app's questions on as written.
    run.app_writes("\x1b[c");

    assert!(run.terminal_received("\x1b[c"));
    assert_eq!(run.app_received, b"");
}

#[test]
fn on_the_hidden_terminal_built_into_windows_a_question_about_the_terminal_is_the_apps() {
    let mut run = Run::start_on(HiddenTerminal::BuiltIn);

    // The built-in one asks only where the cursor is.
    run.app_writes("\x1b[6n");
    run.app_writes("\x1b[c");

    assert_eq!(run.app_received, b"\x1b[4;1R");
    assert!(run.terminal_received("\x1b[c"));
}
