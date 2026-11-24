//! What farcli cannot read with confidence it shows as the Target app drew
//! it, and goes on correcting the rest. A screen that matches nothing in
//! the App profile is shown as drawn from corner to corner. And nothing an
//! app draws makes farcli fall over.
//!
//! The screens here are ones the Region finders were not built for: a
//! recording of one app played under the other app's profile, screens of
//! random cells, and layouts that neither app draws today but a later
//! version might. The cases a finder was built for are in the tests of each
//! app's Agent output.
//!
//! Every test goes through the session.

mod claude_code;
mod codex;

use std::path::PathBuf;

use claude_code::{as_drawn, laid_out, placed};
use farcli::app_profile::AppProfile;
use farcli::fake_terminal::FakeTerminal;
use farcli::layout::Direction;
use farcli::recording::{Recording, Replay};
use farcli::screen::Row;
use farcli::session::Session;
use farcli::Size;

/// A session joined to the Terminal that shows its output.
struct Run {
    session: Session,
    terminal: FakeTerminal,
    /// A second Terminal that is sent exactly what the app wrote, with no
    /// farcli in between: it shows what the app drew.
    without_farcli: FakeTerminal,
}

impl Run {
    fn start(profile: AppProfile, cols: u16, rows: u16) -> Run {
        let terminal = FakeTerminal::new(Size { cols, rows });
        Run {
            session: Session::start(profile, terminal.size(), terminal.cursor()),
            terminal,
            without_farcli: FakeTerminal::new(Size { cols, rows }),
        }
    }

    fn app_writes(&mut self, bytes: impl AsRef<str>) {
        let reaction = self.session.app_wrote(bytes.as_ref().as_bytes());
        self.terminal.receive(&reaction.for_terminal);
        // The Terminal without farcli shows everything the app has drawn
        // so far. Given the marks an app puts around a frame, it would
        // wait for the end of a frame as farcli does, and for a frame that
        // never ends it would wait as long as its own clock says.
        let drawing = bytes
            .as_ref()
            .replace("[?2026h", "")
            .replace("[?2026l", "");
        self.without_farcli.receive(drawing.as_bytes());
    }

    fn user_types(&mut self, bytes: impl AsRef<[u8]>) {
        let reaction = self.session.user_typed(bytes.as_ref());
        self.terminal.receive(&reaction.for_terminal);
    }

    fn terminal_resizes(&mut self, cols: u16, rows: u16) {
        let size = Size { cols, rows };
        self.terminal.resize(size);
        self.without_farcli.resize(size);
        let reaction = self.session.terminal_resized(size);
        self.terminal.receive(&reaction.for_terminal);
    }

    fn row(&self, row: usize) -> String {
        self.terminal.screen_text().swap_remove(row)
    }

    fn drawn_row(&self, row: usize) -> String {
        self.without_farcli.screen_text().swap_remove(row)
    }

    /// True when the Terminal shows the row at `row` exactly as the app
    /// drew it: every character, colour and style.
    fn row_is_as_drawn(&self, row: usize) -> bool {
        self.terminal.screen()[row] == self.without_farcli.screen()[row]
    }

    /// True when the whole screen and the cursor are as the app drew them.
    fn screen_is_as_drawn(&self) -> bool {
        self.terminal.screen() == self.without_farcli.screen()
            && self.terminal.cursor() == self.without_farcli.cursor()
            && self.terminal.cursor_visible() == self.without_farcli.cursor_visible()
    }

    /// The rows the Terminal shows that are not as the app drew them.
    fn rows_not_as_drawn(&self) -> Vec<String> {
        let drawn = self.without_farcli.screen_text();
        (0..drawn.len())
            .filter(|row| !self.row_is_as_drawn(*row))
            .map(|row| format!("row {row}: {:?} shown as {:?}", drawn[row], self.row(row)))
            .collect()
    }
}

// ---------------------------------------------------------------------
// One app's recording under the other app's profile
// ---------------------------------------------------------------------

