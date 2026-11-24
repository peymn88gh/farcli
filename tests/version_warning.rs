//! At start farcli says in one line that the Target app is newer than the
//! version its App profile was tested with, and says nothing otherwise.
//!
//! The line is written to the Terminal before the app starts, so it stays
//! above whatever the app draws on the main screen. An app that moves to
//! the alternate screen hides it within a moment, so there the session
//! shows it over the top row until the user presses a key.
//!
//! Every test goes through the App profile and the session.
//! `version_warning_at_start.rs` runs the real `farcli`.

mod claude_code;
mod codex;

use std::fs;
use std::path::PathBuf;

use claude_code::{at_right_edge, from_the_right};
use farcli::app_profile::AppProfile;
use farcli::fake_terminal::FakeTerminal;
use farcli::recording::Recording;
use farcli::session::Session;
use farcli::Size;

#[test]
fn an_app_newer_than_the_tested_version_gets_one_line_that_names_both_versions() {
    // What each app prints when asked for its version.
    let claude_code = AppProfile::claude_code().newer_version_warning("2.1.400 (Claude Code)");
    let codex = AppProfile::codex().newer_version_warning("codex-cli 0.172.0\r\n");

    assert_eq!(
        claude_code.as_deref(),
        Some(
            "farcli: Claude Code 2.1.400 is newer than the tested 2.1.322. \
             F12 switches the correction off and on."
        )
    );
    assert_eq!(
        codex.as_deref(),
        Some(
            "farcli: Codex 0.172.0 is newer than the tested 0.171.0. \
             F12 switches the correction off and on."
        )
    );
}

#[test]
fn the_tested_version_and_every_older_one_get_nothing() {
    let claude_code = AppProfile::claude_code();
    let codex = AppProfile::codex();

    for said in [
        "2.1.322 (Claude Code)",
        "2.1.321 (Claude Code)",
        "2.0.999",
        "1.9.9",
    ] {
        assert_eq!(claude_code.newer_version_warning(said), None, "{said}");
    }
    for said in [
        "codex-cli 0.171.0",
        "codex-cli 0.170.9",
        "0.99.0",
        "0.171.0-alpha.2",
    ] {
        assert_eq!(codex.newer_version_warning(said), None, "{said}");
    }
}

#[test]
fn versions_are_compared_number_by_number_and_not_as_text() {
    let claude_code = AppProfile::claude_code();

    // As text each of these would sort before 2.1.322.
    for said in ["2.1.1000", "2.10.0", "10.0.0", "2.1.322.1"] {
        assert!(claude_code.newer_version_warning(said).is_some(), "{said}");
    }
    assert_eq!(claude_code.newer_version_warning("2.1.99"), None);
}

#[test]
fn an_answer_with_no_version_in_it_gets_nothing() {
    for said in ["", "\r\n", "unknown option --version", "v", "version 7"] {
        assert_eq!(
            AppProfile::claude_code().newer_version_warning(said),
            None,
            "{said}"
        );
        assert_eq!(
            AppProfile::codex().newer_version_warning(said),
            None,
            "{said}"
        );
    }
}

#[test]
fn a_program_farcli_knows_nothing_about_gets_nothing() {
    assert_eq!(AppProfile::plain().newer_version_warning("99.0.0"), None);
}

#[test]
fn the_tested_version_of_each_app_is_the_newest_its_recordings_were_made_with() {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("recordings");
    let mut made_with_the_tested_version = Vec::new();
    for entry in fs::read_dir(folder).unwrap() {
        let file = entry.unwrap().path();
        if file.extension().is_none_or(|ending| ending != "recording") {
            continue;
        }
        let recording = Recording::load(&file).unwrap();
        let profile = recording.profile();
        let said = &recording.about.app_version;

        // No recording was made with an app newer than the tested version.
        assert_eq!(
            profile.newer_version_warning(said),
            None,
            "{}",
            file.display()
        );
        // And the next version after the recorded one would be newer than
        // it only if the recorded one is the tested version itself.
        let (before_last_number, last_number) = said
            .split_once(' ')
            .map_or(said.as_str(), |(first, second)| match first.contains('.') {
                true => first,
                false => second,
            })
            .rsplit_once('.')
            .unwrap();
        let next = format!(
            "{before_last_number}.{}",
            last_number.parse::<u32>().unwrap() + 1
        );
        if profile.newer_version_warning(&next).is_some() {
            made_with_the_tested_version.push(recording.about.app.clone());
        }
    }
    made_with_the_tested_version.sort();
    made_with_the_tested_version.dedup();

    assert_eq!(made_with_the_tested_version, ["claude", "codex"]);
}

