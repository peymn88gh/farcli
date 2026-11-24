//! Shared by the tests that run the real `farcli` in a Terminal with nobody
//! at the keyboard and read what that Terminal would show: `farcli` on a
//! hidden terminal, a stand-in Terminal that answers what a real one
//! answers, and a folder for stand-in Target apps that comes first on the
//! path `farcli` is given.
#![allow(dead_code)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use farcli::app_runner::{AppInput, RunningApp};
use farcli::fake_terminal::FakeTerminal;
use farcli::Size;

/// The key `a`, the Enter key and F12 going down and coming up, as Windows
/// Terminal sends them to a program that asked for keys in Windows' own
/// form, which `farcli` does.
pub const KEY_A: &str = "\x1b[65;30;97;1;0;1_\x1b[65;30;97;0;0;1_";
pub const KEY_ENTER: &str = "\x1b[13;28;13;1;0;1_\x1b[13;28;13;0;0;1_";
pub const KEY_F12: &str = "\x1b[123;88;0;1;0;1_\x1b[123;88;0;0;0;1_";

/// A folder to put stand-in apps in, removed when the test ends.
pub struct Folder(pub PathBuf);

impl Folder {
    pub fn new(test: &str) -> Folder {
        let folder = env::temp_dir().join(format!("farcli-test-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        Folder(folder)
    }

    /// Writes a file at `place`, a path inside the folder written with
    /// forward slashes.
    pub fn put(&self, place: &str, contents: &str) -> &Folder {
        let file = place
            .split('/')
            .fold(self.0.clone(), |path, part| path.join(part));
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
        self
    }

    /// The path a program sees when this folder comes first on it.
    pub fn first_on_the_path(&self) -> OsString {
        let mut path = OsString::from(&self.0);
        path.push(";");
        path.push(env::var_os("PATH").unwrap());
        path
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A pretend machine where Setup has already run, so that farcli leaves
/// the real settings of whoever runs the tests alone.
pub fn pretend_machine() -> PathBuf {
    let machine = env::temp_dir().join("farcli-in-a-terminal-test-machine");
    fs::create_dir_all(&machine).unwrap();
    fs::write(machine.join("setup-done"), "made by a test\n").unwrap();
    machine
}

/// `farcli` running in a Terminal with nobody at the keyboard.
pub struct InATerminal {
    terminal: Arc<Mutex<FakeTerminal>>,
    input: AppInput,
    reader: JoinHandle<()>,
    waiter: JoinHandle<std::io::Result<i32>>,
    started: Instant,
}

impl InATerminal {
    /// Runs `farcli` with `args` in a Terminal 110 columns wide and 12
    /// rows high, with `folder` first on its path.
    pub fn run(folder: &Folder, args: &[&str]) -> InATerminal {
        // A program started on a hidden terminal is given this test
        // program's own environment, which the tests of one file share.
        static ENVIRONMENT: Mutex<()> = Mutex::new(());
        let size = Size {
            cols: 110,
            rows: 12,
        };
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let farcli = OsString::from(env!("CARGO_BIN_EXE_farcli"));
        let started = Instant::now();
        let mut app = {
            let _alone = ENVIRONMENT.lock().unwrap();
            let path = env::var_os("PATH").unwrap();
            env::set_var("PATH", folder.first_on_the_path());
            env::set_var("FARCLI_PRETEND_MACHINE", pretend_machine());
            let app = RunningApp::start(&farcli, &args, size);
            env::set_var("PATH", path);
            app.unwrap()
        };
        let mut output = app.take_output();
        let input = app.take_input();
        let terminal = Arc::new(Mutex::new(FakeTerminal::new(size)));
        let reader = {
            let terminal = Arc::clone(&terminal);
            let mut answers = input.try_clone().unwrap();
            thread::spawn(move || {
                let mut buffer = [0u8; 65536];
                while let Ok(count @ 1..) = output.read(&mut buffer) {
                    let bytes = &buffer[..count];
                    let mut terminal = terminal.lock().unwrap();
                    terminal.receive(bytes);
                    // A real Terminal answers when asked where its cursor
                    // is and what kind of terminal it is.
                    if bytes.windows(4).any(|window| window == b"\x1b[6n") {
                        let cursor = terminal.cursor();
                        let answer = format!("\x1b[{};{}R", cursor.row + 1, cursor.col + 1);
                        let _ = answers.write_all(answer.as_bytes());
                    }
                    if bytes.windows(3).any(|window| window == b"\x1b[c") {
                        let _ = answers.write_all(b"\x1b[?61c");
                    }
                }
            })
        };
        let app = Arc::new(app);
        let waiter = thread::spawn(move || app.wait());
        InATerminal {
            terminal,
            input,
            reader,
            waiter,
            started,
        }
    }

    /// The text of each row the Terminal shows now.
    pub fn screen(&self) -> Vec<String> {
        self.terminal.lock().unwrap().screen_text()
    }

    /// Waits until `reached` says so of the screen. `what` names what is
    /// waited for, in case it never comes.
    pub fn wait_until(&self, what: &str, mut reached: impl FnMut(&[String]) -> bool) {
        let give_up_at = Instant::now() + Duration::from_secs(60);
        while !reached(&self.screen()) {
            assert!(
                Instant::now() < give_up_at,
                "never came: {what}\n{}",
                self.screen().join("\n")
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Writes to the hidden terminal what a Terminal sends for `keys`.
    pub fn user_types(&mut self, keys: &str) {
        self.input.write_all(keys.as_bytes()).unwrap();
    }

    /// Waits for farcli to end. Returns everything the Terminal showed,
    /// top to bottom and without its empty rows, farcli's exit status and
    /// how long the run took.
    pub fn end(self) -> (Vec<String>, i32, Duration) {
        let give_up_at = Instant::now() + Duration::from_secs(60);
        while !self.waiter.is_finished() {
            assert!(Instant::now() < give_up_at, "farcli did not end");
            thread::sleep(Duration::from_millis(20));
        }
        let took = self.started.elapsed();
        let status = self.waiter.join().unwrap().unwrap();
        self.reader.join().unwrap();
        let terminal = self.terminal.lock().unwrap();
        let mut shown = terminal.scrollback_text();
        shown.extend(terminal.screen_text());
        shown.retain(|row| !row.is_empty());
        (shown, status, took)
    }
}
