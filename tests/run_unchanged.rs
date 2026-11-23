//! A Target app run through farcli with no correction looks and behaves as it
//! does on its own.
//!
//! Every test goes through the session: it feeds the bytes a Target app
//! wrote, the keys a user pressed and the Terminal's size, then checks what a
//! Terminal shows, where its cursor is and what the app received.

use farcli::app_profile::AppProfile;
use farcli::fake_terminal::FakeTerminal;
use farcli::screen::{Color, Position, Style, Underline};
use farcli::session::{Reaction, Session};
use farcli::Size;

/// A session joined to the Terminal that shows its output.
struct Run {
    session: Session,
    terminal: FakeTerminal,
    /// Everything forwarded to the Target app so far.
    app_received: Vec<u8>,
    /// Everything sent to the Terminal so far, for the few things a
    /// Terminal acts on without showing them.
    terminal_received: Vec<u8>,
}

impl Run {
    /// Starts in an empty Terminal of the given size.
    fn start(cols: u16, rows: u16) -> Run {
        let size = Size { cols, rows };
        Run::start_in(FakeTerminal::new(size))
    }

    /// Starts in a Terminal that may already show something.
    fn start_in(terminal: FakeTerminal) -> Run {
        let session = Session::start(AppProfile::plain(), terminal.size(), terminal.cursor());
        Run {
            session,
            terminal,
            app_received: Vec::new(),
            terminal_received: Vec::new(),
        }
    }

    fn app_writes(&mut self, bytes: impl AsRef<[u8]>) {
        let reaction = self.session.app_wrote(bytes.as_ref());
        self.take(reaction);
    }

    fn user_types(&mut self, bytes: impl AsRef<[u8]>) {
        let reaction = self.session.user_typed(bytes.as_ref());
        self.take(reaction);
    }

    /// The user gives the Terminal's window a new size.
    fn terminal_resizes(&mut self, cols: u16, rows: u16) {
        let size = Size { cols, rows };
        self.terminal.resize(size);
        let reaction = self.session.terminal_resized(size);
        self.take(reaction);
    }

    fn take(&mut self, reaction: Reaction) {
        self.terminal.receive(&reaction.for_terminal);
        self.terminal_received.extend(reaction.for_terminal);
        self.app_received.extend(reaction.for_app);
    }
}

/// What a Terminal of this size shows when the app writes straight to it.
fn without_farcli(cols: u16, rows: u16, app_output: &str) -> FakeTerminal {
    let mut terminal = FakeTerminal::new(Size { cols, rows });
    terminal.receive(app_output.as_bytes());
    terminal
}

fn contains(bytes: &[u8], wanted: &[u8]) -> bool {
    bytes.windows(wanted.len()).any(|window| window == wanted)
}

fn at(row: u16, col: u16) -> Position {
    Position { row, col }
}

#[test]
fn text_the_app_writes_shows_on_the_terminal_where_the_app_put_it() {
    let mut run = Run::start(20, 4);

    run.app_writes("hello\r\nworld");

    assert_eq!(run.terminal.screen_text(), ["hello", "world", "", ""]);
    assert_eq!(run.terminal.cursor(), at(1, 5));
}

#[test]
fn farsi_wide_and_accented_characters_show_as_the_app_wrote_them() {
    // Farsi with a Half-space, characters two cells wide, a letter with a
    // separate accent, and a wide character too close to the right edge.
    let output = "سلام می\u{200c}خواهم\r\n日本語 ok\r\ne\u{301}!\r\n123456789012345678日";
    let mut run = Run::start(20, 5);

    run.app_writes(output);

    let direct = without_farcli(20, 5, output);
    assert_eq!(run.terminal.screen(), direct.screen());
    assert_eq!(run.terminal.cursor(), direct.cursor());
    assert_eq!(run.terminal.screen_text()[1], "日本語 ok");
}

#[test]
fn a_line_longer_than_the_terminal_wraps_as_it_does_without_farcli() {
    let output = "0123456789abcdefghijKLMNO\r\nnext";
    let mut run = Run::start(20, 4);

    run.app_writes(output);

    assert_eq!(
        run.terminal.screen_text(),
        ["0123456789abcdefghij", "KLMNO", "next", ""]
    );
    assert_eq!(run.terminal.cursor(), at(2, 4));
}

#[test]
fn text_drawn_over_and_erased_shows_only_what_is_left() {
    let mut run = Run::start(20, 4);

    run.app_writes("first draft of line\r\nsecond line");
    run.app_writes("\x1b[1;1Hfinal\x1b[K\x1b[2;7H\x1b[1K");

    assert_eq!(run.terminal.screen_text(), ["final", "       line", "", ""]);
}

