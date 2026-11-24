//! The real `farcli` says in one line at start that the Target app is newer
//! than the version farcli was tested with, says nothing when it is not,
//! and nothing when it steps aside for a pipe.
//!
//! Each test puts a stand-in `claude` or `codex` first on the path: a batch
//! file that tells a version when asked and otherwise draws a little. The
//! tests that need a Terminal run `farcli` on a hidden terminal with nobody
//! at the keyboard and read what that Terminal would show.
#![cfg(windows)]

mod in_a_terminal;

use std::env;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use in_a_terminal::{pretend_machine, Folder, InATerminal, KEY_A, KEY_ENTER};

/// A stand-in Target app that says it is `version` when asked, and
/// otherwise prints one line and ends with status 3.
fn app_that_says(version: &str) -> String {
    format!(
        "@echo off\r\n\
         if \"%1\"==\"--version\" (\r\n\
         \x20 echo {version}\r\n\
         \x20 exit /b 0\r\n\
         )\r\n\
         echo the app ran\r\n\
         exit /b 3\r\n"
    )
}

fn line_for(app: &str, version: &str, tested: &str) -> String {
    format!(
        "farcli: {app} {version} is newer than the tested {tested}. \
         F12 switches the correction off and on."
    )
}

#[test]
fn farcli_says_one_line_before_an_app_that_is_newer_than_the_tested_version() {
    let folder = Folder::new("newer");
    folder.put("claude.cmd", &app_that_says("2.1.400 ^(Claude Code^)"));
    folder.put("codex.cmd", &app_that_says("codex-cli 0.172.0"));

    let (claude_code, claude_code_status, _) = InATerminal::run(&folder, &["claude"]).end();
    let (codex, codex_status, _) = InATerminal::run(&folder, &["codex"]).end();

    // The line, and under it what the app drew. Nothing else.
    assert_eq!(
        claude_code,
        [
            line_for("Claude Code", "2.1.400", "2.1.322").as_str(),
            "the app ran"
        ]
    );
    assert_eq!(
        codex,
        [
            line_for("Codex", "0.172.0", "0.171.0").as_str(),
            "the app ran"
        ]
    );
    assert_eq!((claude_code_status, codex_status), (3, 3));
}

#[test]
fn farcli_says_nothing_before_an_app_of_the_tested_version_or_an_older_one() {
    let folder = Folder::new("not-newer");
    folder.put("claude.cmd", &app_that_says("2.1.322 ^(Claude Code^)"));
    folder.put("codex.cmd", &app_that_says("codex-cli 0.99.7"));

    let (claude_code, _, _) = InATerminal::run(&folder, &["claude"]).end();
    let (codex, _, _) = InATerminal::run(&folder, &["codex"]).end();

    assert_eq!(claude_code, ["the app ran"]);
    assert_eq!(codex, ["the app ran"]);
}

#[test]
fn farcli_says_nothing_when_it_steps_aside_for_a_pipe() {
    let folder = Folder::new("piped");
    folder.put("codex.cmd", &app_that_says("codex-cli 0.172.0"));

    let output = Command::new(env!("CARGO_BIN_EXE_farcli"))
        .arg("codex")
        .env("PATH", folder.first_on_the_path())
        .env("FARCLI_PRETEND_MACHINE", pretend_machine())
        .output()
        .unwrap();

    // What the app wrote, byte for byte, and not a word of farcli's own.
    assert_eq!(output.stdout, b"the app ran\r\n");
    assert_eq!(output.stderr, b"");
    assert_eq!(output.status.code(), Some(3));
}

/// A stand-in app that takes the alternate screen as Codex does, waits for
/// a line to be typed, and leaves.
const APP_ON_THE_ALTERNATE_SCREEN: &str = "@echo off\r\n\
     if \"%1\"==\"--version\" (\r\n\
     \x20 echo codex-cli 0.172.0\r\n\
     \x20 exit /b 0\r\n\
     )\r\n\
     echo \x1b[?1049h\x1b[2J\x1b[2;1H  the app's own screen\r\n\
     set /p said=\x1b[4;1Hsay: \r\n\
     echo \x1b[?1049lthe app left\r\n";

