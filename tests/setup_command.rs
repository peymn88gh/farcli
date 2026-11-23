//! `farcli setup` runs Setup on demand and prints what it did.
//!
//! These tests run the real `farcli` program. `FARCLI_PRETEND_MACHINE`
//! names a temporary folder that stands in for the machine, so no real
//! settings, fonts or registry are read or changed.

mod pretend_pc;

use std::process::{Command, Output};

use pretend_pc::{assert_same_file, fixture, Pc};

fn farcli(pc: &Pc) -> Command {
    let mut farcli = Command::new(env!("CARGO_BIN_EXE_farcli"));
    farcli.env("FARCLI_PRETEND_MACHINE", &pc.root);
    farcli
}

fn printed(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn farcli_setup_prints_each_change_and_where_each_backup_is() {
    let pc = Pc::new()
        .with_windows_terminal("windows-terminal/no-font.json")
        .with_wezterm("wezterm/hand-written.lua")
        .with_vscode("vscode/terminal-font.json");

    let output = farcli(&pc).arg("setup").output().unwrap();

    assert!(output.status.success());
    let printed = printed(&output);
    for expected in [
        // The font.
        format!(
            "installed Vazir Code WOL for this user:\n    {}",
            pc.path("fonts/Vazir-Code-WOL.ttf")
        ),
        // Windows Terminal: the file, the change, the backup.
        format!("changed {}", pc.path("windows-terminal/settings.json")),
        "profiles.defaults.font.face is now \"Cascadia Mono, Vazir Code WOL\"".to_string(),
        format!(
            "backup: {}",
            pc.path("windows-terminal/settings.json.farcli-backup")
        ),
        // WezTerm: left alone, with what to add by hand.
        format!("left alone: {}", pc.path("wezterm/wezterm.lua")),
        "{ family = 'Vazir Code WOL', scale = 1.2 },".to_string(),
        // VS Code: the file, the change, the backup.
        format!("changed {}", pc.path("vscode/settings.json")),
        "terminal.integrated.fontFamily is now \"'Cascadia Code', 'Vazir Code WOL', \\\"Courier New\\\", monospace\""
            .to_string(),
        format!("backup: {}", pc.path("vscode/settings.json.farcli-backup")),
    ] {
        assert!(printed.contains(&expected), "{expected}\nnot in\n{printed}");
    }
    assert_same_file(
        &pc.read("windows-terminal/settings.json"),
        &fixture("windows-terminal/no-font.after.json"),
    );
    assert_same_file(
        &pc.read("vscode/settings.json"),
        &fixture("vscode/terminal-font.after.json"),
    );
}

#[test]
fn farcli_setup_run_again_says_everything_is_in_place() {
    let pc = Pc::new()
        .with_windows_terminal("windows-terminal/no-font.json")
        .with_vscode("vscode/no-fonts.json");
    farcli(&pc).arg("setup").output().unwrap();
    let after_first_run = [
        pc.read("windows-terminal/settings.json"),
        pc.read("vscode/settings.json"),
    ];

    let output = farcli(&pc).arg("setup").output().unwrap();

    assert!(output.status.success());
    let printed = printed(&output);
    assert!(!printed.contains("changed"), "{printed}");
    assert!(!printed.contains("backup"), "{printed}");
    assert_eq!(printed.matches("already in place").count(), 2, "{printed}");
    assert_same_file(
        &pc.read("windows-terminal/settings.json"),
        &after_first_run[0],
    );
    assert_same_file(&pc.read("vscode/settings.json"), &after_first_run[1]);
}

#[test]
fn farcli_setup_ends_with_an_error_status_when_a_change_could_not_be_made() {
    let pc = Pc::new().with_windows_terminal("windows-terminal/no-font.json");
    let settings = pc.file("windows-terminal/settings.json");
    let mut permissions = std::fs::metadata(&settings).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&settings, permissions).unwrap();

    let output = farcli(&pc).arg("setup").output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(printed(&output).contains("could not change"));
}

#[test]
fn a_piped_first_run_does_no_setup_and_passes_the_apps_output_byte_for_byte() {
    // Setup has never run on this machine. A script that pipes farcli must
    // still get exactly what the app wrote, and nothing may be changed.
    let pc = Pc::new().with_windows_terminal("windows-terminal/no-font.json");
    let mut farcli = farcli(&pc);
    if cfg!(windows) {
        farcli.args(["cmd", "/d", "/c", "echo from the app"]);
    } else {
        farcli.args(["sh", "-c", "echo from the app"]);
    }

    let output = farcli.output().unwrap();

    let line_end = if cfg!(windows) { "\r\n" } else { "\n" };
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("from the app{line_end}")
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(pc.names_in(""), ["windows-terminal"]);
    assert_same_file(
        &pc.read("windows-terminal/settings.json"),
        &fixture("windows-terminal/no-font.json"),
    );
}