#[test]
fn a_full_screen_app_takes_over_the_terminal_and_gives_it_back() {
    let mut run = Run::start(20, 4);
    run.app_writes("before\r\n");

    run.app_writes("\x1b[?1049h\x1b[H\x1b[7m full screen \x1b[m");
    assert!(run.terminal.on_alternate_screen());
    assert_eq!(run.terminal.screen_text()[0], " full screen");

    run.app_writes("\x1b[?1049lafter");
    assert!(!run.terminal.on_alternate_screen());
    assert_eq!(run.terminal.screen_text(), ["before", "after", "", ""]);
    assert_eq!(run.terminal.scrollback_text(), [] as [&str; 0]);
}

#[test]
fn a_resized_terminal_shows_what_the_app_draws_for_the_new_size() {
    let mut run = Run::start(10, 4);
    run.app_writes("abcdefghij\r\nsecond");

    run.terminal_resizes(6, 4);
    // The app redraws its whole screen for the new size, row by row.
    run.app_writes("\x1b[Hab\x1b[K\r\ncd\x1b[K\r\n\x1b[K\r\n\x1b[K");

    assert_eq!(run.terminal.screen_text(), ["ab", "cd", "", ""]);
}

#[test]
fn a_wider_terminal_gives_the_app_the_new_columns() {
    let mut run = Run::start(10, 3);
    run.app_writes("narrow");

    run.terminal_resizes(30, 3);
    run.app_writes("\x1b[1;26Hright");

    assert_eq!(
        run.terminal.screen_text()[0],
        "narrow                   right"
    );
    assert_eq!(run.terminal.cursor(), at(0, 29));
}

#[test]
fn keys_the_user_presses_reach_the_app_unchanged() {
    let mut run = Run::start(20, 4);

    // A letter, the Up arrow, Ctrl+C, a Farsi letter, and Shift+Enter in
    // the key report format farcli reads keys in on Windows.
    let keys = "a\x1b[A\x03س\x1b[13;28;13;1;16;1_";
    run.user_types(keys);

    assert_eq!(run.app_received, keys.as_bytes());
}

#[test]
fn colours_and_styles_the_app_uses_show_on_the_terminal() {
    let mut run = Run::start(20, 4);

    run.app_writes("\x1b[1;31mE\x1b[0;3;4;38;5;200mF\x1b[0;9;2;48;2;10;20;30mG\x1b[0;7mH\x1b[mi");

    let bold_red = run.terminal.cell(at(0, 0)).style;
    assert!(bold_red.bold);
    assert_eq!(bold_red.foreground, Color::Palette(1));
    let italic_underlined = run.terminal.cell(at(0, 1)).style;
    assert!(italic_underlined.italic && !italic_underlined.bold);
    assert_eq!(italic_underlined.underline, Underline::Single);
    assert_eq!(italic_underlined.foreground, Color::Palette(200));
    let struck_dim = run.terminal.cell(at(0, 2)).style;
    assert!(struck_dim.strikeout && struck_dim.dim && !struck_dim.italic);
    assert_eq!(struck_dim.background, Color::Rgb(10, 20, 30));
    assert!(run.terminal.cell(at(0, 3)).style.inverse);
    assert_eq!(run.terminal.cell(at(0, 4)).style, Style::default());
    assert_eq!(run.terminal.screen_text()[0], "EFGHi");
}

#[test]
fn links_and_underline_kinds_the_app_uses_show_on_the_terminal() {
    let mut run = Run::start(20, 4);

    run.app_writes(
        "\x1b]8;;https://example.com\x1b\\docs\x1b]8;;\x1b\\ \x1b[4:3;58:5:196mtypo\x1b[m",
    );

    let link = run.terminal.cell(at(0, 0)).style.link.expect("a link");
    assert_eq!(link.uri, "https://example.com");
    assert_eq!(run.terminal.cell(at(0, 3)).style.link, Some(link));
    assert_eq!(run.terminal.cell(at(0, 4)).style.link, None);
    let typo = run.terminal.cell(at(0, 5)).style;
    assert_eq!(typo.underline, Underline::Curly);
    assert_eq!(typo.underline_color, Some(Color::Palette(196)));
    assert_eq!(typo.link, None);
}

#[test]
fn the_terminal_cursor_is_where_the_app_put_its_cursor_and_hidden_when_the_app_hides_it() {
    let mut run = Run::start(20, 4);

    run.app_writes("one\r\ntwo\x1b[1;2H");
    assert_eq!(run.terminal.cursor(), at(0, 1));
    assert!(run.terminal.cursor_visible());

    run.app_writes("\x1b[?25l");
    assert!(!run.terminal.cursor_visible());

    run.app_writes("\x1b[3;7H\x1b[?25h");
    assert_eq!(run.terminal.cursor(), at(2, 6));
    assert!(run.terminal.cursor_visible());
}

