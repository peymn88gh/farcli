//! Setup: installs the Farsi font, backs up each Supported terminal's
//! settings file, adds the font for Farsi only, and reports what it changed.
//!
//! Setup is told where everything is through a [`Machine`]. The real one is
//! found from the user's account; a pretend one is a folder that stands in
//! for a whole machine, so that Setup can be run and checked without
//! touching real settings.

mod font;
pub(crate) mod jsonc;
mod vscode;
mod wezterm;
mod windows_account;
mod windows_terminal;

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use font::{FontList, FontListFile, FARSI_FONT};
use jsonc::Settings;

/// Where Setup finds the things it reads and changes.
pub struct Machine {
    /// The folder that holds the fonts installed for the current user.
    pub fonts_folder: PathBuf,
    /// The system's list of the fonts installed for the current user.
    pub font_list: Box<dyn FontList>,
    /// Every place a Windows Terminal settings file may be. One that is not
    /// there belongs to a Windows Terminal that is not installed.
    pub windows_terminal_settings: Vec<PathBuf>,
    /// The configuration file WezTerm reads, whether it exists yet or not.
    /// `None` when WezTerm is not installed.
    pub wezterm_config: Option<PathBuf>,
    /// VS Code's settings files: the user's own, which may not exist yet,
    /// and one for each profile that has one. Empty when VS Code is not
    /// installed.
    pub vscode_settings: Vec<PathBuf>,
    /// The file whose presence says Setup has run on this machine.
    pub setup_done_mark: PathBuf,
}

impl Machine {
    /// A folder that stands in for a whole machine:
    ///
    /// ```text
    /// fonts/                               the user's fonts folder
    /// installed-fonts.txt                  the user's font list
    /// windows-terminal/settings.json       Windows Terminal's settings
    /// wezterm/                             there when WezTerm is installed
    /// wezterm/wezterm.lua                  WezTerm's configuration
    /// vscode/                              there when VS Code is installed
    /// vscode/settings.json                 VS Code's user settings
    /// vscode/profiles/NAME/settings.json   settings of a VS Code profile
    /// setup-done                           there once Setup has run
    /// ```
    pub fn pretend(root: &Path) -> Machine {
        let wezterm = root.join("wezterm");
        let vscode = root.join("vscode");
        Machine {
            fonts_folder: root.join("fonts"),
            font_list: Box::new(FontListFile::new(root.join("installed-fonts.txt"))),
            windows_terminal_settings: vec![root.join("windows-terminal").join("settings.json")],
            wezterm_config: wezterm.is_dir().then(|| wezterm.join("wezterm.lua")),
            vscode_settings: vscode::settings_files_in(&vscode),
            setup_done_mark: root.join("setup-done"),
        }
    }

    /// The machine farcli is running on, or `None` where Setup cannot work
    /// yet (anywhere but Windows).
    ///
    /// When the environment variable `FARCLI_PRETEND_MACHINE` names a
    /// folder, that folder is the machine (see [`Machine::pretend`]). This
    /// is how the real program is run without touching real settings.
    pub fn from_environment() -> Option<Machine> {
        if let Some(root) = env::var_os(PRETEND_MACHINE) {
            return Some(Machine::pretend(Path::new(&root)));
        }
        #[cfg(windows)]
        let found = {
            let fonts = Box::new(windows_account::registry::UserFonts);
            Machine::of_windows_account(|name| env::var_os(name), fonts)
        };
        // The Terminals farcli supports so far are Windows programs, also
        // when farcli itself runs inside WSL.
        #[cfg(not(windows))]
        let found = None;
        found
    }
}

const PRETEND_MACHINE: &str = "FARCLI_PRETEND_MACHINE";

/// A Farsi word as farcli sends it to a Terminal: letters already joined,
/// in Screen order.
const JOINED_SAMPLE: &str = "\u{fbfd}\u{feb3}\u{fead}\u{fe8e}\u{fed3}";

