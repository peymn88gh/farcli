//! The Region finder for Codex CLI, as version 0.171.0 draws its screen. It
//! reads rows from the top down:
//!
//! - A reply starts on a row with the reply mark, dim, in the first column.
//!   Its text, and every row the text wraps onto, starts in the third
//!   column and may run to the last one. The parts of a reply are set apart
//!   by an empty row.
//! - A message the user sent, and the Input box, are drawn on a background
//!   of their own that fills each row, with the prompt mark in the first
//!   column. The Input box is the last of them, and its mark is bold where
//!   a sent message's is bold and dim. Everything from the Input box to the
//!   bottom of the screen is the app's interface. What follows a sent
//!   message is the reply to it. Codex draws the backgrounds only when the
//!   Terminal has told it its colours; without them a sent message is told
//!   by its mark alone.
//! - Any other row with something in its first two columns is the app's
//!   interface: a tool call, the line that says the app is working, a
//!   notice. Rows under it that start further in belong to it.
//!
//! Codex draws the conversation in Logical order, so a row's text begins at
//! the left whatever its direction.
//!
//! While a reply arrives, Codex shows the part still being written right
//! under the parts that are done, and now and then with no empty row
//! between the two. So inside a run of rows each row is first read by
//! itself:
//!
//! - a row drawn wholly in colour is code: Codex colours every word of a
//!   code block in a language it knows. A heading is drawn wholly in colour
//!   too, but bold or italic from end to end, which code never is;
//! - a row drawn wholly dim is the app's own: the line under a finished
//!   reply that says how long it took, or the model's thinking;
//! - from its first list item, table, quotation or tool result on, a run
//!   is that and nothing else;
//! - a row that starts further in than text does makes code of the plain
//!   rows around it.
//!
//! What is left is prose. Where Codex wrapped a text its rows are one
//! paragraph, and where the writer broke the line (a poem, an address) each
//! line is a paragraph of its own.
//!
//! A list item begins with a marker, a bullet or a number and a full stop,
//! which Codex draws in a colour, four columns further in for each list the
//! item is inside. The rows its text wraps onto start two or three columns
//! further in than the marker. Each item is a region of its own.
//!
//! A table has no upright bars. Under its header and between its rows
//! Codex draws a rule, a stretch of dashes for each column, and that is
//! what tells a table and where its columns are. The whole table is one
//! region. While the model is still writing a table Codex shows its source,
//! which is left as drawn.
//!
//! A list ends at the first row that is neither an item nor a row an item
//! wrapped onto, and a table at the first row that starts at the left edge
//! and is not a rule. Plain rows there are what the model is writing next,
//! drawn right under the part before it, and are read as prose.
//!
//! One kind of code looks exactly like lines of prose and is read as such:
//! a block with no language named, or in a language Codex does not know,
//! whose lines all start at the left edge. Codex gives it no colour.
//!
//! The same grammar serves the inline display, where rows scroll off the
//! top for good, and the default one, where Codex draws the conversation
//! again on the alternate screen at whatever place the user scrolled to,
//! with the start of the message sent laid over the top row.

use crate::screen::{Cell, Color, Row};

use super::{ItemStart, Kind, ParagraphStart, Region};

/// The first column of a message's text.
const TEXT_COL: usize = 2;

/// The mark that starts a reply. Codex draws it dim. The same mark in
/// other styles starts a tool call or the line that says Codex is working.
const REPLY_MARK: char = '•';

/// Marks that start a sent message and the Input box: the usual one, and
/// the one drawn at the highest reasoning effort.
const PROMPT_MARKS: [char; 2] = ['›', '»'];

pub(crate) struct Finder {
    /// What lies above the first row the finder is given next.
    above: Above,
}

/// What lies above a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Above {
    /// No message: the app's interface, or nothing of the app's at all.
    Outside,
    /// Not known. It may be the middle of a reply.
    Unknown,
    /// A reply, which the row may carry on.
    Reply(Option<Cut>),
    /// A message the user sent, drawn with no background: only its mark
    /// tells it from a reply.
    Sent,
    /// The Input box. All that is under it is the app's interface.
    InputBox,
}