fn recording(name: &str) -> Recording {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("recordings")
        .join(format!("{name}.recording"));
    Recording::load(file).unwrap()
}

/// Plays a recording under `profile` beside the same recording with no
/// farcli at all, and returns how many times the two Terminals were
/// compared: after everything that happened, except while the app was
/// part-way through a frame, when the Terminal with farcli is behind on
/// purpose.
fn assert_shown_as_drawn_throughout(name: &str, profile: AppProfile) -> usize {
    let recording = recording(name);
    let mut with_farcli = Replay::start(&recording, profile);
    let mut without_farcli = Replay::without_farcli(&recording);
    let mut compared = 0;
    let mut event = 0;
    while with_farcli.step().is_some() {
        without_farcli.step();
        event += 1;
        if with_farcli.holding_frame() {
            continue;
        }
        compared += 1;
        let shown = with_farcli.terminal.screen_text();
        let drawn = without_farcli.terminal.screen_text();
        for (row, (shown, drawn)) in shown.iter().zip(&drawn).enumerate() {
            assert_eq!(shown, drawn, "{name}, event {event}, row {row}");
        }
        assert!(
            with_farcli.terminal.screen() == without_farcli.terminal.screen(),
            "{name}, event {event}: a colour or style differs"
        );
        assert_eq!(
            with_farcli.terminal.cursor(),
            without_farcli.terminal.cursor(),
            "{name}, event {event}"
        );
    }
    with_farcli.finish();
    assert!(
        with_farcli.terminal.scrollback() == without_farcli.terminal.scrollback(),
        "{name}: a row that scrolled off the top is not as drawn"
    );
    compared
}

#[test]
fn recordings_of_codex_played_under_claude_codes_profile_are_shown_as_codex_drew_them() {
    // Codex's marks, its backgrounds and its Logical order are none of
    // them what Claude Code's profile looks for. Each recording holds a
    // whole Farsi reply, or Farsi typed into the Input box.
    for name in ["codex-reply", "codex-inline-reply", "codex-typing"] {
        let compared = assert_shown_as_drawn_throughout(name, AppProfile::claude_code());
        assert!(compared > 100, "{name}: {compared}");
    }
}

#[test]
fn recordings_of_claude_code_played_under_codexs_profile_are_shown_as_claude_code_drew_them() {
    for name in [
        "claude-classic-reply",
        "claude-fullscreen-reply",
        "claude-classic-typing",
        "claude-fullscreen-markdown-shapes",
    ] {
        let compared = assert_shown_as_drawn_throughout(name, AppProfile::codex());
        assert!(compared > 20, "{name}: {compared}");
    }
}

// ---------------------------------------------------------------------
// Screens of random cells
// ---------------------------------------------------------------------

/// A source of numbers that look random and are the same on every run.
struct Dice(u64);

impl Dice {
    fn new(seed: u64) -> Dice {
        Dice(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }

    /// A number from 0 up to, but not, `sides`.
    fn roll(&mut self, sides: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as usize % sides
    }

    fn chance(&mut self, in_a_hundred: usize) -> bool {
        self.roll(100) < in_a_hundred
    }

    fn pick<'a>(&mut self, of: &[&'a str]) -> &'a str {
        of[self.roll(of.len())]
    }
}

/// English letters, digits, spaces and punctuation.
const ENGLISH: &[&str] = &[
    "a", "b", "e", "k", "m", "z", "A", "Q", "0", "1", "7", "9", " ", " ", " ", " ", ".", ",", ":",
    ";", "!", "?", "(", ")", "[", "]", "{", "}", "<", ">", "\"", "'", "`", "-", "*", "+", "/", "=",
    "_", "|", "#", "@", "%", "&", "\\",
];

