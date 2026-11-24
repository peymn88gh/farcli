//! farcli carries Microsoft's hidden terminal inside its program and
//! unpacks it into a folder before use. What lies in that folder is used
//! only if it is exactly what farcli carries.
#![cfg(all(windows, target_arch = "x86_64"))]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use farcli::app_runner::ShippedFiles;

/// A folder of this test's own, not yet made.
fn fresh_folder(name: &str) -> PathBuf {
    let folder = env::temp_dir().join(format!("farcli-shipped-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    folder.join("hidden-terminal").join("version-x64")
}

fn remove(folder: &Path) {
    fs::remove_dir_all(folder.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn the_two_files_are_unpacked_with_microsofts_licence_beside_them() {
    let folder = fresh_folder("unpack");

    let files = ShippedFiles::ready_in(&folder).unwrap();

    assert_eq!(files.folder(), folder);
    // The sizes of the two files in Microsoft's package 1.25.261109001.
    assert_eq!(
        fs::metadata(folder.join("conpty.dll")).unwrap().len(),
        119_608
    );
    assert_eq!(
        fs::metadata(folder.join("OpenConsole.exe")).unwrap().len(),
        1_089_336
    );
    let licence = fs::read_to_string(folder.join("LICENSE.txt")).unwrap();
    assert!(licence.contains("Copyright (c) Microsoft Corporation"));
    assert!(licence.contains("MIT License"));
    let notice = fs::read_to_string(folder.join("NOTICE.txt")).unwrap();
    assert!(notice.contains("not covered by farcli's MIT"));
    // Nothing is left over from the unpacking, here or one folder up.
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 4);
    assert_eq!(fs::read_dir(folder.parent().unwrap()).unwrap().count(), 1);
    drop(files);
    remove(&folder);
}

#[test]
fn files_already_unpacked_are_used_as_they_are() {
    let folder = fresh_folder("again");
    drop(ShippedFiles::ready_in(&folder).unwrap());
    let written = |name: &str| fs::metadata(folder.join(name)).unwrap().modified().unwrap();
    let first = (written("conpty.dll"), written("OpenConsole.exe"));
    std::thread::sleep(std::time::Duration::from_millis(50));

    drop(ShippedFiles::ready_in(&folder).unwrap());

    assert_eq!((written("conpty.dll"), written("OpenConsole.exe")), first);
    remove(&folder);
}

#[test]
fn a_file_that_was_changed_is_put_back_before_use() {
    let folder = fresh_folder("changed");
    drop(ShippedFiles::ready_in(&folder).unwrap());
    fs::write(folder.join("conpty.dll"), b"not Microsoft's library").unwrap();
    fs::remove_file(folder.join("OpenConsole.exe")).unwrap();

    let files = ShippedFiles::ready_in(&folder).unwrap();

    assert_eq!(
        fs::metadata(folder.join("conpty.dll")).unwrap().len(),
        119_608
    );
    assert_eq!(
        fs::metadata(folder.join("OpenConsole.exe")).unwrap().len(),
        1_089_336
    );
    drop(files);
    remove(&folder);
}

#[test]
fn a_file_farcli_did_not_put_there_is_refused() {
    let folder = fresh_folder("stranger");
    drop(ShippedFiles::ready_in(&folder).unwrap());
    // The hidden terminal would load a library of this name from its own
    // folder.
    fs::write(folder.join("icu.dll"), b"someone else's").unwrap();

    let refused = ShippedFiles::ready_in(&folder).err().unwrap();

    assert!(refused.to_string().contains("icu.dll"), "{refused}");
    remove(&folder);
}

#[test]
fn the_files_cannot_be_changed_while_they_are_in_use() {
    let folder = fresh_folder("held");

    let files = ShippedFiles::ready_in(&folder).unwrap();

    for name in ["conpty.dll", "OpenConsole.exe"] {
        assert!(fs::write(folder.join(name), b"changed").is_err(), "{name}");
        assert!(fs::remove_file(folder.join(name)).is_err(), "{name}");
        assert!(
            fs::rename(folder.join(name), folder.join("moved")).is_err(),
            "{name}"
        );
    }
    drop(files);
    remove(&folder);
}

#[test]
fn a_folder_that_cannot_be_made_is_an_error_and_not_a_crash() {
    let folder = fresh_folder("blocked");
    // A file where the folder should be.
    fs::create_dir_all(folder.parent().unwrap()).unwrap();
    fs::write(&folder, b"in the way").unwrap();

    assert!(ShippedFiles::ready_in(&folder).is_err());
    remove(&folder);
}