/// The part of a reply that the row above belongs to, when the row below
/// may belong to it too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cut {
    Paragraph,
    /// A list item: the rows below may be rows it wrapped onto.
    List,
    /// A table: the rows below are its other rows.
    Table,
    /// Anything else: its other rows are not to be touched either.
    Other,
}

/// What a row is, going by its first columns and its background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Empty,
    /// Something with a background of its own is drawn on the row.
    Covered,
    ReplyHead,
    /// A prompt mark with no background: a sent message, or the Input box.
    PromptHead,
    /// Nothing in the first two columns, and text further on.
    Indented,
    Other,
}

impl Finder {
    /// A finder for a screen the app has drawn nothing on yet.
    pub fn new() -> Finder {
        Finder {
            above: Above::Outside,
        }
    }

    /// A finder for rows of which nothing is known about what lies above
    /// them: the default display, scrolled to any place.
    pub fn knowing_nothing_above() -> Finder {
        Finder {
            above: Above::Unknown,
        }
    }

    /// The app cleared the screen: nothing of a message is above its top.
    pub fn screen_cleared(&mut self) {
        self.above = Above::Outside;
    }

    /// The regions of `rows`, which follow one another from the top down.
    /// The first `leaving` rows are scrolling off the top of the screen:
    /// the finder keeps what it needs to carry on from the row after them.
    /// `draws_in_colour` says whether the app has drawn anything in colour
    /// in this session: with colours switched off, less can be told.
    pub fn find(&mut self, rows: &[Row], leaving: usize, draws_in_colour: bool) -> Vec<Region> {
        let mut walk = Walk {
            rows,
            draws_in_colour,
            regions: Vec::new(),
            state: match self.above {
                Above::Reply(_) | Above::Unknown => Above::Reply(None),
                above => above,
            },
            cut_above: match self.above {
                Above::Reply(cut) => cut,
                Above::Unknown => Some(Cut::Paragraph),
                _ => None,
            },
            started_unknown: self.above == Above::Unknown,
            block: Vec::new(),
            head: None,
            covered_from: None,
            span_start: 0,
            input_box_mark: rows
                .iter()
                .rposition(|row| holds_prompt_mark(row))
                .filter(|last| is_input_box_mark(&rows[*last][0])),
        };
        let mut state_at_split = self.above;
        for at in 0..rows.len() {
            walk.settle(at);
            if at == leaving {
                state_at_split = walk.state;
            }
            walk.step(at);
        }
        walk.close(rows.len());

        if leaving > 0 && leaving < rows.len() {
            // A region cut by the top of the screen carries on below it.
            let cut = walk
                .regions
                .iter()
                .find(|region| region.rows.start < leaving && region.rows.end > leaving);
            self.above = match cut.map(|region| region.kind) {
                Some(Kind::Paragraph(_)) => Above::Reply(Some(Cut::Paragraph)),
                Some(Kind::ListItem(_)) => Above::Reply(Some(Cut::List)),
                Some(Kind::Table) => Above::Reply(Some(Cut::Table)),
                Some(Kind::Code | Kind::Unsure) => Above::Reply(Some(Cut::Other)),
                // The Input box is at the bottom of the screen. If it ever
                // scrolls off, the app is leaving, and what it draws next
                // is under no Input box.
                Some(Kind::SentMessage | Kind::InputBox) | None => match state_at_split {
                    Above::InputBox => Above::Outside,
                    state => state,
                },
            };
        }
        walk.regions
    }
}

