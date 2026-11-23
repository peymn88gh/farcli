//! WezTerm: the Farsi font goes into its font fallback list.
//!
//! WezTerm's configuration is a Lua program. farcli cannot change a program
//! someone wrote without risking what it does, so an existing configuration
//! is left alone and the user is told what to add. Only when there is no
//! configuration at all does Setup write one.

use std::fs;
use std::io;
use std::path::Path;

use super::{Report, FARSI_FONT};

/// The font WezTerm uses when the configuration names none.
const DEFAULT_FONT: &str = "JetBrains Mono";
/// How much larger the Farsi font is drawn so that its letters are as wide
/// as a cell of the default font: 600 against 500 units in 1000.
const SCALE: &str = "1.2";

pub(super) fn give_farsi_font(config: Option<&Path>, report: &mut Report) {
    report.heading("WezTerm");
    let Some(config) = config else {
        report.line("not installed: skipped");
        return;
    };
    let fallback = [
        "config.font = wezterm.font_with_fallback {".to_string(),
        format!("  '{DEFAULT_FONT}',"),
        format!("  {{ family = '{FARSI_FONT}', scale = {SCALE} }},"),
        "}".to_string(),
    ];
    match fs::read(config) {
        Ok(written) if names_farsi_font(&String::from_utf8_lossy(&written)) => {
            report.line(format!(
                "{FARSI_FONT} is already in place: {}",
                config.display()
            ));
        }
        Ok(_) => {
            report.line(format!("left alone: {}", config.display()));
            report.line("  It is a Lua program, which farcli cannot change safely.");
            report.line(format!(
                "  To do it by hand, name {FARSI_FONT} right after your English font:"
            ));
            for line in &fallback {
                report.line(format!("      {line}"));
            }
            report.line(format!(
                "  {DEFAULT_FONT} is the font WezTerm uses when none is set; put yours there if"
            ));
            report.line(format!(
                "  you chose one. `scale` makes Farsi letters as wide as a cell: {SCALE} fits"
            ));
            report.line(format!("  {DEFAULT_FONT}."));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let contents = format!(
                "-- Written by farcli setup. WezTerm had no configuration file before.\n\
                 local wezterm = require 'wezterm'\n\
                 local config = wezterm.config_builder()\n\
                 \n\
                 -- English stays in {DEFAULT_FONT}, the font WezTerm uses when none is set.\n\
                 -- Farsi comes from {FARSI_FONT}, made larger so its letters fill a cell.\n\
                 {}\n\
                 \n\
                 return config\n",
                fallback.join("\n")
            );
            match create(config, &contents) {
                Ok(()) => {
                    report.line(format!("created {}", config.display()));
                    report.line(format!(
                        "  its font is {DEFAULT_FONT}, then {FARSI_FONT} for Farsi"
                    ));
                }
                Err(error) => {
                    report.failure(format!("could not create {}: {error}", config.display()))
                }
            }
        }
        Err(error) => report.failure(format!("could not read {}: {error}", config.display())),
    }
}

/// Whether the configuration names the Farsi font outside a comment.
fn names_farsi_font(config: &str) -> bool {
    config.lines().any(|line| {
        let code = line.split("--").next().unwrap_or_default();
        code.contains(&format!("'{FARSI_FONT}'")) || code.contains(&format!("\"{FARSI_FONT}\""))
    })
}

fn create(file: &Path, contents: &str) -> io::Result<()> {
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    fs::write(file, contents)
}
