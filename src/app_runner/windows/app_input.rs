//! The hidden terminal's input: where keys and answers for the Target app
//! are written.
//!
//! The hidden terminal takes at most 256 bytes from its input at a time and
//! makes sense of each lot on its own. A key code cut by the end of a lot
//! reaches the app as an Escape key followed by the code's own characters
//! as text. On a plain pipe the lots are cut wherever the 256th byte falls,
//! so a paste, which is hundreds of key codes at once, arrived in pieces
//! however it was written.
//!
//! So the input is a pipe that keeps writes apart: the hidden terminal is
//! handed one write at a time, never part of one and never two together.
//! `AppInput` cuts what it is given between key codes, into writes of at
//! most 256 bytes.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::process;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND};
use windows_sys::Win32::System::Pipes::{
    CreateNamedPipeW, PIPE_READMODE_MESSAGE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE,
    PIPE_WAIT,
};

/// The most bytes the hidden terminal takes from its input at a time. A
/// write longer than this would not be cut short: the hidden terminal would
/// stop reading its input for good.
const LARGEST_WRITE: usize = 256;

/// Where keys and answers for the app are written.
///
/// Write whole key codes: what is given to one `write_all` reaches the app
/// as the same keys, however many they are and however fast they come. A
/// key code split over two `write_all` calls reaches the app as an Escape
/// key and loose characters.
pub struct AppInput {
    pipe: File,
}

impl AppInput {
    /// Makes the hidden terminal's input and returns the end the hidden
    /// terminal reads from, and the end to write to.
    pub(super) fn open() -> io::Result<(OwnedHandle, AppInput)> {
        static OPENED: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            r"\\.\pipe\farcli-app-input-{}-{}",
            process::id(),
            OPENED.fetch_add(1, Ordering::Relaxed)
        );
        let wide_name: Vec<u16> = OsStr::new(&name).encode_wide().chain(Some(0)).collect();
        // SAFETY: the name is a string ending in zero, and on success the
        // handle is new and owned by nobody else.
        let for_hidden_terminal = unsafe {
            let handle = CreateNamedPipeW(
                wide_name.as_ptr(),
                // Only this process may make a pipe of this name.
                PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                // Only one writer may ever connect: the one opened below.
                1,
                0,
                64 * 1024,
                0,
                ptr::null(),
            );
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            OwnedHandle::from_raw_handle(handle as _)
        };
        let pipe = OpenOptions::new().write(true).open(&name)?;
        Ok((for_hidden_terminal, AppInput { pipe }))
    }

    /// A second way to write to the same input, for another thread. Its
    /// writes never land inside a key code written here.
    pub fn try_clone(&self) -> io::Result<AppInput> {
        Ok(AppInput {
            pipe: self.pipe.try_clone()?,
        })
    }
}

impl Write for AppInput {
    /// Writes as many whole key codes from the start of `bytes` as the
    /// hidden terminal takes at once.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            // An empty write would look like the end of the input.
            return Ok(0);
        }
        self.pipe.write(&bytes[..whole_codes(bytes)])
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// How many bytes at the start of `bytes` make up as many whole key codes
/// and characters as fit in one write. A single code longer than a write,
/// which no key has, is cut.
fn whole_codes(bytes: &[u8]) -> usize {
    let mut fitting = 0;
    while fitting < bytes.len() {
        let with_next = fitting + first_code(&bytes[fitting..]);
        if with_next > LARGEST_WRITE {
            break;
        }
        fitting = with_next;
    }
    if fitting == 0 {
        LARGEST_WRITE.min(bytes.len())
    } else {
        fitting
    }
}

/// The length of the key code or character `bytes` starts with.
fn first_code(bytes: &[u8]) -> usize {
    const ESCAPE: u8 = 0x1b;
    let length = match bytes {
        // `ESC [`, any numbers, and one last character: what farcli sends
        // for a key, a focus change and an answer.
        [ESCAPE, b'[', rest @ ..] => {
            match rest.iter().position(|byte| (0x40..=0x7e).contains(byte)) {
                Some(last) => 2 + last + 1,
                None => bytes.len(),
            }
        }
        // `ESC O` and a letter: some keys as older Terminals send them.
        [ESCAPE, b'O', _, ..] => 3,
        // Escape before a character is that character with Alt held down.
        [ESCAPE, next, ..] if *next != ESCAPE => 1 + character(&bytes[1..]),
        _ => character(bytes),
    };
    length.min(bytes.len())
}

/// The length in bytes of the character `bytes` starts with.
fn character(bytes: &[u8]) -> usize {
    let following = bytes[1..]
        .iter()
        .take_while(|byte| (0x80..0xc0).contains(*byte))
        .count();
    match bytes[0] {
        0xc0..=0xdf => 1 + following.min(1),
        0xe0..=0xef => 1 + following.min(2),
        0xf0..=0xf7 => 1 + following.min(3),
        _ => 1,
    }
}