/// `farcli setup`: runs Setup on this machine and prints what it did.
/// Returns the exit status.
pub fn run_as_command() -> u8 {
    let Some(mut machine) = Machine::from_environment() else {
        eprintln!(
            "farcli: Setup works on Windows only so far. Run `farcli setup` in PowerShell \
             on the PC whose Terminals you use."
        );
        return 2;
    };
    let report = run(&mut machine);
    println!("farcli setup: giving this PC's Terminals the Farsi font ({FARSI_FONT})\n");
    print!("{report}");
    println!("\n{}", closing_words(&report));
    u8::from(report.failed())
}

/// Runs Setup before a Target app starts, if this is the first time farcli
/// is used on this machine, and prints what it did.
///
/// It does nothing when farcli steps aside for a pipe or a script: there
/// farcli must add nothing to the app's output and change nothing.
pub fn run_before_first_app() {
    if !in_a_terminal() {
        return;
    }
    let Some(mut machine) = Machine::from_environment() else {
        return;
    };
    if let Some(report) = run_on_first_use(&mut machine) {
        println!(
            "farcli: first use on this PC, so Setup gives its Terminals the Farsi font \
             ({FARSI_FONT})\n"
        );
        print!("{report}");
        println!("\n{}\n", closing_words(&report));
    }
}

fn closing_words(report: &Report) -> String {
    if report.failed() {
        "Something could not be done. Run `farcli setup` to try again.".to_string()
    } else {
        format!(
            "Farsi letters should now join, as in this word: {JOINED_SAMPLE}\n\
             If they do not, close the Terminal and open it again. \
             Run `farcli setup` at any time to do this again."
        )
    }
}

/// Whether farcli's input and output are both a Terminal, which is when it
/// corrects the Target app instead of stepping aside.
fn in_a_terminal() -> bool {
    #[cfg(windows)]
    return crate::terminal::Terminal::open().is_some();
    #[cfg(not(windows))]
    {
        use std::io::IsTerminal;
        io::stdin().is_terminal() && io::stdout().is_terminal()
    }
}

/// What Setup did, in the words it prints.
#[derive(Debug, Default)]
pub struct Report {
    lines: Vec<String>,
    failed: bool,
}

impl Report {
    /// Whether something went wrong that running Setup again may put right.
    pub fn failed(&self) -> bool {
        self.failed
    }

    fn heading(&mut self, heading: &str) {
        self.lines.push(heading.to_string());
    }

    fn line(&mut self, line: impl AsRef<str>) {
        self.lines.push(format!("  {}", line.as_ref()));
    }

    fn failure(&mut self, line: impl AsRef<str>) {
        self.line(line);
        self.failed = true;
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for line in &self.lines {
            writeln!(f, "{line}")?;
        }
        Ok(())
    }
}

/// Runs Setup on `machine` and returns what it did.
pub fn run(machine: &mut Machine) -> Report {
    let mut report = Report::default();
    font::install(
        &machine.fonts_folder,
        machine.font_list.as_mut(),
        &mut report,
    );
    windows_terminal::give_farsi_font(&machine.windows_terminal_settings, &mut report);
    wezterm::give_farsi_font(machine.wezterm_config.as_deref(), &mut report);
    vscode::give_farsi_font(&machine.vscode_settings, &mut report);
    if !report.failed {
        // A run that failed is left unmarked, so that the next use of farcli
        // tries again.
        if let Err(error) = mark_done(&machine.setup_done_mark) {
            report.heading("farcli");
            report.failure(format!(
                "could not note that Setup has run, in {}: {error}",
                machine.setup_done_mark.display()
            ));
        }
    }
    report
}

/// Runs Setup if it has not run on `machine` before. Returns what it did,
/// or `None` when there was nothing to do.
pub fn run_on_first_use(machine: &mut Machine) -> Option<Report> {
    (!machine.setup_done_mark.exists()).then(|| run(machine))
}

