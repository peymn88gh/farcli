//! The real Terminal on Windows, reached through the console farcli was
//! started in.

use std::io;
use std::mem;
use std::ptr;

use windows_sys::Win32::Foundation::{BOOL, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetConsoleScreenBufferInfo, GetNumberOfConsoleInputEvents, GetStdHandle,
    ReadConsoleInputW, SetConsoleCtrlHandler, SetConsoleMode, WriteConsoleW, CONSOLE_MODE,
    CONSOLE_SCREEN_BUFFER_INFO, CTRL_BREAK_EVENT, CTRL_C_EVENT, DISABLE_NEWLINE_AUTO_RETURN,
    ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    ENABLE_WINDOW_INPUT, INPUT_RECORD, KEY_EVENT, LEFT_CTRL_PRESSED, RIGHT_CTRL_PRESSED,
    STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, WINDOW_BUFFER_SIZE_EVENT,
};

use crate::screen::Position;
use crate::Size;

use super::TerminalEvent;

/// Asks the Terminal to report every key in full (Windows' own key report
/// format), so that key combinations such as Shift+Enter reach the Target
/// app as they do without farcli. A Terminal that does not know the request
/// ignores it and keeps sending plain key codes.
const FULL_KEY_REPORTS_ON: &str = "\x1b[?9001h";
const FULL_KEY_REPORTS_OFF: &str = "\x1b[?9001l";

const VK_SPACE: u16 = 0x20;
const VK_MENU: u16 = 0x12;

/// The Terminal farcli was started in.
#[derive(Clone, Copy)]
pub struct Terminal {
    input: HANDLE,
    output: HANDLE,
}

// SAFETY: console handles are process-wide and may be used from any thread.
unsafe impl Send for Terminal {}

/// Puts the Terminal back as it was when dropped.
pub struct Restore {
    terminal: Terminal,
    input_mode: CONSOLE_MODE,
    output_mode: CONSOLE_MODE,
}

impl Terminal {
    /// The Terminal, or `None` when farcli's input or output is not one
    /// (piped, redirected or scripted).
    pub fn open() -> Option<Terminal> {
        // SAFETY: plain calls with valid out-pointers.
        unsafe {
            let terminal = Terminal {
                input: GetStdHandle(STD_INPUT_HANDLE),
                output: GetStdHandle(STD_OUTPUT_HANDLE),
            };
            let mut mode = 0;
            let is_console = |handle: HANDLE, mode: &mut CONSOLE_MODE| {
                !handle.is_null()
                    && handle != INVALID_HANDLE_VALUE
                    && GetConsoleMode(handle, mode) != 0
            };
            (is_console(terminal.input, &mut mode) && is_console(terminal.output, &mut mode))
                .then_some(terminal)
        }
    }

    pub fn size(&self) -> io::Result<Size> {
        let info = self.screen_info()?;
        Ok(Size {
            cols: (info.srWindow.Right - info.srWindow.Left + 1).max(1) as u16,
            rows: (info.srWindow.Bottom - info.srWindow.Top + 1).max(1) as u16,
        })
    }

    /// Where the cursor is on the visible screen.
    pub fn cursor(&self) -> io::Result<Position> {
        let info = self.screen_info()?;
        let size = self.size()?;
        let row = info.dwCursorPosition.Y - info.srWindow.Top;
        let col = info.dwCursorPosition.X - info.srWindow.Left;
        Ok(Position {
            row: (row.max(0) as u16).min(size.rows - 1),
            col: (col.max(0) as u16).min(size.cols - 1),
        })
    }

    fn screen_info(&self) -> io::Result<CONSOLE_SCREEN_BUFFER_INFO> {
        // SAFETY: the out-pointer is valid, and the struct is plain data.
        unsafe {
            let mut info: CONSOLE_SCREEN_BUFFER_INFO = mem::zeroed();
            if GetConsoleScreenBufferInfo(self.output, &mut info) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(info)
        }
    }

