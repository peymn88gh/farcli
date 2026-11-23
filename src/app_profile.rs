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

    /// True when each row that holds Farsi is taken as a paragraph of its
    /// own, as wide as the Terminal. That is all farcli can do for a program
    /// it knows nothing about. A Target app's screen has an interface around
    /// its text, so its rows are left as drawn until farcli can tell the
    /// two apart.
    pub(crate) fn every_row_is_a_paragraph(&self) -> bool {
        self.app == App::Plain
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