struct Walk<'a> {
    rows: &'a [Row],
    draws_in_colour: bool,
    regions: Vec<Region>,
    state: Above,
    /// The part of a reply that the first row may carry on from above the
    /// rows given, until a row shows that it does not.
    cut_above: Option<Cut>,
    started_unknown: bool,
    /// The rows of the part of a reply being gathered.
    block: Vec<usize>,
    /// The row that holds the mark of the reply being read, when it is
    /// among the rows given.
    head: Option<usize>,
    /// Where the rows on a background that are being gathered start.
    covered_from: Option<usize>,
    /// Where the sent message or Input box being gathered starts, when its
    /// mark is what told.
    span_start: usize,
    /// The row that holds the Input box's mark: the last prompt mark among
    /// the rows given, unless it is drawn as a sent message's is. The Input
    /// box is under everything else, so a prompt mark above another is a
    /// message the user sent, however it is drawn.
    input_box_mark: Option<usize>,
}

impl Walk<'_> {
    /// Settles what lies above the row `at`, as far as the rows before it
    /// could not: rows on a background are named when the first row
    /// without one shows where they end.
    fn settle(&mut self, at: usize) {
        if self.state != Above::InputBox && !is_covered(&self.rows[at]) {
            self.end_covered(at);
        }
    }

    fn step(&mut self, at: usize) {
        if self.state == Above::InputBox {
            return;
        }
        match shape(&self.rows[at]) {
            Shape::Covered => {
                if self.covered_from.is_some() {
                    return;
                }
                // The default display lays the start of the message sent
                // over the top row. What is under it may be the middle of
                // a paragraph of the reply.
                let cut_above = self.cut_above;
                self.close(at);
                self.covered_from = Some(at);
                if self.started_unknown && at == 0 {
                    self.cut_above = cut_above;
                }
            }
            Shape::ReplyHead => {
                self.close(at);
                self.state = Above::Reply(None);
                self.head = Some(at);
                self.block.push(at);
            }
            Shape::PromptHead => {
                self.close(at);
                self.state = match self.input_box_mark == Some(at) {
                    true => Above::InputBox,
                    false => Above::Sent,
                };
                self.span_start = at;
            }
            Shape::Other => {
                self.close(at);
                self.state = Above::Outside;
            }
            Shape::Empty => self.close_block(),
            Shape::Indented => {
                if let Above::Reply(_) = self.state {
                    self.block.push(at);
                }
            }
        }
    }

    /// Ends the rows on a background being gathered, at the row `end`, and
    /// names them: the Input box when its mark is among them, a message
    /// the user sent when the background fills the rows. A background on
    /// part of a row only is something the app laid over the conversation,
    /// and is in no region.
    fn end_covered(&mut self, end: usize) {
        let Some(start) = self.covered_from.take() else {
            return;
        };
        let rows = &self.rows[start..end];
        let input_box_mark = self.input_box_mark.filter(|at| (start..end).contains(at));
        if input_box_mark.is_some() {
            self.state = Above::InputBox;
            self.span_start = start;
            return;
        }
        if rows.iter().any(|row| has_background(&row[0])) {
            let cols = self.rows[start].len();
            self.regions.push(Region {
                rows: start..end,
                cols: TEXT_COL.min(cols)..cols,
                kind: Kind::SentMessage,
                cut_above: false,
            });
            // What follows a sent message is the reply to it, even when
            // the reply's mark is not in sight: the default display keeps
            // the start of the message on the top row while the reply
            // scrolls under it.
            self.state = Above::Reply(None);
            self.head = None;
        }
    }

    /// Ends whatever is being gathered, at the row `end`.
    fn close(&mut self, end: usize) {
        self.close_block();
        self.end_covered(end);
        let (kind, first_col) = match self.state {
            Above::Sent => (Kind::SentMessage, TEXT_COL),
            Above::InputBox => (Kind::InputBox, 0),
            _ => return,
        };
        if self.span_start < end {
            let cols = self.rows[self.span_start].len();
            self.regions.push(Region {
                rows: self.span_start..end,
                cols: first_col.min(cols)..cols,
                kind,
                cut_above: false,
            });
        }
        if self.state == Above::Sent {
            self.state = Above::Outside;
        }
    }

    /// Ends the part of a reply being gathered and names what it holds.
    fn close_block(&mut self) {
        // Whatever was cut by the top of the rows given ends with the
        // first block, or before it.
        let top = self.cut_above.take();
        let Some(&first) = self.block.first() else {
            return;
        };
        let last = self.block[self.block.len() - 1];
        self.block.clear();
        let all_rows = self.rows;
        let rows = &all_rows[first..=last];

        let mut classes: Vec<RowClass> = rows.iter().map(|row| row_class(row)).collect();
        if self.head == Some(first) && is_tool_title(&rows[0]) {
            classes[0] = RowClass::NotProse;
        }
        let parts = match top {
            Some(Cut::Other) => vec![(0, rows.len(), Kind::Unsure)],
            Some(Cut::Table) => {
                let end = table_end(rows, 0);
                let mut parts = Vec::new();
                if end > 0 {
                    parts.push((0, end, Kind::Table));
                }
                self.what_follows(&mut parts, rows, &classes, end);
                parts
            }
            _ => self.parts(rows, &classes, top),
        };
        let cols = TEXT_COL.min(rows[0].len())..rows[0].len();
        self.regions
            .extend(parts.into_iter().map(|(from, to, kind)| Region {
                rows: first + from..first + to,
                cols: cols.clone(),
                kind,
                cut_above: top.is_some(),
            }));
    }

    /// The parts of one block of a reply: for each, its first row, the row
    /// after its last, and what it is.
    fn parts(
        &self,
        rows: &[Row],
        classes: &[RowClass],
        top: Option<Cut>,
    ) -> Vec<(usize, usize, Kind)> {
        let mut parts = Vec::new();
        // From its first list item, table, quotation or tool result on, a
        // block is that and nothing else. A table's header row stands
        // above its first rule.
        let claim = classes.iter().position(|class| {
            matches!(
                class,
                RowClass::ListItem | RowClass::Rule | RowClass::Marked
            )
        });
        let free = match claim {
            Some(at) if classes[at] == RowClass::Rule => at.saturating_sub(1),
            Some(at) => at,
            None => rows.len(),
        };
        // Rows that start further in than a paragraph's, at the top of a
        // block whose start is out of sight, with a list after them or a
        // list item cut by the top of the screen above them: they are rows
        // that item wrapped onto.
        let in_a_list =
            claim.is_some_and(|at| classes[at] == RowClass::ListItem) || top == Some(Cut::List);
        let carried_on = match top.is_some() && in_a_list {
            true if classes[..free].iter().all(|class| *class == RowClass::Deep) => free,
            _ => 0,
        };
        if carried_on > 0 {
            parts.push((0, carried_on, Kind::ListItem(ItemStart::Above)));
        }
        // A row at the left edge under a list item that was cut by the top
        // carries on nothing: it begins a paragraph.
        let top = top.filter(|cut| *cut != Cut::List);

        let plain = |class: &RowClass| matches!(class, RowClass::Text | RowClass::Deep);
        let mut at = carried_on;
        while at < free {
            if !plain(&classes[at]) {
                let kind = match classes[at] {
                    RowClass::Coloured => Kind::Code,
                    _ => Kind::Unsure,
                };
                parts.push((at, at + 1, kind));
                at += 1;
                continue;
            }
            let end = classes[at..free]
                .iter()
                .position(|class| !plain(class))
                .map_or(free, |len| at + len);
            self.plain_rows(&mut parts, rows, at..end, &classes[at..end], top);
            at = end;
        }
        match claim.map(|at| classes[at]) {
            Some(RowClass::ListItem) => {
                let end = list_items(&mut parts, rows, free, classes);
                self.what_follows(&mut parts, rows, classes, end);
            }
            Some(RowClass::Rule) => {
                let end = table_end(rows, free);
                parts.push((free, end, Kind::Table));
                self.what_follows(&mut parts, rows, classes, end);
            }
            Some(_) => parts.push((free, rows.len(), Kind::Unsure)),
            None => {}
        }
        parts
    }

    /// Names the rows that stand right under a list or a table, from the
    /// row `from` on. While a reply arrives, Codex draws the part the model
    /// is still writing under the part before it with no empty row between
    /// the two: plain rows from the left edge there are prose. Anything
    /// else is left as drawn.
    fn what_follows(
        &self,
        parts: &mut Vec<(usize, usize, Kind)>,
        rows: &[Row],
        classes: &[RowClass],
        from: usize,
    ) {
        if from >= rows.len() {
            return;
        }
        if classes[from..].iter().all(|class| *class == RowClass::Text) {
            self.plain_rows(parts, rows, from..rows.len(), &classes[from..], None);
        } else {
            parts.push((from, rows.len(), Kind::Unsure));
        }
    }

    /// Names a run of rows in the Terminal's own colours, `rows[run]`.
    fn plain_rows(
        &self,
        parts: &mut Vec<(usize, usize, Kind)>,
        rows: &[Row],
        run: std::ops::Range<usize>,
        classes: &[RowClass],
        top: Option<Cut>,
    ) {
        // Code keeps its own indentation. A paragraph starts every row at
        // the left edge of its area.
        if classes.contains(&RowClass::Deep) {
            parts.push((run.start, run.end, Kind::Code));
            return;
        }
        // Codex wraps a paragraph only where the next word does not fit. A
        // row that left room for the first word of the row below it ends a
        // line: the writer broke the line there, as in a poem or an
        // address, and the next row starts a paragraph of its own.
        let width = rows[0].len().saturating_sub(TEXT_COL);
        let mut lines = vec![run.start];
        lines.extend(
            (run.start + 1..run.end).filter(|row| !wraps_into(&rows[row - 1], &rows[*row], width)),
        );
        // Lines of code in a language Codex knows are told by their
        // colours. With colours switched off, several lines under one
        // another may be either, and code must stay as drawn.
        if lines.len() > 1 && !self.draws_in_colour {
            parts.push((run.start, run.end, Kind::Unsure));
            return;
        }
        lines.push(run.end);
        for line in lines.windows(2) {
            let begins = match (line[0], top) {
                (_, None) => ParagraphStart::Here,
                (0, Some(_)) => ParagraphStart::Above,
                (_, Some(_)) if self.started_unknown => ParagraphStart::HereUnderLinesOutOfSight,
                (_, Some(_)) => ParagraphStart::Here,
            };
            parts.push((line[0], line[1], Kind::Paragraph(begins)));
        }
    }
}

