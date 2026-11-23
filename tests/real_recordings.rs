//! The recordings of real Claude Code and Codex sessions in
//! `tests/recordings` play through the session, hold what the reading and
//! typing work needs, and hold nothing personal.
//!
//! Each `NAME.recording` was made by running the app under the real
//! `farcli` on a hidden terminal with `NAME.steps` played into it. A reply
//! recording has the text the model wrote beside it in `NAME.reply.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use farcli::hidden_terminal::HiddenTerminal;
use farcli::recording::{Event, Recording, Replay};

const REPLIES: [&str; 4] = [
    "claude-classic-reply",
    "claude-fullscreen-reply",
    "codex-reply",
    "codex-inline-reply",
];

const TYPING: [&str; 3] = [
    "claude-classic-typing",
    "claude-fullscreen-typing",
    "codex-typing",
];

/// Probe strings typed into Claude Code's Input box one after another.
const INPUT_PROBES: &str = "claude-classic-input-probes";

/// Claude Code started with `--ax-screen-reader`.
const SCREEN_READER: &str = "claude-screen-reader-typing";

/// What `scrub_recording` put where the user's name was.
const SCRUBBED_USER: &str = "farsi.speaker";

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("recordings")
}

fn all_names() -> Vec<&'static str> {
    let mut names = REPLIES.to_vec();
    names.extend(TYPING);
    names.push(INPUT_PROBES);
    names.push(SCREEN_READER);
    names
}

fn load(name: &str) -> Recording {
    Recording::load(folder().join(format!("{name}.recording"))).unwrap()
}

fn is_farsi(character: char) -> bool {
    ('\u{600}'..='\u{6ff}').contains(&character)
}

fn farsi_letters(text: &str) -> usize {
    text.chars()
        .filter(|character| is_farsi(*character))
        .count()
}

/// Every row the Terminal showed at any moment of the recording.
fn rows_ever_shown(recording: &Recording) -> BTreeSet<String> {
    let mut rows = BTreeSet::new();
    let mut replay = Replay::without_farcli(recording);
    loop {
        rows.extend(replay.terminal.screen_text());
        if replay.step().is_none() {
            break;
        }
    }
    rows
}

/// A key the user pressed, as farcli received it on Windows.
struct Key {
    /// The key's number on the keyboard (37 is Left), or 0.
    key: u32,
    character: Option<char>,
    down: bool,
}

/// The keys of a recording made on Windows, where each reaches farcli as
/// `ESC [ key ; scan ; character ; down ; modifiers ; repeat _`.
fn keys(recording: &Recording) -> Vec<Key> {
    let mut typed = Vec::new();
    for (_, event) in &recording.events {
        if let Event::UserTyped(bytes) = event {
            typed.extend(bytes);
        }
    }
    String::from_utf8(typed)
        .unwrap()
        .split('_')
        .filter_map(|code| {
            let fields: Vec<u32> = code
                .strip_prefix("\x1b[")?
                .split(';')
                .map(|field| field.parse().unwrap())
                .collect();
            Some(Key {
                key: fields[0],
                character: char::from_u32(fields[2]).filter(|character| *character != '\0'),
                down: fields[3] == 1,
            })
        })
        .collect()
}

#[test]
fn every_recording_tells_the_app_its_version_and_the_system() {
    for name in all_names() {
        let about = load(name).about;
        let app = name.split('-').next().unwrap();
        assert_eq!(about.app, app, "{name}");
        assert!(
            about
                .app_version
                .contains(|digit: char| digit.is_ascii_digit()),
            "{name}: {:?}",
            about.app_version
        );
        assert_eq!(about.system, "Windows native", "{name}");
    }
}

