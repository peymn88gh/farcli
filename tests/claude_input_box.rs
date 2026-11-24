//! Farsi typed in Claude Code's own Input box looks like a right-to-left
//! text field: joined, in the right order, starting at the right edge, with
//! the cursor where the next character will appear.
//!
//! These tests replay the recordings in which a message is typed into the
//! real app and nothing is sent. Each goes through the session and checks
//! what a Terminal shows, where its cursor is, and what the app was sent.
//!
//! The joined letters a Terminal should show are written as text that reads
//! from the right, the way a person reads it, and `from_the_right` turns
//! that into Screen order.

mod claude_code;

use std::fs;
use std::path::PathBuf;

use claude_code::{as_drawn, at, at_right_edge, cells, direction_of, from_the_right, laid_out};
use farcli::layout::Direction;
use farcli::recording::{Recording, Replay};
use farcli::screen::{Position, Row};

/// The two sessions in which the same keys are typed into the Input box:
/// one in each of Claude Code's display modes. The Terminal is 60 columns
/// wide.
const TYPING: [&str; 2] = ["claude-classic-typing", "claude-fullscreen-typing"];

/// A session in which 43 short texts are typed into the Input box one
/// after another, in a Terminal 100 columns wide.
const INPUT_PROBES: &str = "claude-classic-input-probes";

/// The columns a message has in a Terminal 60 columns wide: all but the two
/// that hold the prompt mark.
const AREA: usize = 58;

/// The prompt mark and the no-break space the app draws after it.
const MARK: &str = "❯\u{a0}";

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/recordings")
}

fn load(name: &str) -> Recording {
    Recording::load(folder().join(format!("{name}.recording"))).unwrap()
}

fn is_farsi(character: char) -> bool {
    ('\u{600}'..='\u{6ff}').contains(&character) || ('\u{fb50}'..='\u{feff}').contains(&character)
}

/// A recorded session played twice, in step: once through farcli, and once
/// with nothing between the app and the Terminal, which shows what the app
/// drew.
struct Typing {
    name: &'static str,
    through_farcli: Replay,
    alone: Replay,
}

impl Typing {
    fn of(name: &'static str) -> Typing {
        let recording = load(name);
        Typing {
            name,
            through_farcli: Replay::start(&recording, recording.profile()),
            alone: Replay::without_farcli(&recording),
        }
    }

    /// Plays one event. Returns false at the end of the recording.
    fn step(&mut self) -> bool {
        self.alone.step();
        self.through_farcli.step().is_some()
    }

    /// Plays until `reached`, and fails if the recording ends first.
    fn play_until(&mut self, what: &str, reached: impl Fn(&Typing) -> bool) {
        while !reached(self) {
            assert!(self.step(), "{}: never reached: {what}", self.name);
        }
    }

    /// Plays until the app has drawn `row` as a row of its screen.
    fn play_until_the_app_draws(&mut self, row: &str) {
        self.play_until(row, |typing| {
            typing.drawn().iter().any(|drawn| drawn == row)
        });
    }

    /// Plays until the app has put its cursor in the column `col`.
    fn play_until_the_apps_cursor_is_in(&mut self, col: u16) {
        self.play_until(&format!("the app's cursor in column {col}"), |typing| {
            typing.alone.terminal.cursor().col == col
        });
    }

    /// The rows the app drew.
    fn drawn(&self) -> Vec<String> {
        self.alone.terminal.screen_text()
    }

    /// The row the Input box starts on: the one with the prompt mark.
    fn first_row_of_the_box(&self) -> Option<usize> {
        self.drawn().iter().position(|row| row.starts_with('❯'))
    }

    /// The rows of the screen that hold the Input box's message.
    fn rows_of_the_box(&self) -> std::ops::Range<usize> {
        let Some(first) = self.first_row_of_the_box() else {
            return 0..0;
        };
        let drawn = self.drawn();
        let rows = drawn[first..]
            .iter()
            .take_while(|row| !row.starts_with('─'))
            .count();
        first..first + rows
    }

    /// The rows of the Input box that the Terminal shows, from the one with
    /// the prompt mark down to the box's lower border.
    fn shown_box(&self) -> Vec<String> {
        self.through_farcli.terminal.screen_text()[self.rows_of_the_box()].to_vec()
    }

    /// Where the Terminal's cursor is, counted from the Input box's first
    /// row.
    fn cursor_in_the_box(&self) -> Position {
        let cursor = self.through_farcli.terminal.cursor();
        at(cursor.row - self.rows_of_the_box().start as u16, cursor.col)
    }