/// Splits the rows of a list, `rows[from..]`, into its items, and returns
/// the row after the list's last. Codex draws the marker of an item four
/// columns further in for each list the item is inside, and the rows an
/// item wraps onto two or three columns further in than its marker. A row
/// that fits neither ends the list.
fn list_items(
    parts: &mut Vec<(usize, usize, Kind)>,
    rows: &[Row],
    from: usize,
    classes: &[RowClass],
) -> usize {
    // The item being gathered: its first row, the column of its marker in
    // the reply's area, and what it is.
    let mut item: Option<(usize, usize, Kind)> = None;
    for at in from..rows.len() {
        let text = &rows[at][TEXT_COL.min(rows[at].len())..];
        let start = text.iter().position(|cell| !is_empty(cell)).unwrap_or(0);
        // A number is drawn with its last digit in the marker's column.
        let marker = marker_len(text, start).filter(|len| {
            let is_bullet = *len == 1;
            classes[at] == RowClass::ListItem
                && (start % 4 == 0 || (!is_bullet && (start + len - 1) % 4 == 1))
        });
        if let Some(len) = marker {
            parts.extend(item.take().map(|(first, _, kind)| (first, at, kind)));
            let text_col = TEXT_COL + start + len + 1;
            item = Some((at, start, Kind::ListItem(ItemStart::Here { text_col })));
            continue;
        }
        let carries_on = matches!(
            classes[at],
            RowClass::Text | RowClass::Deep | RowClass::ListItem
        ) && item.is_some_and(|(_, marker_col, _)| start > marker_col);
        if !carries_on {
            parts.extend(item.take().map(|(first, _, kind)| (first, at, kind)));
            return at;
        }
    }
    parts.extend(item.map(|(first, _, kind)| (first, rows.len(), kind)));
    rows.len()
}