/// The key `a` going down and coming up, as Windows Terminal sends it.
const KEY_A: &str = "\x1b[65;30;97;1;0;1_\x1b[65;30;97;0;0;1_";

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
    /// Starts `command` in a Terminal `cols` wide, as the real farcli does
    /// when the app is newer than the tested version: the line is written
    /// under the command, the session starts with the cursor under the
    /// line, and is told to keep the line in sight.
    fn start_with_the_line(profile: AppProfile, command: &str, line: &str, cols: u16) -> Run {
        let before = format!("PS> {command}\r\n{line}\r\n");
        let mut terminal = FakeTerminal::new(Size { cols, rows: 8 });
        terminal.receive(before.as_bytes());
        let mut without_farcli = FakeTerminal::new(Size { cols, rows: 8 });
        without_farcli.receive(before.as_bytes());
        let mut session = Session::start(profile, terminal.size(), terminal.cursor());
        session.keep_in_sight(line);
        Run {
            session,
            terminal,
            without_farcli,
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

    fn row(&self, row: usize) -> String {
        self.terminal.screen_text().swap_remove(row)
    }
}

fn claude_code_line() -> String {
    AppProfile::claude_code()
        .newer_version_warning("2.1.400 (Claude Code)")
        .unwrap()
}

fn codex_line() -> String {
    AppProfile::codex()
        .newer_version_warning("codex-cli 0.172.0")
        .unwrap()
}

/// Codex's first screen in its default display, on the alternate screen:
/// an empty top row, its name, and a reply.
fn codex_on_the_alternate_screen() -> String {
    let rows = [
        String::new(),
        "  >_ OpenAI Codex (v0.172.0)".to_string(),
        String::new(),
        codex::reply_start("سلام دنیا"),
    ];
    format!("\x1b[?1049h{}", codex::screen(&rows))
}

#[test]
fn the_line_stays_above_what_the_app_draws_on_the_main_screen() {
    let line = claude_code_line();
    let mut run = Run::start_with_the_line(AppProfile::claude_code(), "farcli claude", &line, 110);

    // Claude Code's classic display draws from where the cursor is.
    run.app_writes(claude_code::reply(&[&["سلام دنیا"]]));
    run.user_types(KEY_A);

    let reply = format!("●{}", at_right_edge(109, &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ")));
    assert_eq!(
        run.terminal.screen_text()[..3],
        ["PS> farcli claude", line.as_str(), reply.as_str()]
    );
}

#[test]
fn on_the_alternate_screen_the_line_is_shown_over_the_top_row_until_a_key_is_pressed() {
    let line = codex_line();
    let mut run = Run::start_with_the_line(AppProfile::codex(), "farcli codex", &line, 110);

    run.app_writes(codex_on_the_alternate_screen());

    // The rest of the app's first screen is as it is without the line.
    let reply = format!("•{}", at_right_edge(109, &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ")));
    assert_eq!(
        run.terminal.screen_text()[..4],
        [
            line.as_str(),
            "  >_ OpenAI Codex (v0.172.0)",
            "",
            reply.as_str()
        ]
    );

    run.user_types(KEY_A);

    // The top row is the app's again, and the key reached the app.
    assert_eq!(
        run.terminal.screen_text()[..4],
        ["", "  >_ OpenAI Codex (v0.172.0)", "", reply.as_str()]
    );
    assert_eq!(run.terminal.screen()[0], run.without_farcli.screen()[0]);
    assert_eq!(String::from_utf8(run.app_received).unwrap(), KEY_A);
}

#[test]
fn what_the_terminal_sends_by_itself_does_not_take_the_line_away() {
    let line = codex_line();
    let mut run = Run::start_with_the_line(AppProfile::codex(), "farcli codex", &line, 110);
    run.app_writes(codex_on_the_alternate_screen());

    // The Terminal says what kind it is and what its background colour is,
    // that its window has the keyboard, and that a key came up.
    run.user_types("\x1b[?61c\x1b]11;rgb:0c0c/0c0c/0c0c\x1b\\");
    run.user_types("\x1b[I");
    run.user_types("\x1b[13;28;13;0;0;1_");
    // An older console host spells an answer out as keys that have no key
    // number: here the kind of terminal again, `ESC [ ? 6 1 c`.
    let spelled_out: String = "\x1b[?61c"
        .chars()
        .map(|character| format!("\x1b[0;0;{};1;0;1_", u32::from(character)))
        .collect();
    run.user_types(&spelled_out);

    assert_eq!(run.row(0), line);
    // The app was sent all of it.
    assert!(run.app_received.ends_with(spelled_out.as_bytes()));
}

#[test]
fn a_farsi_letter_that_comes_with_no_key_number_is_the_user_typing_and_takes_the_line_away() {
    let line = codex_line();
    let mut run = Run::start_with_the_line(AppProfile::codex(), "farcli codex", &line, 110);
    run.app_writes(codex_on_the_alternate_screen());

    // The letter seen, as some Terminals send a letter the keyboard layout
    // types: the letter and nothing about the key.
    run.user_types("\x1b[0;0;1587;1;0;1_");

    assert_eq!(run.row(0), "");
}

#[test]
fn a_line_wider_than_the_terminal_is_cut_at_its_right_edge_on_the_alternate_screen() {
    let line = codex_line();
    let mut run = Run::start_with_the_line(AppProfile::codex(), "farcli codex", &line, 40);

    run.app_writes(codex_on_the_alternate_screen());

    assert_eq!(run.row(0), "farcli: Codex 0.172.0 is newer than the");
    assert!(line.starts_with("farcli: Codex 0.172.0 is newer than the tested"));
    assert_eq!(run.row(1), "  >_ OpenAI Codex (v0.172.0)");
}

#[test]
fn the_line_covers_a_top_row_the_app_drew_on_and_gives_it_back_as_drawn() {
    let line = claude_code_line();
    let mut run = Run::start_with_the_line(AppProfile::claude_code(), "farcli claude", &line, 110);
    // Claude Code's fullscreen display starts with its name on the top row,
    // in colour.
    let first_screen = "\x1b[?1049h\x1b[2J\x1b[H\x1b[38;2;215;119;87m ▐▛███▜▌\x1b[39m   \
                        \x1b[1mClaude Code\x1b[22m v2.1.400\r\n";

    run.app_writes(first_screen);
    assert_eq!(run.row(0), line);

    run.user_types(KEY_A);

    assert_eq!(run.row(0), " ▐▛███▜▌   Claude Code v2.1.400");
    assert_eq!(run.terminal.screen(), run.without_farcli.screen());
}

#[test]
fn back_on_the_main_screen_the_line_is_where_it_was_written() {
    let line = codex_line();
    let mut run = Run::start_with_the_line(AppProfile::codex(), "farcli codex", &line, 110);
    run.app_writes(codex_on_the_alternate_screen());

    // The app ends without a key ever having been pressed.
    run.app_writes("\x1b[?1049l");

    assert_eq!(
        run.terminal.screen_text()[..3],
        ["PS> farcli codex", line.as_str(), ""]
    );
    assert_eq!(run.terminal.screen(), run.without_farcli.screen());
}

#[test]
fn a_key_pressed_on_the_main_screen_ends_it_for_an_alternate_screen_opened_later() {
    let line = claude_code_line();
    let mut run = Run::start_with_the_line(AppProfile::claude_code(), "farcli claude", &line, 110);
    run.app_writes(claude_code::reply(&[&["سلام دنیا"]]));

    // The user works in the classic display, and later opens something
    // that Claude Code shows on the alternate screen.
    run.user_types(KEY_A);
    run.app_writes("\x1b[?1049h\x1b[2J\x1b[Hthe whole conversation\r\n");

    assert_eq!(run.row(0), "the whole conversation");
}

#[test]
fn a_session_with_no_line_to_keep_in_sight_shows_the_alternate_screen_as_it_is() {
    let mut terminal = FakeTerminal::new(Size { cols: 110, rows: 8 });
    terminal.receive(b"PS> farcli codex\r\n");
    let mut session = Session::start(AppProfile::codex(), terminal.size(), terminal.cursor());

    let reaction = session.app_wrote(codex_on_the_alternate_screen().as_bytes());
    terminal.receive(&reaction.for_terminal);

    assert_eq!(
        terminal.screen_text()[..2],
        ["", "  >_ OpenAI Codex (v0.172.0)"]
    );
}
