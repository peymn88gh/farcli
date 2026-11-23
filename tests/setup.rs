//! Setup gives every Supported terminal on a machine the Farsi font.
//!
//! Every test runs Setup on a pretend machine: a temporary folder holding
//! copies of hand-written settings files. It then checks what a person would
//! find afterwards: the settings file, its backup, and what Setup printed.
//! No test reads or changes real settings, the real fonts folder or the
//! registry.

mod pretend_pc;

use std::fs;

use farcli::setup::{self, Machine};
use pretend_pc::{assert_same_file, fixture, Pc};

#[test]
fn windows_terminal_gets_the_farsi_font_after_its_english_font() {
    let pc = Pc::new().with_windows_terminal("windows-terminal/no-font.json");

    let printed = pc.run_setup();

    // Cascadia Mono is the font Windows Terminal uses when none is set.
    assert_same_file(
        &pc.read("windows-terminal/settings.json"),
        &fixture("windows-terminal/no-font.after.json"),
    );
    assert_same_file(
        &pc.read("windows-terminal/settings.json.farcli-backup"),
        &fixture("windows-terminal/no-font.json"),
    );
    assert!(printed.contains(&pc.path("windows-terminal/settings.json")));
    assert!(printed.contains("Cascadia Mono, Vazir Code WOL"));
    assert!(printed.contains(&pc.path("windows-terminal/settings.json.farcli-backup")));
}

#[test]
fn fonts_the_user_chose_in_windows_terminal_stay_first_and_their_notes_stay() {
    // The defaults name two fonts, one profile names its own, and the file
    // has comments and commas after last members.
    let pc = Pc::new().with_windows_terminal("windows-terminal/own-fonts.json");

    let printed = pc.run_setup();

    assert_same_file(
        &pc.read("windows-terminal/settings.json"),
        &fixture("windows-terminal/own-fonts.after.json"),
    );
    assert_same_file(
        &pc.read("windows-terminal/settings.json.farcli-backup"),
        &fixture("windows-terminal/own-fonts.json"),
    );
    assert!(printed.contains("Fira Code, Vazir Code WOL, Symbols Nerd Font"));
    assert!(printed.contains("\"Notes\""));
    assert!(printed.contains("Consolas, Vazir Code WOL"));
}

#[test]
fn running_setup_again_changes_nothing_that_is_in_place() {
    let pc = Pc::new().with_windows_terminal("windows-terminal/no-font.json");
    pc.run_setup();
    let after_first_run = pc.read("windows-terminal/settings.json");

    let printed = pc.run_setup();

    assert_same_file(&pc.read("windows-terminal/settings.json"), &after_first_run);
    assert!(!pc.has("windows-terminal/settings.json.farcli-backup-2"));
    assert!(printed.contains("already in place"));
    assert!(printed.contains("already installed"));
    assert!(!printed.contains("changed"));
    assert!(!printed.contains("backup"));
}

#[test]
fn the_farsi_font_is_installed_for_the_current_user() {
    let pc = Pc::new();

    let printed = pc.run_setup();

    // A TrueType file, put in the user's fonts folder and on the user's
    // font list under the name Terminals will ask for.
    let font = pc.read("fonts/Vazir-Code-WOL.ttf");
    assert_eq!(font[..4], [0x00, 0x01, 0x00, 0x00]);
    assert!(font.len() > 20_000);
    assert_eq!(
        String::from_utf8(pc.read("installed-fonts.txt")).unwrap(),
        format!(
            "Vazir Code WOL (TrueType)={}\n",
            pc.path("fonts/Vazir-Code-WOL.ttf")
        )
    );
    assert!(printed.contains("installed Vazir Code WOL"));
    assert!(printed.contains(&pc.path("fonts/Vazir-Code-WOL.ttf")));
}

#[test]
fn a_terminal_that_is_not_installed_is_skipped_without_an_error() {
    let pc = Pc::new();

    let report = setup::run(&mut Machine::pretend(&pc.root));

    assert!(!report.failed());
    let printed = report.to_string();
    for terminal in ["Windows Terminal", "WezTerm", "VS Code"] {
        let about = printed.split(terminal).nth(1).unwrap_or_default();
        assert!(
            about.trim_start().starts_with("not installed"),
            "{terminal}: {printed}"
        );
    }
    // Nothing but the font, its list entry and Setup's own mark was created.
    assert_eq!(
        pc.names_in(""),
        ["fonts", "installed-fonts.txt", "setup-done"]
    );
}