/// The row after the last row of the table that `rows[from]` is a row of.
/// A rule is a row of a table, and so is a row whose text starts further
/// in than the left edge of the reply's area: Codex leaves a column empty
/// before the text of every cell.
fn table_end(rows: &[Row], from: usize) -> usize {
    let is_a_table_row = |row: &Row| {
        let text = &row[TEXT_COL.min(row.len())..];
        match text.iter().position(|cell| !is_empty(cell)) {
            Some(0) => is_line_drawing(text[0].character),
            _ => true,
        }
    };
    (from..rows.len())
        .find(|at| !is_a_table_row(&rows[*at]))
        .unwrap_or(rows.len())
}

/// How many cells the list marker at `start` takes, without the space
/// after it: one for a bullet, and for a number its digits and the mark
/// after them. `None` when no marker stands there.
fn marker_len(text: &[Cell], start: usize) -> Option<usize> {
    let then_empty = |at: usize| text.get(at).is_none_or(is_empty);
    let first = text.get(start)?.character;
    if matches!(first, '-' | '*' | '+' | '•') {
        return then_empty(start + 1).then_some(1);
    }
    let digits = text[start..]
        .iter()
        .take_while(|cell| cell.character.is_ascii_digit())
        .count();
    (digits > 0
        && text
            .get(start + digits)
            .is_some_and(|cell| matches!(cell.character, '.' | ')'))
        && then_empty(start + digits + 1))
    .then_some(digits + 1)
}

