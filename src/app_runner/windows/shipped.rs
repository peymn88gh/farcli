//! The hidden terminal farcli ships: Microsoft's `conpty.dll` and
//! `OpenConsole.exe`, carried inside farcli and unpacked for the current
//! user the first time they are needed. See `docs/adr/0002` and
//! `assets/hidden-terminal/NOTICE.txt`.

use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::ptr;

use windows_sys::Win32::Foundation::{HANDLE, HMODULE};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
use windows_sys::Win32::System::Console::{COORD, HPCON};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};

use crate::hidden_terminal::SHIPPED_VERSION;

use super::Api;

/// The library a program calls to start a hidden terminal.
const LIBRARY: &str = "conpty.dll";

/// The program that is the hidden terminal. The library starts the copy
/// that lies beside it; with none there it would quietly use the one built
/// into Windows.
const PROGRAM: &str = "OpenConsole.exe";

const LICENCE: &str = "LICENSE.txt";
const NOTICE: &str = "NOTICE.txt";

#[cfg(target_arch = "x86_64")]
mod carried {
    /// The processor the carried files are for, as Microsoft names it.
    pub const PROCESSOR: &str = "x64";
    pub static LIBRARY: &[u8] = include_bytes!("../../../assets/hidden-terminal/conpty.dll");
    pub static PROGRAM: &[u8] = include_bytes!("../../../assets/hidden-terminal/OpenConsole.exe");
}

static LICENCE_TEXT: &[u8] = include_bytes!("../../../assets/hidden-terminal/LICENSE");
static NOTICE_TEXT: &[u8] = include_bytes!("../../../assets/hidden-terminal/NOTICE.txt");

/// The shipped hidden terminal's files, lying in a folder exactly as farcli
/// carries them, and kept from being changed for as long as this lives.
pub struct ShippedFiles {
    folder: PathBuf,
    /// Open for reading in a way that lets others read and run the files
    /// but not write, rename or delete them.
    _held: [File; 2],
}

impl ShippedFiles {
    /// The folder of the current user that farcli unpacks into. Each version
    /// of the files has a folder of its own, so a farcli that carries a
    /// newer version never has to replace files an older farcli is running.
    pub fn folder_of_this_user() -> io::Result<PathBuf> {
        let local = env::var_os("LOCALAPPDATA")
            .filter(|folder| !folder.is_empty())
            .ok_or_else(|| io::Error::other("this account has no local application data"))?;
        Ok(folder_under(Path::new(&local)))
    }

    /// Makes the files ready in `folder`, which is made if it is missing.
    ///
    /// A file that is already there is used only if it is byte for byte the
    /// one farcli carries; one that is missing or differs is put there
    /// afresh. Anything else lying in the folder is refused: the hidden
    /// terminal loads libraries from its own folder, so a file farcli did
    /// not put there could end up running.
    #[cfg(target_arch = "x86_64")]
    pub fn ready_in(folder: &Path) -> io::Result<ShippedFiles> {
        fs::create_dir_all(folder)?;
        let held = [
            in_place(folder, LIBRARY, carried::LIBRARY)?,
            in_place(folder, PROGRAM, carried::PROGRAM)?,
        ];
        in_place(folder, LICENCE, LICENCE_TEXT)?;
        in_place(folder, NOTICE, NOTICE_TEXT)?;
        for entry in fs::read_dir(folder)? {
            let name = entry?.file_name();
            let known = [LIBRARY, PROGRAM, LICENCE, NOTICE]
                .iter()
                .any(|known| name.eq_ignore_ascii_case(known));
            if !known {
                return Err(io::Error::other(format!(
                    "{} holds a file farcli did not put there: {}",
                    folder.display(),
                    name.to_string_lossy()
                )));
            }
        }
        Ok(ShippedFiles {
            folder: folder.to_path_buf(),
            _held: held,
        })
    }

    #[cfg(not(target_arch = "x86_64"))]
    pub fn ready_in(_folder: &Path) -> io::Result<ShippedFiles> {
        let _ = (LICENCE_TEXT, NOTICE_TEXT);
        Err(io::Error::other(
            "farcli carries it for x64 processors only",
        ))
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Loads the library and finds what farcli calls in it.
    pub(super) fn load(&self) -> io::Result<Api> {
        let file: Vec<u16> = self
            .folder
            .join(LIBRARY)
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: the path ends in a zero. The library is never unloaded,
        // so the functions found in it stay valid, and each is given the
        // signature Microsoft documents for it in `conpty.h`.
        unsafe {
            // Whatever else the library needs is looked for in the Windows
            // system folder only, never beside it.
            let library =
                LoadLibraryExW(file.as_ptr(), ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
            if library.is_null() {
                return Err(io::Error::last_os_error());
            }
            Ok(Api {
                create: std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> i32,
                >(function(library, c"ConptyCreatePseudoConsole")?),
                resize: std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    unsafe extern "system" fn(HPCON, COORD) -> i32,
                >(function(library, c"ConptyResizePseudoConsole")?),
                close: std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    unsafe extern "system" fn(HPCON),
                >(function(library, c"ConptyClosePseudoConsole")?),
            })
        }
    }
}

/// Where the files go under an account's local application data.
fn folder_under(local: &Path) -> PathBuf {
    #[cfg(target_arch = "x86_64")]
    let processor = carried::PROCESSOR;
    #[cfg(not(target_arch = "x86_64"))]
    let processor = "none";
    local
        .join("farcli")
        .join("hidden-terminal")
        .join(format!("{SHIPPED_VERSION}-{processor}"))
}

unsafe fn function(
    library: HMODULE,
    name: &std::ffi::CStr,
) -> io::Result<unsafe extern "system" fn() -> isize> {
    GetProcAddress(library, name.as_ptr().cast())
        .ok_or_else(|| io::Error::other(format!("{LIBRARY} has no {}", name.to_string_lossy())))
}

/// Makes `folder\name` hold exactly `carried` and returns it open, so that
/// nobody can write to it, rename it or delete it until it is closed.
#[cfg(target_arch = "x86_64")]
fn in_place(folder: &Path, name: &str, carried: &[u8]) -> io::Result<File> {
    let file = folder.join(name);
    if let Some(held) = held_if_same(&file, carried) {
        return Ok(held);
    }
    // Written under another name first and then moved, so that the file is
    // never there half written. A farcli started at the same moment does
    // the same; whichever move comes second finds the work done.
    let mut being_written = OsString::from(folder.as_os_str());
    being_written.push(format!(".{name}.{}.new", std::process::id()));
    let being_written = PathBuf::from(being_written);
    fs::write(&being_written, carried)?;
    let moved = fs::rename(&being_written, &file);
    if moved.is_err() {
        let _ = fs::remove_file(&being_written);
    }
    held_if_same(&file, carried).ok_or_else(|| {
        let why = match moved {
            Err(error) => format!(" ({error})"),
            Ok(()) => String::new(),
        };
        io::Error::other(format!(
            "{} is not the file farcli carries and could not be replaced{why}",
            file.display()
        ))
    })
}

/// Opens `file` so that it cannot change, and returns it if it holds
/// exactly `carried`.
#[cfg(target_arch = "x86_64")]
fn held_if_same(file: &Path, carried: &[u8]) -> Option<File> {
    let mut held = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(file)
        .ok()?;
    let mut found = Vec::with_capacity(carried.len());
    held.read_to_end(&mut found).ok()?;
    (found == carried).then_some(held)
}