    /// The character the Terminal's cursor is on.
    fn character_under_the_cursor(&self) -> char {
        let terminal = &self.through_farcli.terminal;
        terminal.cell(terminal.cursor()).character
    }

    fn shown_screen(&self) -> Vec<Row> {
        self.through_farcli.terminal.screen()
    }

    fn drawn_screen(&self) -> Vec<Row> {
        self.alone.terminal.screen()
    }
}

/// The first row of the Input box when `on_screen` is what its message
/// looks like from the left, ending at the right edge.
fn first_row(on_screen: &str) -> String {
    format!("{MARK}{}", at_right_edge(AREA, on_screen))
}

/// A row the message wraps onto.
fn next_row(on_screen: &str) -> String {
    format!("  {}", at_right_edge(AREA, on_screen))
}

/// The column of the cursor after a right-to-left message of `typed` cells:
/// the cell to the left of the last character.
fn left_of(typed: usize) -> u16 {
    (2 + AREA - 1 - typed) as u16
}

#[test]
fn a_farsi_word_typed_in_the_input_box_is_joined_and_starts_at_the_right_edge() {
    for name in TYPING {
        let mut typing = Typing::of(name);
        // What the app draws for «سلام»: unjoined, from the left, after an
        // empty cell where it means its cursor to be.
        typing.play_until_the_app_draws("❯\u{a0} مالس");

        assert_eq!(
            typing.shown_box(),
            [first_row(&from_the_right("ﺳﻠﺎﻡ"))],
            "{name}"
        );
        // The next letter will appear to the left of the last one.
        assert_eq!(typing.cursor_in_the_box(), at(0, left_of(4)), "{name}");
    }
}

#[test]
fn a_sentence_of_farsi_english_and_numbers_is_in_order_after_every_key() {
    for name in TYPING {
        let mut typing = Typing::of(name);
        // The message so far, and what is typed after it one key at a time.
        let mut message = String::from("سلام می\u{200c}خوم");
        typing.play_until_the_app_draws(&format!("{MARK}{}", as_drawn(&format!("{message} "))));

        for key in " فایل config.json را باز کن و 123 خط".chars() {
            message.push(key);
            // What the app draws: the row in Screen order by the row's own
            // first letter, with an empty cell for its cursor at the end.
            typing.play_until_the_app_draws(&format!("{MARK}{}", as_drawn(&format!("{message} "))));

            // What Layout makes of the message as it was typed.
            assert_eq!(
                typing.shown_box(),
                [format!(
                    "{MARK}{}",
                    laid_out(&message, Direction::RightToLeft, AREA)
                )],
                "{name}: after {message:?}"
            );
            assert_eq!(
                typing.cursor_in_the_box(),
                at(0, left_of(cells(&message))),
                "{name}: after {message:?}"
            );
        }

        // The whole sentence, written out: it starts at the right edge, and
        // the file name and the number read left to right where the
        // sentence reaches them.
        let on_screen = [
            from_the_right(" ﺧﻂ").as_str(),
            "123",
            from_the_right(" ﺭﺍ ﺑﺎﺯ ﮐﻦ ﻭ ").as_str(),
            "config.json",
            from_the_right("ﺳﻠﺎﻡ ﻣﯽﺧﻮﻡ ﻓﺎﯾﻞ ").as_str(),
        ]
        .concat();
        assert_eq!(typing.shown_box(), [first_row(&on_screen)], "{name}");
    }
}

