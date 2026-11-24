//! Painter: compares the screen farcli wants with what the Terminal already
//! shows and sends the difference, keeping styles.

use std::fmt::Write as _;
use std::mem;

use crate::screen::{
    Cell, CellWidth, Color, EraseScreen, Position, Row, ScreenChange, Style, Underline,
};
use crate::Size;

/// The screen farcli wants the Terminal to show.
pub struct Frame {
    /// What happened to the whole screen since the frame before, in order.
    /// Rows that scrolled off the top are written to the Terminal first, so
    /// that scrolling back finds them.
    pub changes: Vec<ScreenChange>,
    /// The rows wanted on screen once all of `changes` has happened.
    pub rows: Vec<Row>,
    pub cursor: Position,
    pub cursor_visible: bool,
}

pub struct Painter {
    size: Size,
    /// What the Terminal shows now.
    shown: Shown,
    /// What the Terminal's main screen shows, while the Terminal is on its
    /// alternate screen.
    main_screen_behind: Option<Shown>,
    /// Where the Terminal's cursor is, when that is certain.
    cursor: Option<Position>,
    /// The style the Terminal gives the next character, when that is certain.
    pen: Option<Style>,
    /// Whether the Terminal draws its cursor, when that is certain.
    cursor_visible: Option<bool>,
}

/// What the Painter believes one of the Terminal's screens shows.
struct Shown {
    rows: Vec<Row>,
    /// False when `rows` is no more than a guess, as after the Terminal has
    /// rearranged its text for a new size.
    certain: bool,
}

impl Shown {
    fn empty(size: Size, certain: bool) -> Shown {
        Shown::filled(size, Cell::blank(), certain)
    }

    fn filled(size: Size, cell: Cell, certain: bool) -> Shown {
        Shown {
            rows: vec![vec![cell; usize::from(size.cols)]; usize::from(size.rows)],
            certain,
        }
    }
}

impl Painter {
    /// A Painter for a Terminal of `size`. Whatever the Terminal shows
    /// already stays wherever the frame's cells are left alone
    /// (`Cell::left_alone`).
    pub fn new(size: Size) -> Painter {
        Painter {
            size,
            shown: Shown::filled(size, Cell::left_alone(), true),
            main_screen_behind: None,
            cursor: None,
            pen: None,
            cursor_visible: None,
        }
    }

    /// The Terminal has a new size and has rearranged what it shows in its
    /// own way. The next frame is drawn in full.
    pub fn terminal_resized(&mut self, size: Size) {
        self.size = size;
        self.shown = Shown::empty(size, false);
        if self.main_screen_behind.is_some() {
            self.main_screen_behind = Some(Shown::empty(size, false));
        }
        self.cursor = None;
    }

    /// Returns the bytes that make the Terminal show `frame`.
    pub fn paint(&mut self, frame: &Frame) -> Vec<u8> {
        let mut drawing = String::new();
        self.make_certain(&mut drawing);
        for change in &frame.changes {
            match change {
                ScreenChange::ScrolledOff(row) => self.scroll_off(&mut drawing, row),
                ScreenChange::Erased { part, cursor } => self.erase(&mut drawing, *part, *cursor),
                ScreenChange::AlternateScreen(alternate) => {
                    self.switch_screen(&mut drawing, *alternate)
                }
            }
        }
        for (row, wanted) in frame.rows.iter().enumerate() {
            self.paint_row(&mut drawing, row, wanted);
        }
        if self
            .pen
            .as_ref()
            .is_some_and(|pen| *pen != Style::default())
        {
            // Whatever writes to the Terminal next starts from plain text.
            self.set_pen(&mut drawing, &Style::default());
        }

        let mut out = String::new();
        if !drawing.is_empty() {
            // The cursor would otherwise be seen jumping about while drawing.
            self.show_cursor(&mut out, false);
        }
        out.push_str(&drawing);
        self.move_cursor(&mut out, frame.cursor);
        self.show_cursor(&mut out, frame.cursor_visible);
        out.into_bytes()
    }

    /// Empties every row of a screen whose content is only a guess, so that
    /// what it shows is known again.
    fn make_certain(&mut self, out: &mut String) {
        if self.shown.certain {
            return;
        }
        self.set_pen(out, &Style::default());
        for row in 0..self.size.rows {
            self.move_cursor(out, Position { row, col: 0 });
            out.push_str("\x1b[2K");
        }
        self.shown = Shown::empty(self.size, true);
    }