fn is_empty(cell: &Cell) -> bool {
    cell.is_left_alone() || cell.character == ' '
}

fn has_background(cell: &Cell) -> bool {
    !cell.is_left_alone() && (cell.style.background != Color::Default || cell.style.inverse)
}

/// True for a row that begins with a prompt mark.
fn holds_prompt_mark(row: &[Cell]) -> bool {
    row.first()
        .is_some_and(|mark| PROMPT_MARKS.contains(&mark.character))
        && row.get(1).is_some_and(is_empty)
}

/// True for a prompt mark drawn as the Input box's is: bold, where the mark
/// of a message the user typed and sent is bold and dim. A box that takes
/// no input has a dim mark that is not bold.
fn is_input_box_mark(mark: &Cell) -> bool {
    !(mark.style.bold && mark.style.dim)
}

/// True when something with a background of its own is drawn on the row.
fn is_covered(row: &[Cell]) -> bool {
    row.iter().any(has_background)
}

fn shape(row: &[Cell]) -> Shape {
    if is_covered(row) {
        return Shape::Covered;
    }
    let Some(first) = row.iter().position(|cell| !is_empty(cell)) else {
        return Shape::Empty;
    };
    if first >= TEXT_COL {
        return Shape::Indented;
    }
    let mark = &row[0];
    let then_empty = row.get(1).is_some_and(is_empty);
    if first == 0 && mark.character == REPLY_MARK && mark.style.dim && then_empty {
        Shape::ReplyHead
    } else if holds_prompt_mark(row) {
        Shape::PromptHead
    } else {
        Shape::Other
    }
}

/// What a row inside a reply is, read by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowClass {
    /// Text from the left edge of the reply's area, in the Terminal's own
    /// colours at least in part.
    Text,
    /// The same, starting further in than a paragraph's rows do.
    Deep,
    /// Every character in a colour: a row of a code block.
    Coloured,
    /// The app's own: every character dim, or the name of a tool call.
    NotProse,
    ListItem,
    /// A row of line-drawing dashes, as Codex draws under a table's header
    /// and between its rows.
    Rule,
    /// A row that begins with the mark of something that is no paragraph:
    /// a quotation, what a tool gave back, or a table the model has not
    /// finished writing.
    Marked,
}

