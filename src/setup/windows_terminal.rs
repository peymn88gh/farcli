//! Windows Terminal: the Farsi font goes into the font face of its profiles.
//!
//! Since version 1.21 a font face may name several fonts with commas between
//! them, and a character missing from the first is taken from the next. The
//! size of a cell still comes from the first font.

use std::path::Path;

use super::jsonc::{quoted, Found, Settings, Step};
use super::{change_settings_file, with_farsi_font, Report, FARSI_FONT};

/// The font Windows Terminal uses when a profile names none.
const DEFAULT_FONT: &str = "Cascadia Mono";

pub(super) fn give_farsi_font(settings_files: &[impl AsRef<Path>], report: &mut Report) {
    report.heading("Windows Terminal");
    let found: Vec<&Path> = settings_files
        .iter()
        .map(AsRef::as_ref)
        .filter(|file| file.exists())
        .collect();
    if found.is_empty() {
        report.line("not installed (no settings file found): skipped");
    }
    let by_hand = [
        format!("To do it by hand, add {FARSI_FONT} after the first font of every font face:"),
        format!("    \"font\": {{ \"face\": \"{DEFAULT_FONT}, {FARSI_FONT}\" }}"),
    ];
    for file in found {
        change_settings_file(file, report, &by_hand, add_font);
    }
}

/// Adds the Farsi font where it is missing and returns a line for each
/// change. The error says why the file cannot be changed safely.
fn add_font(settings: &mut Settings) -> Result<Vec<String>, String> {
    let mut changes = Vec::new();
    let profiles = Step::Key("profiles");
    if settings.find(&[profiles]) != Found::Object {
        return Err("its profiles are not in the shape farcli knows".to_string());
    }

    // What every profile starts from.
    let defaults = [profiles, Step::Key("defaults")];
    let face = add_to_face(settings, &defaults, true)?;
    changes.extend(face.map(|change| format!("profiles.defaults.font.face {change}")));

    // A profile with a font face of its own does not use the one above.
    let list = [profiles, Step::Key("list")];
    let count = match settings.find(&list) {
        Found::List(count) => count,
        Found::Nothing => 0,
        _ => return Err("its list of profiles is not a list".to_string()),
    };
    for index in 0..count {
        let profile = [profiles, Step::Key("list"), Step::Index(index)];
        let name = match settings.find(&[&profile[..], &[Step::Key("name")]].concat()) {
            Found::Text(name) => format!("\"{name}\""),
            _ => format!("number {}", index + 1),
        };
        let face = add_to_face(settings, &profile, false)?;
        changes.extend(face.map(|change| format!("profile {name} font.face {change}")));
    }
    Ok(changes)
}

/// Adds the Farsi font to the font face of the profile at `profile`.
/// Returns the words for what changed, or `None` when nothing had to.
fn add_to_face(
    settings: &mut Settings,
    profile: &[Step],
    set_when_missing: bool,
) -> Result<Option<String>, String> {
    // The way a font face was written before Windows Terminal 1.10.
    if settings.find(&[profile, &[Step::Key("fontFace")]].concat()) != Found::Nothing {
        return Err("it sets a font the old way, with \"fontFace\"".to_string());
    }
    let face = [profile, &[Step::Key("font"), Step::Key("face")]].concat();
    let (fonts, was) = match settings.find(&face) {
        Found::Text(fonts) if !fonts.trim().is_empty() => {
            let was = format!("was {}", quoted(&fonts));
            (fonts, was)
        }
        Found::Nothing | Found::Text(_) if set_when_missing => (
            DEFAULT_FONT.to_string(),
            format!("was not set, which means {DEFAULT_FONT}"),
        ),
        Found::Nothing | Found::Text(_) => return Ok(None),
        _ => return Err("a font face is not text".to_string()),
    };
    let Some(fonts) = with_farsi_font(&fonts, FARSI_FONT) else {
        return Ok(None);
    };
    settings.set_text(&face, &fonts)?;
    Ok(Some(format!("is now {} ({was})", quoted(&fonts))))
}
