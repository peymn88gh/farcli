//! The session: everything farcli does between a Target app and a Terminal,
//! with no real app, no real Terminal and no clock.
//!
//! It is fed the bytes the Target app wrote, the keys the user pressed and
//! the Terminal's size. Each time it answers with the bytes to send to the
//! Terminal and the bytes to forward to the app.

use crate::app_profile::AppProfile;
use crate::correction::Correction;
use crate::hidden_screen::{Aside, HiddenScreen};
use crate::hidden_terminal::HiddenTerminal;
use crate::hotkey::{Hotkey, Pressed};
use crate::painter::{Frame, Painter};
use crate::screen::{Cell, Position, Row, Style};
use crate::terminal_input::{Input, Key, TerminalInput};
use crate::Size;

pub struct Session {
    correction: Correction,
    hidden_screen: HiddenScreen,
    painter: Painter,
    /// True until the hidden terminal's own question about what kind of
    /// terminal it is on has been answered.
    hidden_terminal_is_asking: bool,
    terminal_input: TerminalInput,
    /// True when the hidden terminal must be given a character that came
    /// with no key as a key, because it would garble the plain character.
    characters_as_keys: bool,
    /// True when the hidden terminal can leave the app waiting for input
    /// it has already been given.
    app_can_be_left_asleep: bool,
    hotkey: Hotkey,
    /// False while the user has switched the correction off with the
    /// hotkey: the Terminal then shows what the Target app drew.
    correcting: bool,
    /// A line of farcli's own that is shown over the top row of the
    /// alternate screen, until the user presses a key.
    line_in_sight: Option<String>,
}

/// What farcli tells a hidden terminal that asks what kind of terminal it
/// is on: a plain one. The Hidden screen keeps no pictures and moves no
/// rectangles, which are the extras a hidden terminal would otherwise use.
const TERMINAL_KIND: &[u8] = b"\x1b[?61c";

/// What farcli does in answer to one thing that happened.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reaction {
    /// Bytes to send to the Terminal.
    pub for_terminal: Vec<u8>,
    /// Bytes to forward to the Target app.
    pub for_app: Vec<u8>,
}

impl Session {
    /// Starts a session in a Terminal of `size` whose cursor is at `cursor`,
    /// fed what the Target app wrote with nothing in between that asks
    /// questions of its own. The Target app's hidden terminal must have the
    /// same size.
    pub fn start(profile: AppProfile, size: Size, cursor: Position) -> Session {
        Session {
            correction: Correction::new(profile),
            hidden_screen: HiddenScreen::new(size, cursor),
            painter: Painter::new(size),
            hidden_terminal_is_asking: false,
            terminal_input: TerminalInput::new(),
            characters_as_keys: false,
            app_can_be_left_asleep: false,
            hotkey: Hotkey::new(),
            correcting: true,
            line_in_sight: None,
        }
    }

    /// Starts a session fed by `hidden_terminal`, which may say things of
    /// its own besides passing on what the Target app wrote.
    pub fn start_on(
        hidden_terminal: HiddenTerminal,
        profile: AppProfile,
        size: Size,
        cursor: Position,
    ) -> Session {
        Session {
            hidden_terminal_is_asking: hidden_terminal.asks_what_it_is_on(),
            characters_as_keys: hidden_terminal.makes_up_keys_for_plain_characters(),
            app_can_be_left_asleep: hidden_terminal.can_leave_an_app_asleep_on_input(),
            ..Session::start(profile, size, cursor)
        }
    }

    /// Whoever started the session wrote `line` to the Terminal just before
    /// it, for the user to read. On the main screen the line stays above
    /// what the Target app draws. An app that moves to the alternate screen
    /// hides it within a moment, so there the session shows it over the top
    /// row, until the user presses a key.
    pub fn keep_in_sight(&mut self, line: &str) {
        self.line_in_sight = Some(line.to_string());
    }

    /// The Target app wrote `bytes`.
    pub fn app_wrote(&mut self, bytes: &[u8]) -> Reaction {
        let mut reaction = Reaction::default();
        for aside in self.hidden_screen.app_wrote(bytes) {
            match aside {
                Aside::ForTerminal(code) => reaction.for_terminal.extend(code),
                Aside::ForApp(answer) => reaction.for_app.extend(answer),
                // The first to ask is the hidden terminal itself, before
                // the app has written anything. After that it is the app,
                // and the app means the Terminal.
                Aside::TerminalKindAsked(_) if self.hidden_terminal_is_asking => {
                    self.hidden_terminal_is_asking = false;
                    reaction.for_app.extend(TERMINAL_KIND);
                }
                Aside::TerminalKindAsked(code) => reaction.for_terminal.extend(code),
            }
        }
        if !self.hidden_screen.mid_frame() {
            reaction.for_terminal.extend(self.paint());
        }
        reaction
    }

    /// The Terminal sent `bytes`: keys the user pressed, text the user
    /// pasted, what the mouse did, or the Terminal's answer to something
    /// the Target app asked.
    pub fn user_typed(&mut self, bytes: &[u8]) -> Reaction {
        let inputs = self.terminal_input.read(bytes);
        self.forward(&inputs)
    }

    /// True while the start of a code the Terminal sent is kept from the
    /// Target app because its end has not come yet.
    pub fn holding_input(&self) -> bool {
        self.terminal_input.holding()
    }

