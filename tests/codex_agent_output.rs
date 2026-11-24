//! Agent output in Codex: the paragraphs of a reply are joined, laid out in
//! the Paragraph direction and right-aligned in the area Codex drew them
//! in. Codex draws Logical order and joins nothing, so there is nothing to
//! undo. Everything that is not a paragraph of a reply stays as drawn.

mod codex;

use codex::{at_right_edge, from_the_right, on_row, reply_row, reply_start, screen, Run};

#[test]
fn a_farsi_paragraph_is_joined_and_laid_out_from_the_right_edge_of_the_replys_area() {
    let mut run = Run::start(30, 4);

    run.app_writes(on_row(0, &reply_start("سلام دنیا")));

    // The reply mark stays in the first column. The text is drawn in the
    // columns from the third to the last, and starts at the last.
    assert_eq!(run.drawn_row(0), "• سلام دنیا");
    assert_eq!(
        run.row(0),
        format!("•{}", at_right_edge(29, &from_the_right("ﺳﻠﺎﻡ ﺩﻧﯿﺎ")))
    );
}