#[test]
fn vs_code_gets_the_farsi_font_after_the_font_its_terminal_uses_by_default() {
    let pc = Pc::new().with_vscode("vscode/no-fonts.json");

    let printed = pc.run_setup();

    // With no font set, VS Code's terminal uses the editor's default list,
    // which on Windows starts with Consolas.
    assert_same_file(
        &pc.read("vscode/settings.json"),
        &fixture("vscode/no-fonts.after.json"),
    );
    assert_same_file(
        &pc.read("vscode/settings.json.farcli-backup"),
        &fixture("vscode/no-fonts.json"),
    );
    assert!(printed.contains(&pc.path("vscode/settings.json")));
    assert!(printed.contains("Consolas, 'Vazir Code WOL', 'Courier New', monospace"));
    assert!(printed.contains(&pc.path("vscode/settings.json.farcli-backup")));
}

#[test]
fn a_terminal_font_the_user_chose_in_vs_code_stays_first() {
    let pc = Pc::new().with_vscode("vscode/terminal-font.json");

    pc.run_setup();

    assert_same_file(
        &pc.read("vscode/settings.json"),
        &fixture("vscode/terminal-font.after.json"),
    );
    assert_same_file(
        &pc.read("vscode/settings.json.farcli-backup"),
        &fixture("vscode/terminal-font.json"),
    );
}

#[test]
fn a_vs_code_terminal_that_follows_the_editor_font_keeps_that_font_for_english() {
    let pc = Pc::new().with_vscode("vscode/editor-font.json");

    pc.run_setup();

    // The editor's own setting is not touched.
    assert_same_file(
        &pc.read("vscode/settings.json"),
        &fixture("vscode/editor-font.after.json"),
    );
}

#[test]
fn vs_code_with_no_settings_file_yet_gets_one() {
    let pc = Pc::new().with_vscode_installed();

    let printed = pc.run_setup();

    assert_same_file(
        &pc.read("vscode/settings.json"),
        &fixture("vscode/created.json"),
    );
    // There was nothing to back up.
    assert_eq!(pc.names_in("vscode"), ["settings.json"]);
    assert!(printed.contains(&pc.path("vscode/settings.json")));
}

#[test]
fn each_vs_code_profile_with_settings_of_its_own_gets_the_farsi_font() {
    let pc = Pc::new().with_vscode("vscode/no-fonts.json");
    pc.put(
        "vscode/profiles/1a2b3c4d/settings.json",
        &fixture("vscode/terminal-font.json"),
    );

    pc.run_setup();

    assert_same_file(
        &pc.read("vscode/profiles/1a2b3c4d/settings.json"),
        &fixture("vscode/terminal-font.after.json"),
    );
    assert_same_file(
        &pc.read("vscode/profiles/1a2b3c4d/settings.json.farcli-backup"),
        &fixture("vscode/terminal-font.json"),
    );
}

#[test]
fn wezterm_with_no_configuration_file_yet_gets_one() {
    let pc = Pc::new().with_wezterm_installed();

    let printed = pc.run_setup();

    assert_same_file(
        &pc.read("wezterm/wezterm.lua"),
        &fixture("wezterm/created.lua"),
    );
    assert_eq!(pc.names_in("wezterm"), ["wezterm.lua"]);
    assert!(printed.contains(&pc.path("wezterm/wezterm.lua")));
}

#[test]
fn a_hand_written_wezterm_configuration_is_left_alone_and_the_user_is_told_what_to_add() {
    let pc = Pc::new().with_wezterm("wezterm/hand-written.lua");

    let report = setup::run(&mut Machine::pretend(&pc.root));

    assert_same_file(
        &pc.read("wezterm/wezterm.lua"),
        &fixture("wezterm/hand-written.lua"),
    );
    assert_eq!(pc.names_in("wezterm"), ["wezterm.lua"]);
    let printed = report.to_string();
    assert!(printed.contains("left alone"));
    assert!(printed.contains(&pc.path("wezterm/wezterm.lua")));
    assert!(printed.contains("by hand"));
    assert!(printed.contains("wezterm.font_with_fallback"));
    assert!(printed.contains("'Vazir Code WOL'"));
    // Telling the user what to add is not a failure.
    assert!(!report.failed());
}