    /// Forwards what was kept back as it came, because the rest of it has
    /// taken too long to come. Whoever drives the session decides what too
    /// long is.
    pub fn release_held_input(&mut self) -> Reaction {
        let inputs = self.terminal_input.release();
        self.forward(&inputs)
    }

    /// Every key, pasted character and mouse report passes through here on
    /// its way to the Target app, one at a time and in the order it came.
    fn forward(&mut self, inputs: &[Input]) -> Reaction {
        let mut reaction = Reaction::default();
        let mut switched = false;
        for input in inputs {
            if self.line_in_sight.is_some() && is_the_user_typing(input) {
                self.line_in_sight = None;
                switched = true;
            }
            // The hotkey is farcli's own and never reaches the Target app.
            match self.hotkey.sees(input) {
                Pressed::Now => {
                    self.correcting = !self.correcting;
                    switched = true;
                    continue;
                }
                Pressed::Still => continue,
                Pressed::No => {}
            }
            self.correction.user_typed(input);
            input.write_for_app(self.characters_as_keys, &mut reaction.for_app);
        }
        // A frame the app has not finished waits for its end as before, and
        // is then shown the way the user now wants it.
        if switched && !self.hidden_screen.mid_frame() {
            reaction.for_terminal.extend(self.paint());
        }
        reaction
    }

    /// True when the Target app's hidden terminal can take what was
    /// forwarded and leave the app waiting for it, until something else
    /// arrives. The app then has to be woken: see `wake_app`.
    pub fn app_can_be_left_asleep_on_input(&self) -> bool {
        self.app_can_be_left_asleep
    }

    /// Wakes the Target app in case it was left waiting for input its
    /// hidden terminal already has. It is to be called a moment after the
    /// last of what the Terminal sent was forwarded: the app goes to sleep
    /// within a fraction of a millisecond of that, and a wake that comes
    /// before it sleeps is left with the input. The session has no clock,
    /// so whoever drives it decides what a moment is.
    ///
    /// What wakes the app is anything more arriving. So the hidden terminal
    /// is given a key that carries nothing, coming up: no app takes that
    /// for a key, and an app that was not asleep loses nothing by it. A
    /// hidden terminal that never leaves an app asleep is given nothing.
    pub fn wake_app(&self) -> Reaction {
        let mut reaction = Reaction::default();
        if self.app_can_be_left_asleep {
            Input::Key(Key::NOTHING_COMING_UP).write_for_app(false, &mut reaction.for_app);
        }
        reaction
    }

    /// The user gave the Terminal's window a new size. The Target app's
    /// hidden terminal must be given the same size.
    pub fn terminal_resized(&mut self, size: Size) -> Reaction {
        self.hidden_screen.resize(size);
        self.painter.terminal_resized(size);
        Reaction {
            for_terminal: self.paint(),
            for_app: Vec::new(),
        }
    }

    /// True while the Target app is part-way through a frame it asked not to
    /// be shown half done, so the Terminal is behind the app.
    pub fn holding_frame(&self) -> bool {
        self.hidden_screen.mid_frame()
    }

    /// Shows what the app has drawn so far, because it has taken too long to
    /// complete its frame. The session has no clock: whoever drives it
    /// decides what too long is.
    pub fn release_held_frame(&mut self) -> Reaction {
        self.hidden_screen.end_frame();
        Reaction {
            for_terminal: self.paint(),
            for_app: Vec::new(),
        }
    }

    fn paint(&mut self) -> Vec<u8> {
        let mut frame = Frame {
            changes: self.hidden_screen.take_changes(),
            rows: self.hidden_screen.rows(),
            cursor: self.hidden_screen.cursor(),
            cursor_visible: self.hidden_screen.cursor_visible(),
        };
        if self.correcting {
            self.correction.correct(&mut frame);
        } else {
            // The frame is shown as the app drew it. The correction is
            // still given a copy, so that it goes on following the session
            // and knows what scrolled off the top when it is switched on
            // again.
            self.correction.correct(&mut Frame {
                changes: frame.changes.clone(),
                rows: frame.rows.clone(),
                cursor: frame.cursor,
                cursor_visible: frame.cursor_visible,
            });
        }
        if let (Some(line), true) = (
            &self.line_in_sight,
            self.hidden_screen.on_alternate_screen(),
        ) {
            if let Some(top) = frame.rows.first_mut() {
                *top = row_of_farclis_own(line, top.len());
            }
        }
        self.painter.paint(&frame)
    }
}

/// True for a key going down, a typed or pasted character, and the start
/// of a paste. What a Terminal sends by itself is none of these: an answer
/// to a question of the app's, a change of focus, a key coming up. A
/// console host that spells such an answer out does it in keys that have
/// no key number and carry a plain English character. A key the user
/// pressed has a key number, or carries a letter of the user's language.
fn is_the_user_typing(input: &Input) -> bool {
    match input {
        Input::Key(key) => key.down && (key.key != 0 || key.character >= 0x80),
        Input::Character(_) | Input::PasteStarts => true,
        _ => false,
    }
}

/// A row `cols` wide that holds `line` and is plainly not the Target app's:
/// drawn from end to end with the Terminal's two colours changed over. A
/// line longer than the row is cut.
fn row_of_farclis_own(line: &str, cols: usize) -> Row {
    let style = Style {
        inverse: true,
        ..Style::default()
    };
    line.chars()
        .chain(std::iter::repeat(' '))
        .take(cols)
        .map(|character| Cell {
            character,
            style: style.clone(),
            ..Cell::blank()
        })
        .collect()
}