#[test]
fn the_cursor_is_on_the_letter_the_next_one_will_push_aside_after_moving_into_a_word() {
    for name in TYPING {
        let mut typing = Typing::of(name);
        typing.play_until_the_app_draws("❯\u{a0} مهاوخی\u{200c}م مالس");
        assert_eq!(typing.cursor_in_the_box(), at(0, left_of(12)), "{name}");

        // Three keys back, the app has its cursor before the tenth
        // character of twelve: the alef of «می‌خواهم».
        typing.play_until_the_apps_cursor_is_in(11);
        assert_eq!(
            typing.shown_box(),
            [first_row(&from_the_right("ﺳﻠﺎﻡ ﻣﯽﺧﻮﺍﻫﻢ"))],
            "{name}"
        );
        assert_eq!(typing.cursor_in_the_box(), at(0, left_of(9)), "{name}");
        assert_eq!(typing.character_under_the_cursor(), 'ﺍ', "{name}");

        // One key on, it is before the «ه».
        typing.play_until_the_apps_cursor_is_in(12);
        assert_eq!(typing.cursor_in_the_box(), at(0, left_of(10)), "{name}");
        assert_eq!(typing.character_under_the_cursor(), 'ﻫ', "{name}");

        // A «ن» typed there appears in that very cell, and the cursor is on
        // the «ه» again, one cell further left.
        typing.play_until_the_app_draws("❯\u{a0}مهناوخی\u{200c}م مالس");
        assert_eq!(
            typing.shown_box(),
            [first_row(&from_the_right("ﺳﻠﺎﻡ ﻣﯽﺧﻮﺍﻧﻬﻢ"))],
            "{name}"
        );
        let cursor = typing.through_farcli.terminal.cursor();
        let typed = at(cursor.row, cursor.col + 1);
        assert_eq!(typing.through_farcli.terminal.cell(typed).character, 'ﻧ');
        assert_eq!(typing.cursor_in_the_box(), at(0, left_of(11)), "{name}");
        assert_eq!(typing.character_under_the_cursor(), 'ﻬ', "{name}");
    }
}

#[test]
fn the_cursor_at_the_start_of_a_farsi_message_is_at_the_right_edge() {
    for name in TYPING {
        let mut typing = Typing::of(name);
        // After Backspace twice and Delete once the message is «سلام می‌خوم»,
        // and then the Home key takes the app's cursor to its start.
        typing.play_until_the_app_draws("❯\u{a0}موخی\u{200c}م مالس");
        typing.play_until_the_apps_cursor_is_in(2);

        assert_eq!(
            typing.shown_box(),
            [first_row(&from_the_right("ﺳﻠﺎﻡ ﻣﯽﺧﻮﻡ"))],
            "{name}"
        );
        assert_eq!(typing.cursor_in_the_box(), at(0, 59), "{name}");
        assert_eq!(typing.character_under_the_cursor(), 'ﺳ', "{name}");
    }
}

#[test]
fn a_message_long_enough_to_wrap_starts_every_row_at_the_right_edge() {
    for name in TYPING {
        let mut typing = Typing::of(name);
        // The last row as the app draws it once the whole message is typed.
        typing.play_until_the_app_draws("   دنکی\u{200c}م هچ 2026 ددع و");

        let rows = [
            first_row(&from_the_right(
                "ﺍﯾﻦ ﯾﮏ ﭘﯿﺎﻡ ﺑﻠﻨﺪ ﺍﺳﺖ ﮐﻪ ﺑﺎﯾﺪ ﺩﺭ ﭼﻨﺪ ﺧﻂ ﺷﮑﺴﺘﻪ ﺷﻮﺩ ﺗﺎ",
            )),
            // The English word ends this row, so it is at the row's left.
            next_row(&format!(
                "English{}",
                from_the_right("ﺑﺒﯿﻨﯿﻢ ﺟﻌﺒﻪﯼ ﻭﺭﻭﺩﯼ ﺑﺎ ﻣﺘﻦ ﻓﺎﺭﺳﯽ ﻃﻮﻟﺎﻧﯽ، ﮐﻠﻤﻪﻫﺎﯼ ")
            )),
            next_row(&format!(
                "{}2026{}",
                from_the_right(" ﭼﻪ ﻣﯽﮐﻨﺪ"),
                from_the_right("ﻭ ﻋﺪﺩ ")
            )),
        ];
        assert_eq!(typing.shown_box(), rows, "{name}");
        assert_eq!(typing.cursor_in_the_box(), at(2, left_of(19)), "{name}");
    }
}

#[test]
fn a_wrapped_row_that_begins_with_an_english_word_starts_at_the_right_edge_too() {
    for name in TYPING {
        let mut typing = Typing::of(name);
        // Two letters typed into the second row push its last word,
        // «English», onto the third. The app then draws the third row from
        // the left, because it begins with an English word.
        typing.play_until_the_app_draws("  English دنکی\u{200c}م هچ 2026 ددع و");

        // The row is still a row of a Farsi message: the word it begins
        // with is at its right end.
        let third = next_row(&format!(
            "{}2026{}English",
            from_the_right(" ﭼﻪ ﻣﯽﮐﻨﺪ"),
            from_the_right(" ﻭ ﻋﺪﺩ ")
        ));
        assert_eq!(typing.shown_box()[2], third, "{name}");
        // The cursor is in the second row, after the fifteen characters up
        // to the two letters and on the letter that follows them.
        assert_eq!(typing.cursor_in_the_box(), at(1, left_of(17)), "{name}");
        assert_eq!(typing.character_under_the_cursor(), 'ﻭ', "{name}");
    }
}

