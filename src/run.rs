//! Runs one Target app from start to exit: joins the real Terminal, the App
//! runner and the session.

use std::ffi::{OsStr, OsString};
use std::io;

use crate::app_profile::AppProfile;
use crate::app_runner;

#[cfg(windows)]
mod tape;

/// Runs `program` with `args` and returns its exit status.
///
/// In a Terminal the app runs on a hidden terminal and farcli repaints the
/// Terminal from the Hidden screen. When farcli's input or output is not a
/// Terminal (piped or scripted), farcli steps aside: the app reads and
/// writes whatever farcli was given, untouched.
pub fn run(profile: AppProfile, program: &OsStr, args: &[OsString]) -> io::Result<i32> {
    #[cfg(windows)]
    if let Some(terminal) = crate::terminal::Terminal::open() {
        return in_terminal::run(terminal, profile, program, args);
    }
    // The hidden terminal exists only for Windows so far. Everywhere else
    // the app runs with no correction.
    let _ = profile;
    app_runner::run_directly(program, args)
}

#[cfg(windows)]
mod in_terminal {
    use std::ffi::{OsStr, OsString};
    use std::io::{self, Read, Write};
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use crate::app_profile::AppProfile;
    use crate::app_runner::{Host, RunningApp};
    use crate::recording::Event as Recorded;
    use crate::session::{Reaction, Session};
    use crate::terminal::{Terminal, TerminalEvent};

    use super::tape::Tape;

    /// How long a frame the Target app marked as unfinished is kept from the
    /// Terminal before it is shown anyway.
    const HELD_FRAME_LIMIT: Duration = Duration::from_millis(150);

    enum Event {
        AppWrote(Vec<u8>),
        AppOutputEnded,
        AppEnded(io::Result<i32>),
        Terminal(TerminalEvent),
        /// The Terminal can no longer be read: its window was closed.
        TerminalGone,
    }

    pub fn run(
        terminal: Terminal,
        profile: AppProfile,
        program: &OsStr,
        args: &[OsString],
    ) -> io::Result<i32> {
        // Said before the cursor is read, so that the app starts below it.
        let host = Host::ready();
        if let Some(why) = host.why_not_shipped() {
            eprintln!(
                "farcli: using the hidden terminal built into Windows, which can damage \
                 rows of Farsi that hold a Half-space or a vowel mark. Microsoft's newer \
                 one could not be made ready: {why}"
            );
        }
        let hidden_terminal = host.hidden_terminal();
        let size = terminal.size()?;
        let cursor = terminal.cursor()?;
        let mut tape = Tape::start(&profile, program, args, size, cursor, hidden_terminal)?;
        let mut session = Session::start_on(hidden_terminal, profile, size, cursor);
        let mut app = RunningApp::start(program, args, size)?;
        let mut app_output = app.take_output();
        let mut app_input = app.take_input();
        let app = Arc::new(app);
        let restore = terminal.take_over()?;

        let (events, happened) = mpsc::channel();
        {
            let events = events.clone();
            thread::spawn(move || {
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    match app_output.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => {
                            if events
                                .send(Event::AppWrote(buffer[..count].to_vec()))
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                }
                let _ = events.send(Event::AppOutputEnded);
            });
        }
        {
            let events = events.clone();
            let app = Arc::clone(&app);
            thread::spawn(move || {
                let _ = events.send(Event::AppEnded(app.wait()));
            });
        }
        thread::spawn(move || {
            let _ = terminal.watch(|event| events.send(Event::Terminal(event)).is_ok());
            let _ = events.send(Event::TerminalGone);
        });

        let mut exit_status = None;
        let mut output_ended = false;
        let mut react = |reaction: Reaction| -> io::Result<()> {
            if !reaction.for_terminal.is_empty() {
                terminal.write(&reaction.for_terminal)?;
            }
            if !reaction.for_app.is_empty() {
                // The app may have ended and taken its input with it.
                let _ = app_input.write_all(&reaction.for_app);
            }
            Ok(())
        };
        // The run is over once the app has ended and everything it drew has
        // reached the Terminal. The two are reported in either order.
        while !(output_ended && exit_status.is_some()) {
            let event = if session.holding_frame() {
                match happened.recv_timeout(HELD_FRAME_LIMIT) {
                    Ok(event) => event,
                    Err(RecvTimeoutError::Timeout) => {
                        tape.add(|| Recorded::HeldFrameReleased);
                        react(session.release_held_frame())?;
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match happened.recv() {
                    Ok(event) => event,
                    Err(_) => break,
                }
            };
            match event {
                Event::AppWrote(bytes) => {
                    tape.add(|| Recorded::AppWrote(bytes.clone()));
                    react(session.app_wrote(&bytes))?
                }
                Event::Terminal(TerminalEvent::Sent(bytes)) => {
                    tape.add(|| Recorded::UserTyped(bytes.clone()));
                    react(session.user_typed(&bytes))?
                }
                Event::Terminal(TerminalEvent::Resized(size)) => {
                    tape.add(|| Recorded::TerminalResized(size));
                    app.resize(size)?;
                    react(session.terminal_resized(size))?;
                }
                // Leaving ends the hidden terminal and the app with it.
                Event::TerminalGone => {
                    return Err(io::Error::other("the Terminal went away"));
                }
                Event::AppEnded(status) => exit_status = Some(status),
                Event::AppOutputEnded => output_ended = true,
            }
        }
        if session.holding_frame() {
            react(session.release_held_frame())?;
        }
        drop(restore);
        exit_status
            .unwrap_or_else(|| Err(io::Error::other("the Target app's exit status is unknown")))
    }
}