#[test]
fn every_recording_was_made_on_the_hidden_terminal_farcli_ships() {
    // What reaches farcli depends on the hidden terminal the app ran on.
    // The one built into Windows damaged rows holding a Half-space (see
    // docs/adr/0002), so these recordings were made again without it.
    for name in all_names() {
        let recording = load(name);
        assert_eq!(
            recording.hidden_terminal(),
            Some(HiddenTerminal::Shipped),
            "{name}: {:?}",
            recording.note("hidden-terminal")
        );
    }
}

#[test]
fn every_recording_replays_through_the_session_and_rows_without_farsi_stay_as_the_app_drew_them() {
    for name in all_names() {
        let recording = load(name);
        let mut through_farcli = Replay::start(&recording, recording.profile());
        let mut alone = Replay::without_farcli(&recording);
        let compare = |through_farcli: &Replay, alone: &Replay, event: usize| {
            let shown = through_farcli.terminal.screen_text();
            let drawn = alone.terminal.screen_text();
            for (row, (shown, drawn)) in shown.iter().zip(&drawn).enumerate() {
                if farsi_letters(drawn) == 0 {
                    assert_eq!(shown, drawn, "{name}: row {row} before event {event}");
                }
            }
        };

        // Compared each time the user is about to press a key, and at the end.
        for (index, (_, event)) in recording.events.iter().enumerate() {
            if matches!(event, Event::UserTyped(_)) {
                compare(&through_farcli, &alone, index);
            }
            through_farcli.step();
            alone.step();
        }
        through_farcli.finish();
        compare(&through_farcli, &alone, recording.events.len());
        assert_eq!(
            through_farcli.terminal.cursor(),
            alone.terminal.cursor(),
            "{name}"
        );
    }
}

