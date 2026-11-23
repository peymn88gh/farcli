//! The session: everything farcli does between a Target app and a Terminal,
//! with no real app, no real Terminal and no clock.
//!
//! It is fed the bytes the Target app wrote, the keys the user pressed and
//! the Terminal's size. Each time it answers with the bytes to send to the
//! Terminal and the bytes to forward to the app.

use crate::app_profile::AppProfile;
use crate::correction;
use crate::hidden_screen::{Aside, HiddenScreen};
use crate::hidden_terminal::HiddenTerminal;
use crate::painter::{Frame, Painter};
use crate::screen::Position;
use crate::terminal_input::{Input, TerminalInput};
use crate::Size;

pub struct Session {
    profile: AppProfile,
    hidden_screen: HiddenScreen,
    painter: Painter,
    /// True until the hidden terminal's own question about what kind of
    /// terminal it is on has been answered.
    hidden_terminal_is_asking: bool,
    terminal_input: TerminalInput,
    /// True when the hidden terminal must be given a character that came
    /// with no key as a key, because it would garble the plain character.
    characters_as_keys: bool,
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
            profile,
            hidden_screen: HiddenScreen::new(size, cursor),
            painter: Painter::new(size),
            hidden_terminal_is_asking: false,
            terminal_input: TerminalInput::new(),
            characters_as_keys: false,
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
            ..Session::start(profile, size, cursor)
        }
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
        for input in inputs {
            input.write_for_app(self.characters_as_keys, &mut reaction.for_app);
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
        correction::correct(&mut frame, &self.profile);
        self.painter.paint(&frame)
    }
}