/// A Terminal in which the user has just run farcli from a shell.
fn terminal_after_shell_command(cols: u16, rows: u16) -> FakeTerminal {
    let mut terminal = FakeTerminal::new(Size { cols, rows });
    terminal.receive(b"PS> dir\r\nnotes.txt\r\nPS> farcli demo\r\n");
    terminal
}

#[test]
fn the_app_starts_below_what_the_terminal_already_shows() {
    let mut run = Run::start_in(terminal_after_shell_command(20, 6));

    run.app_writes("Welcome\r\n> ");

    assert_eq!(
        run.terminal.screen_text(),
        [
            "PS> dir",
            "notes.txt",
            "PS> farcli demo",
            "Welcome",
            ">",
            ""
        ]
    );
    assert_eq!(run.terminal.cursor(), at(4, 2));
}

#[test]
fn rows_the_app_erases_lose_what_the_terminal_showed_there_before() {
    let mut run = Run::start_in(terminal_after_shell_command(20, 6));

    // The app empties every row from the top down, then writes on the fourth.
    run.app_writes("\x1b[H\x1b[K\r\n\x1b[K\r\n\x1b[K\r\n\x1b[Kfresh\r\n\x1b[K\r\n\x1b[K");

    assert_eq!(run.terminal.screen_text(), ["", "", "", "fresh", "", ""]);
}

#[test]
fn drawing_over_what_the_terminal_showed_before_changes_only_the_cells_drawn() {
    let app_output = "\x1b[1;5Hxy\x1b[2;4H\x1b[K\x1b[4;1Hnew";
    let mut run = Run::start_in(terminal_after_shell_command(20, 6));

    run.app_writes(app_output);

    let mut direct = terminal_after_shell_command(20, 6);
    direct.receive(app_output.as_bytes());
    assert_eq!(run.terminal.screen(), direct.screen());
    assert_eq!(
        run.terminal.screen_text(),
        ["PS> xyr", "not", "PS> farcli demo", "new", "", ""]
    );
}

#[test]
fn what_the_terminal_showed_before_scrolls_back_with_the_apps_output() {
    let mut run = Run::start_in(terminal_after_shell_command(20, 4));

    run.app_writes("one\r\ntwo\r\nthree");

    assert_eq!(run.terminal.scrollback_text(), ["PS> dir", "notes.txt"]);
    assert_eq!(
        run.terminal.screen_text(),
        ["PS> farcli demo", "one", "two", "three"]
    );
}

#[test]
fn clearing_the_screen_also_clears_what_the_terminal_showed_before() {
    let mut run = Run::start_in(terminal_after_shell_command(20, 6));

    run.app_writes("Welcome\x1b[2J\x1b[Hfresh");

    assert_eq!(run.terminal.screen_text(), ["fresh", "", "", "", "", ""]);
    assert_eq!(run.terminal.cursor(), at(0, 5));
}

#[test]
fn clearing_from_the_cursor_down_clears_only_below_the_cursor() {
    let mut run = Run::start(20, 4);

    run.app_writes("keep\r\ngone 1\r\ngone 2\x1b[2;1H\x1b[Jnew");

    assert_eq!(run.terminal.screen_text(), ["keep", "new", "", ""]);
}

#[test]
fn clearing_scrollback_clears_what_scrolling_back_would_show() {
    let mut run = Run::start(20, 2);

    run.app_writes("old 1\r\nold 2\r\nold 3\r\n");
    assert_eq!(run.terminal.scrollback_text(), ["old 1", "old 2"]);

    run.app_writes("\x1b[3J");

    assert_eq!(run.terminal.scrollback_text(), [] as [&str; 0]);
    assert_eq!(run.terminal.screen_text(), ["old 3", ""]);
}

#[test]
fn what_the_app_asks_of_the_terminal_itself_is_passed_on_unchanged() {
    let mut run = Run::start(20, 4);

    let requests = [
        "\x1b]0;my title\x07",      // window title
        "\x1b]2;other title\x1b\\", // window title, ended the other way
        "\x1b[?2004h",              // bracketed paste
        "\x1b[?1000h",              // mouse reporting
        "\x1b[?1004h",              // focus reporting
        "\x1b[?1h",                 // application cursor keys
        "\x1b=",                    // application keypad
        "\x1b[5 q",                 // cursor shape
        "\x1b[>1u",                 // keyboard protocol
        "\x1b[c",                   // what kind of Terminal are you
        "\x1b]11;?\x1b\\",          // what is your background colour
        "\x1b]52;c;aGk=\x07",       // copy to the clipboard
        "\x1bP+q544e\x1b\\",        // what can you do
        "\x07",                     // the bell
    ];
    for request in requests {
        run.app_writes(format!("before{request}after\r"));

        assert!(
            contains(&run.terminal_received, request.as_bytes()),
            "the Terminal did not receive {request:?}"
        );
        assert_eq!(run.terminal.screen_text()[0], "beforeafter");
    }
}

