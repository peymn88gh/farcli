//! When farcli's output is not a Terminal it steps aside: the Target app runs
//! with no correction and its output arrives byte for byte.
//!
//! These tests run the real `farcli` program with its output piped, the way
//! a script would.

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

fn farcli() -> Command {
    let mut farcli = Command::new(env!("CARGO_BIN_EXE_farcli"));
    // Setup must not run when farcli is piped. Should that ever break, it
    // would run on this folder and not on the real settings of whoever runs
    // the tests.
    farcli.env(
        "FARCLI_PRETEND_MACHINE",
        std::env::temp_dir().join("farcli-piped-test-machine"),
    );
    farcli
}

#[test]
fn piped_output_arrives_byte_for_byte() {
    // Colour codes, Farsi, a zero byte and bytes that are not text at all:
    // none of it may be touched.
    let mut written = b"\x1b[1;31mred\x1b[m \x1b]0;title\x07".to_vec();
    written.extend("سلام می\u{200c}خواهم".as_bytes());
    written.extend([0x00, 0xff, 0xfe, b'\r', b'\n', b'\n']);
    let folder = std::env::temp_dir();
    let name = format!("farcli-piped-test-{}", std::process::id());
    fs::write(folder.join(&name), &written).unwrap();

    let mut farcli = farcli();
    if cfg!(windows) {
        farcli.args(["cmd", "/d", "/c", "type", &name]);
    } else {
        farcli.args(["cat", &name]);
    }
    let output = farcli.current_dir(&folder).output().unwrap();
    fs::remove_file(folder.join(&name)).unwrap();

    assert_eq!(output.stdout, written);
    assert!(output.status.success());
}

#[test]
fn piped_run_returns_the_apps_exit_status() {
    let mut farcli = farcli();
    if cfg!(windows) {
        farcli.args(["cmd", "/d", "/c", "exit 7"]);
    } else {
        farcli.args(["sh", "-c", "exit 7"]);
    }

    let output = farcli.output().unwrap();

    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"");
}

#[test]
fn piped_input_reaches_the_app() {
    let mut farcli = farcli()
        .arg("sort")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    farcli
        .stdin
        .take()
        .unwrap()
        .write_all(b"second\r\nfirst\r\n")
        .unwrap();
    let output = farcli.wait_with_output().unwrap();

    assert_eq!(output.stdout, b"first\r\nsecond\r\n");
}
