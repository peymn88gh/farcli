//! Agent output in Claude Code reads correctly through farcli: a Farsi
//! paragraph is joined, right to left and right-aligned, and everything that
//! is not a paragraph of Agent output stays as the app drew it.
//!
//! Claude Code on Windows reorders each row of Farsi itself, so every test
//! here feeds the session what Claude Code draws, never Logical order. The
//! small cases are written by hand in the shape Claude Code 2.1.322 draws a
//! reply; the rest replay the recordings in `tests/recordings`.

mod claude_code;

use claude_code::{at_right_edge, from_the_right, reply, Run};

#[test]
fn a_farsi_paragraph_in_a_reply_is_joined_right_to_left_and_right_aligned() {
    let mut run = Run::start(30, 6);

    run.app_writes(reply(&[&["سلام دنیا"]]));

    // The reply mark stays where the app drew it. The paragraph ends at the
    // right edge of the area the app drew it in, which is the Terminal's.
    assert_eq!(
        run.terminal.screen_text()[0],
        format!("●{}", at_right_edge(29, &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ")))
    );
}
