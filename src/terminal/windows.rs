//! The real Terminal on Windows, reached through the console farcli was
//! started in.

use std::fmt::Write as _;
use std::io;
use std::mem;
use std::ptr;

use windows_sys::Win32::Foundation::{BOOL, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetConsoleScreenBufferInfo, GetStdHandle, ReadConsoleInputW,
    SetConsoleCtrlHandler, SetConsoleMode, WriteConsoleW, CONSOLE_MODE, CONSOLE_SCREEN_BUFFER_INFO,
    CTRL_BREAK_EVENT, CTRL_C_EVENT, DISABLE_NEWLINE_AUTO_RETURN, ENABLE_PROCESSED_OUTPUT,
    ENABLE_VIRTUAL_TERMINAL_PROCESSING, ENABLE_WINDOW_INPUT, FOCUS_EVENT, INPUT_RECORD, KEY_EVENT,
    STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, WINDOW_BUFFER_SIZE_EVENT,
};

use crate::screen::Position;
use crate::Size;

use super::TerminalEvent;

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

    /// Makes the Terminal take drawing codes as they are and hand over every
    /// key as it is pressed, until the returned value is dropped.
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
            if SetConsoleMode(self.input, ENABLE_WINDOW_INPUT) == 0
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
    ///
    /// Keys are reported in Windows' own key report format (win32-input-mode,
    /// `ESC [ Vk ; Sc ; Uc ; Kd ; Cs ; Rc _`): one code for every key going
    /// down or up, carrying everything Windows says about it. The hidden
    /// terminal turns each code back into the same key, so the Target app
    /// sees exactly the keys it would see without farcli, Shift+Enter and
    /// the like included.
    pub fn watch(&self, mut report: impl FnMut(TerminalEvent) -> bool) -> io::Result<()> {
        let mut size = self.size()?;
        // SAFETY: `INPUT_RECORD` is plain data, the buffer pointer and length
        // match, and a record's union is read only as its type says.
        unsafe {
            let mut records: [INPUT_RECORD; 256] = mem::zeroed();
            loop {
                let mut count = 0u32;
                if ReadConsoleInputW(self.input, records.as_mut_ptr(), 256, &mut count) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let mut keys = String::new();
                let mut resized = false;
                for record in &records[..count as usize] {
                    match u32::from(record.EventType) {
                        KEY_EVENT => {
                            let key = record.Event.KeyEvent;
                            let _ = write!(
                                keys,
                                "\x1b[{};{};{};{};{};{}_",
                                key.wVirtualKeyCode,
                                key.wVirtualScanCode,
                                key.uChar.UnicodeChar,
                                u8::from(key.bKeyDown != 0),
                                key.dwControlKeyState,
                                key.wRepeatCount,
                            );
                        }
                        // The window gained or lost the keyboard.
                        FOCUS_EVENT => {
                            let focused = record.Event.FocusEvent.bSetFocus != 0;
                            keys.push_str(if focused { "\x1b[I" } else { "\x1b[O" });
                        }
                        WINDOW_BUFFER_SIZE_EVENT => resized = true,
                        _ => {}
                    }
                }
                if !keys.is_empty() && !report(TerminalEvent::Keys(keys.into_bytes())) {
                    return Ok(());
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
        // SAFETY: plain calls on console handles.
        unsafe {
            SetConsoleMode(self.terminal.input, self.input_mode);
            SetConsoleMode(self.terminal.output, self.output_mode);
            SetConsoleCtrlHandler(Some(ignore_break), 0);
        }
    }
}

unsafe extern "system" fn ignore_break(event: u32) -> BOOL {
    BOOL::from(event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT)
}