fn mark_done(mark: &Path) -> io::Result<()> {
    if let Some(folder) = mark.parent() {
        fs::create_dir_all(folder)?;
    }
    fs::write(
        mark,
        "farcli setup has run for this user. Remove this file to have it run again by itself.\n",
    )
}

/// `fonts` with the Farsi font added right after its first font, or `None`
/// when the Farsi font is in the list already.
///
/// `fonts` is a list of font names with commas between them. A Terminal
/// takes each character from the first font in the list that has it. The
/// first font stays first, so English looks as it did; the Farsi font comes
/// next, ahead of any other font that might also have Farsi letters.
/// `farsi_font` is the Farsi font's name as this list wants it written.
fn with_farsi_font(fonts: &str, farsi_font: &str) -> Option<String> {
    let mut commas = Vec::new();
    let mut quote = None;
    for (at, c) in fonts.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            (None, ',') => commas.push(at),
            _ => {}
        }
    }
    let starts = std::iter::once(0).chain(commas.iter().map(|comma| comma + 1));
    let ends = commas.iter().copied().chain([fonts.len()]);
    let listed = starts.zip(ends).any(|(start, end)| {
        let name = fonts[start..end].trim().trim_matches(['"', '\'']);
        name.eq_ignore_ascii_case(FARSI_FONT)
    });
    if listed {
        return None;
    }
    let (first, rest) = fonts.split_at(commas.first().copied().unwrap_or(fonts.len()));
    Some(format!("{}, {farsi_font}{rest}", first.trim_end()))
}

/// Changes one settings file written as JSON with comments, and reports it.
///
/// `add_font` is given the settings as read and adds the Farsi font where
/// it is missing. It returns a line for each change it made, or the reason
/// the file cannot be changed safely. In that case the file is left alone
/// and `by_hand` is printed: what the user can add themselves.
fn change_settings_file(
    file: &Path,
    report: &mut Report,
    by_hand: &[String],
    add_font: impl FnOnce(&mut Settings) -> Result<Vec<String>, String>,
) {
    let before = match fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) => {
            report.failure(format!("could not read {}: {error}", file.display()));
            return;
        }
    };
    let changed = Settings::read(&before).and_then(|mut settings| {
        let changes = add_font(&mut settings)?;
        Ok((settings, changes))
    });
    let (settings, changes) = match changed {
        Ok(changed) => changed,
        Err(why) => {
            report.line(format!("left alone: {}", file.display()));
            report.line(format!("  farcli cannot change this file safely: {why}."));
            for line in by_hand {
                report.line(format!("  {line}"));
            }
            return;
        }
    };
    if changes.is_empty() {
        report.line(format!(
            "{FARSI_FONT} is already in place: {}",
            file.display()
        ));
        return;
    }
    match write_with_backup(file, &settings.to_bytes()) {
        Ok(backup) => {
            report.line(format!("changed {}", file.display()));
            for change in changes {
                report.line(format!("  {change}"));
            }
            report.line(format!("backup: {}", backup.display()));
        }
        Err(error) => report.failure(format!("could not change {}: {error}", file.display())),
    }
}

/// Copies `file` to a backup beside it, then writes `contents` to it.
/// Returns where the backup is. An earlier backup is never written over.
fn write_with_backup(file: &Path, contents: &[u8]) -> io::Result<PathBuf> {
    // Find out that the file cannot be written before leaving a backup.
    fs::OpenOptions::new().write(true).open(file)?;
    let name = file
        .file_name()
        .ok_or_else(|| io::Error::other("the settings file has no name"))?
        .to_string_lossy();
    let mut number = 1;
    let backup = loop {
        let backup = file.with_file_name(match number {
            1 => format!("{name}.farcli-backup"),
            _ => format!("{name}.farcli-backup-{number}"),
        });
        if !backup.exists() {
            break backup;
        }
        number += 1;
    };
    fs::copy(file, &backup)?;
    fs::write(file, contents)?;
    Ok(backup)
}