/// The marks the two apps draw, and the characters lines and boxes are
/// drawn with.
const MARKS: &[&str] = &[
    "●", "⏺", "❯", ">", "•", "›", "»", "─", "━", "│", "┃", "╭", "╮", "╰", "╯", "┌", "┐", "└", "┘",
    "├", "┤", "┼", "⎿", "▎", "▐", "▛", "█", "…", "✳", "◦",
];

/// A letter with an accent of its own, and a space that is not a space.
const ODD: &[&str] = &["e\u{301}", "\u{a0}", "é"];

/// Characters two cells wide.
const WIDE: &[&str] = &["日", "本", "語", "😀"];

/// Farsi letters, some with a Half-space or a vowel mark after them,
/// Persian and Arabic digits, Farsi punctuation, and two Hebrew letters. A
/// Half-space or vowel mark never comes alone: written after a cell the
/// app has not drawn, it would ride on that cell, which farcli leaves to
/// the Terminal.
const FARSI: &[&str] = &[
    "ا",
    "ب",
    "پ",
    "ت",
    "ج",
    "چ",
    "د",
    "ر",
    "ز",
    "ژ",
    "س",
    "ش",
    "ع",
    "ف",
    "ق",
    "ک",
    "گ",
    "ل",
    "م",
    "ن",
    "و",
    "ه",
    "ی",
    "آ",
    "ئ",
    "لا",
    "می\u{200c}",
    "ه\u{200c}",
    "ب\u{64e}",
    "ش\u{651}",
    "۰",
    "۱",
    "۲",
    "۹",
    "٣",
    "،",
    "؛",
    "؟",
    "«",
    "»",
    "א",
    "ב",
    " ",
    " ",
    " ",
];

const STYLES: &[&str] = &[
    "\x1b[0m",
    "\x1b[0m",
    "\x1b[0m",
    "\x1b[1m",
    "\x1b[2m",
    "\x1b[3m",
    "\x1b[4m",
    "\x1b[7m",
    "\x1b[31m",
    "\x1b[38;5;120m",
    "\x1b[38;2;177;185;249m",
    "\x1b[48;2;55;55;55m",
    "\x1b[22;23;27;39;49m",
];

/// The ways the two apps begin a row that means something to them: a
/// reply, a message the user sent, the Input box and its borders, each in
/// the styles and on the backgrounds the apps use, and some near misses.
const ROW_STARTS: &[&str] = &[
    "● ",
    "⏺ ",
    "\x1b[2m• \x1b[22m",
    "• ",
    "❯ ",
    "> ",
    "› ",
    "\x1b[1m›\x1b[22m ",
    "\x1b[1;2m› \x1b[22m",
    "\x1b[48;2;50;50;50m\x1b[1;2m› \x1b[22m",
    "\x1b[48;2;41;41;41m\x1b[1m›\x1b[22m ",
    "\x1b[48;2;55;55;55m❯ ",
    "──────────────────────────────────────────────",
    "  ",
    "  ",
    "    ",
    " ",
];

fn random_text(dice: &mut Dice, characters: usize, palettes: &[&[&str]]) -> String {
    (0..characters)
        .map(|_| {
            let palette = palettes[dice.roll(palettes.len())];
            dice.pick(palette)
        })
        .collect()
}