#[test]
fn a_half_space_breaks_the_join_and_leaves_no_gap() {
    for name in TYPING {
        let mut typing = Typing::of(name);
        // «می», a Half-space, and the first letter of the rest of the word.
        typing.play_until_the_app_draws("❯\u{a0} خی\u{200c}م مالس");

        // The «ی» ends its word's join and the «خ» stands apart from it, in
        // the very next cell.
        assert_eq!(
            typing.shown_box(),
            [first_row(&from_the_right("ﺳﻠﺎﻡ ﻣﯽﺥ"))],
            "{name}"
        );
        // The Half-space takes no cell: the cursor is after eight characters.
        assert_eq!(typing.cursor_in_the_box(), at(0, left_of(8)), "{name}");
        // The app was sent the Half-space, as the key that types it.
        let sent = String::from_utf8_lossy(&typing.through_farcli.app_received).into_owned();
        assert!(sent.contains("\x1b[0;0;8204;1;0;1_"), "{name}");
    }
}

#[test]
fn pasted_farsi_is_joined_and_in_order() {
    for name in TYPING {
        let mut typing = Typing::of(name);
        typing.play_until_the_app_draws(
            "❯\u{a0} طخ 42 و farcli زا 2.1 یه\u{200c}خسن :هدمآ رگید یاج زا نتم نیا",
        );

        let on_screen = [
            from_the_right(" ﺧﻂ").as_str(),
            "42",
            from_the_right(" ﻭ ").as_str(),
            "farcli",
            from_the_right(" ﺍﺯ ").as_str(),
            "2.1",
            from_the_right("ﺍﯾﻦ ﻣﺘﻦ ﺍﺯ ﺟﺎﯼ ﺩﯾﮕﺮ ﺁﻣﺪﻩ: ﻧﺴﺨﻪﯼ ").as_str(),
        ]
        .concat();
        assert_eq!(typing.shown_box(), [first_row(&on_screen)], "{name}");
        assert_eq!(typing.cursor_in_the_box(), at(0, left_of(53)), "{name}");
    }
}

#[test]
fn every_probe_text_typed_into_the_input_box_reads_as_it_was_typed() {
    // Short texts that mix Farsi with English words, numbers, brackets and
    // punctuation in every way that matters. Several of them the app draws
    // exactly as it draws another text, so the screen alone does not say
    // which was typed.
    const COLS: usize = 100;
    let probes = fs::read_to_string(folder().join(format!("{INPUT_PROBES}.typed.txt"))).unwrap();
    let mut typing = Typing::of(INPUT_PROBES);
    let mut right_to_left = 0;

    for probe in probes.lines() {
        let drawn = format!("{MARK}{}", as_drawn(&format!("{probe} ")));
        typing.play_until_the_app_draws(drawn.trim_end());

        let direction = direction_of(probe);
        assert_eq!(
            typing.shown_box(),
            [format!("{MARK}{}", laid_out(probe, direction, COLS - 2))],
            "{probe}"
        );
        let cursor = match direction {
            Direction::RightToLeft => {
                right_to_left += 1;
                COLS - 1 - cells(probe)
            }
            Direction::LeftToRight => 2 + cells(probe),
        };
        assert_eq!(typing.cursor_in_the_box(), at(0, cursor as u16), "{probe}");
    }
    assert!(right_to_left > 25);
}

#[test]
fn an_english_message_and_its_cursor_are_exactly_as_the_app_drew_them() {
    for name in TYPING {
        let mut typing = Typing::of(name);
        let mut english = 0;
        loop {
            let boxed = typing.rows_of_the_box();
            let message = typing.drawn()[boxed].concat();
            if !message.chars().any(is_farsi) {
                english += usize::from(message.contains("hello"));
                assert_eq!(typing.shown_screen(), typing.drawn_screen(), "{name}");
                assert_eq!(
                    typing.through_farcli.terminal.cursor(),
                    typing.alone.terminal.cursor(),
                    "{name}: {message:?}"
                );
            }
            if !typing.step() {
                break;
            }
        }
        // «hello world 42», a move into it and a letter typed there.
        assert!(english > 15, "{name}");
    }
}

