//! The hidden terminal on Windows: a pseudoconsole (ConPTY) with the Target
//! app attached to it.

mod app_input;
mod command_line;

use std::ffi::{c_void, OsStr, OsString};
use std::fs::File;
use std::io;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::{Command, Stdio};
use std::ptr;
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
    PSEUDOCONSOLE_INHERIT_CURSOR,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, UpdateProcThreadAttribute, WaitForSingleObject,
    CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, INFINITE, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

use crate::Size;

pub use app_input::AppInput;

/// A Target app running on a hidden terminal.
pub struct RunningApp {
    hidden_terminal: Mutex<Option<HPCON>>,
    process: OwnedHandle,
    output: Option<File>,
    input: Option<AppInput>,
}

impl RunningApp {
    /// Starts `program` with `args` on a hidden terminal of `size`.
    ///
    /// The hidden terminal starts where the real Terminal's cursor is and
    /// leaves the rows above it alone. To learn where that is, its first
    /// output is the question `ESC [ 6 n`, and nothing more comes until the
    /// answer `ESC [ row ; column R` is written to its input.
    ///
    /// After a resize the hidden terminal draws its whole screen again, so
    /// whoever reads the output need not rearrange text the way it does.
    pub fn start(program: &OsStr, args: &[OsString], size: Size) -> io::Result<RunningApp> {
        let start_line = command_line::start_line(program, args)?;

        let (input_for_app, input) = AppInput::open()?;
        let (output, output_from_app) = pipe()?;

        let mut hidden_terminal: HPCON = 0;
        // SAFETY: both pipe ends are open handles, and the out-pointer is valid.
        let result = unsafe {
            CreatePseudoConsole(
                coord(size),
                input_for_app.as_raw_handle() as HANDLE,
                output_from_app.as_raw_handle() as HANDLE,
                PSEUDOCONSOLE_INHERIT_CURSOR,
                &mut hidden_terminal,
            )
        };
        if result < 0 {
            return Err(io::Error::from_raw_os_error(result));
        }
        // The hidden terminal keeps its own copies of these ends. Ours must
        // close, or the output would never end after the app does.
        drop(input_for_app);
        drop(output_from_app);

        let process = match start_attached(&start_line, hidden_terminal) {
            Ok(process) => process,
            Err(error) => {
                // SAFETY: the pseudoconsole was created above and not yet closed.
                unsafe { ClosePseudoConsole(hidden_terminal) };
                return Err(error);
            }
        };

        Ok(RunningApp {
            hidden_terminal: Mutex::new(Some(hidden_terminal)),
            process,
            output: Some(File::from(output)),
            input: Some(input),
        })
    }

    /// What the app draws, as the hidden terminal passes it on. Reading ends
    /// after `wait` has returned and everything drawn has been read.
    pub fn take_output(&mut self) -> File {
        self.output
            .take()
            .expect("the app's output was already taken")
    }

    /// Where keys for the app are written, and answers to what the hidden
    /// terminal asks.
    pub fn take_input(&mut self) -> AppInput {
        self.input
            .take()
            .expect("the app's input was already taken")
    }

    /// Gives the hidden terminal, and so the app, a new size.
    pub fn resize(&self, size: Size) -> io::Result<()> {
        let hidden_terminal = self.hidden_terminal.lock().unwrap();
        if let Some(hidden_terminal) = *hidden_terminal {
            // SAFETY: the pseudoconsole is open while it is in the mutex.
            let result = unsafe { ResizePseudoConsole(hidden_terminal, coord(size)) };
            if result < 0 {
                return Err(io::Error::from_raw_os_error(result));
            }
        }
        Ok(())
    }