/// What an app might write for one frame on a screen `cols` by `rows`:
/// cells from `palettes` in any style at any place, rows that begin the
/// way the apps' own rows begin, and now and then a part of the screen
/// erased, rows scrolled off the top, or the alternate screen. Some frames
/// are marked as the apps mark theirs, and some of those never end.
fn random_frame(dice: &mut Dice, cols: usize, rows: usize, palettes: &[&[&str]]) -> String {
    let mut bytes = String::new();
    let marked = dice.chance(40);
    if marked {
        bytes.push_str("\x1b[?2026h");
    }
    for _ in 0..1 + dice.roll(25) {
        let row = 1 + dice.roll(rows);
        match dice.roll(24) {
            0 => bytes.push_str("\x1b[2J"),
            1 => bytes.push_str("\x1b[J"),
            2 => bytes.push_str("\x1b[2K"),
            3 => bytes.push_str("\x1b[K"),
            4 => bytes.push_str(dice.pick(&["\x1b[?1049h", "\x1b[?1049l"])),
            5 => bytes.push_str(dice.pick(&["\x1b[?25l", "\x1b[?25h"])),
            6 => {
                // Lines written at the bottom push rows off the top.
                bytes.push_str(&format!("\x1b[{rows};1H\x1b[0m"));
                for _ in 0..1 + dice.roll(4) {
                    let length = dice.roll(cols);
                    bytes.push_str(dice.pick(ROW_STARTS));
                    bytes.push_str(&random_text(dice, length, palettes));
                    bytes.push_str("\x1b[0m\r\n");
                }
            }
            7..=11 => {
                // A row that begins like one of the apps' own.
                let length = dice.roll(cols);
                bytes.push_str(&format!("\x1b[{row};1H\x1b[0m\x1b[2K"));
                bytes.push_str(dice.pick(ROW_STARTS));
                bytes.push_str(&random_text(dice, length, palettes));
                if dice.chance(30) {
                    bytes.push_str("\x1b[K");
                }
            }
            12..=14 => {
                // Rows of text from the third column, as a reply's are,
                // some filling the row to its end as a wrapped row does.
                for below in 0..1 + dice.roll(4) {
                    let length = match dice.chance(40) {
                        true => cols - 2,
                        false => dice.roll(cols),
                    };
                    bytes.push_str(&format!("\x1b[{};3H\x1b[0m", row + below));
                    bytes.push_str(&random_text(dice, length, palettes));
                }
            }
            _ => {
                let col = 1 + dice.roll(cols);
                let length = 1 + dice.roll(12);
                bytes.push_str(&format!("\x1b[{row};{col}H"));
                bytes.push_str(dice.pick(STYLES));
                bytes.push_str(&random_text(dice, length, palettes));
            }
        }
    }
    bytes.push_str(&format!(
        "\x1b[0m\x1b[{};{}H",
        1 + dice.roll(rows),
        1 + dice.roll(cols)
    ));
    if marked && dice.chance(90) {
        bytes.push_str("\x1b[?2026l");
    }
    bytes
}

