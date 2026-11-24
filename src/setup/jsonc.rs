//! Settings files written as JSON with comments, as Windows Terminal and
//! VS Code keep them.
//!
//! A change touches only the characters of the one value it sets. Comments,
//! spacing, line endings and everything else in the file stay as the user
//! wrote them. After every change the new text is read again and compared
//! with what the change was meant to produce; a change that fails this check
//! is refused and the text stays as it was.

use std::ops::Range;

/// One step on the way from the top of a settings file to a value in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step<'a> {
    /// The member of an object with this name.
    Key(&'a str),
    /// The item of a list at this place, counted from zero.
    Index(usize),
}

/// What is found at some place in a settings file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// Nothing: the place, or something on the way to it, is not there.
    Nothing,
    Text(String),
    Object,
    /// A list with this many items.
    List(usize),
    /// A number, `true`, `false` or `null`.
    Other,
    /// The name on the way there appears twice in one object.
    Twice,
}

/// A settings file that has been read.
pub struct Settings {
    starts_with_byte_order_mark: bool,
    text: String,
    root: Node,
    comments: Vec<Range<usize>>,
}

/// What a settings file says, without its comments and spacing.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Object(Vec<(String, Value)>),
    List(Vec<Value>),
    Text(String),
    /// A number, `true`, `false` or `null`, as written.
    Other(String),
}

struct Node {
    span: Range<usize>,
    kind: Kind,
}

enum Kind {
    Object(Vec<Member>),
    List(Vec<Node>),
    Text(String),
    Other,
}

struct Member {
    name: String,
    /// Where the member's name starts in the text.
    start: usize,
    value: Node,
}

const BYTE_ORDER_MARK: &str = "\u{feff}";