    /// Waits for the app to end and returns its exit status. The hidden
    /// terminal is then closed, which ends the output once it has been read.
    /// Something else must keep reading the output while this runs: closing
    /// waits until what the app drew last has been taken.
    pub fn wait(&self) -> io::Result<i32> {
        let process = self.process.as_raw_handle() as HANDLE;
        let mut status = 0u32;
        // SAFETY: the process handle stays open for as long as `self` lives.
        unsafe {
            if WaitForSingleObject(process, INFINITE) != WAIT_OBJECT_0 {
                return Err(io::Error::last_os_error());
            }
            if GetExitCodeProcess(process, &mut status) == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        if let Some(hidden_terminal) = self.hidden_terminal.lock().unwrap().take() {
            // SAFETY: taking it out of the mutex makes this the only close.
            unsafe { ClosePseudoConsole(hidden_terminal) };
        }
        Ok(status as i32)
    }
}

impl Drop for RunningApp {
    fn drop(&mut self) {
        if let Some(hidden_terminal) = self.hidden_terminal.lock().unwrap().take() {
            // Stop reading first, so that closing cannot wait on us.
            self.output = None;
            // SAFETY: taking it out of the mutex makes this the only close.
            unsafe { ClosePseudoConsole(hidden_terminal) };
        }
    }
}

/// Runs `program` with `args` with no hidden terminal: it reads and writes
/// whatever farcli itself was given. Returns the program's exit status.
pub fn run_directly(program: &OsStr, args: &[OsString]) -> io::Result<i32> {
    let file = command_line::find_program(program)?;
    let status = Command::new(file).args(args).status()?;
    Ok(status.code().unwrap_or(1))
}

/// Runs `program` with `args` away from any terminal, waits for it to end
/// and returns what it printed.
pub fn printed_by(program: &OsStr, args: &[&str]) -> io::Result<String> {
    let file = command_line::find_program(program)?;
    let output = Command::new(file)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn coord(size: Size) -> COORD {
    COORD {
        X: size.cols.min(i16::MAX as u16) as i16,
        Y: size.rows.min(i16::MAX as u16) as i16,
    }
}

/// Makes a pipe and returns its (read, write) ends.
fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read: HANDLE = ptr::null_mut();
    let mut write: HANDLE = ptr::null_mut();
    // SAFETY: both out-pointers are valid, and on success both handles are
    // new and owned by nobody else.
    unsafe {
        if CreatePipe(&mut read, &mut write, ptr::null(), 0) == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((
            OwnedHandle::from_raw_handle(read as _),
            OwnedHandle::from_raw_handle(write as _),
        ))
    }
}

/// Starts the program with the pseudoconsole as its terminal.
fn start_attached(
    start_line: &command_line::StartLine,
    hidden_terminal: HPCON,
) -> io::Result<OwnedHandle> {
    let program: Vec<u16> = start_line
        .program
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut command_line: Vec<u16> = start_line
        .command_line
        .iter()
        .copied()
        .chain(Some(0))
        .collect();

    // SAFETY: the attribute list lives in `attributes` until it is deleted
    // below, every pointer handed over is valid for the call it is handed
    // to, and the handles returned on success are new and owned by us.
    unsafe {
        let mut attributes_len = 0usize;
        InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut attributes_len);
        let mut attributes = vec![0u8; attributes_len];
        let attribute_list = attributes.as_mut_ptr() as *mut c_void;
        if InitializeProcThreadAttributeList(attribute_list, 1, 0, &mut attributes_len) == 0 {
            return Err(io::Error::last_os_error());
        }
        let started = (|| {
            if UpdateProcThreadAttribute(
                attribute_list,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                hidden_terminal as *const c_void,
                mem::size_of::<HPCON>(),
                ptr::null_mut(),
                ptr::null(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }

            let mut startup: STARTUPINFOEXW = mem::zeroed();
            startup.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
            // Empty standard handles make the app use the hidden terminal
            // even when farcli's own were redirected.
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.lpAttributeList = attribute_list;

            let mut started: PROCESS_INFORMATION = mem::zeroed();
            if CreateProcessW(
                program.as_ptr(),
                command_line.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                ptr::null(),
                ptr::null(),
                &startup.StartupInfo,
                &mut started,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            drop(OwnedHandle::from_raw_handle(started.hThread as _));
            Ok(OwnedHandle::from_raw_handle(started.hProcess as _))
        })();
        DeleteProcThreadAttributeList(attribute_list);
        started
    }
}
