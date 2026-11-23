//! Developer tool: prints lines of Farsi the way any program would, to see
//! what farcli makes of them in a real Terminal:
//!
//! ```text
//! farcli farsi_lines.exe [CASE] [COLS]
//!
//!   lines     one line for each layout rule (the default)
//!   stream    one line that arrives a few letters at a time
//!   border    a border after a Half-space, placed with spaces and by column
//!   full      a row exactly COLS cells wide that holds a Half-space
//! ```

use std::env;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

fn main() {
    let mut args = env::args().skip(1);
    let case = args.next().unwrap_or_else(|| "lines".to_owned());
    let cols: usize = args.next().and_then(|cols| cols.parse().ok()).unwrap_or(80);
    match case.as_str() {
        "lines" => lines(),
        "stream" => stream(),
        "border" => border(),
        "full" => full(cols),
        other => eprintln!("unknown case {other:?}"),
    }
}

fn lines() {
    println!("سلام دنیا");
    println!("فایل config.json را باز کن و 123 خط اضافه کن");
    println!("متن (مهم) و [config] است «گل»");
    println!("The word سلام means hello (درود).");
    println!("می\u{200c}خواهم کتاب\u{200c}ها را ببینم");
    println!("پاپ چاپچی ژاکت مژه گچ کیک یکی");
    println!("لا کلاس سلام");
    println!("کتاب خوب |");
    println!("سلام کلا |");
    println!("abcdefgh |");
    println!("a سلام c |");
    println!("Nothing to change here: (a) [b] 33 x 20");
    println!("\x1b[1;31mسلام\x1b[m \x1b[4;32mدنیا\x1b[m");
}

fn stream() {
    for piece in ["می", "\u{200c}خوا", "هم", " بروم", " home", "\n"] {
        print!("{piece}");
        io::stdout().flush().unwrap();
        thread::sleep(Duration::from_millis(300));
    }
    println!("next line");
}

fn border() {
    // Padded with spaces, counting the Half-space as no cell.
    println!("abcdefgh |");
    println!("می\u{200c}خواهم  |");
    println!("a می\u{200c}رویم |");
    // Placed by column: the border goes to the tenth column of the row.
    println!("abcdefgh\x1b[10G|");
    println!("می\u{200c}خواهم\x1b[10G|");
    println!("a می\u{200c}رویم\x1b[10G|");
}

fn full(cols: usize) {
    // Seven cells of «می‌خواهم», then letters up to the last column.
    let filler = "ب".repeat(cols.saturating_sub(8));
    println!("می\u{200c}خواهم {filler}");
    println!("the row below");
    io::stdout().flush().unwrap();
    thread::sleep(Duration::from_millis(300));
    // Back up one row and write over it, the way a progress line does.
    println!("\x1b[1A\rwritten over it");
    // Placed by row: on the sixth row of the screen, when started on the
    // first.
    println!("\x1b[6;1Hthe sixth row");
}
