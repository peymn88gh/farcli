//! A farcli session can be written to a recording file and played again
//! through the session, with no real app, no real Terminal and no clock.

use farcli::app_profile::AppProfile;
use farcli::recording::{About, Event, Recorder, Recording, Replay};
use farcli::screen::Position;
use farcli::Size;

fn about_a_claude_session() -> About {
    About {
        app: "claude".to_string(),
        app_version: "2.1.322 (Claude Code)".to_string(),
        system: "Windows native".to_string(),
        size: Size { cols: 20, rows: 4 },
        cursor: Position { row: 1, col: 0 },
        notes: vec![("display-mode".to_string(), "classic".to_string())],
    }
}

#[test]
fn a_recording_file_gives_back_the_app_its_version_the_system_and_everything_that_happened() {
    // Drawing codes, Farsi with a Half-space, a letter cut in half between
    // two pieces of output, bytes that are no text at all, and a backslash.
    let farsi = "می\u{200c}خواهم".as_bytes();
    let happened = [
        (
            0,
            Event::AppWrote(b"\x1b[1;31mred\x1b[m\r\n\ttab \\ ".to_vec()),
        ),
        (12, Event::AppWrote(farsi[..3].to_vec())),
        (13, Event::AppWrote(farsi[3..].to_vec())),
        (40, Event::UserTyped(b"\x1b[65;30;97;1;0;1_".to_vec())),
        (41, Event::UserTyped(vec![0x00, 0xff, 0x7f, b' '])),
        (300, Event::TerminalResized(Size { cols: 30, rows: 8 })),
        (450, Event::HeldFrameReleased),
    ];

    let mut file = Vec::new();
    let mut recorder = Recorder::start(&mut file, &about_a_claude_session()).unwrap();
    for (milliseconds, event) in &happened {
        recorder.record(*milliseconds, event).unwrap();
    }
    let recording = Recording::read(&String::from_utf8(file).unwrap()).unwrap();

    assert_eq!(recording.about, about_a_claude_session());
    assert_eq!(recording.events, happened);
}

/// A short session written by hand: the app asks where its cursor is, draws
/// two rows, the user presses a key, and the app draws a third row.
const HAND_WRITTEN: &str = "\
farcli recording 1
app: demo
app-version: 1.0
system: Windows native
size: 20x4
cursor: 1,0

0 app \\e[6nhello\\r\\n
5 app very \\e[1mbold\\e[m
90 keys \\e[65;30;97;1;0;1_
120 app \\r\\nthird
";

#[test]
fn a_replayed_recording_shows_what_the_terminal_showed_and_gives_the_app_what_it_received() {
    let recording = Recording::read(HAND_WRITTEN).unwrap();

    let mut replay = Replay::start(&recording, AppProfile::plain());
    replay.finish();

    assert_eq!(
        replay.terminal.screen_text(),
        ["", "hello", "very bold", "third"]
    );
    assert_eq!(replay.terminal.cursor(), Position { row: 3, col: 5 });
    assert!(replay.terminal.cell(Position { row: 2, col: 5 }).style.bold);
    // The answer to the app's question, then the key.
    assert_eq!(replay.app_received, b"\x1b[2;1R\x1b[65;30;97;1;0;1_");
}

#[test]
fn a_replay_can_stop_part_way_to_look_at_the_terminal() {
    let recording = Recording::read(HAND_WRITTEN).unwrap();
    let mut replay = Replay::start(&recording, AppProfile::plain());

    let reached = replay.play_until(|replay| replay.app_received.ends_with(b"_"));

    assert!(reached);
    assert_eq!(
        replay.terminal.screen_text(),
        ["", "hello", "very bold", ""]
    );
    assert_eq!(replay.step(), Some(Event::AppWrote(b"\r\nthird".to_vec())));
    assert_eq!(replay.step(), None);
}

#[test]
fn a_recording_played_without_farcli_shows_what_the_app_drew() {
    // A program farcli knows nothing about writes a Farsi line: farcli
    // corrects it, and without farcli the Terminal shows it as written.
    let recording = Recording::read(&HAND_WRITTEN.replace("hello", "سلام دنیا")).unwrap();

    let mut alone = Replay::without_farcli(&recording);
    alone.finish();
    let mut through_farcli = Replay::start(&recording, recording.profile());
    through_farcli.finish();

    assert_eq!(
        alone.terminal.screen_text(),
        ["", "سلام دنیا", "very bold", "third"]
    );
    assert_eq!(alone.app_received, b"");
    assert_ne!(through_farcli.terminal.screen_text()[1], "سلام دنیا");
    assert_eq!(
        through_farcli.terminal.screen_text()[2..],
        alone.terminal.screen_text()[2..]
    );
}

#[test]
fn a_recording_file_with_windows_line_ends_reads_the_same() {
    let with_windows_line_ends = HAND_WRITTEN.replace('\n', "\r\n");

    assert_eq!(
        Recording::read(&with_windows_line_ends),
        Recording::read(HAND_WRITTEN)
    );
}

#[test]
fn a_file_that_is_not_a_recording_is_refused_with_the_line_at_fault() {
    assert!(Recording::read("hello").is_err());
    let bad_event = HAND_WRITTEN.replace("90 keys", "90 mouse");
    assert_eq!(
        Recording::read(&bad_event).unwrap_err(),
        "line 10: unknown kind of event"
    );
}