#[test]
fn a_wezterm_configuration_that_names_the_farsi_font_needs_nothing() {
    let pc = Pc::new().with_wezterm("wezterm/with-farsi-font.lua");

    let printed = pc.run_setup();

    assert_same_file(
        &pc.read("wezterm/wezterm.lua"),
        &fixture("wezterm/with-farsi-font.lua"),
    );
    assert!(printed.contains("already in place"));
    assert!(!printed.contains("by hand"));
}

#[test]
fn a_settings_file_setup_cannot_change_safely_is_left_alone_and_the_user_is_told_what_to_add() {
    for (place, settings, to_add) in [
        (
            "windows-terminal/settings.json",
            "windows-terminal/old-font-setting.json",
            "Cascadia Mono, Vazir Code WOL",
        ),
        (
            "windows-terminal/settings.json",
            "windows-terminal/broken.json",
            "Cascadia Mono, Vazir Code WOL",
        ),
        (
            "vscode/settings.json",
            "vscode/font-set-twice.json",
            "\"terminal.integrated.fontFamily\": \"Consolas, 'Vazir Code WOL', 'Courier New', monospace\"",
        ),
    ] {
        let pc = Pc::new();
        pc.put(place, &fixture(settings));

        let report = setup::run(&mut Machine::pretend(&pc.root));

        assert_same_file(&pc.read(place), &fixture(settings));
        assert!(!pc.has(&format!("{place}.farcli-backup")), "{settings}");
        let printed = report.to_string();
        assert!(printed.contains("left alone"), "{settings}: {printed}");
        assert!(printed.contains(&pc.path(place)), "{settings}: {printed}");
        assert!(printed.contains("by hand"), "{settings}: {printed}");
        assert!(printed.contains(to_add), "{settings}: {printed}");
        assert!(!report.failed());
    }
}

#[test]
fn the_farsi_font_is_added_to_settings_written_in_other_shapes() {
    let vs_code = "vscode/settings.json";
    let windows_terminal = "windows-terminal/settings.json";
    let terminal_fonts = r#""terminal.integrated.fontFamily": "Consolas, 'Vazir Code WOL', 'Courier New', monospace""#;
    for (place, before, after) in [
        // An empty settings file.
        (vs_code, "{}\n", format!("{{\n    {terminal_fonts}\n}}\n")),
        // Everything on one line.
        (
            vs_code,
            "{ \"editor.tabSize\": 2 }",
            format!("{{ {terminal_fonts}, \"editor.tabSize\": 2 }}"),
        ),
        // Nothing but a comment.
        (
            vs_code,
            "{\n    // nothing yet\n}\n",
            format!("{{\n    {terminal_fonts}\n    // nothing yet\n}}\n"),
        ),
        // A terminal font set to nothing, which VS Code reads as not set.
        (
            vs_code,
            "{\n  \"terminal.integrated.fontFamily\": \"\"\n}\n",
            format!("{{\n  {terminal_fonts}\n}}\n"),
        ),
        // Windows Terminal with no defaults for its profiles.
        (
            windows_terminal,
            "{\n  \"profiles\": {\n    \"list\": []\n  }\n}\n",
            concat!(
                "{\n  \"profiles\": {\n",
                "    \"defaults\": {\n",
                "      \"font\": {\n",
                "        \"face\": \"Cascadia Mono, Vazir Code WOL\"\n",
                "      }\n",
                "    },\n",
                "    \"list\": []\n  }\n}\n"
            )
            .to_string(),
        ),
        // A font size but no font face, on one line.
        (
            windows_terminal,
            "{ \"profiles\": { \"defaults\": { \"font\": { \"size\": 11 } }, \"list\": [] } }",
            concat!(
                "{ \"profiles\": { \"defaults\": { \"font\": { ",
                "\"face\": \"Cascadia Mono, Vazir Code WOL\", ",
                "\"size\": 11 } }, \"list\": [] } }"
            )
            .to_string(),
        ),
        // A font face written with an escape, and Farsi in a name.
        (
            windows_terminal,
            "{\"profiles\":{\"defaults\":{\"font\":{\"face\":\"Fira\\u0020Code\"}},\"list\":[{\"name\":\"پوسته\"}]}}",
            "{\"profiles\":{\"defaults\":{\"font\":{\"face\":\"Fira Code, Vazir Code WOL\"}},\"list\":[{\"name\":\"پوسته\"}]}}"
                .to_string(),
        ),
    ] {
        let pc = Pc::new();
        pc.put(place, before.as_bytes());

        pc.run_setup();

        assert_same_file(&pc.read(place), after.as_bytes());
        assert_same_file(
            &pc.read(&format!("{place}.farcli-backup")),
            before.as_bytes(),
        );
    }
}

