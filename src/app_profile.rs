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
