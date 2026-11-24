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

    /// The app's name as a person says it, and the newest version of it
    /// that farcli was tested with: the one the recordings in
    /// `tests/recordings` were made with. A newer one may draw its screen
    /// in a way the profile does not know.
    fn tested_with(&self) -> Option<(&'static str, &'static str)> {
        match self.app {
            App::ClaudeCode => Some(("Claude Code", "2.1.322")),
            App::Codex => Some(("Codex", "0.171.0")),
            App::Plain => None,
        }
    }

    /// The one line farcli says at start when the app is newer than the
    /// version the profile was tested with. `said` is what the app printed
    /// when asked for its version. `None` when the app is not newer, or
    /// when what it printed holds no version.
    pub fn newer_version_warning(&self, said: &str) -> Option<String> {
        let (app, tested) = self.tested_with()?;
        let (version, numbers) = version_in(said)?;
        let (_, tested_numbers) = version_in(tested)?;
        (numbers > tested_numbers).then(|| {
            format!(
                "farcli: {app} {version} is newer than the tested {tested}. \
                 F12 switches the correction off and on."
            )
        })
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

/// The first version number in `text`, such as the `2.1.322` of
/// `2.1.322 (Claude Code)`: as it is written, and as numbers to compare.
/// A version has at least two numbers with a full stop between them.
fn version_in(text: &str) -> Option<(&str, Vec<u64>)> {
    let is_part = |character: char| character.is_ascii_digit() || character == '.';
    let mut rest = text;
    loop {
        rest = &rest[rest.find(|character: char| character.is_ascii_digit())?..];
        let length = rest
            .find(|character| !is_part(character))
            .unwrap_or(rest.len());
        let (found, after) = rest.split_at(length);
        let found = found.trim_end_matches('.');
        let numbers: Option<Vec<u64>> =
            found.split('.').map(|number| number.parse().ok()).collect();
        match numbers {
            Some(numbers) if numbers.len() > 1 => return Some((found, numbers)),
            _ => rest = after,
        }
    }
}
