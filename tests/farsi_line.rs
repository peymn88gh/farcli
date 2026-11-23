//! A line of Farsi printed by any program run through farcli reads correctly:
//! joined, in Screen order and right-aligned.
//!
//! Every test goes through the session: it feeds the bytes a program wrote and
//! checks what a Terminal shows.
//!
//! The joined letters a Terminal should show are written here as text that
//! reads from the right, the way a person reads it, and `from_the_right`
//! turns that into Screen order.

use farcli::app_profile::AppProfile;
use farcli::fake_terminal::FakeTerminal;
use farcli::screen::{CellWidth, Color, Position, Style};
use farcli::session::Session;
use farcli::Size;

/// A session joined to the Terminal that shows its output.
struct Run {
    session: Session,
    terminal: FakeTerminal,
}

impl Run {
    /// Starts a program farcli knows nothing about in an empty Terminal.
    fn start(cols: u16, rows: u16) -> Run {
        Run::start_in(FakeTerminal::new(Size { cols, rows }))
    }

    /// Starts in a Terminal that may already show something.
    fn start_in(terminal: FakeTerminal) -> Run {
        let session = Session::start(AppProfile::plain(), terminal.size(), terminal.cursor());
        Run { session, terminal }
    }

    fn app_writes(&mut self, bytes: impl AsRef<[u8]>) {
        let reaction = self.session.app_wrote(bytes.as_ref());
        self.terminal.receive(&reaction.for_terminal);
    }
}

/// The first row of a Terminal `cols` wide after a program wrote `line`.
fn shown(cols: u16, line: &str) -> String {
    let mut run = Run::start(cols, 3);
    run.app_writes(format!("{line}\r\n"));
    run.terminal.screen_text().remove(0)
}

/// Text that reads from the right, as it sits on a screen from the left.
fn from_the_right(text: &str) -> String {
    text.chars().rev().collect()
}

/// A row `cols` wide with `text` against its right edge.
fn at_right_edge(cols: usize, text: &str) -> String {
    format!("{text:>cols$}")
}

fn at(row: u16, col: u16) -> Position {
    Position { row, col }
}

#[test]
fn a_farsi_word_is_shown_joined_and_right_to_left_at_the_right_edge() {
    let row = shown(20, "سلام");

    assert_eq!(row, at_right_edge(20, &from_the_right("ﺳﻠﺎﻡ")));
}

#[test]
fn a_file_name_and_a_number_in_a_farsi_sentence_read_left_to_right_in_their_place() {
    let row = shown(50, "فایل config.json را باز کن و 123 خط اضافه کن");

    // The sentence starts at the right edge. The file name and the number sit
    // where the sentence reaches them, each reading left to right.
    let on_screen = [
        from_the_right(" ﺧﻂ ﺍﺿﺎﻓﻪ ﮐﻦ").as_str(),
        "123",
        from_the_right(" ﺭﺍ ﺑﺎﺯ ﮐﻦ ﻭ ").as_str(),
        "config.json",
        from_the_right("ﻓﺎﯾﻞ ").as_str(),
    ]
    .concat();
    assert_eq!(row, at_right_edge(50, &on_screen));
}

#[test]
fn brackets_in_a_farsi_sentence_face_the_text_they_enclose() {
    let row = shown(30, "متن (مهم) و [config] است «گل»");

    // On screen each pair opens on the left and closes on the right, around
    // the same word it enclosed in the sentence.
    let on_screen = [
        "«",
        from_the_right("ﮔﻞ").as_str(),
        "» ",
        from_the_right("ﺍﺳﺖ").as_str(),
        " [config] ",
        from_the_right("ﻭ").as_str(),
        " (",
        from_the_right("ﻣﻬﻢ").as_str(),
        ") ",
        from_the_right("ﻣﺘﻦ").as_str(),
    ]
    .concat();
    assert_eq!(row, at_right_edge(30, &on_screen));
}

#[test]
fn brackets_around_english_in_a_farsi_sentence_stay_a_pair() {
    let row = shown(30, "سلام (hello) world");

    let on_screen = ["world (hello) ", from_the_right("ﺳﻠﺎﻡ").as_str()].concat();
    assert_eq!(row, at_right_edge(30, &on_screen));
}

#[test]
fn a_line_that_starts_with_an_english_letter_stays_left_to_right_with_its_farsi_joined() {
    let row = shown(40, "The word سلام means hello (درود).");

    let on_screen = [
        "The word ",
        from_the_right("ﺳﻠﺎﻡ").as_str(),
        " means hello (",
        from_the_right("ﺩﺭﻭﺩ").as_str(),
        ").",
    ]
    .concat();
    assert_eq!(row, on_screen);
}