fn row_class(row: &[Cell]) -> RowClass {
    let text = &row[TEXT_COL.min(row.len())..];
    let Some(start) = text.iter().position(|cell| !is_empty(cell)) else {
        return RowClass::Text;
    };
    let first = text[start].character;
    let then_empty = |at: usize| text.get(at).is_none_or(is_empty);
    let characters = || text.iter().filter(|cell| !is_empty(cell));
    // Every character in a colour, and neither bold from end to end nor
    // italic from end to end: that is how Codex draws code.
    let coloured_as_code = |cells: &[Cell]| {
        let characters = || cells.iter().filter(|cell| !is_empty(cell));
        characters().next().is_some()
            && characters().all(|cell| cell.style.foreground != Color::Default)
            && !characters().all(|cell| cell.style.bold)
            && !characters().all(|cell| cell.style.italic)
    };
    // A line of code can begin with a dash too, as the lines of a
    // comparison of two texts do. Codex gives a list's marker a colour and
    // leaves the item's text in the Terminal's own, unless the whole item
    // is bold or slanted.
    let list_marker =
        || marker_len(text, start).is_some_and(|len| !coloured_as_code(&text[start + len..]));
    if is_line_drawing(first) {
        if characters().all(|cell| matches!(cell.character, '─' | '━' | '═')) {
            RowClass::Rule
        } else {
            RowClass::Marked
        }
    } else if first == '|' || (first == '>' && then_empty(start + 1)) {
        RowClass::Marked
    } else if list_marker() {
        RowClass::ListItem
    } else if characters().all(|cell| cell.style.dim) {
        RowClass::NotProse
    } else if characters().all(|cell| cell.style.foreground != Color::Default)
        && !characters().all(|cell| cell.style.bold)
        && !characters().all(|cell| cell.style.italic)
    {
        RowClass::Coloured
    } else if start > 0 {
        RowClass::Deep
    } else {
        RowClass::Text
    }
}

fn is_line_drawing(character: char) -> bool {
    ('\u{2500}'..='\u{257f}').contains(&character)
}

/// True for the first row of a tool call: after the mark, a name in bold
/// English letters (`Ran`, `Explored`, `Updated Plan`) and nothing in the
/// plain style a reply's text is drawn in. A reply that begins with a bold
/// word goes on in plain text.
fn is_tool_title(row: &[Cell]) -> bool {
    let text = &row[TEXT_COL.min(row.len())..];
    let plain = |cell: &Cell| {
        let style = &cell.style;
        style.foreground == Color::Default && !style.bold && !style.dim && !style.italic
    };
    text.first()
        .is_some_and(|cell| cell.character.is_ascii_alphabetic() && cell.style.bold)
        && !text.iter().any(|cell| !is_empty(cell) && plain(cell))
}

/// True when Codex, wrapping one text in an area `width` cells wide, would
/// have broken it between `row` and `next`: the first word of `next` did
/// not fit after the last word of `row`.
fn wraps_into(row: &[Cell], next: &[Cell], width: usize) -> bool {
    let text = |row: &'_ [Cell]| -> std::ops::Range<usize> {
        let from = TEXT_COL.min(row.len());
        let start = row[from..]
            .iter()
            .position(|cell| !is_empty(cell))
            .map_or(from, |start| from + start);
        let end = row
            .iter()
            .rposition(|cell| !is_empty(cell))
            .map_or(start, |last| last + 1);
        start..end.max(start)
    };
    let next_text = &next[text(next)];
    let first_word = next_text
        .iter()
        .position(is_empty)
        .unwrap_or(next_text.len());
    text(row).len() + 1 + first_word > width
}
