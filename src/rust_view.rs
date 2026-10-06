use crate::markdown::MdView;

#[cfg(feature = "lang-rust")]
use std::ops::Range;

#[cfg(feature = "lang-rust")]
use crate::markdown::{RenderedLine, Seg};
#[cfg(feature = "lang-rust")]
use crate::syntax::Role;
#[cfg(feature = "lang-rust")]
use crate::theme;

/// Rendered Rust with type annotations removed. Each source line stays one row,
/// and every kept character still points at its source column.
pub fn build(lines: &[String]) -> Option<MdView> {
    #[cfg(feature = "lang-rust")]
    {
        parsed(lines)
    }
    #[cfg(not(feature = "lang-rust"))]
    {
        let _ = lines;
        None
    }
}

#[cfg(feature = "lang-rust")]
fn parsed(lines: &[String]) -> Option<MdView> {
    use tree_sitter::Parser;

    let mut parser = Parser::new();
    parser.set_language(&crate::syntax::Lang::Rust.language()).ok()?;
    let source = join(lines);
    let tree = parser.parse(&source, None)?;
    let mut cuts = Vec::new();
    collect(tree.root_node(), &source, &mut cuts);
    let cuts = merge(cuts);
    let roles = crate::syntax::Highlighter::new(crate::syntax::Lang::Rust, lines)
        .map(|hl| hl.roles(0..lines.len()))
        .unwrap_or_default();
    let starts = line_starts(lines);
    let lines = lines
        .iter()
        .enumerate()
        .map(|(i, line)| paint(line, starts[i], &cuts, roles.get(i).map(Vec::as_slice).unwrap_or(&[])))
        .collect();
    Some(MdView { lines })
}

#[cfg(feature = "lang-rust")]
fn collect(node: tree_sitter::Node, source: &str, cuts: &mut Vec<Range<usize>>) {
    match node.kind() {
        "function_item" | "function_signature_item" | "closure_expression" => {
            if let Some(params) = node.child_by_field_name("type_parameters") {
                cuts.push(params.byte_range());
            }
            if let Some(ret) = node.child_by_field_name("return_type") {
                cuts.push(returned(source, ret.start_byte(), ret.end_byte()));
            }
            cut_where(node, source, cuts);
        }
        "struct_item" | "enum_item" | "union_item" | "trait_item" | "impl_item" => {
            if let Some(params) = node.child_by_field_name("type_parameters") {
                cuts.push(params.byte_range());
            }
            cut_where(node, source, cuts);
        }
        "parameter" | "let_declaration" | "const_item" | "static_item" | "field_declaration" => {
            if let Some(ty) = node.child_by_field_name("type") {
                cuts.push(annotation(source, ty.start_byte(), ty.end_byte()));
            }
        }
        "self_parameter" => {
            let text = &source[node.start_byte()..node.end_byte()];
            if text.as_bytes().contains(&b'&') {
                let mut i = 0;
                while let Some(child) = node.child(i) {
                    if child.kind() == "self" {
                        cuts.push(node.start_byte()..child.start_byte());
                        break;
                    }
                    i += 1;
                }
            }
        }
        "generic_function" => {
            if let (Some(func), Some(args)) = (node.child_by_field_name("function"), node.child_by_field_name("type_arguments")) {
                cuts.push(func.end_byte()..args.end_byte());
            }
        }
        "generic_type" => {
            if let (Some(ty), Some(args)) = (node.child_by_field_name("type"), node.child_by_field_name("type_arguments")) {
                if source[ty.end_byte()..args.start_byte()].contains("::") {
                    cuts.push(ty.end_byte()..args.end_byte());
                }
            }
        }
        "reference_expression" => {
            if let Some(value) = node.child_by_field_name("value") {
                cuts.push(node.start_byte()..value.start_byte());
            }
        }
        "integer_literal" | "float_literal" => {
            if let Some(range) = suffix_range(node.start_byte(), &source[node.byte_range()]) {
                cuts.push(range);
            }
        }
        _ => {}
    }
    let mut i = 0;
    while let Some(child) = node.child(i) {
        collect(child, source, cuts);
        i += 1;
    }
}

#[cfg(feature = "lang-rust")]
fn cut_where(node: tree_sitter::Node, source: &str, cuts: &mut Vec<Range<usize>>) {
    let mut i = 0;
    while let Some(child) = node.child(i) {
        if child.kind() == "where_clause" {
            cuts.push(leading_space(source, child.start_byte())..child.end_byte());
        }
        i += 1;
    }
}

#[cfg(feature = "lang-rust")]
fn annotation(source: &str, ty_start: usize, ty_end: usize) -> Range<usize> {
    let bytes = source.as_bytes();
    let mut at = ty_start;
    while at > 0 && bytes[at - 1].is_ascii_whitespace() {
        at -= 1;
    }
    if at > 0 && bytes[at - 1] == b':' && (at < 2 || bytes[at - 2] != b':') {
        at -= 1;
        return leading_space(source, at)..ty_end;
    }
    ty_start..ty_end
}