#[test]
fn the_line_is_over_the_top_row_of_an_alternate_screen_until_a_key_is_pressed() {
    let folder = Folder::new("alternate");
    folder.put("codex.cmd", APP_ON_THE_ALTERNATE_SCREEN);
    let line = line_for("Codex", "0.172.0", "0.171.0");
    let mut farcli = InATerminal::run(&folder, &["codex"]);

    farcli.wait_until("the app's screen", |screen| screen[3].starts_with("say:"));
    // The line hides nothing but the top row, which this app left empty.
    assert_eq!(
        farcli.screen()[..4],
        [line.as_str(), "  the app's own screen", "", "say:"]
    );

    farcli.user_types(KEY_A);
    farcli.wait_until("the key the user pressed", |screen| screen[3] == "say: a");
    assert_eq!(
        farcli.screen()[..4],
        ["", "  the app's own screen", "", "say: a"]
    );

    farcli.user_types(KEY_ENTER);
    let (shown, _, _) = farcli.end();
    // Back on the main screen, the line is where it was written at start.
    assert_eq!(shown, [line.as_str(), "the app left"]);
}

/// The file npm writes to start a package's program on Windows, as it is
/// for Codex. It runs `node` with the package's script.
const NPM_SHIM: &str = "@ECHO off\r\n\
     GOTO start\r\n\
     :find_dp0\r\n\
     SET dp0=%~dp0\r\n\
     EXIT /b\r\n\
     :start\r\n\
     SETLOCAL\r\n\
     CALL :find_dp0\r\n\
     \r\n\
     IF EXIST \"%dp0%\\node.exe\" (\r\n\
     \x20 SET \"_prog=%dp0%\\node.exe\"\r\n\
     ) ELSE (\r\n\
     \x20 SET \"_prog=node\"\r\n\
     \x20 SET PATHEXT=%PATHEXT:;.JS;=;%\r\n\
     )\r\n\
     \r\n\
     endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n";

/// A stand-in for `node` that takes thirty seconds to tell a version, far
/// longer than the real one, and otherwise prints one line.
const SLOW_NODE: &str = "@echo off\r\n\
     if \"%2\"==\"--version\" (\r\n\
     \x20 ping -n 31 127.0.0.1 >nul\r\n\
     \x20 echo codex-cli 0.172.0\r\n\
     \x20 exit /b 0\r\n\
     )\r\n\
     echo the app ran\r\n";

#[test]
fn the_version_of_an_app_that_npm_installed_is_read_from_its_package_without_running_it() {
    let folder = Folder::new("npm");
    folder
        .put("codex.cmd", NPM_SHIM)
        .put("node.cmd", SLOW_NODE)
        .put("node_modules/@openai/codex/bin/codex.js", "")
        .put(
            "node_modules/@openai/codex/package.json",
            "{\n  \"name\": \"@openai/codex\",\n  \"version\": \"0.999.0\",\n  \"bin\": {}\n}\n",
        );

    let (shown, _, took) = InATerminal::run(&folder, &["codex"]).end();

    assert_eq!(
        shown,
        [
            line_for("Codex", "0.999.0", "0.171.0").as_str(),
            "the app ran"
        ]
    );
    assert!(took < Duration::from_secs(20), "{took:?}");
}

/// A stand-in app that never answers when asked for its version.
const APP_THAT_DOES_NOT_ANSWER: &str = "@echo off\r\n\
     if \"%1\"==\"--version\" (\r\n\
     \x20 ping -n 31 127.0.0.1 >nul\r\n\
     \x20 echo 2.1.400 ^(Claude Code^)\r\n\
     \x20 exit /b 0\r\n\
     )\r\n\
     echo the app ran\r\n";

#[test]
fn an_app_that_does_not_tell_its_version_soon_is_started_anyway_with_no_line() {
    let folder = Folder::new("silent");
    folder.put("claude.cmd", APP_THAT_DOES_NOT_ANSWER);

    let (shown, _, took) = InATerminal::run(&folder, &["claude"]).end();

    assert_eq!(shown, ["the app ran"]);
    assert!(took < Duration::from_secs(20), "{took:?}");
}

/// The stand-in apps are found by farcli the way it finds the real ones.
#[test]
fn the_stand_in_apps_are_first_on_the_path_farcli_is_given() {
    let folder = Folder::new("path");
    let first = env::split_paths(&folder.first_on_the_path()).next();
    assert_eq!(first.as_deref(), Some(Path::new(&folder.0)));
}
