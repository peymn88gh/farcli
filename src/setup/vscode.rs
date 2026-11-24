//! VS Code's terminal: the Farsi font goes into its font family.
//!
//! The font family is a list in the form CSS uses, and a character missing
//! from the first font is taken from the next. When the terminal has no
//! font family of its own, VS Code gives it the editor's.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::jsonc::{quoted, Found, Settings, Step};
use super::{change_settings_file, with_farsi_font, Report, FARSI_FONT};

const TERMINAL_FONTS: &str = "terminal.integrated.fontFamily";
const EDITOR_FONTS: &str = "editor.fontFamily";
/// The editor's font family on Windows when the user has set none.
const DEFAULT_FONTS: &str = "Consolas, 'Courier New', monospace";

/// VS Code's settings files in its user folder: the user's own, whether it
/// exists yet or not, and that of each profile that has one. Empty when the
/// folder is not there, which means VS Code is not installed.
pub(super) fn settings_files_in(user_folder: &Path) -> Vec<PathBuf> {
    if !user_folder.is_dir() {
        return Vec::new();
    }
    let mut files = vec![user_folder.join("settings.json")];
    if let Ok(profiles) = fs::read_dir(user_folder.join("profiles")) {
        let mut of_profiles: Vec<PathBuf> = profiles
            .filter_map(|profile| Some(profile.ok()?.path().join("settings.json")))
            .filter(|settings| settings.is_file())
            .collect();
        of_profiles.sort();
        files.extend(of_profiles);
    }
    files
}

pub(super) fn give_farsi_font(settings_files: &[PathBuf], report: &mut Report) {
    report.heading("VS Code");
    if settings_files.is_empty() {
        report.line("not installed: skipped");
    }
    let farsi_font = format!("'{FARSI_FONT}'");
    let by_default = with_farsi_font(DEFAULT_FONTS, &farsi_font).unwrap_or_default();
    let by_hand = [
        format!("To do it by hand, add {farsi_font} after the first font of the terminal's:"),
        format!("    \"{TERMINAL_FONTS}\": \"{by_default}\""),
    ];
    for file in settings_files {
        if file.exists() {
            change_settings_file(file, report, &by_hand, |settings| {
                add_font(settings, &farsi_font)
            });
            continue;
        }
        // VS Code is installed but the user has changed no setting yet.
        let contents = format!("{{\n    \"{TERMINAL_FONTS}\": \"{by_default}\"\n}}\n");
        match create(file, &contents) {
            Ok(()) => {
                report.line(format!("created {}", file.display()));
                report.line(format!("  {TERMINAL_FONTS} is \"{by_default}\""));
            }
            Err(error) => report.failure(format!("could not create {}: {error}", file.display())),
        }
    }
}

fn create(file: &Path, contents: &str) -> io::Result<()> {
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    fs::write(file, contents)
}

/// Adds the Farsi font where it is missing and returns a line for each
/// change. The error says why the file cannot be changed safely.
fn add_font(settings: &mut Settings, farsi_font: &str) -> Result<Vec<String>, String> {
    let terminal = [Step::Key(TERMINAL_FONTS)];
    let (fonts, was) = match settings.find(&terminal) {
        Found::Text(fonts) if !fonts.trim().is_empty() => {
            let was = format!("was {}", quoted(&fonts));
            (fonts, was)
        }
        Found::Nothing | Found::Text(_) => match settings.find(&[Step::Key(EDITOR_FONTS)]) {
            Found::Text(fonts) if !fonts.trim().is_empty() => {
                (fonts, format!("was not set, so it followed {EDITOR_FONTS}"))
            }
            Found::Twice => return Err(format!("\"{EDITOR_FONTS}\" appears twice")),
            _ => (
                DEFAULT_FONTS.to_string(),
                "was not set, which means VS Code's own list".to_string(),
            ),
        },
        Found::Twice => return Err(format!("\"{TERMINAL_FONTS}\" appears twice")),
        _ => return Err(format!("\"{TERMINAL_FONTS}\" is not text")),
    };
    let Some(fonts) = with_farsi_font(&fonts, farsi_font) else {
        return Ok(Vec::new());
    };
    settings.set_text(&terminal, &fonts)?;
    Ok(vec![format!(
        "{TERMINAL_FONTS} is now {} ({was})",
        quoted(&fonts)
    )])
}
