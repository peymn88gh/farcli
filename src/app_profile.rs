//! App profile: what farcli knows about one Target app.

use std::ffi::{OsStr, OsString};

/// What farcli knows about one Target app. There is one profile for Claude
/// Code CLI, one for Codex CLI, and a plain one for any other program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppProfile {
    app: App,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum App {
    ClaudeCode,
    Codex,
    Plain,
}

/// What farcli knows about how a Target app lays out its screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    /// Each row that holds Farsi is taken as a paragraph of its own, as
    /// wide as the Terminal. That is all farcli can do for a program it
    /// knows nothing about.
    EveryRowAParagraph,
    /// Claude Code's screen: see `regions::claude_code`.
    ClaudeCode,
    /// Codex's screen: see `regions::codex`.
    Codex,
}

impl AppProfile {
    pub fn claude_code() -> AppProfile {
        AppProfile {
            app: App::ClaudeCode,
        }
    }

    pub fn codex() -> AppProfile {
        AppProfile { app: App::Codex }
    }

    /// The profile for a program farcli knows nothing about.
    pub fn plain() -> AppProfile {
        AppProfile { app: App::Plain }
    }

    /// The arguments that make the app print its version and end, when
    /// farcli knows the app well enough to ask.
    pub fn version_args(&self) -> Option<&'static [&'static str]> {
        match self.app {
            App::ClaudeCode | App::Codex => Some(&["--version"]),
            App::Plain => None,
        }
    }

    /// How the app's screen is split into regions.
    pub(crate) fn screen(&self) -> Screen {
        match self.app {
            App::ClaudeCode => Screen::ClaudeCode,
            App::Codex => Screen::Codex,
            App::Plain => Screen::EveryRowAParagraph,
        }
    }

    /// True when the app puts Farsi into Screen order itself, row by row,
    /// so that its rows have to be turned back into Logical order before
    /// they are laid out. Claude Code does on Windows.
    pub(crate) fn reorders_farsi_itself(&self) -> bool {
        self.app == App::ClaudeCode
    }

    /// The profile for the app the user named after `farcli`, together with
    /// the program to run for it.
    pub fn for_command(name: &OsStr) -> (AppProfile, OsString) {
        match name.to_str() {
            Some("claude") => (AppProfile::claude_code(), OsString::from("claude")),
            Some("codex") => (AppProfile::codex(), OsString::from("codex")),
            _ => (AppProfile::plain(), name.to_os_string()),
        }
    }
}