#[test]
fn every_key_the_user_pressed_reaches_the_app_unchanged() {
    for name in all_names() {
        let recording = load(name);
        let typed: Vec<u8> = recording
            .events
            .iter()
            .filter_map(|(_, event)| match event {
                Event::UserTyped(bytes) => Some(bytes.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        let mut replay = Replay::start(&recording, recording.profile());

        replay.finish();

        // Besides the keys, the app's side receives farcli's answers. First,
        // once, what kind of terminal farcli is, which the hidden terminal
        // asks as it starts. Then where the cursor is, each time it is
        // asked: `ESC [ row ; column R`.
        let mut received = String::from_utf8(replay.app_received).unwrap();
        const TERMINAL_KIND: &str = "\x1b[?61c";
        assert_eq!(received.matches(TERMINAL_KIND).count(), 1, "{name}");
        received = received.replacen(TERMINAL_KIND, "", 1);
        while let Some(end) = received.find('R') {
            let start = received[..end].rfind("\x1b[").unwrap();
            received.replace_range(start..=end, "");
        }
        assert_eq!(received.as_bytes(), typed, "{name}");
    }
}

#[test]
fn every_reply_recording_shows_farsi_prose_a_mixed_sentence_code_a_list_and_a_table() {
    for name in REPLIES {
        let rows = rows_ever_shown(&load(name));
        let shown = |wanted: &dyn Fn(&str) -> bool| rows.iter().any(|row| wanted(row));

        assert!(shown(&|row| farsi_letters(row) >= 40), "{name}: prose");
        assert!(
            shown(&|row| {
                farsi_letters(row) >= 10
                    && row.contains(|letter: char| letter.is_ascii_alphabetic())
                    && row.contains(|digit: char| digit.is_ascii_digit())
            }),
            "{name}: Farsi with English and numbers"
        );
        assert!(shown(&|row| row.contains("print(")), "{name}: code");
        assert!(
            shown(&|row| {
                let item = row.trim_start();
                (item.starts_with("- ") || item.starts_with("• ")) && farsi_letters(row) > 0
            }),
            "{name}: list"
        );
        // Claude Code draws a table with upright bars between the columns,
        // Codex with a heavy line under the header row.
        let with_bars = shown(&|row| row.matches('│').count() >= 3 && farsi_letters(row) > 0);
        let with_header_line = shown(&|row| row.trim_start().starts_with('━'))
            && shown(&|row| row.contains("توضیح") && row.contains("تعداد"));
        assert!(with_bars || with_header_line, "{name}: table");
    }
}

#[test]
fn every_reply_recording_has_the_text_the_model_wrote_beside_it() {
    for name in REPLIES {
        let reply = fs::read_to_string(folder().join(format!("{name}.reply.md"))).unwrap();
        assert!(farsi_letters(&reply) > 200, "{name}");
    }
}

#[test]
fn every_typing_recording_has_a_half_space_arrows_backspace_delete_and_a_paste() {
    for name in TYPING {
        let recording = load(name);
        let keys = keys(&recording);
        let pressed = |key: u32| {
            keys.iter()
                .any(|pressed| pressed.down && pressed.key == key)
        };
        let typed: String = keys
            .iter()
            .filter(|key| key.down)
            .filter_map(|key| key.character)
            .collect();

        assert!(typed.contains("می\u{200c}خواهم"), "{name}: Half-space");
        assert!(pressed(37), "{name}: Left");
        assert!(pressed(39), "{name}: Right");
        assert!(pressed(38), "{name}: Up");
        assert!(pressed(8), "{name}: Backspace");
        assert!(pressed(46), "{name}: Delete");
        assert!(typed.contains("config.json"), "{name}: mixed sentence");
        // The hidden terminal the session ran in kept the marks a Terminal
        // puts around a paste to itself: farcli saw only the text, arriving
        // several keys at a time.
        assert!(typed.contains(PASTED), "{name}: paste");
        let most_farsi_keys_at_once = recording
            .events
            .iter()
            .filter_map(|(_, event)| match event {
                Event::UserTyped(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
                _ => None,
            })
            // The key going down for a Farsi letter carries its number.
            .map(|codes| {
                codes
                    .split('_')
                    .filter(|code| {
                        let fields: Vec<&str> = code.split(';').collect();
                        fields.len() == 6
                            && fields[3] == "1"
                            && fields[2]
                                .parse()
                                .ok()
                                .and_then(char::from_u32)
                                .is_some_and(is_farsi)
                    })
                    .count()
            })
            .max();
        assert!(
            most_farsi_keys_at_once >= Some(3),
            "{name}: paste in a burst"
        );
    }
}

/// The text pasted in every typing recording.
const PASTED: &str = "این متن از جای دیگر آمده: نسخه\u{200c}ی 2.1 از farcli و 42 خط";

#[test]
fn every_typing_recording_shows_a_farsi_message_wrapped_over_three_rows() {
    for name in TYPING {
        let recording = load(name);
        let mut replay = Replay::without_farcli(&recording);
        let wrapped = replay.play_until(|replay| {
            replay
                .terminal
                .screen_text()
                .windows(3)
                .any(|rows| rows.iter().all(|row| farsi_letters(row) >= 10))
        });
        assert!(wrapped, "{name}");
    }
}

#[test]
fn the_input_probes_recording_types_every_probe_string() {
    let typed: String = keys(&load(INPUT_PROBES))
        .iter()
        .filter(|key| key.down)
        .filter_map(|key| key.character)
        .collect();
    let probes = fs::read_to_string(folder().join(format!("{INPUT_PROBES}.typed.txt"))).unwrap();

    assert!(probes.lines().count() > 30);
    for probe in probes.lines() {
        assert!(typed.contains(probe), "{probe}");
    }
}

/// The row that starts with the Input box's marker, without the marker.
fn input_row(replay: &Replay, marker: char) -> Option<String> {
    replay
        .terminal
        .screen_text()
        .iter()
        .find_map(|row| row.strip_prefix(marker).map(|text| text.trim().to_string()))
}

// The three tests below hold what the recordings found (ticket #3), so that
// a recording made with a newer app shows at once if the app has changed.

#[test]
fn claude_code_on_windows_draws_farsi_already_reordered_row_by_row() {
    let shows = |name: &str, drawn: &str| {
        let mut replay = Replay::without_farcli(&load(name));
        replay.play_until(|replay| {
            replay
                .terminal
                .screen_text()
                .iter()
                .any(|row| row.contains(drawn))
        })
    };
    // Typed: «اندازه 33 x 20 است». Drawn: every letter in Screen order,
    // unjoined, with the numbers swapped.
    assert!(shows(INPUT_PROBES, "تسا x 20 33 هزادنا"));
    // The same in both display modes: in the message «... و عددهایی مثل
    // 123 و 33 x 20 (داخل پرانتز) ...» as sent, and in the reply.
    let sent = ")زتنارپ لخاد( x 20 33 و 123 لثم";
    assert!(shows("claude-classic-reply", sent));
    assert!(shows("claude-fullscreen-reply", sent));
    // The model wrote «... و اندازه‌ی تصویر را (33 x 20) تنظیم کنید.» A
    // Half-space stays after the letter it follows.
    assert!(shows(
        "claude-fullscreen-reply",
        ".دینک میظنت )x 20 33( ار ریوصت یه\u{200c}زادنا و"
    ));
    // A wrapped row that begins with an English word is laid out left to
    // right on its own, inside a right-to-left paragraph. The model wrote
    // «... مقدار timeout را روی 123 میلی‌ثانیه بگذار و اندازه‌ی آیکون را
    // (33 x 20) نگه دار.»
    // In the classic display a long reply is written in one piece and its
    // first rows are found by scrolling back.
    let mut replay = Replay::without_farcli(&load("claude-classic-reply"));
    replay.finish();
    assert!(replay
        .terminal
        .scrollback_text()
        .iter()
        .any(|row| {
            row.contains(
        "timeout ار نوکیآ یه\u{200c}زادنا و راذگب هیناثی\u{200c}لیم 123 یور ار (33 x 20) راد هگن."
    )
        }));
    // Brackets keep their shape and so face the wrong way.
    let mut replay = Replay::without_farcli(&load(INPUT_PROBES));
    assert!(replay.play_until(|replay| {
        input_row(replay, '❯').is_some_and(|row| row == "بوخ )ایند( مالس")
    }));
}

#[test]
fn claude_code_with_the_screen_reader_display_draws_logical_order() {
    let mut replay = Replay::without_farcli(&load(SCREEN_READER));

    let found = replay.play_until(|replay| {
        replay
            .terminal
            .screen_text()
            .iter()
            .any(|row| row == "اندازه (33 x 20) است")
    });

    assert!(found);
}

#[test]
fn codex_draws_farsi_in_logical_order() {
    let mut replay = Replay::without_farcli(&load("codex-typing"));

    let found = replay.play_until(|replay| {
        input_row(replay, '›')
            .is_some_and(|row| row.ends_with("فایل config.json را باز کن و 123 خط"))
    });

    assert!(found);
}

/// The message pasted in a reply recording, as its steps file gives it:
/// five rows long in a Terminal of 100 columns, with nine Half-spaces.
fn pasted_message(name: &str) -> String {
    let steps = fs::read_to_string(folder().join(format!("{name}.steps"))).unwrap();
    let pasted = steps
        .lines()
        .find_map(|step| step.strip_prefix("paste:"))
        .unwrap();
    pasted.replace("\\u{200c}", "\u{200c}")
}

fn without_spaces(text: &str) -> Vec<char> {
    let mut characters: Vec<char> = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    characters.sort_unstable();
    characters
}

// The two tests below hold what the hidden terminal farcli ships changed
// (the follow-up on ticket #3). On the one built into Windows the same
// message arrived with a row garbled, a stray piece of border, spaces lost
// between words and, in Codex, letters in the wrong places.

#[test]
fn claude_codes_input_box_keeps_a_message_with_half_spaces_whole_inside_its_borders() {
    for name in ["claude-classic-reply", "claude-fullscreen-reply"] {
        let message = pasted_message(name);
        assert_eq!(message.matches('\u{200c}').count(), 9, "{name}");
        let mut replay = Replay::without_farcli(&load(name));
        // The last words of the message, as Claude Code draws them.
        let pasted = replay.play_until(|replay| {
            replay
                .terminal
                .screen_text()
                .iter()
                .any(|row| row.contains(".یطخه\u{200c}س Python دک کولب کی"))
        });
        assert!(pasted, "{name}");

        let screen = replay.terminal.screen_text();
        let is_border = |row: &String| row.chars().count() == 100 && row.chars().all(|c| c == '─');
        let borders: Vec<usize> = (0..screen.len())
            .filter(|row| is_border(&screen[*row]))
            .collect();
        let [.., top, bottom] = borders[..] else {
            panic!("{name}: no Input box in {screen:?}");
        };
        let rows = &screen[top + 1..bottom];
        assert_eq!(rows.len(), 5, "{name}: {rows:?}");
        assert!(rows[0].starts_with('❯'), "{name}: {rows:?}");
        // Claude Code reorders each row, so the rows hold the message's
        // characters and words in another order: none missing, none added,
        // no two words run together.
        let inside: String = rows
            .iter()
            .map(|row| format!("{} ", row.trim_start_matches('❯')))
            .collect();
        assert_eq!(
            without_spaces(&inside),
            without_spaces(&message),
            "{name}: {rows:?}"
        );
        assert_eq!(
            inside.split_whitespace().count(),
            message.split_whitespace().count(),
            "{name}: {rows:?}"
        );
    }
}

#[test]
fn codexs_input_box_shows_a_message_with_half_spaces_exactly_as_it_was_pasted() {
    for name in ["codex-reply", "codex-inline-reply"] {
        let message = pasted_message(name);
        let mut replay = Replay::without_farcli(&load(name));

        // The Input box: the row with Codex's marker and the rows under it,
        // up to the first empty row. Codex draws Logical order.
        let shown_whole = replay.play_until(|replay| {
            let screen = replay.terminal.screen_text();
            screen
                .iter()
                .position(|row| row.starts_with('›'))
                .is_some_and(|first| {
                    let rows: Vec<&str> = screen[first..]
                        .iter()
                        .take_while(|row| !row.is_empty())
                        .map(|row| row.trim_start_matches('›').trim())
                        .collect();
                    rows.len() == 5 && rows.join(" ") == message
                })
        });

        assert!(shown_whole, "{name}");
    }
}

#[test]
fn no_recording_names_a_home_folder_an_email_address_or_a_long_secret() {
    for entry in fs::read_dir(folder()).unwrap() {
        let file = entry.unwrap().path();
        let text = fs::read_to_string(&file).unwrap();
        let name = file.display();

        // The only folder under `Users` a recording may name.
        for (at, _) in text.match_indices("Users") {
            let after = text[at + "Users".len()..].trim_start_matches(['\\', '/']);
            assert!(
                after.starts_with("Public") || after.starts_with(SCRUBBED_USER),
                "{name}: a folder under Users at byte {at}"
            );
        }
        for (at, _) in text.match_indices('@') {
            let before = text[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric());
            let after: String = text[at + 1..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
                .collect();
            let address = before && after.trim_end_matches('.').contains('.');
            assert!(!address, "{name}: an address at byte {at}");
        }
        // Keys and tokens are long unbroken runs of letters and digits.
        let longest = text
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .filter(|run| run.chars().any(|c| c.is_ascii_digit()))
            .filter(|run| run.chars().any(|c| c.is_ascii_alphabetic()))
            .map(str::len)
            .max()
            .unwrap_or(0);
        assert!(longest < 32, "{name}: an unbroken run of {longest}");
    }
}