    /// Makes the Terminal's top row `row` and then scrolls it off the top,
    /// into what the Terminal keeps for scrolling back.
    fn scroll_off(&mut self, out: &mut String, row: &Row) {
        self.paint_row(out, 0, row);
        // A Terminal may give the new bottom row the pen's background.
        self.set_pen(out, &Style::default());
        let bottom = self.size.rows.saturating_sub(1);
        if self.cursor.is_none_or(|at| at.row != bottom) {
            self.move_cursor(
                out,
                Position {
                    row: bottom,
                    col: 0,
                },
            );
        }
        // The line feed scrolls. The carriage return makes the cursor's
        // column certain whether or not the Terminal adds one of its own.
        out.push_str("\r\n");
        self.cursor = Some(Position {
            row: bottom,
            col: 0,
        });
        self.shown.rows.remove(0);
        self.shown
            .rows
            .push(vec![Cell::blank(); usize::from(self.size.cols)]);
    }

    /// Has the Terminal clear the same part of its screen that the Target
    /// app cleared, in the Terminal's own way. A Terminal may do more than
    /// empty the cells: some keep a cleared screen for scrolling back.
    fn erase(&mut self, out: &mut String, part: EraseScreen, cursor: Position) {
        self.move_cursor(out, cursor);
        self.set_pen(out, &Style::default());
        let cols = usize::from(self.size.cols);
        let cells = usize::from(self.size.rows) * cols;
        let at_cursor = usize::from(cursor.row) * cols + usize::from(cursor.col);
        let (code, cleared) = match part {
            EraseScreen::Below => ("\x1b[J", at_cursor..cells),
            EraseScreen::Above => ("\x1b[1J", 0..at_cursor + 1),
            EraseScreen::All => ("\x1b[2J", 0..cells),
            EraseScreen::Scrollback => ("\x1b[3J", 0..0),
        };
        out.push_str(code);
        let cleared_len = cleared.len();
        let cells = self.shown.rows.iter_mut().flatten();
        for cell in cells.skip(cleared.start).take(cleared_len) {
            *cell = Cell::blank();
        }
    }

    /// Moves the Terminal to its alternate screen, or back to its main one.
    fn switch_screen(&mut self, out: &mut String, alternate: bool) {
        if alternate == self.main_screen_behind.is_some() {
            return;
        }
        if alternate {
            // The Terminal starts its alternate screen empty.
            out.push_str("\x1b[?1049h");
            let main_screen = mem::replace(&mut self.shown, Shown::empty(self.size, true));
            self.main_screen_behind = Some(main_screen);
        } else if let Some(main_screen) = self.main_screen_behind.take() {
            // The Terminal brings its main screen back as it left it.
            out.push_str("\x1b[?1049l");
            self.shown = main_screen;
        }
        self.cursor = None;
        self.make_certain(out);
    }

    fn paint_row(&mut self, out: &mut String, row: usize, wanted: &Row) {
        let Some(shown) = self.shown.rows.get(row) else {
            return;
        };
        if shown == wanted {
            return;
        }
        let cols = wanted.len().min(shown.len());

        // The empty cells at the end of the row are erased in one go, the
        // way a Target app does it, instead of being written as spaces.
        let text_end = wanted[..cols]
            .iter()
            .rposition(|cell| !cell.is_blank())
            .map_or(0, |last| last + 1);
        if shown[text_end..cols].iter().any(|cell| !cell.is_blank()) {
            self.move_cursor(
                out,
                Position {
                    row: row as u16,
                    col: text_end as u16,
                },
            );
            self.set_pen(out, &Style::default());
            out.push_str("\x1b[K");
            self.shown.rows[row][text_end..cols].fill(Cell::blank());
        }

        // A cell the app has not drawn is left as the Terminal has it. One
        // that farcli drew on all the same, to lay out the row it is in,
        // is emptied again: what the Terminal showed there before is gone.
        let emptied = Cell::blank();
        let mut col = 0;
        while col < text_end {
            let shown = &self.shown.rows[row][col];
            if wanted[col] == *shown {
                col += 1;
                continue;
            }
            if wanted[col].is_left_alone() {
                if !shown.is_left_alone() && !shown.is_blank() {
                    self.move_cursor(
                        out,
                        Position {
                            row: row as u16,
                            col: col as u16,
                        },
                    );
                    self.set_pen(out, &emptied.style);
                    out.push(emptied.character);
                    self.cursor = match self.cursor {
                        Some(at) if col + 1 < cols => Some(Position {
                            row: at.row,
                            col: (col + 1) as u16,
                        }),
                        _ => None,
                    };
                    self.shown.rows[row][col] = emptied.clone();
                }
                col += 1;
                continue;
            }
            // A wide character is always written from its first cell.
            let start = match wanted[col].width {
                CellWidth::SecondHalf if col > 0 => col - 1,
                _ => col,
            };
            let cell = &wanted[start];
            let width = match cell.width {
                CellWidth::Two => 2,
                _ => 1,
            };
            self.move_cursor(
                out,
                Position {
                    row: row as u16,
                    col: start as u16,
                },
            );
            self.set_pen(out, &cell.style);
            out.push(cell.character);
            out.extend(&cell.marks);
            self.cursor = match self.cursor {
                Some(at) if advance_is_certain(cell) && start + width < cols => Some(Position {
                    row: at.row,
                    col: (start + width) as u16,
                }),
                _ => None,
            };
            let end = (start + width).min(cols);
            self.shown.rows[row][start..end].clone_from_slice(&wanted[start..end]);
            col = end;
        }
    }

