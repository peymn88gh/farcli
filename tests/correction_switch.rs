//! One key, F12, switches the correction off and on in a running session.
//! Switched off, the Terminal shows exactly what the Target app drew,
//! cursor included. Switched on again, it shows the corrected screen with
//! nothing left over. The key itself never reaches the app.
//!
//! Every test goes through the session. The keys are in the form Windows
//! Terminal sends them once farcli has asked for Windows' own key reports:
//! one code for the key going down and one for it coming up.

mod claude_code;
mod codex;

use std::path::PathBuf;

use claude_code::{at, at_right_edge, from_the_right, reply};
use farcli::app_profile::AppProfile;
use farcli::fake_terminal::FakeTerminal;
use farcli::recording::{Event, Recording};
use farcli::session::Session;
use farcli::Size;

/// F12 going down and coming up: key number 123, the keyboard's own number
/// 88, no character, nothing held with it.
const F12_DOWN: &str = "\x1b[123;88;0;1;0;1_";
const F12_UP: &str = "\x1b[123;88;0;0;0;1_";

/// A session joined to the Terminal that shows its output.
struct Run {
    session: Session,
    terminal: FakeTerminal,
    /// A second Terminal that is sent exactly what the app wrote, with no
    /// farcli in between: it shows what the app drew.
    without_farcli: FakeTerminal,
    /// Everything forwarded to the Target app so far.
    app_received: Vec<u8>,
}

impl Run {
    fn start(profile: AppProfile, cols: u16, rows: u16) -> Run {
        let terminal = FakeTerminal::new(Size { cols, rows });
        Run {
            session: Session::start(profile, terminal.size(), terminal.cursor()),
            terminal,
            without_farcli: FakeTerminal::new(Size { cols, rows }),
            app_received: Vec::new(),
        }
    }

    fn app_writes(&mut self, bytes: impl AsRef<[u8]>) {
        let reaction = self.session.app_wrote(bytes.as_ref());
        self.terminal.receive(&reaction.for_terminal);
        self.app_received.extend(reaction.for_app);
        self.without_farcli.receive(bytes.as_ref());
    }

    fn user_types(&mut self, bytes: impl AsRef<[u8]>) {
        let reaction = self.session.user_typed(bytes.as_ref());
        self.terminal.receive(&reaction.for_terminal);
        self.app_received.extend(reaction.for_app);
    }

    fn user_presses_f12(&mut self) {
        self.user_types(F12_DOWN);
        self.user_types(F12_UP);
    }

    /// True when the Terminal shows what the app drew and nothing else:
    /// every character, colour and style, and the cursor.
    fn shows_what_the_app_drew(&self) -> bool {
        self.terminal.screen() == self.without_farcli.screen()
            && self.terminal.cursor() == self.without_farcli.cursor()
            && self.terminal.cursor_visible() == self.without_farcli.cursor_visible()
    }
}