impl Settings {
    /// Reads a settings file. The error says why it could not be read.
    pub fn read(bytes: &[u8]) -> Result<Settings, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "it is not UTF-8 text".to_string())?;
        let (starts_with_byte_order_mark, text) = match text.strip_prefix(BYTE_ORDER_MARK) {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (root, comments) = parse(text)?;
        if !matches!(root.kind, Kind::Object(_)) {
            return Err("it does not hold one object of settings".to_string());
        }
        Ok(Settings {
            starts_with_byte_order_mark,
            text: text.to_string(),
            root,
            comments,
        })
    }

    /// The file as it should be written back.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mark = if self.starts_with_byte_order_mark {
            BYTE_ORDER_MARK
        } else {
            ""
        };
        format!("{mark}{}", self.text).into_bytes()
    }

    /// What the file holds at `path`.
    pub fn find(&self, path: &[Step]) -> Found {
        let mut node = &self.root;
        for step in path {
            node = match (step, &node.kind) {
                (Step::Key(name), Kind::Object(members)) => match member(members, name) {
                    Ok(Some(member)) => &member.value,
                    Ok(None) => return Found::Nothing,
                    Err(()) => return Found::Twice,
                },
                (Step::Index(index), Kind::List(items)) => match items.get(*index) {
                    Some(item) => item,
                    None => return Found::Nothing,
                },
                _ => return Found::Nothing,
            };
        }
        match &node.kind {
            Kind::Object(_) => Found::Object,
            Kind::List(items) => Found::List(items.len()),
            Kind::Text(text) => Found::Text(text.clone()),
            Kind::Other => Found::Other,
        }
    }

    /// Sets the text at `path`, adding the objects on the way that are not
    /// there yet. The error says why the file cannot be changed safely; the
    /// text is then as it was before the call.
    pub fn set_text(&mut self, path: &[Step], text: &str) -> Result<(), String> {
        let (at, replacement) = self.change_for(path, text)?;
        let mut changed = self.text.clone();
        changed.replace_range(at, &replacement);

        let mut meant = self.root.value(&self.text);
        meant.set_text(path, text);
        let (root, comments) =
            parse(&changed).map_err(|_| "the change would break the file".to_string())?;
        let comments_kept = comments.len() == self.comments.len()
            && comments
                .iter()
                .zip(&self.comments)
                .all(|(new, old)| changed[new.clone()] == self.text[old.clone()]);
        if root.value(&changed) != meant || !comments_kept {
            return Err("the change would alter more than the font".to_string());
        }
        self.text = changed;
        self.root = root;
        self.comments = comments;
        Ok(())
    }

    /// The characters to replace and what to put there.
    fn change_for(&self, path: &[Step], text: &str) -> Result<(Range<usize>, String), String> {
        let mut node = &self.root;
        for (done, step) in path.iter().enumerate() {
            node = match (step, &node.kind) {
                (Step::Key(name), Kind::Object(members)) => match member(members, name) {
                    Ok(Some(member)) => &member.value,
                    Ok(None) => return self.addition(node, members, &path[done..], text),
                    Err(()) => return Err(format!("\"{name}\" appears twice")),
                },
                (Step::Index(index), Kind::List(items)) => items
                    .get(*index)
                    .ok_or_else(|| "a list is shorter than expected".to_string())?,
                (Step::Key(name), _) => {
                    return Err(format!("the place for \"{name}\" is not an object"))
                }
                (Step::Index(_), _) => return Err("a list is not a list".to_string()),
            };
        }
        match node.kind {
            Kind::Text(_) => Ok((node.span.clone(), quoted(text))),
            _ => Err("the value to change is not text".to_string()),
        }
    }

    /// Adds `names`, each inside the one before, ending in `text`, as the
    /// first member of `object`.
    fn addition(
        &self,
        object: &Node,
        members: &[Member],
        names: &[Step],
        text: &str,
    ) -> Result<(Range<usize>, String), String> {
        let names: Vec<&str> = names
            .iter()
            .map(|step| match step {
                Step::Key(name) => Ok(*name),
                Step::Index(_) => Err("a list is missing".to_string()),
            })
            .collect::<Result<_, _>>()?;
        let newline = if self.text.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let unit = self.indent_unit();
        let inside = object.span.start + 1..object.span.end - 1;

        if let Some(first) = members.first() {
            let before = self.line_start(first.start)..first.start;
            return Ok(if self.text[before.clone()].trim().is_empty() {
                // The members are one to a line: the new one gets its own.
                let indent = &self.text[before];
                let member = on_lines(&names, text, indent, &unit, newline);
                (
                    first.start..first.start,
                    format!("{member},{newline}{indent}"),
                )
            } else {
                let member = on_one_line(&names, text);
                (first.start..first.start, format!("{member}, "))
            });
        }
        let object_indent = self.indent_of_line(object.span.start);
        let indent = format!("{object_indent}{unit}");
        let member = on_lines(&names, text, &indent, &unit, newline);
        Ok(if self.text[inside.clone()].trim().is_empty() {
            (
                inside,
                format!("{newline}{indent}{member}{newline}{object_indent}"),
            )
        } else {
            // Only comments inside: they stay where they are.
            (
                inside.start..inside.start,
                format!("{newline}{indent}{member}"),
            )
        })
    }

    fn line_start(&self, at: usize) -> usize {
        self.text[..at].rfind('\n').map_or(0, |newline| newline + 1)
    }

    /// The spaces or tabs the line holding `at` starts with.
    fn indent_of_line(&self, at: usize) -> &str {
        let line = &self.text[self.line_start(at)..];
        let end = line
            .find(|c: char| c != ' ' && c != '\t')
            .unwrap_or(line.len());
        &line[..end]
    }

    /// What one level of nesting is indented by in this file.
    fn indent_unit(&self) -> String {
        if let Kind::Object(members) = &self.root.kind {
            if let Some(first) = members.first() {
                let before = &self.text[self.line_start(first.start)..first.start];
                if !before.is_empty() && before.trim().is_empty() {
                    return before.to_string();
                }
            }
        }
        "    ".to_string()
    }
}

/// The member called `name`, or an error when two are.
fn member<'a>(members: &'a [Member], name: &str) -> Result<Option<&'a Member>, ()> {
    let mut named = members.iter().filter(|member| member.name == name);
    match (named.next(), named.next()) {
        (found, None) => Ok(found),
        _ => Err(()),
    }
}