#[test]
fn line_endings_and_a_byte_order_mark_stay_as_they_were() {
    let as_windows_saves = |settings: Vec<u8>| {
        let text = String::from_utf8(settings).unwrap().replace('\n', "\r\n");
        format!("\u{feff}{text}").into_bytes()
    };
    let pc = Pc::new();
    pc.put(
        "windows-terminal/settings.json",
        &as_windows_saves(fixture("windows-terminal/no-font.json")),
    );
    pc.put(
        "vscode/settings.json",
        &as_windows_saves(fixture("vscode/no-fonts.json")),
    );

    pc.run_setup();

    assert_same_file(
        &pc.read("windows-terminal/settings.json"),
        &as_windows_saves(fixture("windows-terminal/no-font.after.json")),
    );
    assert_same_file(
        &pc.read("vscode/settings.json"),
        &as_windows_saves(fixture("vscode/no-fonts.after.json")),
    );
}

#[test]
fn an_earlier_backup_is_never_written_over() {
    let pc = Pc::new().with_windows_terminal("windows-terminal/no-font.json");
    pc.run_setup();
    // Later the user goes back to settings without the Farsi font.
    pc.put(
        "windows-terminal/settings.json",
        &fixture("windows-terminal/own-fonts.json"),
    );

    let printed = pc.run_setup();

    assert_same_file(
        &pc.read("windows-terminal/settings.json.farcli-backup"),
        &fixture("windows-terminal/no-font.json"),
    );
    assert_same_file(
        &pc.read("windows-terminal/settings.json.farcli-backup-2"),
        &fixture("windows-terminal/own-fonts.json"),
    );
    assert!(printed.contains(&pc.path("windows-terminal/settings.json.farcli-backup-2")));
}

#[test]
fn setup_runs_by_itself_only_the_first_time() {
    let pc = Pc::new().with_windows_terminal("windows-terminal/no-font.json");

    let first = setup::run_on_first_use(&mut Machine::pretend(&pc.root));
    let second = setup::run_on_first_use(&mut Machine::pretend(&pc.root));

    let printed = first.expect("Setup runs on first use").to_string();
    assert!(printed.contains("Cascadia Mono, Vazir Code WOL"));
    assert_same_file(
        &pc.read("windows-terminal/settings.json"),
        &fixture("windows-terminal/no-font.after.json"),
    );
    assert!(second.is_none());
}

#[test]
fn setup_run_on_demand_counts_as_the_first_time() {
    let pc = Pc::new();
    pc.run_setup();

    assert!(setup::run_on_first_use(&mut Machine::pretend(&pc.root)).is_none());
}

#[test]
fn setup_runs_again_on_demand_for_a_terminal_installed_later() {
    let pc = Pc::new();
    pc.run_setup();
    let pc = pc.with_vscode("vscode/no-fonts.json");

    pc.run_setup();

    assert_same_file(
        &pc.read("vscode/settings.json"),
        &fixture("vscode/no-fonts.after.json"),
    );
}

#[test]
fn a_settings_file_that_cannot_be_written_is_reported_and_setup_runs_again_next_time() {
    let pc = Pc::new().with_windows_terminal("windows-terminal/no-font.json");
    let settings = pc.file("windows-terminal/settings.json");
    let mut permissions = fs::metadata(&settings).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&settings, permissions.clone()).unwrap();

    let report = setup::run_on_first_use(&mut Machine::pretend(&pc.root)).unwrap();

    assert!(report.failed());
    assert!(report.to_string().contains("could not change"));
    assert!(report
        .to_string()
        .contains(&pc.path("windows-terminal/settings.json")));
    assert_eq!(pc.names_in("windows-terminal"), ["settings.json"]);

    // Once the file can be written, the next first use finishes the job.
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&settings, permissions).unwrap();
    let report = setup::run_on_first_use(&mut Machine::pretend(&pc.root)).unwrap();
    assert!(!report.failed());
    assert_same_file(
        &pc.read("windows-terminal/settings.json"),
        &fixture("windows-terminal/no-font.after.json"),
    );
}