#[test]
fn a_half_space_ends_the_join_and_leaves_no_gap() {
    let row = shown(20, "می\u{200c}خواهم");

    // The yeh ends its word and the kheh starts a new one, in the next cell.
    assert_eq!(row, at_right_edge(20, &from_the_right("ﻣﯽﺧﻮﺍﻫﻢ")));
}

#[test]
fn the_letters_only_farsi_has_are_joined_at_the_start_middle_and_end_of_a_word() {
    let row = shown(30, "پپپ چچچ گگگ ککک ییی ژبژ");

    // Zheh never joins the letter after it, so it only has an end form.
    assert_eq!(
        row,
        at_right_edge(30, &from_the_right("ﭘﭙﭗ ﭼﭽﭻ ﮔﮕﮓ ﮐﮑﮏ ﯾﯿﯽ ﮊﺑﮋ"))
    );
}

#[test]
fn lam_and_alef_are_drawn_as_two_joined_letters_in_two_cells() {
    let row = shown(10, "لا کلا");

    assert_eq!(row, at_right_edge(10, &from_the_right("ﻟﺎ ﮐﻠﺎ")));
}

#[test]
fn a_border_after_the_text_stays_aligned_from_row_to_row() {
    let mut run = Run::start(20, 8);

    // Each row is eight cells of text, a space and a border. The rows hold
    // lam and alef, a Half-space, and Farsi inside English.
    run.app_writes("کتاب خوب |\r\n");
    run.app_writes("سلام کلا |\r\n");
    run.app_writes("می\u{200c}خواهم  |\r\n");
    run.app_writes("abcdefgh |\r\n");
    run.app_writes("a سلام c |\r\n");
    run.app_writes("a کلا لا |\r\n");
    run.app_writes("a می\u{200c}رویم |\r\n");

    // A right-to-left row ends at the right edge, so its border is on the
    // left of the text. A left-to-right row keeps its border where it was.
    for row in 0..3 {
        assert_eq!(run.terminal.cell(at(row, 10)).character, '|', "row {row}");
    }
    for row in 3..7 {
        assert_eq!(run.terminal.cell(at(row, 9)).character, '|', "row {row}");
    }
}

#[test]
fn every_character_sent_takes_one_cell_of_its_own() {
    let mut run = Run::start(20, 3);

    run.app_writes("می\u{200c}خواهم لا\r\n");

    for cell in &run.terminal.screen()[0] {
        assert_eq!(cell.width, CellWidth::One);
        assert_eq!(cell.marks, [] as [char; 0]);
    }
}

#[test]
fn a_vowel_mark_stays_on_its_letter_and_takes_no_cell() {
    let mut run = Run::start(20, 3);

    // «لطفاً»: the mark is written after the alef, the word's last letter.
    run.app_writes("لطفا\u{64b}\r\n");

    let alef = run.terminal.cell(at(0, 16));
    assert_eq!(alef.character, 'ﺎ');
    assert_eq!(alef.marks, ['\u{64b}']);
    assert_eq!(run.terminal.cell(at(0, 15)).character, ' ');
    assert_eq!(run.terminal.cell(at(0, 19)).character, 'ﻟ');
}

#[test]
fn a_line_with_no_farsi_is_shown_as_the_program_wrote_it() {
    let output = "\x1b[32mok\x1b[m see (config.json) [1, 2] <a> «b» 50% -3\r\n";
    let mut run = Run::start(50, 3);

    run.app_writes(output);

    let mut direct = FakeTerminal::new(Size { cols: 50, rows: 3 });
    direct.receive(output.as_bytes());
    assert_eq!(run.terminal.screen(), direct.screen());
    assert_eq!(run.terminal.cursor(), direct.cursor());
}

#[test]
fn colours_and_styles_stay_on_the_letters_they_were_given_to() {
    let mut run = Run::start(20, 3);

    run.app_writes("\x1b[1;31mسلام\x1b[m دنیا\r\n");

    // «سلام» is at the right edge and still bold red, from its first letter
    // in the last column to its last letter four columns in.
    let seen = run.terminal.cell(at(0, 19));
    assert_eq!(seen.character, 'ﺳ');
    assert!(seen.style.bold);
    assert_eq!(seen.style.foreground, Color::Palette(1));
    let meem = run.terminal.cell(at(0, 16));
    assert_eq!(meem.character, 'ﻡ');
    assert_eq!(meem.style, seen.style);
    // The space and «دنیا» to its left are plain.
    assert_eq!(run.terminal.cell(at(0, 15)).style, Style::default());
    let dal = run.terminal.cell(at(0, 14));
    assert_eq!(dal.character, 'ﺩ');
    assert_eq!(dal.style, Style::default());
}