    fn move_cursor(&mut self, out: &mut String, to: Position) {
        if self.cursor != Some(to) {
            let _ = write!(out, "\x1b[{};{}H", to.row + 1, to.col + 1);
            self.cursor = Some(to);
        }
    }

    fn show_cursor(&mut self, out: &mut String, visible: bool) {
        if self.cursor_visible != Some(visible) {
            out.push_str(if visible { "\x1b[?25h" } else { "\x1b[?25l" });
            self.cursor_visible = Some(visible);
        }
    }

    fn set_pen(&mut self, out: &mut String, style: &Style) {
        if self.pen.as_ref() == Some(style) {
            return;
        }
        out.push_str(&style_codes(style));
        let old_link = self.pen.as_ref().and_then(|pen| pen.link.as_ref());
        if self.pen.is_none() || old_link != style.link.as_ref() {
            match &style.link {
                Some(link) => {
                    let _ = write!(out, "\x1b]8;id={};{}\x1b\\", link.id, link.uri);
                }
                None => out.push_str("\x1b]8;;\x1b\\"),
            }
        }
        self.pen = Some(style.clone());
    }
}

/// True when every Terminal moves its cursor exactly one cell to the right
/// after drawing this cell. After any other cell the Painter says where the
/// cursor goes instead of trusting the Terminal.
fn advance_is_certain(cell: &Cell) -> bool {
    cell.width == CellWidth::One
        && cell.marks.is_empty()
        && matches!(cell.character, ' '..='~' | '\u{2500}'..='\u{259f}')
}

/// The codes that give the Terminal's pen exactly this style.
fn style_codes(style: &Style) -> String {
    let mut codes = String::from("\x1b[0");
    let flags = [
        (style.bold, "1"),
        (style.dim, "2"),
        (style.italic, "3"),
        (style.inverse, "7"),
        (style.hidden, "8"),
        (style.strikeout, "9"),
    ];
    for (_, code) in flags.iter().filter(|(on, _)| *on) {
        codes.push(';');
        codes.push_str(code);
    }
    match style.underline {
        Underline::None => {}
        Underline::Single => codes.push_str(";4"),
        Underline::Double => codes.push_str(";21"),
        Underline::Curly => codes.push_str(";4:3"),
        Underline::Dotted => codes.push_str(";4:4"),
        Underline::Dashed => codes.push_str(";4:5"),
    }
    push_color(&mut codes, style.foreground, 30, 90, "38");
    push_color(&mut codes, style.background, 40, 100, "48");
    match style.underline_color {
        None | Some(Color::Default) => {}
        Some(Color::Palette(index)) => {
            let _ = write!(codes, ";58:5:{index}");
        }
        Some(Color::Rgb(red, green, blue)) => {
            let _ = write!(codes, ";58:2::{red}:{green}:{blue}");
        }
    }
    codes.push('m');
    codes
}

fn push_color(codes: &mut String, color: Color, normal: u8, bright: u8, extended: &str) {
    let _ = match color {
        Color::Default => Ok(()),
        Color::Palette(index @ 0..=7) => write!(codes, ";{}", normal + index),
        Color::Palette(index @ 8..=15) => write!(codes, ";{}", bright + index - 8),
        Color::Palette(index) => write!(codes, ";{extended};5;{index}"),
        Color::Rgb(red, green, blue) => write!(codes, ";{extended};2;{red};{green};{blue}"),
    };
}
