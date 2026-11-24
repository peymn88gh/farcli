//! On Windows, Setup finds each Terminal's settings file in the user's
//! account.
//!
//! Every test lays out a pretend account in a temporary folder, with the
//! folders Windows and the Terminals use, and tells Setup where the account
//! is the way Windows would: through environment variables. The font list
//! is a file standing in for the registry.

mod pretend_pc;

use std::ffi::OsString;

use farcli::setup::{self, FontListFile, Machine};
use pretend_pc::{assert_same_file, fixture, Pc};

const WINDOWS_TERMINAL: &str =
    "AppData/Local/Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json";
const WINDOWS_TERMINAL_PREVIEW: &str =
    "AppData/Local/Packages/Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe/LocalState/settings.json";
const WINDOWS_TERMINAL_FROM_A_ZIP: &str = "AppData/Local/Microsoft/Windows Terminal/settings.json";
const VS_CODE: &str = "AppData/Roaming/Code/User/settings.json";

/// The machine Setup finds for the account in `pc`, with `more` added to
/// the environment.
fn account(pc: &Pc, more: &[(&str, &str)]) -> Machine {
    let mut environment = vec![
        ("LOCALAPPDATA", pc.file("AppData/Local").into_os_string()),
        ("APPDATA", pc.file("AppData/Roaming").into_os_string()),
        ("USERPROFILE", pc.root.clone().into_os_string()),
    ];
    for (name, place) in more {
        environment.push((name, pc.file(place).into_os_string()));
    }
    let variable = |name: &str| -> Option<OsString> {
        environment
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, value)| value.clone())
    };
    let font_list = FontListFile::new(pc.file("font-list.txt"));
    Machine::of_windows_account(variable, Box::new(font_list)).expect("an account")
}

#[test]
fn every_windows_terminal_the_account_has_gets_the_farsi_font() {
    let pc = Pc::new();
    for place in [
        WINDOWS_TERMINAL,
        WINDOWS_TERMINAL_PREVIEW,
        WINDOWS_TERMINAL_FROM_A_ZIP,
    ] {
        pc.put(place, &fixture("windows-terminal/no-font.json"));
    }

    setup::run(&mut account(&pc, &[]));

    for place in [
        WINDOWS_TERMINAL,
        WINDOWS_TERMINAL_PREVIEW,
        WINDOWS_TERMINAL_FROM_A_ZIP,
    ] {
        assert_same_file(
            &pc.read(place),
            &fixture("windows-terminal/no-font.after.json"),
        );
        assert_same_file(
            &pc.read(&format!("{place}.farcli-backup")),
            &fixture("windows-terminal/no-font.json"),
        );
    }
}

#[test]
fn vs_code_settings_and_its_profiles_are_found_in_the_account() {
    let pc = Pc::new();
    let profile = "AppData/Roaming/Code/User/profiles/-5f3a9c1/settings.json";
    pc.put(VS_CODE, &fixture("vscode/no-fonts.json"));
    pc.put(profile, &fixture("vscode/editor-font.json"));

    setup::run(&mut account(&pc, &[]));

    assert_same_file(&pc.read(VS_CODE), &fixture("vscode/no-fonts.after.json"));
    assert_same_file(&pc.read(profile), &fixture("vscode/editor-font.after.json"));
}

#[test]
fn the_font_goes_where_windows_keeps_a_users_own_fonts() {
    let pc = Pc::new();

    let report = setup::run(&mut account(&pc, &[]));

    let font = "AppData/Local/Microsoft/Windows/Fonts/Vazir-Code-WOL.ttf";
    assert!(pc.has(font));
    assert_eq!(
        String::from_utf8(pc.read("font-list.txt")).unwrap(),
        format!("Vazir Code WOL (TrueType)={}\n", pc.path(font))
    );
    // Setup notes that it has run in farcli's own folder in the account.
    assert!(pc.has("AppData/Local/farcli/setup-done"));
    // No Terminal is installed in this account.
    assert!(!report.failed());
    assert_eq!(report.to_string().matches("not installed").count(), 3);
    assert_eq!(pc.names_in(""), ["AppData", "font-list.txt"]);
}

#[test]
fn wezterm_found_on_the_path_with_no_configuration_gets_one_in_the_home_folder() {
    let pc = Pc::new();
    pc.put("Programs/WezTerm/wezterm.exe", b"");

    setup::run(&mut account(&pc, &[("PATH", "Programs/WezTerm")]));

    assert_same_file(&pc.read(".wezterm.lua"), &fixture("wezterm/created.lua"));
}

#[test]
fn the_wezterm_configuration_setup_reads_is_the_one_wezterm_reads() {
    // WezTerm looks in these places in this order and reads the first.
    let places = [
        ("WEZTERM_CONFIG_FILE", "elsewhere/my-wezterm.lua"),
        ("XDG_CONFIG_HOME", "xdg/wezterm/wezterm.lua"),
        ("", ".config/wezterm/wezterm.lua"),
        ("", ".wezterm.lua"),
    ];
    for first in 0..places.len() {
        let pc = Pc::new();
        // The first place holds a configuration that already names the
        // Farsi font; every later place holds one that does not.
        pc.put(places[first].1, &fixture("wezterm/with-farsi-font.lua"));
        for (_, later) in &places[first + 1..] {
            pc.put(later, &fixture("wezterm/hand-written.lua"));
        }
        let environment = [
            ("WEZTERM_CONFIG_FILE", "elsewhere/my-wezterm.lua"),
            ("XDG_CONFIG_HOME", "xdg"),
        ];
        let environment: Vec<_> = environment
            .into_iter()
            .filter(|(name, _)| places[first..].iter().any(|(set_by, _)| set_by == name))
            .collect();

        let printed = setup::run(&mut account(&pc, &environment)).to_string();

        assert!(
            printed.contains(&format!("already in place: {}", pc.path(places[first].1))),
            "{}: {printed}",
            places[first].1
        );
    }
}
