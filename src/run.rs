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
    use std::time::{Duration, Instant};

    use crate::app_profile::AppProfile;
    use crate::app_runner::{Host, RunningApp};
    use crate::recording::Event as Recorded;
    use crate::session::{Reaction, Session};
    use crate::terminal::{Terminal, TerminalEvent};

    use super::tape::Tape;

    /// How long a frame the Target app marked as unfinished is kept from the
    /// Terminal before it is shown anyway.
    const HELD_FRAME_LIMIT: Duration = Duration::from_millis(150);

    /// How long the start of a code the Terminal sent is kept from the
    /// Target app while the rest of it has not come. The rest of a code
    /// that Windows cut in two is already waiting to be read; only an
    /// Escape key from a Terminal that sends keys in no other form waits
    /// this long.
    const HELD_INPUT_LIMIT: Duration = Duration::from_millis(100);

    /// How long after the last of what the Terminal sent was forwarded the
    /// Target app is woken, on a hidden terminal that can leave an app
    /// waiting for input it already has. An app left so went to sleep
    /// within a fraction of a millisecond of that input arriving. This is
    /// longer than the pieces of one paste are apart, so that a paste is
    /// followed by one wake, and short enough that the end of a paste
    /// arriving this late passes unnoticed.
    const WAKE_AFTER: Duration = Duration::from_millis(20);

    /// When to wake the Target app, given what was just forwarded to it
    /// and when it was to be woken before that.
    fn wake_time(
        session: &Session,
        forwarded: &Reaction,
        before: Option<Instant>,
    ) -> Option<Instant> {
        if session.app_can_be_left_asleep_on_input() && !forwarded.for_app.is_empty() {
            Some(Instant::now() + WAKE_AFTER)
        } else {
            before
        }
    }

    /// What farcli does of its own accord when nothing has happened for
    /// long enough.
    enum Due {
        HeldInput,
        Wake,
        HeldFrame,
    }

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
        // When the app is to be woken, if input was forwarded to a hidden
        // terminal that can leave it asleep and it was not woken since.
        let mut wake_at: Option<Instant> = None;
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
            // Whichever of these comes first, if nothing happens before it.
            // Held input and a held frame are counted from now, a wake
            // from the last input, whatever the app draws meanwhile.
            let now = Instant::now();
            let due = [
                (session.holding_input()).then_some((HELD_INPUT_LIMIT, Due::HeldInput)),
                wake_at.map(|at| (at.saturating_duration_since(now), Due::Wake)),
                (session.holding_frame()).then_some((HELD_FRAME_LIMIT, Due::HeldFrame)),
            ];
            let first_due = due.into_iter().flatten().min_by_key(|(wait, _)| *wait);
            let event = match first_due {
                None => match happened.recv() {
                    Ok(event) => event,
                    Err(_) => break,
                },
                Some((wait, due)) => match happened.recv_timeout(wait) {
                    Ok(event) => event,
                    Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {
                        match due {
                            Due::HeldInput => {
                                tape.add(|| Recorded::HeldInputReleased);
                                let released = session.release_held_input();
                                wake_at = wake_time(&session, &released, wake_at);
                                react(released)?;
                            }
                            // A wake gives the session nothing to replay,
                            // so a recording does not hold it.
                            Due::Wake => {
                                wake_at = None;
                                react(session.wake_app())?;
                            }
                            Due::HeldFrame => {
                                tape.add(|| Recorded::HeldFrameReleased);
                                react(session.release_held_frame())?;
                            }
                        }
                        continue;
                    }
                },
            };
            match event {
                Event::AppWrote(bytes) => {
                    tape.add(|| Recorded::AppWrote(bytes.clone()));
                    react(session.app_wrote(&bytes))?
                }
                Event::Terminal(TerminalEvent::Sent(bytes)) => {
                    tape.add(|| Recorded::UserTyped(bytes.clone()));
                    let forwarded = session.user_typed(&bytes);
                    wake_at = wake_time(&session, &forwarded, wake_at);
                    react(forwarded)?
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