#[cfg(feature = "lang-rust")]
fn returned(source: &str, ty_start: usize, ty_end: usize) -> Range<usize> {
    let bytes = source.as_bytes();
    let mut at = ty_start;
    while at > 0 && bytes[at - 1].is_ascii_whitespace() {
        at -= 1;
    }
    if at >= 2 && &bytes[at - 2..at] == b"->" {
        at -= 2;
        return leading_space(source, at)..ty_end;
    }
    ty_start..ty_end
}

#[cfg(feature = "lang-rust")]
fn leading_space(source: &str, mut at: usize) -> usize {
    let bytes = source.as_bytes();
    while at > 0 && bytes[at - 1].is_ascii_whitespace() {
        at -= 1;
    }
    at
}

#[cfg(feature = "lang-rust")]
fn suffix_range(origin: usize, text: &str) -> Option<Range<usize>> {
    const SUFFIXES: &[&str] = &[
        "usize", "isize", "u128", "i128", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8", "f64", "f32",
    ];
    let suffix = SUFFIXES.iter().find(|suffix| text.ends_with(*suffix))?;
    let mut from = text.len() - suffix.len();
    let bytes = text.as_bytes();
    while from > 0 && bytes[from - 1] == b'_' {
        from -= 1;
    }
    (from > 0).then(|| origin + from..origin + text.len())
}

#[cfg(feature = "lang-rust")]
fn merge(mut cuts: Vec<Range<usize>>) -> Vec<Range<usize>> {
    cuts.retain(|range| range.start < range.end);
    cuts.sort_by_key(|range| (range.start, range.end));
    let mut out: Vec<Range<usize>> = Vec::new();
    for range in cuts {
        if let Some(last) = out.last_mut() {
            if range.start <= last.end {
                if range.end > last.end {
                    last.end = range.end;
                }
                continue;
            }
        }
        out.push(range);
    }
    out
}

#[cfg(feature = "lang-rust")]
fn paint(line: &str, origin: usize, cuts: &[Range<usize>], roles: &[(Range<usize>, Role)]) -> RenderedLine {
    let mut keeps = Vec::new();
    let mut byte = 0usize;
    let mut cut_at = 0usize;
    for (idx, ch) in line.chars().enumerate() {
        let abs = origin + byte;
        let len = ch.len_utf8();
        while cut_at < cuts.len() && cuts[cut_at].end <= abs {
            cut_at += 1;
        }
        if !(cut_at < cuts.len() && cuts[cut_at].start < abs + len) {
            keeps.push((idx, ch));
        }
        byte += len;
    }
    let mut segs = Vec::new();
    let mut i = 0;
    while i < keeps.len() {
        let (start, ch) = keeps[i];
        let role = role_at(roles, start);
        let mut text = String::new();
        text.push(ch);
        let mut end = start + 1;
        i += 1;
        while i < keeps.len() && keeps[i].0 == end && role_at(roles, keeps[i].0) == role {
            text.push(keeps[i].1);
            end = keeps[i].0 + 1;
            i += 1;
        }
        segs.push(Seg {
            text,
            style: theme::syntax(role),
            src: start..end,
            verbatim: true,
            link: None,
        });
    }
    RenderedLine { segs, ..RenderedLine::default() }
}

#[cfg(feature = "lang-rust")]
fn role_at(roles: &[(Range<usize>, Role)], idx: usize) -> Role {
    roles.iter().find(|(range, _)| range.contains(&idx)).map(|(_, role)| *role).unwrap_or(Role::Plain)
}

#[cfg(feature = "lang-rust")]
fn join(lines: &[String]) -> String {
    let mut source = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            source.push('\n');
        }
        source.push_str(line);
    }
    source
}

#[cfg(feature = "lang-rust")]
fn line_starts(lines: &[String]) -> Vec<usize> {
    let mut starts = Vec::with_capacity(lines.len());
    let mut n = 0;
    for (i, line) in lines.iter().enumerate() {
        starts.push(n);
        n += line.len();
        if i + 1 != lines.len() {
            n += 1;
        }
    }
    starts
}

#[cfg(all(test, feature = "lang-rust"))]
mod tests {
    use super::*;
    use crate::theme::pal;

    fn lines_of(text: &str) -> Vec<String> {
        text.split('\n').map(|line| line.to_string()).collect()
    }

    fn shown(text: &str) -> String {
        let lines = lines_of(text);
        let view = build(&lines).unwrap();
        let rendered: Vec<String> = view.lines.iter().map(|line| line.cells().iter().map(|cell| cell.0).collect()).collect();
        for (i, line) in view.lines.iter().enumerate() {
            let raw: Vec<char> = lines[i].chars().collect();
            let text: String = line.cells().iter().map(|cell| cell.0).collect();
            for (col, ch) in text.chars().enumerate() {
                if ch.is_alphanumeric() {
                    assert_eq!(raw[line.display_to_source(col, raw.len())], ch, "column {col} of {text:?}");
                }
            }
        }
        rendered.join("\n")
    }