    /// Makes the Terminal take drawing codes as they are and hand over keys
    /// one by one, until the returned value is dropped.
    pub fn take_over(&self) -> io::Result<Restore> {
        // SAFETY: plain calls on console handles with valid out-pointers.
        unsafe {
            let mut input_mode = 0;
            let mut output_mode = 0;
            if GetConsoleMode(self.input, &mut input_mode) == 0
                || GetConsoleMode(self.output, &mut output_mode) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let restore = Restore {
                terminal: *self,
                input_mode,
                output_mode,
            };
            // No line editing, no echo, and Ctrl+C as a key like any other:
            // all of that is the Target app's business.
            if SetConsoleMode(
                self.input,
                ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_WINDOW_INPUT,
            ) == 0
                || SetConsoleMode(
                    self.output,
                    output_mode
                        | ENABLE_PROCESSED_OUTPUT
                        | ENABLE_VIRTUAL_TERMINAL_PROCESSING
                        | DISABLE_NEWLINE_AUTO_RETURN,
                ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            // Ctrl+Break must not end farcli under the Target app.
            SetConsoleCtrlHandler(Some(ignore_break), 1);
            self.write(FULL_KEY_REPORTS_ON.as_bytes())?;
            Ok(restore)
        }
    }

    /// Sends bytes to the Terminal.
    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        let text: Vec<u16> = String::from_utf8_lossy(bytes).encode_utf16().collect();
        for chunk in text.chunks(8192) {
            let mut rest = chunk;
            while !rest.is_empty() {
                let mut written = 0u32;
                // SAFETY: the pointer and length describe `rest`.
                let ok = unsafe {
                    WriteConsoleW(
                        self.output,
                        rest.as_ptr(),
                        rest.len() as u32,
                        &mut written,
                        ptr::null(),
                    )
                };
                if ok == 0 || written == 0 {
                    return Err(io::Error::last_os_error());
                }
                rest = &rest[written as usize..];
            }
        }
        Ok(())
    }

    /// Waits for the user to do something and reports it to `report`, for
    /// as long as `report` returns true.
    pub fn watch(&self, mut report: impl FnMut(TerminalEvent) -> bool) -> io::Result<()> {
        let mut size = self.size()?;
        let mut first_half: Option<u16> = None;
        // SAFETY: `INPUT_RECORD` is plain data, the buffer pointer and length
        // match, and a record's union is read only as its type says.
        unsafe {
            let mut records: [INPUT_RECORD; 256] = mem::zeroed();
            let mut keys = String::new();
            loop {
                let mut count = 0u32;
                if ReadConsoleInputW(self.input, records.as_mut_ptr(), 256, &mut count) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let mut resized = false;
                for record in &records[..count as usize] {
                    match u32::from(record.EventType) {
                        KEY_EVENT => {
                            let key = record.Event.KeyEvent;
                            let unit = key.uChar.UnicodeChar;
                            // Alt released with a character is how pasted
                            // and composed characters arrive.
                            let carries_text = key.bKeyDown != 0 || key.wVirtualKeyCode == VK_MENU;
                            if carries_text && unit != 0 {
                                for _ in 0..key.wRepeatCount.max(1) {
                                    push_unit(&mut keys, &mut first_half, unit);
                                }
                            } else if key.bKeyDown != 0
                                && key.wVirtualKeyCode == VK_SPACE
                                && key.dwControlKeyState & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED)
                                    != 0
                            {
                                // Ctrl+Space is the zero character.
                                keys.push('\0');
                            }
                        }
                        WINDOW_BUFFER_SIZE_EVENT => resized = true,
                        _ => {}
                    }
                }
                // A key code is reported whole. The hidden terminal takes a
                // code that arrives in pieces for separate key presses.
                let mut waiting = 0u32;
                let more_is_waiting = ends_inside_key_code(&keys)
                    && GetNumberOfConsoleInputEvents(self.input, &mut waiting) != 0
                    && waiting > 0;
                if !keys.is_empty() && !more_is_waiting {
                    let keys = mem::take(&mut keys).into_bytes();
                    if !report(TerminalEvent::Keys(keys)) {
                        return Ok(());
                    }
                }
                if resized {
                    let new_size = self.size()?;
                    if new_size != size {
                        size = new_size;
                        if !report(TerminalEvent::Resized(size)) {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = self.terminal.write(FULL_KEY_REPORTS_OFF.as_bytes());
        // SAFETY: plain calls on console handles.
        unsafe {
            SetConsoleMode(self.terminal.input, self.input_mode);
            SetConsoleMode(self.terminal.output, self.output_mode);
            SetConsoleCtrlHandler(Some(ignore_break), 0);
        }
    }
}

/// Adds one UTF-16 unit of typed text, joining the two halves of a
/// character that arrives as a pair.
fn push_unit(keys: &mut String, first_half: &mut Option<u16>, unit: u16) {
    let units = match first_half.take() {
        Some(first) => vec![first, unit],
        None if (0xd800..0xdc00).contains(&unit) => {
            *first_half = Some(unit);
            return;
        }
        None => vec![unit],
    };
    keys.extend(char::decode_utf16(units).map(|decoded| decoded.unwrap_or('\u{fffd}')));
}

/// True when `keys` stops part-way through the code for one key: after the
/// escape character, or inside a control sequence that has not reached its
/// final character.
fn ends_inside_key_code(keys: &str) -> bool {
    let Some(start) = keys.rfind('\x1b') else {
        return false;
    };
    match &keys.as_bytes()[start + 1..] {
        [] => true,
        [b'[', rest @ ..] => !rest.last().is_some_and(|last| (0x40..=0x7e).contains(last)),
        [b'O'] => true,
        _ => false,
    }
}

unsafe extern "system" fn ignore_break(event: u32) -> BOOL {
    BOOL::from(event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT)
}