#[test]
fn the_borders_and_the_prompt_mark_stay_where_the_app_drew_them() {
    for name in TYPING {
        let mut typing = Typing::of(name);
        let mut farsi = 0;
        loop {
            let boxed = typing.rows_of_the_box();
            farsi += usize::from(typing.drawn()[boxed.clone()].concat().chars().any(is_farsi));
            // Nothing differs from what the app drew but the cells the
            // message is drawn in: on the box's rows, from the third column.
            let shown = typing.shown_screen();
            for (row, drawn) in typing.drawn_screen().iter().enumerate() {
                let kept = if boxed.contains(&row) { 2 } else { drawn.len() };
                assert_eq!(shown[row][..kept], drawn[..kept], "{name}: row {row}");
            }
            if !typing.step() {
                break;
            }
        }
        assert!(farsi > 200, "{name}");
    }
}

#[test]
fn the_app_is_sent_the_characters_typed_in_the_order_they_were_typed() {
    for name in TYPING {
        let mut typing = Typing::of(name);
        while typing.step() {}

        // Each character typed reaches the app as the key that types it,
        // going down: `ESC [ key ; scan ; character ; 1 ; modifiers ; 1 _`.
        let sent = String::from_utf8(typing.through_farcli.app_received.clone()).unwrap();
        let typed: String = sent
            .split("\x1b[")
            .filter_map(|code| {
                let fields: Vec<&str> = code.strip_suffix('_')?.split(';').collect();
                match fields[..] {
                    [_, _, character, "1", _, _] => char::from_u32(character.parse().ok()?),
                    _ => None,
                }
            })
            .filter(|character| *character != '\0')
            .collect();
        for message in [
            "سلام می\u{200c}خواهم",
            " فایل config.json را باز کن و 123 خط",
            "hello world 42",
            "این یک پیام بلند است که باید در چند خط شکسته شود تا ببینیم جعبه\u{200c}ی \
             ورودی با متن فارسی طولانی، کلمه\u{200c}های English و عدد 2026 چه می\u{200c}کند",
        ] {
            assert!(typed.contains(message), "{name}: {message}");
        }
        // A paste reaches it whole, between the marks of a paste.
        let pasted = "این متن از جای دیگر آمده: نسخه\u{200c}ی 2.1 از farcli و 42 خط";
        assert!(
            sent.contains(&format!("\x1b[200~{pasted}\x1b[201~")),
            "{name}"
        );
    }
}

/// The two sessions in which one long message is pasted and sent, in a
/// Terminal 100 columns wide.
const REPLIES: [&str; 2] = ["claude-classic-reply", "claude-fullscreen-reply"];

/// The message a recording's steps paste, as it was pasted.
fn pasted_message(name: &str) -> String {
    let steps = fs::read_to_string(folder().join(format!("{name}.steps"))).unwrap();
    let pasted = steps
        .lines()
        .find_map(|step| step.strip_prefix("paste:"))
        .unwrap();
    pasted.replace("\\u{200c}", "\u{200c}")
}

/// How many characters of `text` show on a screen.
fn characters_that_show(text: &str) -> usize {
    cells(&text.replace([' ', '\u{a0}'], ""))
}

#[test]
fn a_long_pasted_message_reads_as_it_was_pasted_on_every_row_it_wraps_onto() {
    for name in REPLIES {
        let message = pasted_message(name);
        let mut typing = Typing::of(name);
        // The end of the message as the app draws it.
        let ending = as_drawn("بدون خط خالی بین");
        typing.play_until(&ending, |typing| {
            typing.drawn().iter().any(|row| row.contains(&ending))
        });

        // The message cut into rows where the app cut it: each row holds as
        // many of its characters as the app drew on that row.
        let drawn = typing.drawn()[typing.rows_of_the_box()].to_vec();
        assert_eq!(drawn.len(), 7, "{name}");
        let mut rest = message.as_str();
        let mut wanted = Vec::new();
        for (row, drawn) in drawn.iter().enumerate() {
            let showing = characters_that_show(drawn.trim_start_matches('❯'));
            let end = rest
                .char_indices()
                .map(|(at, character)| at + character.len_utf8())
                .find(|end| characters_that_show(&rest[..*end]) == showing)
                .unwrap();
            // What rides on the last character stays with it.
            let end = rest[end..]
                .find(|character: char| character != '\u{200c}')
                .map_or(rest.len(), |more| end + more);
            let margin = if row == 0 { MARK } else { "  " };
            wanted.push(format!(
                "{margin}{}",
                laid_out(&rest[..end], Direction::RightToLeft, 98)
            ));
            rest = rest[end..].trim_start_matches(' ');
        }
        assert_eq!(rest, "", "{name}");

        assert_eq!(typing.shown_box(), wanted, "{name}");
    }
}