#[test]
fn f12_shows_the_screen_as_the_app_drew_it_and_f12_again_shows_it_corrected() {
    let mut run = Run::start(AppProfile::claude_code(), 30, 6);
    run.app_writes(reply(&[&["سلام دنیا"]]));
    let corrected = format!("●{}", at_right_edge(29, &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ")));
    assert_eq!(run.terminal.screen_text()[0], corrected);
    let corrected_screen = run.terminal.screen();

    run.user_presses_f12();

    // What Claude Code drew: its own Screen order, letters unjoined.
    assert_eq!(run.terminal.screen_text()[0], "● ایند مالس");
    assert!(run.shows_what_the_app_drew());

    run.user_presses_f12();

    assert_eq!(run.terminal.screen(), corrected_screen);
}

/// The key `a` going down and coming up.
const KEY_A: &str = "\x1b[65;30;97;1;0;1_\x1b[65;30;97;0;0;1_";

#[test]
fn f12_never_reaches_the_app_and_every_other_key_does_whichever_way_the_switch_is() {
    let mut run = Run::start(AppProfile::codex(), 30, 6);

    run.user_types(KEY_A);
    run.user_presses_f12();
    run.user_types(KEY_A);
    run.user_presses_f12();
    run.user_types(KEY_A);

    assert_eq!(
        String::from_utf8(run.app_received).unwrap(),
        [KEY_A, KEY_A, KEY_A].concat()
    );
}

#[test]
fn f12_with_shift_ctrl_or_alt_held_is_the_apps_key_and_switches_nothing() {
    let mut run = Run::start(AppProfile::claude_code(), 30, 6);
    run.app_writes(reply(&[&["سلام دنیا"]]));
    let corrected_screen = run.terminal.screen();
    // Windows' bits for Shift, either Ctrl and either Alt.
    let held = [0x10, 0x08, 0x04, 0x02, 0x01, 0x18];
    let mut sent = String::new();

    for modifiers in held {
        let keys = format!("\x1b[123;88;0;1;{modifiers};1_\x1b[123;88;0;0;{modifiers};1_");
        run.user_types(&keys);
        sent.push_str(&keys);
    }

    assert_eq!(String::from_utf8(run.app_received).unwrap(), sent);
    assert_eq!(run.terminal.screen(), corrected_screen);
}

#[test]
fn f12_switches_whatever_lock_keys_are_on() {
    let mut run = Run::start(AppProfile::claude_code(), 30, 6);
    run.app_writes(reply(&[&["سلام دنیا"]]));
    // Num Lock, Scroll Lock and Caps Lock all on.
    let locks = 0x20 | 0x40 | 0x80;

    run.user_types(format!(
        "\x1b[123;88;0;1;{locks};1_\x1b[123;88;0;0;{locks};1_"
    ));

    assert!(run.shows_what_the_app_drew());
    assert_eq!(run.app_received, b"");
}

#[test]
fn f12_held_down_switches_once() {
    let mut run = Run::start(AppProfile::claude_code(), 30, 6);
    run.app_writes(reply(&[&["سلام دنیا"]]));

    // A key held down goes down again and again before it comes up.
    run.user_types([F12_DOWN, F12_DOWN, F12_DOWN, F12_DOWN, F12_UP].concat());

    assert!(run.shows_what_the_app_drew());
    assert_eq!(run.app_received, b"");
}

#[test]
fn f12_let_go_in_another_window_does_not_cost_the_next_press() {
    let mut run = Run::start(AppProfile::claude_code(), 30, 6);
    run.app_writes(reply(&[&["سلام دنیا"]]));
    let corrected_screen = run.terminal.screen();

    // The key goes down, and the user moves to another window before
    // letting it go: the Terminal never hears of the key coming up.
    run.user_types(F12_DOWN);
    run.user_types("\x1b[O");
    assert!(run.shows_what_the_app_drew());
    // Back in the Terminal, F12 is pressed once more.
    run.user_types("\x1b[I");
    run.user_presses_f12();

    assert_eq!(run.terminal.screen(), corrected_screen);
    // Only the news about the window reached the app.
    assert_eq!(run.app_received, b"\x1b[O\x1b[I");
}

#[test]
fn f12_from_a_terminal_with_no_windows_form_for_keys_switches_too() {
    let mut run = Run::start(AppProfile::claude_code(), 30, 6);
    run.app_writes(reply(&[&["سلام دنیا"]]));
    let corrected_screen = run.terminal.screen();

    // Such a Terminal sends one code for the key, and none when it comes up.
    run.user_types("\x1b[24~");
    assert!(run.shows_what_the_app_drew());
    run.user_types("\x1b[24~");

    assert_eq!(run.terminal.screen(), corrected_screen);
    assert_eq!(run.app_received, b"");
}

#[test]
fn the_code_for_f12_in_pasted_text_is_text() {
    let mut run = Run::start(AppProfile::claude_code(), 30, 6);
    run.app_writes(reply(&[&["سلام دنیا"]]));
    let corrected_screen = run.terminal.screen();
    let paste = "\x1b[200~before \x1b[24~ after\x1b[201~";

    run.user_types(paste);

    assert_eq!(String::from_utf8(run.app_received).unwrap(), paste);
    assert_eq!(run.terminal.screen(), corrected_screen);
}

#[test]
fn f12_in_no_windows_form_and_spelled_out_by_an_older_console_host_goes_to_the_app_as_before() {
    // Known gap, and a safe one. The console host built into Windows hands
    // on a code its Terminal sent one character at a time, each as a key
    // with no key number. farcli does not put those five back together, so
    // here F12 is still the app's key and switches nothing.
    let mut run = Run::start(AppProfile::claude_code(), 30, 6);
    run.app_writes(reply(&[&["سلام دنیا"]]));
    let corrected_screen = run.terminal.screen();
    let spelled_out: String = "\x1b[24~"
        .chars()
        .map(|character| format!("\x1b[0;0;{};1;0;1_", u32::from(character)))
        .collect();

    run.user_types(&spelled_out);
    // The session waits for more of the code until told that none came.
    let reaction = run.session.release_held_input();
    run.app_received.extend(reaction.for_app);

    assert_eq!(String::from_utf8(run.app_received).unwrap(), spelled_out);
    assert_eq!(run.terminal.screen(), corrected_screen);
}

#[test]
fn switched_off_the_cursor_is_where_the_app_has_it() {
    // A program farcli knows nothing about prints a Farsi word and waits
    // after it. Corrected, the word is at the right edge and the cursor to
    // its left, where the next letter would appear.
    let mut run = Run::start(AppProfile::plain(), 20, 3);
    run.app_writes("سلام");
    let corrected_cursor = run.terminal.cursor();
    assert_eq!(corrected_cursor, at(0, 15));

    run.user_presses_f12();

    assert_eq!(run.terminal.screen_text()[0], "سلام");
    assert_eq!(run.terminal.cursor(), at(0, 4));
    assert!(run.shows_what_the_app_drew());

    run.user_presses_f12();

    assert_eq!(run.terminal.cursor(), corrected_cursor);
}

#[test]
fn what_the_app_draws_while_the_correction_is_off_is_shown_as_drawn_and_corrected_once_it_is_on() {
    let paragraphs = [&["سلام دنیا"][..], &["این خط دوم است"][..]];
    let mut never_off = Run::start(AppProfile::claude_code(), 30, 8);
    never_off.app_writes(reply(&paragraphs[..1]));
    never_off.app_writes(reply(&paragraphs[1..]));
    let mut run = Run::start(AppProfile::claude_code(), 30, 8);
    run.app_writes(reply(&paragraphs[..1]));

    run.user_presses_f12();
    run.app_writes(reply(&paragraphs[1..]));

    assert!(run.shows_what_the_app_drew());

    run.user_presses_f12();

    assert_eq!(run.terminal.screen(), never_off.terminal.screen());
    assert_eq!(run.terminal.cursor(), never_off.terminal.cursor());
}

#[test]
fn f12_pressed_while_the_app_is_part_way_through_a_frame_shows_nothing_half_done() {
    let mut run = Run::start(AppProfile::codex(), 30, 6);
    run.app_writes(codex::screen(&[codex::reply_start("سلام دنیا")]));
    let before = run.terminal.screen();
    // The app starts a frame and has not finished it.
    run.app_writes(format!("\x1b[?2026h{}", codex::on_row(2, "half")));

    run.user_presses_f12();

    assert_eq!(run.terminal.screen(), before);

    run.app_writes(format!("{}\x1b[?2026l", codex::on_row(3, "done")));

    assert!(run.shows_what_the_app_drew());
    assert_eq!(
        run.terminal.screen_text()[..4],
        ["• سلام دنیا", "", "half", "done"]
    );
}

/// Recordings of real sessions. Four have a reply in them: Claude Code's
/// classic display and Codex started with `--no-alt-screen`, where rows
/// scroll off the top, and both apps' displays on the alternate screen. In
/// the other two Farsi is typed into Claude Code's Input box, which farcli
/// shows as a right-to-left text field with a cursor of its own placing.
const SESSIONS: [&str; 6] = [
    "claude-classic-reply",
    "claude-fullscreen-reply",
    "codex-reply",
    "codex-inline-reply",
    "claude-classic-typing",
    "claude-fullscreen-typing",
];

fn recording(name: &str) -> Recording {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("recordings")
        .join(format!("{name}.recording"));
    Recording::load(file).unwrap()
}

/// Plays a recording through a session, pressing F12 just before each of
/// the events numbered in `f12_before`, and calls `look` after every event.
fn play(recording: &Recording, f12_before: &[usize], mut look: impl FnMut(usize, &Run)) -> Run {
    let about = &recording.about;
    let place_cursor = format!("\x1b[{};{}H", about.cursor.row + 1, about.cursor.col + 1);
    let mut terminal = FakeTerminal::new(about.size);
    terminal.receive(place_cursor.as_bytes());
    let mut without_farcli = FakeTerminal::new(about.size);
    without_farcli.receive(place_cursor.as_bytes());
    let mut run = Run {
        session: Session::start_on(
            recording.hidden_terminal().unwrap(),
            recording.profile(),
            about.size,
            about.cursor,
        ),
        terminal,
        without_farcli,
        app_received: Vec::new(),
    };
    for (index, (_, event)) in recording.events.iter().enumerate() {
        if f12_before.contains(&index) {
            run.user_presses_f12();
        }
        let reaction = match event {
            Event::AppWrote(bytes) => {
                run.without_farcli.receive(bytes);
                run.session.app_wrote(bytes)
            }
            Event::UserTyped(bytes) => run.session.user_typed(bytes),
            Event::TerminalResized(size) => {
                run.terminal.resize(*size);
                run.without_farcli.resize(*size);
                run.session.terminal_resized(*size)
            }
            Event::HeldFrameReleased => run.session.release_held_frame(),
            Event::HeldInputReleased => run.session.release_held_input(),
        };
        run.terminal.receive(&reaction.for_terminal);
        run.app_received.extend(reaction.for_app);
        look(index, &run);
    }
    run
}

#[test]
fn switched_off_from_the_start_a_recorded_session_shows_what_the_app_drew_at_every_step() {
    for name in SESSIONS {
        let recording = recording(name);
        let mut looked = 0;

        let off = play(&recording, &[0], |index, run| {
            // While the app is part-way through a frame the Terminal with
            // farcli is behind on purpose.
            if run.session.holding_frame() {
                return;
            }
            looked += 1;
            assert!(run.shows_what_the_app_drew(), "{name}, event {index}");
        });

        assert!(looked > 30, "{name}: {looked}");
        // Rows that scrolled off the top while the correction was off are
        // kept by the Terminal as the app drew them.
        assert_eq!(
            off.terminal.scrollback(),
            off.without_farcli.scrollback(),
            "{name}"
        );
        // The app is sent what it is sent with the correction on, in a
        // session that the correction does change.
        let mut corrected = 0;
        let on = play(&recording, &[], |_, run| {
            corrected +=
                usize::from(!run.session.holding_frame() && !run.shows_what_the_app_drew());
        });
        assert_eq!(off.app_received, on.app_received, "{name}");
        assert!(corrected > 0, "{name}");
    }
}

#[test]
fn switched_on_again_a_recorded_session_looks_as_if_the_correction_had_never_been_off() {
    for name in SESSIONS {
        let recording = recording(name);
        let events = recording.events.len();
        let never_off = play(&recording, &[], |_, _| {});

        // Off for the whole session and on at its end, and off for the
        // middle third of it, in which the reply arrives.
        for (off_at, on_at) in [(0, events - 1), (events / 3, events * 2 / 3)] {
            let run = play(&recording, &[off_at, on_at], |_, _| {});

            assert_eq!(
                run.terminal.screen_text(),
                never_off.terminal.screen_text(),
                "{name}, off before event {off_at} and on before event {on_at}"
            );
            assert_eq!(run.terminal.screen(), never_off.terminal.screen(), "{name}");
            assert_eq!(run.terminal.cursor(), never_off.terminal.cursor(), "{name}");
        }
    }
}