    #[test]
    fn types_come_off_and_the_program_stays() {
        let tally = "\
use std::collections::HashMap;

fn tally<'a, I: Iterator<Item = &'a str>>(words: I) -> HashMap<&'a str, u32> {
    let mut counts: HashMap<&'a str, u32> = HashMap::new();
    for word in words {
        *counts.entry(word).or_insert(0u32) += 1;
    }
    counts
}

fn main() {
    let text: &'static str = \"beautiful code is motivating code\";
    let counts: HashMap<&str, u32> = tally(text.split_whitespace());
    let mut pairs: Vec<(&str, u32)> = counts.into_iter().collect::<Vec<(&str, u32)>>();
    pairs.sort_by(|a: &(&str, u32), b: &(&str, u32)| b.1.cmp(&a.1));
    println!(\"{:?}\", pairs);
}";
        let tally_plain = "\
use std::collections::HashMap;

fn tally(words) {
    let mut counts = HashMap::new();
    for word in words {
        *counts.entry(word).or_insert(0) += 1;
    }
    counts
}

fn main() {
    let text = \"beautiful code is motivating code\";
    let counts = tally(text.split_whitespace());
    let mut pairs = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1));
    println!(\"{:?}\", pairs);
}";
        let copy = "\
fn copy_flush<R: io::Read, W: io::Write>(reader: &mut R, writer: &mut W) -> io::Result<u64> {
    let mut buffer = [0_u8; 16 * 1024];
    let mut total = 0;

    loop {
        let bytes_read = match reader.read(&mut buffer) {
            Ok(0) => return Ok(total),
            Ok(bytes_read) => bytes_read,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };

        writer.write_all(&buffer[..bytes_read])?;
        writer.flush()?;
        total += bytes_read as u64;
    }
}";
        let copy_plain = "\
fn copy_flush(reader, writer) {
    let mut buffer = [0; 16 * 1024];
    let mut total = 0;

    loop {
        let bytes_read = match reader.read(buffer) {
            Ok(0) => return Ok(total),
            Ok(bytes_read) => bytes_read,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };

        writer.write_all(buffer[..bytes_read])?;
        writer.flush()?;
        total += bytes_read as u64;
    }
}";
        let cases = [
            (tally, tally_plain),
            (copy, copy_plain),
            (
                "#[cfg(unix)]\nfn copy_flush(reader: &mut u8) -> u8 { reader }",
                "#[cfg(unix)]\nfn copy_flush(reader) { reader }",
            ),
            (
                "fn main() {\n    let bits = 1 & 2;\n    let both = true && false;\n    let &x = &1;\n}",
                "fn main() {\n    let bits = 1 & 2;\n    let both = true && false;\n    let &x = 1;\n}",
            ),
            (
                "fn main() {\n    let s = \"fn f<T>(x: T) -> u32\";\n    // let x: i32 = 1;\n}",
                "fn main() {\n    let s = \"fn f<T>(x: T) -> u32\";\n    // let x: i32 = 1;\n}",
            ),
            (
                "fn main() {\n    let s: u32 = \"é\";\n    let v = Vec::<u8>::new();\n    let bytes = vec![0u8];\n}",
                "fn main() {\n    let s = \"é\";\n    let v = Vec::new();\n    let bytes = vec![0];\n}",
            ),
            (
                "const SCALE: u32 = 1_000u32;\n\nstruct Point<T> {\n    x: T,\n    y: T,\n}\n\ntrait Get {\n    fn get(&mut self, n: u32) -> u32;\n}",
                "const SCALE = 1_000;\n\nstruct Point {\n    x,\n    y,\n}\n\ntrait Get {\n    fn get(self, n);\n}",
            ),
            (
                "fn tally(\n    words: &'static str,\n) -> u32 {\n    1\n}",
                "fn tally(\n    words,\n) {\n    1\n}",
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(shown(source), expected, "source: {source}");
        }
    }

    #[test]
    fn kept_tokens_keep_their_colours() {
        let source = "fn main() { // note\n    let name: &'static str = \"code\";\n    let n: u32 = 0u32;\n}";
        let view = build(&lines_of(source)).unwrap();
        let cells = view.lines[0].cells();
        let shown: String = cells.iter().map(|cell| cell.0).collect();
        let note = shown.find("note").unwrap();
        let note_col = shown[..note].chars().count();
        assert_eq!(cells[0].1.fg, Some(pal().ice), "fn");
        assert_eq!(cells[note_col].1.fg, Some(pal().ghost), "comment");
        assert!(cells[note_col].1.add_modifier.contains(ratatui::style::Modifier::ITALIC));
        let value = view.lines[1].cells();
        let text: String = value.iter().map(|cell| cell.0).collect();
        let quote = text.find('"').unwrap();
        let quote_col = text[..quote].chars().count();
        assert_eq!(value[text.find('l').unwrap()].1.fg, Some(pal().ice), "let");
        assert_eq!(value[quote_col + 1].1.fg, Some(pal().amber), "string");
        assert!(!text.contains("static"), "{text}");
        let number = view.lines[2].cells();
        let digits: String = number.iter().map(|cell| cell.0).collect();
        let zero = digits.find('0').unwrap();
        assert_eq!(number[zero].1.fg, Some(pal().hot), "the suffix is gone and 0 stays a number: {digits}");
    }
}