#[test]
fn modes_set_in_one_code_each_go_where_they_belong() {
    let mut run = Run::start(20, 4);

    // Hide the cursor (drawing) and turn off bracketed paste (the Terminal's).
    run.app_writes("\x1b[?25;2004l");

    assert!(!run.terminal.cursor_visible());
    assert!(contains(&run.terminal_received, b"\x1b[?2004l"));
}

#[test]
fn the_app_is_told_where_its_cursor_is_when_it_asks() {
    let mut run = Run::start_in(terminal_after_shell_command(20, 6));

    run.app_writes("\x1b[6n");
    assert_eq!(run.app_received, b"\x1b[4;1R");

    run.app_writes("abc\x1b[6n");
    assert_eq!(run.app_received, b"\x1b[4;1R\x1b[4;4R");
}

#[test]
fn output_cut_anywhere_shows_the_same_as_output_in_one_piece() {
    let output = "\x1b[1;31mرنگ red\x1b[m \x1b]0;title\x07\x1b]8;id=1;http://a.example\x1b\\link\x1b]8;;\x1b\\\r\n\x1b[?25l\x1b[2;3Hx\x1b[?2004h";
    let mut whole = Run::start(20, 4);
    whole.app_writes(output);

    for cut in 1..output.len() {
        let mut pieces = Run::start(20, 4);
        pieces.app_writes(&output.as_bytes()[..cut]);
        pieces.app_writes(&output.as_bytes()[cut..]);

        assert_eq!(
            pieces.terminal.screen(),
            whole.terminal.screen(),
            "cut at {cut}"
        );
        assert_eq!(
            pieces.terminal.cursor(),
            whole.terminal.cursor(),
            "cut at {cut}"
        );
        assert_eq!(
            pieces.terminal.cursor_visible(),
            whole.terminal.cursor_visible(),
            "cut at {cut}"
        );
        assert!(
            contains(&pieces.terminal_received, b"\x1b]0;title\x07"),
            "cut at {cut}"
        );
        assert!(
            contains(&pieces.terminal_received, b"\x1b[?2004h"),
            "cut at {cut}"
        );
    }
}

#[test]
fn a_frame_the_app_marks_as_unfinished_is_not_shown_half_done() {
    let mut run = Run::start(20, 4);
    run.app_writes("old frame");

    run.app_writes("\x1b[?2026h\x1b[2K\rnew ");
    assert_eq!(run.terminal.screen_text()[0], "old frame");
    assert!(run.session.holding_frame());

    run.app_writes("frame\x1b[?2026l");
    assert_eq!(run.terminal.screen_text()[0], "new frame");
    assert!(!run.session.holding_frame());
}

#[test]
fn a_frame_the_app_never_finishes_is_shown_when_released() {
    let mut run = Run::start(20, 4);

    run.app_writes("\x1b[?2026hstuck");
    assert_eq!(run.terminal.screen_text()[0], "");

    let reaction = run.session.release_held_frame();
    run.terminal.receive(&reaction.for_terminal);
    assert_eq!(run.terminal.screen_text()[0], "stuck");

    run.app_writes(" and on");
    assert_eq!(run.terminal.screen_text()[0], "stuck and on");
}

#[test]
fn scrolling_back_shows_earlier_output() {
    let mut run = Run::start(20, 3);

    for line in 1..=5 {
        run.app_writes(format!("line {line}\r\n"));
    }

    assert_eq!(
        run.terminal.scrollback_text(),
        ["line 1", "line 2", "line 3"]
    );
    assert_eq!(run.terminal.screen_text(), ["line 4", "line 5", ""]);
    assert_eq!(run.terminal.cursor(), at(2, 0));
}

#[test]
fn scrolling_back_shows_output_that_went_by_faster_than_a_screenful() {
    let mut run = Run::start(20, 3);

    let burst: String = (1..=9).map(|line| format!("line {line}\r\n")).collect();
    run.app_writes(burst);

    assert_eq!(
        run.terminal.scrollback_text(),
        ["line 1", "line 2", "line 3", "line 4", "line 5", "line 6", "line 7"]
    );
    assert_eq!(run.terminal.screen_text(), ["line 8", "line 9", ""]);
}

#[test]
fn scrolled_off_output_keeps_its_colours() {
    let mut run = Run::start(20, 2);

    run.app_writes("\x1b[32mgreen\x1b[m\r\nplain\r\nlast");
    run.terminal.resize(Size { cols: 20, rows: 3 });

    // Making the Terminal taller brings the scrolled-off row back into view.
    assert_eq!(run.terminal.screen_text(), ["green", "plain", "last"]);
    assert_eq!(
        run.terminal.cell(at(0, 0)).style.foreground,
        Color::Palette(2)
    );
}