fn profiles() -> [(&'static str, AppProfile); 3] {
    [
        ("Claude Code", AppProfile::claude_code()),
        ("Codex", AppProfile::codex()),
        ("any other program", AppProfile::plain()),
    ]
}

/// Plays `frames` random frames into a session for each of 60 seeds and
/// each App profile, and calls `look` after every frame that farcli has
/// shown: with the run, whether the correction is on, and words that say
/// which frame it is. With `switched`, the user switches the correction
/// off and on now and then, and with `resized` the window changes size.
fn play_random_frames(
    palettes: &[&[&str]],
    switched: bool,
    resized: bool,
    mut look: impl FnMut(&Run, bool, &str),
) {
    for seed in 0..60 {
        for (app, profile) in profiles() {
            let mut dice = Dice::new(seed);
            let (mut cols, mut rows) = (30 + dice.roll(30), 6 + dice.roll(10));
            let mut run = Run::start(profile, cols as u16, rows as u16);
            let mut correcting = true;
            for frame in 0..25 {
                run.app_writes(random_frame(&mut dice, cols, rows, palettes));
                if resized && dice.chance(8) {
                    (cols, rows) = (20 + dice.roll(50), 4 + dice.roll(14));
                    run.terminal_resizes(cols as u16, rows as u16);
                }
                if switched && dice.chance(15) {
                    run.user_types(F12);
                    correcting = !correcting;
                }
                if run.session.holding_frame() {
                    // The app never ended its frame. Half the time farcli
                    // gives up waiting, as it does when a frame takes too
                    // long, and shows what there is.
                    if dice.chance(50) {
                        continue;
                    }
                    let reaction = run.session.release_held_frame();
                    run.terminal.receive(&reaction.for_terminal);
                }
                look(
                    &run,
                    correcting,
                    &format!("seed {seed}, as {app}, frame {frame}"),
                );
            }
        }
    }
}

/// F12 going down and coming up, as Windows Terminal sends it.
const F12: &str = "\x1b[123;88;0;1;0;1_\x1b[123;88;0;0;0;1_";

#[test]
fn screens_of_random_cells_that_hold_no_farsi_are_shown_as_drawn_whatever_they_look_like() {
    // Every mark and border the two apps draw is among the cells, in the
    // places the apps draw them and in every other place. With no letter
    // of a right-to-left script on the screen there is nothing to correct,
    // so whatever a Region finder takes these screens for, the Terminal
    // must show them as they are.
    let mut looked = 0;

    play_random_frames(
        &[ENGLISH, ENGLISH, MARKS, ODD],
        false,
        false,
        |run, _, which| {
            looked += 1;
            assert!(
                run.screen_is_as_drawn(),
                "{which}:\n{}",
                run.rows_not_as_drawn().join("\n")
            );
        },
    );

    assert!(looked > 2500, "{looked}");
}

/// The characters of a row that no correction changes or moves to another
/// row: English letters and digits, the apps' marks, lines and the sides
/// of boxes. Sorted, because a correction may put them in another order on
/// the row.
fn unchanging_characters(row: &Row) -> Vec<char> {
    // Marks that Layout turns round in text that runs from the right.
    let turned_round = ['>', '»', '›'];
    let mut characters: Vec<char> = row
        .iter()
        .map(|cell| cell.character)
        .filter(|character| {
            character.is_ascii_alphanumeric()
                || MARKS.iter().any(|mark| mark.starts_with(*character))
        })
        .filter(|character| !turned_round.contains(character))
        .collect();
    characters.sort_unstable();
    characters
}

#[test]
fn random_farsi_among_the_apps_marks_loses_no_character_and_is_shown_as_drawn_when_switched_off() {
    // Farsi in every place and in no order, between the apps' marks and
    // borders, while the correction is switched off and on. The window
    // keeps its size: when it changes, a Terminal moves rows between its
    // screen and what it keeps for scrolling back, in its own way, and
    // the two Terminals compared here would no longer show the same rows.
    let palettes = [FARSI, FARSI, FARSI, ENGLISH, MARKS, ODD];
    let (mut looked_on, mut looked_off, mut corrected) = (0, 0, 0);

    play_random_frames(&palettes, true, false, |run, correcting, which| {
        if !correcting {
            looked_off += 1;
            assert!(
                run.screen_is_as_drawn(),
                "{which}, correction off:\n{}",
                run.rows_not_as_drawn().join("\n")
            );
            return;
        }
        looked_on += 1;
        corrected += usize::from(!run.screen_is_as_drawn());
        // Whatever farcli made of a row, what it shows on that row is what
        // the app drew on it.
        let shown = run.terminal.screen();
        let drawn = run.without_farcli.screen();
        for (row, (shown, drawn)) in shown.iter().zip(&drawn).enumerate() {
            assert_eq!(
                unchanging_characters(shown),
                unchanging_characters(drawn),
                "{which}, row {row}: {:?} shown as {:?}",
                run.drawn_row(row),
                run.row(row)
            );
        }
    });

    assert!(
        looked_on > 1000 && looked_off > 1000,
        "{looked_on} {looked_off}"
    );
    // The screens are ones that the correction does lay hands on.
    assert!(corrected > 500, "{corrected}");
}

#[test]
fn nothing_an_app_draws_makes_farcli_fall_over() {
    // The same with the window changing size, and with characters two
    // cells wide thrown in. Half of such a
    // character can be left on a screen when the other half is drawn
    // over, and no Terminal can be made to show that half again, so here
    // the rows are not compared: farcli must only get through it all and
    // still be drawing afterwards.
    let palettes = [FARSI, FARSI, ENGLISH, MARKS, ODD, WIDE];
    let mut looked = 0;

    play_random_frames(&palettes, true, true, |run, _, which| {
        looked += 1;
        assert_eq!(
            run.terminal.screen().len(),
            run.without_farcli.screen().len(),
            "{which}"
        );
    });

    assert!(looked > 2500, "{looked}");
    for (app, profile) in profiles() {
        let mut run = Run::start(profile, 40, 6);
        let mut dice = Dice::new(7);
        for _ in 0..200 {
            run.app_writes(random_frame(&mut dice, 40, 6, &palettes));
        }
        run.app_writes("\x1b[?2026l\x1b[?1049l\x1b[0m\x1b[2J\x1b[Hstill drawing");
        assert_eq!(run.row(0), "still drawing", "as {app}");
    }
}

// ---------------------------------------------------------------------
// Layouts a later version of an app might draw
// ---------------------------------------------------------------------

/// Claude Code's Input box as it is today: two borders and the prompt mark
/// between them.
fn claude_code_input_box(cols: usize) -> String {
    let border = "─".repeat(cols);
    format!("{border}\r\n❯ \r\n{border}\r\n")
}

/// A box with a line of Farsi in it, which neither app draws today.
const PANEL: [&str; 3] = [
    "╭────────────────╮",
    "│ متن درون کادر  │",
    "╰────────────────╯",
];

#[test]
fn a_claude_code_reply_under_a_mark_farcli_does_not_know_is_shown_as_drawn() {
    let paragraph = "سلام دنیا، این یک پاسخ است";
    let mut run = Run::start(AppProfile::claude_code(), 40, 10);

    // Everything is as today but the mark before the reply.
    run.app_writes(format!(
        "◆{}\r\n\r\n{}",
        placed(2, &as_drawn(paragraph)),
        claude_code_input_box(40)
    ));

    assert_eq!(run.row(0), format!("◆ {}", as_drawn(paragraph)));
    assert!(run.screen_is_as_drawn(), "{:?}", run.rows_not_as_drawn());
}

#[test]
fn a_box_in_a_claude_code_reply_is_left_as_drawn_and_the_paragraphs_around_it_are_corrected() {
    let (first, last) = ("این پاراگراف اول است", "و این پاراگراف آخر است");
    let mut run = Run::start(AppProfile::claude_code(), 40, 12);

    run.app_writes(claude_code::reply(&[&[first][..], &PANEL[..], &[last][..]]));

    assert_eq!(
        run.row(0),
        format!("● {}", laid_out(first, Direction::RightToLeft, 38))
    );
    for row in 2..5 {
        assert!(run.row_is_as_drawn(row), "{:?}", run.rows_not_as_drawn());
    }
    assert_eq!(run.drawn_row(2), "  ╭────────────────╮");
    assert_eq!(
        run.row(6),
        format!("  {}", laid_out(last, Direction::RightToLeft, 38))
    );
}

#[test]
fn a_box_in_a_codex_reply_is_left_as_drawn_and_the_paragraphs_around_it_are_corrected() {
    let (first, last) = ("این پاراگراف اول است", "و این پاراگراف آخر است");
    let mut run = Run::start(AppProfile::codex(), 40, 12);

    run.app_writes(codex::screen(&[
        codex::reply_start(first),
        String::new(),
        codex::reply_row(PANEL[0]),
        codex::reply_row(PANEL[1]),
        codex::reply_row(PANEL[2]),
        String::new(),
        codex::reply_row(last),
    ]));

    assert_eq!(
        run.row(0),
        format!("• {}", laid_out(first, Direction::RightToLeft, 38))
    );
    for row in 2..5 {
        assert!(run.row_is_as_drawn(row), "{:?}", run.rows_not_as_drawn());
    }
    assert_eq!(run.drawn_row(3), "  │ متن درون کادر  │");
    assert_eq!(
        run.row(6),
        format!("  {}", laid_out(last, Direction::RightToLeft, 38))
    );
}

#[test]
fn text_that_shares_its_rows_with_a_border_is_left_as_drawn_in_claude_code() {
    // A later version draws something beside the reply, set off by a line
    // down the screen. Laying the rows out again would move the line.
    let beside_a_border = "این متن پاسخ است      │ ستون کناری";
    let after = "این پاراگراف بعدی است";
    let mut run = Run::start(AppProfile::claude_code(), 50, 10);

    run.app_writes(claude_code::reply(&[&[beside_a_border], &[after]]));

    assert_eq!(run.drawn_row(0), format!("● {}", as_drawn(beside_a_border)));
    assert!(run.drawn_row(0).contains('│'));
    assert!(run.row_is_as_drawn(0), "{:?}", run.rows_not_as_drawn());
    // What follows is a paragraph like any other.
    assert_eq!(
        run.row(2),
        format!("  {}", laid_out(after, Direction::RightToLeft, 48))
    );
}

#[test]
fn text_that_shares_its_rows_with_a_border_is_left_as_drawn_in_codex() {
    let beside_a_border = "این متن پاسخ است      │ ستون کناری";
    let after = "این پاراگراف بعدی است";
    let mut run = Run::start(AppProfile::codex(), 50, 10);

    run.app_writes(codex::screen(&[
        codex::reply_start(beside_a_border),
        String::new(),
        codex::reply_row(after),
    ]));

    assert_eq!(run.row(0), format!("• {beside_a_border}"));
    assert!(run.row_is_as_drawn(0), "{:?}", run.rows_not_as_drawn());
    assert_eq!(
        run.row(2),
        format!("  {}", laid_out(after, Direction::RightToLeft, 48))
    );
}

#[test]
fn a_codex_that_draws_its_replies_on_a_background_is_shown_as_drawn() {
    let mut run = Run::start(AppProfile::codex(), 40, 8);

    // Today only the message sent and the Input box have a background.
    run.app_writes(codex::screen(&[
        format!(
            "\x1b[48;2;30;30;46m{}\x1b[K",
            codex::reply_start("سلام دنیا")
        ),
        format!(
            "\x1b[48;2;30;30;46m{}\x1b[K",
            codex::reply_row("این خط دوم است")
        ),
    ]));

    assert_eq!(run.row(0), "• سلام دنیا");
    assert!(run.screen_is_as_drawn(), "{:?}", run.rows_not_as_drawn());
}

#[test]
fn a_codex_reply_under_a_mark_in_a_style_farcli_does_not_know_is_shown_as_drawn() {
    let mut run = Run::start(AppProfile::codex(), 40, 8);

    // Codex draws a reply's mark dim. In any other style the same mark
    // starts something of the app's own.
    run.app_writes(codex::screen(&[
        "• سلام دنیا".to_string(),
        codex::reply_row("این خط دوم است"),
        String::new(),
        codex::input_box(""),
    ]));

    assert_eq!(run.row(0), "• سلام دنیا");
    assert!(run.screen_is_as_drawn(), "{:?}", run.rows_not_as_drawn());
}

#[test]
fn a_screen_that_shows_nothing_of_the_apps_own_is_shown_as_drawn_on_the_alternate_screen_too() {
    // Text from the third column under the top of the alternate screen is
    // how the middle of a long reply looks when the user has scrolled. By
    // itself that is not enough to go by.
    let rows = [
        "  این متن از ستون سوم شروع می‌شود",
        "  و خط دوم آن",
        "",
        "  پاراگراف دیگر",
    ];
    for (app, profile) in profiles() {
        if profile == AppProfile::plain() {
            continue;
        }
        let mut run = Run::start(profile, 40, 8);

        run.app_writes(format!("\x1b[?1049h\x1b[2J\x1b[H{}", rows.join("\r\n")));

        assert!(
            run.screen_is_as_drawn(),
            "as {app}: {:?}",
            run.rows_not_as_drawn()
        );
        assert_eq!(run.row(0), rows[0], "as {app}");
    }
}
