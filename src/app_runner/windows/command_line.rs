//! Finding a Target app's program file and building the Windows command line
//! that hands it its arguments unchanged.
//!
//! Windows passes a program one string, not a list of arguments, so every
//! argument has to be quoted the way the receiving program will un-quote it.
//! Programs and batch files (`.cmd` shims such as the one npm installs for
//! Codex) follow different rules. The rules here are the ones Rust's standard
//! library uses.

use std::env;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

/// What `CreateProcessW` needs to start a Target app.
pub struct StartLine {
    /// The program file to run.
    pub program: PathBuf,
    /// The whole command line, without the ending zero.
    pub command_line: Vec<u16>,
}

/// Finds `program` the way a shell would and builds its command line.
pub fn start_line(program: &OsStr, args: &[OsString]) -> io::Result<StartLine> {
    let file = find_program(program)?;
    if is_batch_file(&file) {
        let command_line = batch_command_line(&file, args)?;
        Ok(StartLine {
            program: command_prompt(),
            command_line,
        })
    } else {
        let mut command_line = Vec::new();
        append_program_arg(&mut command_line, file.as_os_str(), true)?;
        for arg in args {
            command_line.push(' ' as u16);
            append_program_arg(&mut command_line, arg, false)?;
        }
        Ok(StartLine {
            program: file,
            command_line,
        })
    }
}

/// Finds the file a shell would run for `program`: the name as given, or the
/// name with one of the `PATHEXT` endings, looked up along `PATH`.
pub fn find_program(program: &OsStr) -> io::Result<PathBuf> {
    let name = Path::new(program);
    let endings = program_endings();
    let has_folder = name.components().count() > 1;
    let found = if has_folder {
        with_program_ending(name, &endings)
    } else {
        env::var_os("PATH").and_then(|path| {
            env::split_paths(&path)
                .filter(|folder| !folder.as_os_str().is_empty())
                .find_map(|folder| with_program_ending(&folder.join(name), &endings))
        })
    };
    found.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("program not found: {}", name.display()),
        )
    })
}

/// True when `file` is run by the command prompt instead of by itself.
pub fn is_batch_file(file: &Path) -> bool {
    file.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ending| {
            ending.eq_ignore_ascii_case("cmd") || ending.eq_ignore_ascii_case("bat")
        })
}

fn program_endings() -> Vec<OsString> {
    let listed = env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
    let wide: Vec<u16> = listed.encode_wide().collect();
    wide.split(|&unit| unit == ';' as u16)
        .filter(|ending| !ending.is_empty())
        .map(OsString::from_wide)
        .collect()
}

fn with_program_ending(candidate: &Path, endings: &[OsString]) -> Option<PathBuf> {
    let known_ending = candidate.extension().is_some_and(|ending| {
        endings.iter().any(|known| {
            let known = known.to_string_lossy();
            known
                .trim_start_matches('.')
                .eq_ignore_ascii_case(&ending.to_string_lossy())
        })
    });
    if known_ending && candidate.is_file() {
        return Some(candidate.to_path_buf());
    }
    endings.iter().find_map(|ending| {
        let mut with_ending = candidate.as_os_str().to_os_string();
        with_ending.push(ending);
        let with_ending = PathBuf::from(with_ending);
        with_ending.is_file().then_some(with_ending)
    })
}

fn command_prompt() -> PathBuf {
    let system = env::var_os("SystemRoot").unwrap_or_else(|| OsString::from(r"C:\Windows"));
    Path::new(&system).join("System32").join("cmd.exe")
}

fn refuse_zero(arg: &OsStr) -> io::Result<()> {
    if arg.encode_wide().any(|unit| unit == 0) {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an argument contains a zero character",
        ))
    } else {
        Ok(())
    }
}

/// Quotes one argument for a program that splits its command line by the
/// usual Windows rules.
fn append_program_arg(line: &mut Vec<u16>, arg: &OsStr, force_quotes: bool) -> io::Result<()> {
    refuse_zero(arg)?;
    let quote = force_quotes
        || arg.is_empty()
        || arg
            .encode_wide()
            .any(|unit| unit == ' ' as u16 || unit == '\t' as u16);
    if quote {
        line.push('"' as u16);
    }
    let mut backslashes = 0;
    for unit in arg.encode_wide() {
        if unit == '\\' as u16 {
            backslashes += 1;
        } else {
            if unit == '"' as u16 {
                // Backslashes before a quote are doubled, then the quote is escaped.
                line.extend(std::iter::repeat_n('\\' as u16, backslashes + 1));
            }
            backslashes = 0;
        }
        line.push(unit);
    }
    if quote {
        // Backslashes before the closing quote are doubled.
        line.extend(std::iter::repeat_n('\\' as u16, backslashes));
        line.push('"' as u16);
    }
    Ok(())
}

/// Builds the command line that makes the command prompt run a batch file
/// with `args` reaching it unchanged.
fn batch_command_line(script: &Path, args: &[OsString]) -> io::Result<Vec<u16>> {
    refuse_zero(script.as_os_str())?;
    let script_units: Vec<u16> = script.as_os_str().encode_wide().collect();
    if script_units.contains(&('"' as u16)) || script_units.last() == Some(&('\\' as u16)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the batch file's path cannot be quoted safely",
        ));
    }
    let mut line: Vec<u16> = "cmd.exe /e:ON /v:OFF /d /c \"".encode_utf16().collect();
    line.push('"' as u16);
    line.extend(script_units);
    line.push('"' as u16);
    for arg in args {
        line.push(' ' as u16);
        append_batch_arg(&mut line, arg)?;
    }
    line.push('"' as u16);
    Ok(line)
}

/// Quotes one argument so the command prompt neither splits it nor expands
/// anything inside it.
fn append_batch_arg(line: &mut Vec<u16>, arg: &OsStr) -> io::Result<()> {
    refuse_zero(arg)?;
    const SAFE_UNQUOTED: &str = r"#$*+-./:?@\_";
    let units: Vec<u16> = arg.encode_wide().collect();
    if units
        .iter()
        .any(|&unit| unit == '\r' as u16 || unit == '\n' as u16)
    {
        // The command prompt would cut the command line at a line break.
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an argument for a batch file contains a line break",
        ));
    }
    let mut quote = units.is_empty() || units.last() == Some(&('\\' as u16));
    for decoded in char::decode_utf16(units.iter().copied()) {
        let Ok(character) = decoded else { continue };
        let ascii_needs_quotes = character.is_ascii()
            && !(character.is_ascii_alphanumeric() || SAFE_UNQUOTED.contains(character));
        if ascii_needs_quotes || character.is_control() {
            quote = true;
        }
    }
    if quote {
        line.push('"' as u16);
    }
    let mut backslashes = 0;
    for unit in units {
        if unit == '\\' as u16 {
            backslashes += 1;
        } else {
            if unit == '"' as u16 {
                line.extend(std::iter::repeat_n('\\' as u16, backslashes));
                // A doubled quote is the command prompt's escaped quote.
                line.push('"' as u16);
            } else if unit == '%' as u16 {
                // `%cd:~,%` expands to nothing, and stops the command prompt
                // from treating the `%` that follows as the start of a
                // variable name.
                line.extend("%%cd:~,".encode_utf16());
            }
            backslashes = 0;
        }
        line.push(unit);
    }
    if quote {
        line.extend(std::iter::repeat_n('\\' as u16, backslashes));
        line.push('"' as u16);
    }
    Ok(())
}
