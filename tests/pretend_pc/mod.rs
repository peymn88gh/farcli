//! A pretend machine for the Setup tests: a temporary folder that holds
//! copies of hand-written settings files and stands in for a whole machine.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use farcli::setup::{self, Machine};

/// A pretend machine in a temporary folder, removed when the test ends.
pub struct Pc {
    pub root: PathBuf,
}

impl Pc {
    /// A machine with no Terminal installed.
    pub fn new() -> Pc {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "farcli-setup-test-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Pc { root }
    }

    /// Installs Windows Terminal with a copy of the named settings file.
    pub fn with_windows_terminal(self, settings: &str) -> Pc {
        self.put("windows-terminal/settings.json", &fixture(settings));
        self
    }

    /// Installs VS Code with no settings file yet.
    pub fn with_vscode_installed(self) -> Pc {
        fs::create_dir_all(self.file("vscode")).unwrap();
        self
    }

    /// Installs VS Code with a copy of the named settings file.
    pub fn with_vscode(self, settings: &str) -> Pc {
        self.put("vscode/settings.json", &fixture(settings));
        self
    }

    /// Installs WezTerm with no configuration file yet.
    pub fn with_wezterm_installed(self) -> Pc {
        fs::create_dir_all(self.file("wezterm")).unwrap();
        self
    }

    /// Installs WezTerm with a copy of the named configuration file.
    pub fn with_wezterm(self, config: &str) -> Pc {
        self.put("wezterm/wezterm.lua", &fixture(config));
        self
    }

    /// Every file and folder in `folder`, by name.
    pub fn names_in(&self, folder: &str) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(self.file(folder))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    /// The file at `place`, a path inside the pretend machine written with
    /// forward slashes.
    pub fn file(&self, place: &str) -> PathBuf {
        place
            .split('/')
            .fold(self.root.clone(), |path, part| path.join(part))
    }

    pub fn put(&self, place: &str, contents: &[u8]) {
        let path = self.file(place);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    /// The path of `place` as Setup prints it.
    pub fn path(&self, place: &str) -> String {
        self.file(place).display().to_string()
    }

    pub fn read(&self, place: &str) -> Vec<u8> {
        fs::read(self.file(place)).unwrap()
    }

    pub fn has(&self, place: &str) -> bool {
        self.file(place).exists()
    }

    /// Runs Setup and returns what it printed.
    pub fn run_setup(&self) -> String {
        setup::run(&mut Machine::pretend(&self.root)).to_string()
    }
}

impl Drop for Pc {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// A hand-written settings file from `tests/fixtures/setup`.
pub fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/setup")
        .join(name);
    fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Compares two settings files as text, so that a failure shows both.
#[track_caller]
pub fn assert_same_file(found: &[u8], expected: &[u8]) {
    assert_eq!(
        String::from_utf8_lossy(found),
        String::from_utf8_lossy(expected)
    );
    assert_eq!(found, expected);
}