#[test]
fn a_farsi_line_reads_correctly_while_it_arrives_letter_by_letter() {
    let mut run = Run::start(20, 3);

    run.app_writes("س");
    assert_eq!(run.terminal.screen_text()[0], at_right_edge(20, "ﺱ"));

    run.app_writes("ل");
    assert_eq!(
        run.terminal.screen_text()[0],
        at_right_edge(20, &from_the_right("ﺳﻞ"))
    );

    run.app_writes("ام");
    assert_eq!(
        run.terminal.screen_text()[0],
        at_right_edge(20, &from_the_right("ﺳﻠﺎﻡ"))
    );
}

#[test]
fn a_farsi_line_that_scrolled_off_the_top_still_reads_correctly_when_scrolling_back() {
    let mut run = Run::start(20, 2);

    run.app_writes("سلام\r\nدنیا\r\nخوب\r\n");

    assert_eq!(
        run.terminal.scrollback_text(),
        [
            at_right_edge(20, &from_the_right("ﺳﻠﺎﻡ")),
            at_right_edge(20, &from_the_right("ﺩﻧﯿﺎ")),
        ]
    );
    assert_eq!(
        run.terminal.screen_text()[0],
        at_right_edge(20, &from_the_right("ﺧﻮﺏ"))
    );
}

#[test]
fn a_character_two_cells_wide_in_a_farsi_line_stays_whole() {
    let mut run = Run::start(20, 3);

    run.app_writes("✅ انجام شد\r\n");

    let tick = run.terminal.cell(at(0, 18));
    assert_eq!(tick.character, '✅');
    assert_eq!(tick.width, CellWidth::Two);
    assert_eq!(
        run.terminal.screen_text()[0].trim_start(),
        [from_the_right("ﺍﻧﺠﺎﻡ ﺷﺪ").as_str(), " ✅"].concat()
    );
}

#[test]
fn the_cursor_is_shown_where_the_map_puts_the_programs_cursor() {
    let mut run = Run::start(20, 3);

    // After a right-to-left line the next character will appear to the left
    // of the text: «نام: » fills the last five columns.
    run.app_writes("نام: ");
    assert_eq!(run.terminal.cursor(), at(0, 14));

    // The program moves its cursor onto the second letter it wrote, the
    // alef, which is shown second from the right edge.
    run.app_writes("\x1b[1;2H");
    assert_eq!(run.terminal.cursor(), at(0, 18));
}

#[test]
fn the_cursor_stays_put_on_a_left_to_right_line_until_it_is_on_farsi() {
    let mut run = Run::start(20, 3);

    run.app_writes("Name نام: ");
    assert_eq!(run.terminal.cursor(), at(0, 10));

    // The program moves onto the first Farsi letter it wrote, the noon,
    // which is shown at the right end of its word.
    run.app_writes("\x1b[1;6H");
    assert_eq!(run.terminal.cursor(), at(0, 7));
}

#[test]
fn claude_code_and_codex_are_shown_as_they_drew_until_farcli_knows_their_screens() {
    for profile in [AppProfile::claude_code(), AppProfile::codex()] {
        let terminal = FakeTerminal::new(Size { cols: 20, rows: 3 });
        let session = Session::start(profile, terminal.size(), terminal.cursor());
        let mut run = Run { session, terminal };

        run.app_writes("│ سلام │\r\n");

        assert_eq!(run.terminal.screen_text()[0], "│ سلام │");
    }
}

#[test]
fn cells_the_program_skipped_inside_a_farsi_line_count_as_empty() {
    // The program writes a word, moves three cells on without drawing them,
    // and draws a border.
    let row = shown(20, "سلام\x1b[3C|");

    let on_screen = ["|   ", from_the_right("ﺳﻠﺎﻡ").as_str()].concat();
    assert_eq!(row, at_right_edge(20, &on_screen));
}

#[test]
fn a_farsi_line_written_after_text_farcli_did_not_draw_is_left_as_the_program_wrote_it() {
    let mut terminal = FakeTerminal::new(Size { cols: 20, rows: 3 });
    terminal.receive("PS> ".as_bytes());
    let mut run = Run::start_in(terminal);

    run.app_writes("سلام\r\n");

    assert_eq!(run.terminal.screen_text()[0], "PS> سلام");
}