fn on_lines(names: &[&str], text: &str, indent: &str, unit: &str, newline: &str) -> String {
    match names {
        [] => quoted(text),
        [name] => format!("{}: {}", quoted(name), quoted(text)),
        [name, rest @ ..] => {
            let deeper = format!("{indent}{unit}");
            let inner = on_lines(rest, text, &deeper, unit, newline);
            format!(
                "{}: {{{newline}{deeper}{inner}{newline}{indent}}}",
                quoted(name)
            )
        }
    }
}

fn on_one_line(names: &[&str], text: &str) -> String {
    match names {
        [] => quoted(text),
        [name] => format!("{}: {}", quoted(name), quoted(text)),
        [name, rest @ ..] => format!("{}: {{ {} }}", quoted(name), on_one_line(rest, text)),
    }
}

/// `text` as it is written in a settings file: in quotes, with escapes.
pub fn quoted(text: &str) -> String {
    let mut quoted = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            c if (c as u32) < 0x20 => quoted.push_str(&format!("\\u{:04x}", c as u32)),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

impl Node {
    fn value(&self, text: &str) -> Value {
        match &self.kind {
            Kind::Object(members) => Value::Object(
                members
                    .iter()
                    .map(|member| (member.name.clone(), member.value.value(text)))
                    .collect(),
            ),
            Kind::List(items) => Value::List(items.iter().map(|item| item.value(text)).collect()),
            Kind::Text(text) => Value::Text(text.clone()),
            Kind::Other => Value::Other(text[self.span.clone()].to_string()),
        }
    }
}

impl Value {
    /// What `Settings::set_text` is meant to do, done to the plain value.
    fn set_text(&mut self, path: &[Step], text: &str) {
        let Some((step, rest)) = path.split_first() else {
            *self = Value::Text(text.to_string());
            return;
        };
        match (step, self) {
            (Step::Key(name), Value::Object(members)) => {
                if !members.iter().any(|(member, _)| member == name) {
                    members.insert(0, (name.to_string(), Value::Object(Vec::new())));
                }
                if let Some((_, value)) = members.iter_mut().find(|(member, _)| member == name) {
                    value.set_text(rest, text);
                }
            }
            (Step::Index(index), Value::List(items)) => {
                if let Some(item) = items.get_mut(*index) {
                    item.set_text(rest, text);
                }
            }
            _ => {}
        }
    }
}

/// Reads `text` and returns its one value and where its comments are.
fn parse(text: &str) -> Result<(Node, Vec<Range<usize>>), String> {
    let mut parser = Parser {
        text,
        at: 0,
        comments: Vec::new(),
    };
    parser.skip_space_and_comments()?;
    let root = parser.value(0)?;
    parser.skip_space_and_comments()?;
    if parser.at != text.len() {
        return Err(parser.problem("something follows the settings"));
    }
    Ok((root, parser.comments))
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
    comments: Vec<Range<usize>>,
}

/// Deeper nesting than any settings file has; a guard against a file made
/// to exhaust the stack.
const DEEPEST: usize = 200;

impl Parser<'_> {
    fn problem(&self, what: &str) -> String {
        let line = self.text[..self.at].matches('\n').count() + 1;
        format!("{what} on line {line}")
    }

    fn rest(&self) -> &str {
        &self.text[self.at..]
    }

    fn next(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn skip_space_and_comments(&mut self) -> Result<(), String> {
        loop {
            let rest = self.rest();
            if rest.starts_with("//") {
                let end = rest.find('\n').unwrap_or(rest.len());
                let end = end - usize::from(rest[..end].ends_with('\r'));
                self.comments.push(self.at..self.at + end);
                self.at += end;
            } else if rest.starts_with("/*") {
                let end = rest
                    .find("*/")
                    .ok_or_else(|| self.problem("a comment is never closed"))?
                    + 2;
                self.comments.push(self.at..self.at + end);
                self.at += end;
            } else if rest.starts_with([' ', '\t', '\r', '\n']) {
                self.at += 1;
            } else {
                return Ok(());
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Node, String> {
        if depth > DEEPEST {
            return Err(self.problem("the nesting is too deep"));
        }
        let start = self.at;
        let kind = match self.next() {
            Some('{') => Kind::Object(self.object(depth)?),
            Some('[') => Kind::List(self.list(depth)?),
            Some('"') => Kind::Text(self.string()?),
            _ => {
                let rest = self.rest();
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || "+-.".contains(c)))
                    .unwrap_or(rest.len());
                let word = &rest[..end];
                let is_number = word.starts_with(|c: char| c == '-' || c.is_ascii_digit());
                if !(is_number || matches!(word, "true" | "false" | "null")) {
                    return Err(self.problem("a value is missing or misspelt"));
                }
                self.at += end;
                Kind::Other
            }
        };
        Ok(Node {
            span: start..self.at,
            kind,
        })
    }

    fn object(&mut self, depth: usize) -> Result<Vec<Member>, String> {
        self.at += 1;
        let mut members = Vec::new();
        loop {
            self.skip_space_and_comments()?;
            match self.next() {
                Some('}') => break,
                Some('"') => {}
                _ => return Err(self.problem("a name in quotes is expected")),
            }
            let start = self.at;
            let name = self.string()?;
            self.skip_space_and_comments()?;
            if self.next() != Some(':') {
                return Err(self.problem("a colon is expected"));
            }
            self.at += 1;
            self.skip_space_and_comments()?;
            let value = self.value(depth + 1)?;
            members.push(Member { name, start, value });
            self.skip_space_and_comments()?;
            match self.next() {
                // A comma may also follow the last member.
                Some(',') => self.at += 1,
                Some('}') => break,
                _ => return Err(self.problem("a comma or a closing brace is expected")),
            }
        }
        self.at += 1;
        Ok(members)
    }

    fn list(&mut self, depth: usize) -> Result<Vec<Node>, String> {
        self.at += 1;
        let mut items = Vec::new();
        loop {
            self.skip_space_and_comments()?;
            if self.next() == Some(']') {
                break;
            }
            items.push(self.value(depth + 1)?);
            self.skip_space_and_comments()?;
            match self.next() {
                Some(',') => self.at += 1,
                Some(']') => break,
                _ => return Err(self.problem("a comma or a closing bracket is expected")),
            }
        }
        self.at += 1;
        Ok(items)
    }

    /// Reads a string in quotes and returns the text it stands for.
    fn string(&mut self) -> Result<String, String> {
        self.at += 1;
        let mut text = String::new();
        loop {
            let c = self
                .next()
                .ok_or_else(|| self.problem("a text is never closed"))?;
            self.at += c.len_utf8();
            match c {
                '"' => return Ok(text),
                '\n' | '\r' => return Err(self.problem("a text is never closed")),
                '\\' => {
                    let escaped = self
                        .next()
                        .ok_or_else(|| self.problem("a text is never closed"))?;
                    self.at += escaped.len_utf8();
                    text.push(match escaped {
                        '"' | '\\' | '/' => escaped,
                        'b' => '\u{8}',
                        'f' => '\u{c}',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        'u' => self.escaped_character()?,
                        _ => return Err(self.problem("a text has an unknown escape")),
                    });
                }
                c => text.push(c),
            }
        }
    }

    /// Reads what follows `\u`: four hex digits, or two such codes that
    /// together stand for one character.
    fn escaped_character(&mut self) -> Result<char, String> {
        let first = self.four_hex_digits()?;
        let code = if (0xd800..0xdc00).contains(&first) && self.rest().starts_with("\\u") {
            self.at += 2;
            let second = self.four_hex_digits()?;
            0x10000 + ((first - 0xd800) << 10) + second.wrapping_sub(0xdc00)
        } else {
            first
        };
        char::from_u32(code).ok_or_else(|| self.problem("a text has a broken escape"))
    }

    fn four_hex_digits(&mut self) -> Result<u32, String> {
        let digits = self
            .rest()
            .get(..4)
            .ok_or_else(|| self.problem("a text has a broken escape"))?;
        let code = u32::from_str_radix(digits, 16)
            .map_err(|_| self.problem("a text has a broken escape"))?;
        self.at += 4;
        Ok(code)
    }
}
