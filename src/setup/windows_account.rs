//! Where a Windows account keeps the fonts and the settings files Setup
//! works on.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{vscode, FontList, Machine};

/// The settings file of Windows Terminal as installed from the Store, of
/// its Preview, and of one unpacked from a zip, inside the account's local
/// application data.
const WINDOWS_TERMINAL_SETTINGS: [&str; 3] = [
    r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json",
    r"Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json",
    r"Microsoft\Windows Terminal\settings.json",
];

impl Machine {
    /// The machine as one Windows account sees it. `variable` gives the
    /// value of an environment variable, and `font_list` is the account's
    /// list of installed fonts. Returns `None` when the environment does
    /// not say where the account keeps its files.
    pub fn of_windows_account(
        variable: impl Fn(&str) -> Option<OsString>,
        font_list: Box<dyn FontList>,
    ) -> Option<Machine> {
        let folder = |name: &str| variable(name).map(PathBuf::from);
        let local = folder("LOCALAPPDATA")?;
        let roaming = folder("APPDATA")?;
        let home = folder("USERPROFILE")?;
        Some(Machine {
            fonts_folder: path(&local, r"Microsoft\Windows\Fonts"),
            font_list,
            windows_terminal_settings: WINDOWS_TERMINAL_SETTINGS
                .iter()
                .map(|settings| path(&local, settings))
                .collect(),
            wezterm_config: wezterm_config(&variable, &home),
            vscode_settings: vscode::settings_files_in(&path(&roaming, r"Code\User")),
            setup_done_mark: path(&local, r"farcli\setup-done"),
        })
    }
}

/// `inside`, written with backslashes, under `folder`.
fn path(folder: &Path, inside: &str) -> PathBuf {
    inside
        .split('\\')
        .fold(folder.to_path_buf(), |path, part| path.join(part))
}

/// The configuration file WezTerm reads in this account, whether it exists
/// yet or not. `None` when WezTerm is not installed: it is not on the
/// `PATH` and has no configuration file.
fn wezterm_config(variable: impl Fn(&str) -> Option<OsString>, home: &Path) -> Option<PathBuf> {
    // WezTerm reads the first of these that applies.
    if let Some(named) = variable("WEZTERM_CONFIG_FILE") {
        return Some(PathBuf::from(named));
    }
    let in_config_folder = variable("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .into_iter()
        .chain([home.join(".config")])
        .map(|folder| path(&folder, r"wezterm\wezterm.lua"));
    let in_home = home.join(".wezterm.lua");
    if let Some(found) = in_config_folder
        .chain([in_home.clone()])
        .find(|file| file.is_file())
    {
        return Some(found);
    }
    let on_path = variable("PATH").is_some_and(|folders| {
        env::split_paths(&folders).any(|folder| {
            ["wezterm.exe", "wezterm-gui.exe"]
                .iter()
                .any(|program| folder.join(program).is_file())
        })
    });
    on_path.then_some(in_home)
}

/// The fonts installed for the current user, as Windows lists them in the
/// registry.
#[cfg(windows)]
pub(super) mod registry {
    use std::ffi::OsStr;
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};
    use std::ptr;

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
    use windows_sys::Win32::Graphics::Gdi::AddFontResourceW;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegGetValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendNotifyMessageW, HWND_BROADCAST, WM_FONTCHANGE,
    };

    use crate::setup::FontList;

    /// Where Windows lists the fonts installed for the current user only.
    /// Fonts listed here need no administrator rights.
    const FONTS_KEY: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Fonts";

    pub struct UserFonts;

    impl FontList for UserFonts {
        fn listed(&self, name: &str) -> io::Result<Option<PathBuf>> {
            let key = wide(FONTS_KEY);
            let name = wide(name);
            let mut file = [0u16; 1024];
            let mut size = std::mem::size_of_val(&file) as u32;
            // SAFETY: every pointer is to a live, null-ended or sized
            // buffer of this function.
            let result = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    RRF_RT_REG_SZ,
                    ptr::null_mut(),
                    file.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            match result {
                ERROR_SUCCESS => {
                    let length = file.iter().position(|&unit| unit == 0).unwrap_or(0);
                    Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
                        &file[..length],
                    ))))
                }
                ERROR_FILE_NOT_FOUND => Ok(None),
                error => Err(failure(error)),
            }
        }

        fn list(&mut self, name: &str, file: &Path) -> io::Result<()> {
            let key = wide(FONTS_KEY);
            let name = wide(name);
            let file = wide(file);
            let mut fonts: HKEY = ptr::null_mut();
            // SAFETY: as above; the key is closed before returning.
            unsafe {
                let opened = RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    0,
                    ptr::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_SET_VALUE,
                    ptr::null(),
                    &mut fonts,
                    ptr::null_mut(),
                );
                if opened != ERROR_SUCCESS {
                    return Err(failure(opened));
                }
                let set = RegSetValueExW(
                    fonts,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    file.as_ptr().cast(),
                    std::mem::size_of_val(&file[..]) as u32,
                );
                RegCloseKey(fonts);
                if set != ERROR_SUCCESS {
                    return Err(failure(set));
                }
                // The registry takes effect at the next sign-in. This makes
                // the font usable now, and tells running programs.
                if AddFontResourceW(file.as_ptr()) == 0 {
                    return Err(io::Error::other(
                        "the font is installed, but Windows could not load it now; \
                         sign out and in again to use it",
                    ));
                }
                SendNotifyMessageW(HWND_BROADCAST, WM_FONTCHANGE, 0, 0);
            }
            Ok(())
        }
    }

    fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
        text.as_ref().encode_wide().chain([0]).collect()
    }

    fn failure(error: WIN32_ERROR) -> io::Error {
        io::Error::from_raw_os_error(error as i32)
    }
}
