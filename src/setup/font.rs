//! The Farsi font: the file farcli carries, and installing it for the
//! current user.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::Report;

/// The name Terminals know the Farsi font by.
///
/// This is the build of Vazir Code that holds Farsi letters only ("WOL":
/// without Latin). A Terminal can take nothing but Farsi from it, so the
/// user's English font keeps drawing everything else.
pub const FARSI_FONT: &str = "Vazir Code WOL";

/// The font file, unchanged from the upstream release (v1.1.2). Its licence
/// texts are in `assets/vazir-code`, and the file carries them itself.
static FONT_FILE: &[u8] = include_bytes!("../../assets/vazir-code/Vazir-Code-WOL.ttf");
const FONT_FILE_NAME: &str = "Vazir-Code-WOL.ttf";
/// The name the font is listed under among the user's installed fonts.
const LISTED_AS: &str = "Vazir Code WOL (TrueType)";

/// The system's list of the fonts installed for the current user. On
/// Windows this is the registry.
pub trait FontList {
    /// The font file listed under `name`, if there is one.
    fn listed(&self, name: &str) -> io::Result<Option<PathBuf>>;

    /// Lists `file` under `name`, and lets programs that are already
    /// running use the font without signing out first.
    fn list(&mut self, name: &str, file: &Path) -> io::Result<()>;
}

/// A font list kept in a text file, one `name=file` per line. It stands in
/// for the registry on a pretend machine.
pub struct FontListFile {
    path: PathBuf,
}

impl FontListFile {
    pub fn new(path: PathBuf) -> FontListFile {
        FontListFile { path }
    }

    fn entries(&self) -> io::Result<Vec<(String, String)>> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };
        Ok(text
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(name, file)| (name.to_string(), file.to_string()))
            .collect())
    }
}

impl FontList for FontListFile {
    fn listed(&self, name: &str) -> io::Result<Option<PathBuf>> {
        Ok(self
            .entries()?
            .into_iter()
            .find(|(listed, _)| listed == name)
            .map(|(_, file)| PathBuf::from(file)))
    }

    fn list(&mut self, name: &str, file: &Path) -> io::Result<()> {
        let mut entries = self.entries()?;
        entries.retain(|(listed, _)| listed != name);
        entries.push((name.to_string(), file.display().to_string()));
        let text: String = entries
            .iter()
            .map(|(name, file)| format!("{name}={file}\n"))
            .collect();
        fs::write(&self.path, text)
    }
}

/// Puts the Farsi font in `folder` and on `list`, unless it is there
/// already.
pub(super) fn install(folder: &Path, list: &mut dyn FontList, report: &mut Report) {
    report.heading("Farsi font");
    let file = folder.join(FONT_FILE_NAME);
    match install_file(&file, list) {
        Ok(true) => {
            report.line(format!("installed {FARSI_FONT} for this user:"));
            report.line(format!("  {}", file.display()));
        }
        Ok(false) => report.line(format!(
            "{FARSI_FONT} is already installed: {}",
            file.display()
        )),
        Err(error) => report.failure(format!(
            "could not install {FARSI_FONT} as {}: {error}",
            file.display()
        )),
    }
}

/// Returns whether anything had to be done.
fn install_file(file: &Path, list: &mut dyn FontList) -> io::Result<bool> {
    let in_place = fs::read(file).is_ok_and(|found| found == FONT_FILE);
    let listed = list.listed(LISTED_AS)?.is_some_and(|listed| listed == file);
    if in_place && listed {
        return Ok(false);
    }
    if !in_place {
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(file, FONT_FILE)?;
    }
    list.list(LISTED_AS, file)?;
    Ok(true)
}
