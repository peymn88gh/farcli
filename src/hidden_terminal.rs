//! Which hidden terminal a Target app runs on. The two differ in what
//! reaches farcli: see `docs/adr/0002`.

/// The hidden terminal a Target app runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HiddenTerminal {
    /// Microsoft's newer one, which farcli ships. It passes on what the app
    /// wrote as the app wrote it, and gives a Half-space and a vowel mark no
    /// cell, as the Target apps and the Hidden screen do.
    Shipped,
    /// The one built into Windows. It draws the app's screen again in its
    /// own words, and gives a Half-space and a vowel mark a cell each.
    BuiltIn,
}

/// The version of Microsoft's package `Microsoft.Windows.Console.ConPTY`
/// the shipped hidden terminal comes from.
pub const SHIPPED_VERSION: &str = "1.25.261109001";

const SHIPPED_NAME: &str = "Microsoft.Windows.Console.ConPTY";
const BUILT_IN_NAME: &str = "ConPTY built into Windows";

impl HiddenTerminal {
    /// How a recording names this hidden terminal.
    pub fn name(self) -> String {
        match self {
            HiddenTerminal::Shipped => {
                format!("{SHIPPED_NAME} {SHIPPED_VERSION}, shipped with farcli")
            }
            HiddenTerminal::BuiltIn => BUILT_IN_NAME.to_string(),
        }
    }

    /// The hidden terminal a recording names, whichever version of it.
    pub fn named(name: &str) -> Option<HiddenTerminal> {
        if name.starts_with(SHIPPED_NAME) {
            Some(HiddenTerminal::Shipped)
        } else if name.starts_with(BUILT_IN_NAME) {
            Some(HiddenTerminal::BuiltIn)
        } else {
            None
        }
    }

    /// True when the hidden terminal itself asks, before the app's first
    /// output, what kind of terminal it is on, and holds the app back
    /// until it is told.
    pub(crate) fn asks_what_it_is_on(self) -> bool {
        self == HiddenTerminal::Shipped
    }

    /// True when a character written to the hidden terminal's input as
    /// plain text may reach the app as something else. For a character the
    /// keyboard layout has no key for, such as a Half-space, `؟` or a
    /// Persian digit, the built-in one makes up a key sequence: Alt held
    /// down while the character's number is typed. Codex drops those.
    pub(crate) fn makes_up_keys_for_plain_characters(self) -> bool {
        self == HiddenTerminal::BuiltIn
    }
}
