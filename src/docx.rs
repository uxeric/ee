use std::io::{self, Cursor, Read, Write};

use ratatui::style::{Modifier, Style};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::markdown::{self, MdView, RenderedLine, Seg};
use crate::theme::pal;

pub(crate) const TABLE: &str = "[table]";
pub(crate) const PICTURE: &str = "[picture]";
pub(crate) const BLOCK: &str = "[block]";

#[derive(Clone, Debug)]
pub(crate) enum Anchor {
    Block(u64),
    Fragment { id: u64, parent: String, start: usize, end: usize },
    Join(Vec<Anchor>),
    Fresh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prefix {
    None,
    Heading(u8),
    Bullet,
    Number,
    Quote,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Marks {
    bold: bool,
    italic: bool,
    strike: bool,
    link: Option<String>,
}

impl Marks {
    fn bare() -> Self {
        Self { bold: false, italic: false, strike: false, link: None }
    }
}

struct Glyph {
    ch: char,
    src: usize,
    marks: Marks,
}

#[derive(Clone)]
struct Run {
    raw: String,
    text: String,
    marks: Marks,
    rpr: String,
}

#[derive(Clone)]
enum Piece {
    Gap(String),
    Run(Run),
}

#[derive(Clone)]
struct Para {
    id: u64,
    lead: String,
    raw: String,
    open: String,
    ppr: String,
    pieces: Vec<Piece>,
    close: String,
    dialect: String,
    prefix: Prefix,
    complex: bool,
}

#[derive(Clone)]
enum Block {
    Para(Para),
    Frozen { id: u64, lead: String, raw: String, placeholder: &'static str },
}

impl Block {
    fn id(&self) -> u64 {
        match self {
            Block::Para(p) => p.id,
            Block::Frozen { id, .. } => *id,
        }
    }
}

pub(crate) struct Session {
    baseline: Vec<u8>,
    baseline_lines: Vec<String>,
    document_xml: String,
    doc_path: String,
    w: String,
    r: String,
    body_prefix: String,
    body_suffix: String,
    sect: String,
    blocks: Vec<Block>,
    anchors: Vec<Anchor>,
    anchored_lines: Vec<String>,
    parts: Vec<(String, Vec<u8>)>,
    rels_path: String,
    rels_xml: String,
    root_rels: String,
    links: Vec<(String, String)>,
    next_rid: u32,
    numbering_path: Option<String>,
    numbering_xml: Option<String>,
    content_types: String,
    bullet_id: Option<String>,
    number_id: Option<String>,
    num_kind: Vec<(String, bool)>,
    rev_before: u64,
    dirty_before: bool,
    next_id: u64,
    rels_dirty: bool,
    numbering_dirty: bool,
    types_dirty: bool,
}

pub(crate) fn is_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".docx")
}

pub(crate) fn load(bytes: &[u8]) -> io::Result<(Vec<String>, Session)> {
    let mut files = read_zip(bytes)?;
    let types = take(&mut files, "[Content_Types].xml").unwrap_or_default();
    let root = take(&mut files, "_rels/.rels").unwrap_or_default();
    let doc_path = office_path(&root).unwrap_or_else(|| "word/document.xml".to_string());
    let document_xml = take(&mut files, &doc_path).ok_or_else(|| invalid("this docx has no document"))?;
    let rels_path = rels_for(&doc_path);
    let rels_xml = take(&mut files, &rels_path).unwrap_or_default();
    let (numbering_path, numbering_xml) = numbering_part(&rels_xml, &rels_path, &mut files);
    let w = prefix_for(&document_xml, "http://schemas.openxmlformats.org/wordprocessingml/2006/main").unwrap_or_else(|| "w".to_string());
    let r = prefix_for(&document_xml, "http://schemas.openxmlformats.org/officeDocument/2006/relationships")
        .or_else(|| prefix_for(&rels_xml, "http://schemas.openxmlformats.org/package/2006/relationships"))
        .unwrap_or_else(|| "r".to_string());
    let (body_prefix, inner, body_suffix) = split_body(&document_xml, &w)?;
    let links = hyperlinks(&rels_xml);
    let next_rid = next_rel_id(&rels_xml);
    let (num_kind, bullet_id, number_id) = list_ids(numbering_xml.as_deref().unwrap_or(""), &w);
    let tops = split_top(&inner);
    let mut sect = String::new();
    let mut children = tops;
    if let Some(Top::Elem { name, raw, lead }) = children.last().cloned() {
        if local_is(&name, "sectPr") {
            sect = format!("{lead}{raw}");
            children.pop();
        }
    }
    let mut blocks = Vec::new();
    let mut next_id = 1u64;
    for top in children {
        match top {
            Top::Gap(g) => {
                if let Some(block) = blocks.last_mut() {
                    match block {
                        Block::Para(p) => p.lead.push_str(&g),
                        Block::Frozen { lead, .. } => lead.push_str(&g),
                    }
                } else {
                    sect = format!("{g}{sect}");
                }
            }
            Top::Elem { name, raw, lead } => {
                let id = next_id;
                next_id += 1;
                blocks.push(classify(id, lead, raw, &name, &w, &r, &links, &num_kind));
            }
        }
    }
    let mut lines: Vec<String> = blocks.iter().map(line_of).collect();
    let mut anchors: Vec<Anchor> = blocks.iter().map(|b| Anchor::Block(b.id())).collect();
    if lines.is_empty() {
        lines.push(String::new());
        anchors.push(Anchor::Fresh);
    }
    let session = Session {
        baseline: bytes.to_vec(),
        baseline_lines: lines.clone(),
        document_xml,
        doc_path,
        w,
        r,
        body_prefix,
        body_suffix,
        sect,
        blocks,
        anchors,
        anchored_lines: lines.clone(),
        parts: files,
        rels_path,
        rels_xml,
        root_rels: root,
        links,
        next_rid,
        numbering_path,
        numbering_xml,
        content_types: types,
        bullet_id,
        number_id,
        num_kind,
        rev_before: 0,
        dirty_before: false,
        next_id,
        rels_dirty: false,
        numbering_dirty: false,
        types_dirty: false,
    };
    Ok((lines, session))
}

pub(crate) fn blank() -> (Vec<String>, Session) {
    let bytes = pack_parts(&blank_parts(&[""]));
    load(&bytes).expect("blank docx")
}

fn line_of(block: &Block) -> String {
    match block {
        Block::Para(p) => p.dialect.clone(),
        Block::Frozen { placeholder, .. } => (*placeholder).to_string(),
    }
}

fn classify(id: u64, lead: String, raw: String, name: &str, w: &str, r: &str, links: &[(String, String)], kinds: &[(String, bool)]) -> Block {
    if local_is(name, "tbl") {
        return Block::Frozen { id, lead, raw, placeholder: TABLE };
    }
    if !local_is(name, "p") {
        return Block::Frozen { id, lead, raw, placeholder: BLOCK };
    }
    if is_picture(&raw) {
        return Block::Frozen { id, lead, raw, placeholder: PICTURE };
    }
    Block::Para(parse_para(id, lead, raw, w, r, links, kinds))
}

fn is_picture(raw: &str) -> bool {
    (has_local(raw, "drawing") || has_local(raw, "pict") || has_local(raw, "object")) && !has_local(raw, "t")
}

fn parse_para(id: u64, lead: String, raw: String, w: &str, r: &str, links: &[(String, String)], kinds: &[(String, bool)]) -> Para {
    let (open, inner, close) = unwrap(&raw);
    let mut ppr = String::new();
    let mut pieces = Vec::new();
    let mut seen_ppr = false;
    for top in split_top(&inner) {
        match top {
            Top::Gap(g) => pieces.push(Piece::Gap(g)),
            Top::Elem { name, raw: eraw, lead: elead } => {
                if !elead.is_empty() {
                    pieces.push(Piece::Gap(elead));
                }
                if local_is(&name, "pPr") && !seen_ppr {
                    ppr = eraw;
                    seen_ppr = true;
                } else if local_is(&name, "r") || local_is(&name, "hyperlink") {
                    pieces.push(Piece::Run(parse_run(eraw, w, r, links)));
                } else {
                    pieces.push(Piece::Gap(eraw));
                }
            }
        }
    }
    let complex = is_complex(&raw);
    let prefix = prefix_of(&ppr, w, kinds);
    let runs = run_refs(&pieces);
    let dialect = encode(prefix, &runs);
    Para { id, lead, raw, open, ppr, pieces, close, dialect, prefix, complex }
}

fn run_refs(pieces: &[Piece]) -> Vec<Run> {
    pieces.iter().filter_map(|p| match p { Piece::Run(r) => Some(r.clone()), _ => None }).collect()
}

fn parse_run(raw: String, w: &str, r: &str, links: &[(String, String)]) -> Run {
    let link = if local_is(&elem_name(&raw), "hyperlink") { link_target(&raw, r, links) } else { None };
    let rpr = extract_rpr(&raw, w);
    let text = collect_text(&raw, w);
    let mut marks = marks_of(&rpr, w);
    marks.link = link;
    Run { raw, text, marks, rpr }
}

fn prefix_of(ppr: &str, w: &str, kinds: &[(String, bool)]) -> Prefix {
    if let Some(style) = attr_of_local(ppr, w, "pStyle", "val") {
        if let Some(level) = heading_level(&style) {
            return Prefix::Heading(level);
        }
        if style.to_ascii_lowercase().contains("quote") {
            return Prefix::Quote;
        }
    }
    if let Some(level) = attr_of_local(ppr, w, "outlineLvl", "val").and_then(|v| v.parse::<u8>().ok()) {
        if level < 6 {
            return Prefix::Heading(level + 1);
        }
    }
    if let Some(id) = attr_of_local(ppr, w, "numId", "val") {
        if id != "0" {
            let bullet = kinds.iter().find(|(n, _)| n == &id).map(|(_, b)| *b).unwrap_or(true);
            return if bullet { Prefix::Bullet } else { Prefix::Number };
        }
    }
    Prefix::None
}

fn heading_level(style: &str) -> Option<u8> {
    let s = style.replace(' ', "").to_ascii_lowercase();
    let rest = s.strip_prefix("heading").or_else(|| s.strip_prefix('h'))?;
    let n: u8 = rest.parse().ok()?;
    (1..=6).contains(&n).then_some(n)
}

fn attr_of_local(xml: &str, w: &str, local: &str, attr: &str) -> Option<String> {
    let needle = format!("<{w}:{local}");
    let alt = format!("<{local}");
    let at = xml.find(&needle).or_else(|| xml.find(&alt))?;
    let tag_end = quote_end(&xml[at..])? + at;
    attr_get(&xml[at..tag_end], &format!("{w}:{attr}")).or_else(|| attr_get(&xml[at..tag_end], attr))
}

pub(crate) fn render(lines: &[String]) -> MdView {
    MdView { lines: lines.iter().map(|line| render_line(line)).collect() }
}

fn render_line(line: &str) -> RenderedLine {
    if line == TABLE {
        return bar("table", line.chars().count());
    }
    if line == PICTURE {
        return bar("picture", line.chars().count());
    }
    if line == BLOCK {
        return bar("block", line.chars().count());
    }
    let parsed = parse(line);
    let mut rl = RenderedLine::default();
    match parsed.prefix {
        Prefix::Heading(_level) => {
            let text: String = parsed.glyphs.iter().map(|g| g.ch).collect();
            rl.anchor = Some(markdown::slug(&text));
        }
        Prefix::Bullet => rl.segs.push(seg("• ", 0..2, Style::default().fg(pal().hot), false, None)),
        Prefix::Number => {
            let prefix: String = line.chars().take_while(|c| *c != ' ').collect::<String>() + " ";
            let end = prefix.chars().count();
            rl.segs.push(seg(&prefix, 0..end, Style::default().fg(pal().amber), false, None));
        }
        Prefix::Quote => rl.segs.push(seg("▌ ", 0..2, Style::default().fg(pal().hot), false, None)),
        Prefix::None => {}
    }
    let mut batch: Vec<&Glyph> = Vec::new();
    let flush = |rl: &mut RenderedLine, batch: &mut Vec<&Glyph>, prefix: Prefix| {
        if batch.is_empty() {
            return;
        }
        let text: String = batch.iter().map(|g| g.ch).collect();
        let start = batch[0].src;
        let end = batch.last().unwrap().src + 1;
        let contiguous = batch.windows(2).all(|w| w[1].src == w[0].src + 1) && end - start == batch.len();
        let marks = &batch[0].marks;
        let mut style = if let Prefix::Heading(level) = prefix { heading_style(level) } else { Style::default() };
        if marks.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if marks.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if marks.strike {
            style = style.add_modifier(Modifier::CROSSED_OUT);
        }
        if marks.link.is_some() {
            style = style.fg(pal().ice).add_modifier(Modifier::UNDERLINED);
        }
        if contiguous {
            rl.segs.push(seg(&text, start..end, style, true, marks.link.clone()));
        } else {
            for g in batch.iter() {
                rl.segs.push(seg(&g.ch.to_string(), g.src..g.src + 1, style.clone(), true, g.marks.link.clone()));
            }
        }
        batch.clear();
    };
    for g in &parsed.glyphs {
        if batch.last().is_some_and(|p: &&Glyph| p.marks != g.marks) {
            flush(&mut rl, &mut batch, parsed.prefix);
        }
        batch.push(g);
    }
    flush(&mut rl, &mut batch, parsed.prefix);
    rl
}

fn seg(text: &str, src: std::ops::Range<usize>, style: Style, verbatim: bool, link: Option<String>) -> Seg {
    Seg { text: text.to_string(), style, src, verbatim, link }
}

fn bar(label: &str, n: usize) -> RenderedLine {
    let style = Style::default().fg(pal().ghost);
    RenderedLine {
        segs: vec![seg(&format!("── {label} "), 0..n.max(1), style, false, None)],
        fill: Some(('─', style)),
        ..RenderedLine::default()
    }
}

fn heading_style(level: u8) -> Style {
    let s = Style::default().add_modifier(Modifier::BOLD);
    match level {
        1 => s.fg(pal().hot).add_modifier(Modifier::UNDERLINED),
        2 => s.fg(pal().amber),
        3 => s.fg(pal().ice),
        _ => s.fg(pal().text),
    }
}

fn parse(line: &str) -> Parsed {
    let chars: Vec<char> = line.chars().collect();
    let (prefix, start) = strip_prefix(&chars);
    let glyphs = parse_range(&chars, start, chars.len(), &Marks::bare());
    Parsed { prefix, glyphs }
}

pub(crate) fn content_index(line: &str, col: usize) -> usize {
    parse(line).glyphs.iter().filter(|g| g.src < col).count()
}

pub(crate) fn column_at_content(line: &str, index: usize) -> usize {
    match parse(line).glyphs.get(index) {
        Some(g) => g.src,
        None => line.chars().count(),
    }
}

struct Parsed {
    prefix: Prefix,
    glyphs: Vec<Glyph>,
}

fn strip_prefix(chars: &[char]) -> (Prefix, usize) {
    if chars.first() == Some(&'\\') {
        return (Prefix::None, 0);
    }
    let hashes = chars.iter().take_while(|c| **c == '#').count();
    if (1..=6).contains(&hashes) && chars.get(hashes) == Some(&' ') {
        return (Prefix::Heading(hashes as u8), hashes + 1);
    }
    if chars.len() >= 2 && chars[0] == '-' && chars[1] == ' ' {
        return (Prefix::Bullet, 2);
    }
    let mut i = 0;
    while chars.get(i).is_some_and(|c| c.is_ascii_digit()) {
        i += 1;
    }
    if i > 0 && chars.get(i) == Some(&'.') && chars.get(i + 1) == Some(&' ') {
        return (Prefix::Number, i + 2);
    }
    if chars.len() >= 2 && chars[0] == '>' && chars[1] == ' ' {
        return (Prefix::Quote, 2);
    }
    (Prefix::None, 0)
}

fn parse_range(chars: &[char], mut i: usize, end: usize, marks: &Marks) -> Vec<Glyph> {
    let mut out = Vec::new();
    while i < end {
        if chars[i] == '\\' && i + 1 < end {
            out.push(Glyph { ch: chars[i + 1], src: i + 1, marks: marks.clone() });
            i += 2;
            continue;
        }
        if chars[i] == '[' {
            if let Some((glyphs, next)) = parse_link(chars, i, end, marks) {
                out.extend(glyphs);
                i = next;
                continue;
            }
        }
        if i + 1 < end && chars[i] == '~' && chars[i + 1] == '~' {
            if let Some(close) = find_token(chars, i + 2, end, &['~', '~']) {
                let mut inner = marks.clone();
                inner.strike = true;
                out.extend(parse_range(chars, i + 2, close, &inner));
                i = close + 2;
                continue;
            }
        }
        let stars = run_of(chars, i, end, '*');
        if let Some((n, close)) = match_stars(chars, i, end, stars) {
            let mut inner = marks.clone();
            if n >= 2 {
                inner.bold = true;
            }
            if n % 2 == 1 {
                inner.italic = true;
            }
            out.extend(parse_range(chars, i + n, close, &inner));
            i = close + n;
            continue;
        }
        out.push(Glyph { ch: chars[i], src: i, marks: marks.clone() });
        i += 1;
    }
    out
}

fn parse_link(chars: &[char], i: usize, end: usize, marks: &Marks) -> Option<(Vec<Glyph>, usize)> {
    let bracket = find_char(chars, i + 1, end, ']')?;
    if chars.get(bracket + 1) != Some(&'(') {
        return None;
    }
    let paren = find_char(chars, bracket + 2, end, ')')?;
    let url: String = chars[bracket + 2..paren].iter().collect();
    let mut inner = marks.clone();
    inner.link = Some(percent_decode(&url));
    let glyphs = parse_range(chars, i + 1, bracket, &inner);
    Some((glyphs, paren + 1))
}

fn match_stars(chars: &[char], i: usize, end: usize, stars: usize) -> Option<(usize, usize)> {
    for n in [3usize, 2, 1] {
        if stars >= n {
            let token = vec!['*'; n];
            if let Some(close) = find_token(chars, i + n, end, &token) {
                return Some((n, close));
            }
        }
    }
    None
}

fn run_of(chars: &[char], i: usize, end: usize, c: char) -> usize {
    chars[i..end].iter().take_while(|ch| **ch == c).count()
}

fn find_token(chars: &[char], mut i: usize, end: usize, token: &[char]) -> Option<usize> {
    while i + token.len() <= end {
        if chars[i] == '\\' && i + 1 < end {
            i += 2;
            continue;
        }
        if chars[i..i + token.len()] == *token {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_char(chars: &[char], mut i: usize, end: usize, want: char) -> Option<usize> {
    while i < end {
        if chars[i] == '\\' && i + 1 < end {
            i += 2;
            continue;
        }
        if chars[i] == want {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn encode(prefix: Prefix, runs: &[Run]) -> String {
    let mut s = match prefix {
        Prefix::None => String::new(),
        Prefix::Heading(n) => format!("{} ", "#".repeat(n as usize)),
        Prefix::Bullet => "- ".to_string(),
        Prefix::Number => "1. ".to_string(),
        Prefix::Quote => "> ".to_string(),
    };
    let mut i = 0;
    while i < runs.len() {
        let marks = &runs[i].marks;
        let mut j = i + 1;
        while j < runs.len() && runs[j].marks == *marks {
            j += 1;
        }
        let text: String = runs[i..j].iter().map(|r| escape_text(&r.text)).collect();
        if !text.is_empty() {
            s.push_str(&wrap(marks, &text));
        }
        i = j;
    }
    s
}

fn wrap(marks: &Marks, text: &str) -> String {
    let mut s = text.to_string();
    if marks.italic {
        s = format!("*{s}*");
    }
    if marks.bold {
        s = format!("**{s}**");
    }
    if marks.strike {
        s = format!("~~{s}~~");
    }
    if let Some(url) = &marks.link {
        s = format!("[{s}]({})", percent_encode(url));
    }
    s
}

fn escape_text(text: &str) -> String {
    let mut s = String::new();
    for c in text.chars() {
        if matches!(c, '\\' | '*' | '~' | '[') {
            s.push('\\');
        }
        s.push(c);
    }
    s
}

fn percent_encode(url: &str) -> String {
    let mut s = String::new();
    for c in url.chars() {
        match c {
            '%' => s.push_str("%25"),
            ')' => s.push_str("%29"),
            ' ' => s.push_str("%20"),
            '\\' => s.push_str("%5C"),
            _ => s.push(c),
        }
    }
    s
}

fn percent_decode(url: &str) -> String {
    let b = url.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&url[i + 1..i + 3], 16) {
                out.push(v as char);
                i += 3;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

impl Session {
    pub(crate) fn note_edit(&mut self, rev: u64, dirty: bool) {
        self.rev_before = rev;
        self.dirty_before = dirty;
    }

    pub(crate) fn rollback_point(&self) -> (u64, bool) {
        (self.rev_before, self.dirty_before)
    }

    pub(crate) fn anchors(&self) -> Vec<Anchor> {
        self.anchors.clone()
    }

    pub(crate) fn restore(&mut self, anchors: Vec<Anchor>, lines: &[String]) {
        self.anchors = anchors;
        self.anchored_lines = lines.to_vec();
    }

    pub(crate) fn realign(&mut self, lines: &[String]) {
        if self.anchored_lines == lines {
            return;
        }
        self.anchors = align_anchors(&self.anchors, &self.anchored_lines, lines);
        self.anchored_lines = lines.to_vec();
    }

    /// Snap frozen lines back, and balance a line split so each half is dialect again.
    pub(crate) fn settle(&mut self, lines: &mut Vec<String>) -> Option<String> {
        self.realign(lines);
        let mut notes = Vec::new();
        for (i, line) in lines.iter_mut().enumerate() {
            if let Some(placeholder) = self.frozen_placeholder(i) {
                if line != placeholder {
                    *line = placeholder.to_string();
                    let note = match placeholder {
                        TABLE => "left a table unchanged",
                        PICTURE => "left a picture unchanged",
                        _ => "left a block unchanged",
                    };
                    if !notes.contains(&note) {
                        notes.push(note);
                    }
                }
            }
        }
        for (i, line) in lines.iter().enumerate() {
            let Anchor::Block(id) = self.anchors.get(i).cloned().unwrap_or(Anchor::Fresh) else {
                continue;
            };
            if let Some(Block::Para(p)) = self.blocks.iter().find(|b| b.id() == id) {
                if p.complex && line != &p.dialect {
                    let note = "rewrote a paragraph that contained Word features ee doesn't edit";
                    if !notes.contains(&note) {
                        notes.push(note);
                    }
                }
            }
        }
        if lines != &self.anchored_lines {
            self.realign(lines);
        }
        self.canonicalize(lines);
        self.anchored_lines = lines.clone();
        if notes.is_empty() { None } else { Some(notes.join("; ")) }
    }

    fn frozen_placeholder(&self, index: usize) -> Option<&'static str> {
        let Anchor::Block(id) = self.anchors.get(index)? else {
            return None;
        };
        match self.blocks.iter().find(|b| b.id() == *id)? {
            Block::Frozen { placeholder, .. } => Some(*placeholder),
            _ => None,
        }
    }

    fn canonicalize(&mut self, lines: &mut [String]) {
        let mut i = 0;
        while i < lines.len() {
            let Anchor::Fragment { id, parent, start, end } = self.anchors.get(i).cloned().unwrap_or(Anchor::Fresh) else {
                i += 1;
                continue;
            };
            let mut group = vec![(i, start, end)];
            let mut j = i + 1;
            while j < lines.len() {
                if let Anchor::Fragment { id: id2, parent: p2, start, end } = &self.anchors[j] {
                    if *id2 == id && p2 == &parent {
                        group.push((j, *start, *end));
                        j += 1;
                        continue;
                    }
                }
                break;
            }
            if let Some(base) = self.base_para(id, &parent) {
                for (idx, start, end) in group {
                    let slice = slice_para(&base, start, end, &self.w);
                    if lines[idx] != slice.dialect {
                        lines[idx] = slice.dialect;
                    }
                }
            }
            i = j;
        }
    }

    fn base_para(&mut self, id: u64, parent: &str) -> Option<Para> {
        let para = match self.blocks.iter().find(|b| b.id() == id)? {
            Block::Para(p) => p.clone(),
            _ => return None,
        };
        if para.complex {
            return None;
        }
        if para.dialect == parent {
            return Some(para);
        }
        let xml = apply_edit(&para, parent, self);
        let mut updated = parse_para(para.id, String::new(), xml, &self.w.clone(), &self.r.clone(), &self.links.clone(), &self.num_kind.clone());
        updated.lead = para.lead.clone();
        Some(updated)
    }

    pub(crate) fn save(&mut self, lines: &mut Vec<String>) -> io::Result<(Vec<u8>, Option<String>)> {
        self.settle(lines);
        if lines == &self.baseline_lines && !self.rels_dirty && !self.numbering_dirty && !self.types_dirty {
            return Ok((self.baseline.clone(), None));
        }
        let mut note = None;
        let built = self.build(lines, &mut note);
        if built == self.document_xml && !self.rels_dirty && !self.numbering_dirty && !self.types_dirty {
            self.baseline_lines = lines.clone();
            return Ok((self.baseline.clone(), note));
        }
        let bytes = self.pack(built.as_bytes())?;
        self.adopt_saved(&built, lines);
        self.document_xml = built;
        self.baseline = bytes.clone();
        self.baseline_lines = lines.clone();
        self.rels_dirty = false;
        self.numbering_dirty = false;
        self.types_dirty = false;
        Ok((bytes, note))
    }

    fn build(&mut self, lines: &[String], note: &mut Option<String>) -> String {
        let mut body = String::new();
        let mut i = 0;
        while i < lines.len() {
            match self.anchors.get(i).cloned().unwrap_or(Anchor::Fresh) {
                Anchor::Fragment { id, .. } => {
                    let mut j = i + 1;
                    while j < lines.len() {
                        if let Anchor::Fragment { id: id2, .. } = &self.anchors[j] {
                            if *id2 == id {
                                j += 1;
                                continue;
                            }
                        }
                        break;
                    }
                    body.push_str(&self.emit_fragments(id, i, &lines[i..j], note));
                    i = j;
                }
                Anchor::Join(parts) => {
                    body.push_str(&self.emit_join(&parts, &lines[i], note));
                    i += 1;
                }
                Anchor::Block(id) => {
                    body.push_str(&self.emit_block(id, &lines[i], note));
                    i += 1;
                }
                Anchor::Fresh => {
                    body.push_str(&self.emit_fresh(&lines[i], note));
                    i += 1;
                }
            }
        }
        format!("{}{}{}{}", self.body_prefix, body, self.sect, self.body_suffix)
    }

    fn emit_block(&mut self, id: u64, line: &str, note: &mut Option<String>) -> String {
        let Some(index) = self.blocks.iter().position(|b| b.id() == id) else {
            return self.emit_fresh(line, note);
        };
        match &self.blocks[index] {
            Block::Frozen { lead, raw, placeholder, .. } => {
                if line != *placeholder {
                    push_note(note, match *placeholder {
                        TABLE => "left a table unchanged",
                        PICTURE => "left a picture unchanged",
                        _ => "left a block unchanged",
                    });
                }
                format!("{lead}{raw}")
            }
            Block::Para(para) => {
                let para = para.clone();
                if line == para.dialect {
                    return format!("{}{}", para.lead, para.raw);
                }
                if para.complex {
                    push_note(note, "rewrote a paragraph that contained Word features ee doesn't edit");
                    let xml = fresh_xml(line, &para.lead, self);
                    return xml;
                }
                let xml = apply_edit(&para, line, self);
                format!("{}{xml}", para.lead)
            }
        }
    }

    fn emit_fresh(&mut self, line: &str, _note: &mut Option<String>) -> String {
        fresh_xml(line, "\n", self)
    }

    fn emit_fragments(&mut self, id: u64, start_index: usize, lines: &[String], note: &mut Option<String>) -> String {
        let Some(para) = self.blocks.iter().find(|b| b.id() == id).and_then(|b| match b {
            Block::Para(p) => Some(p.clone()),
            _ => None,
        }) else {
            return lines.iter().map(|l| self.emit_fresh(l, note)).collect();
        };
        let mut ranges = Vec::new();
        for (offset, _) in lines.iter().enumerate() {
            if let Anchor::Fragment { start, end, parent, .. } = &self.anchors[start_index + offset] {
                ranges.push((*start, *end, parent.clone()));
            }
        }
        if ranges.len() != lines.len() {
            return lines.iter().map(|l| fresh_xml(l, &para.lead, self)).collect();
        }
        if para.complex {
            push_note(note, "rewrote a paragraph that contained Word features ee doesn't edit");
            return lines.iter().map(|l| fresh_xml(l, &para.lead, self)).collect();
        }
        let parent = ranges[0].2.clone();
        let base = if para.dialect == parent {
            para.clone()
        } else {
            let xml = apply_edit(&para, &parent, self);
            parse_para(para.id, para.lead.clone(), xml, &self.w.clone(), &self.r.clone(), &self.links.clone(), &self.num_kind.clone())
        };
        let mut out = String::new();
        for (n, line) in lines.iter().enumerate() {
            let (start, end, _) = &ranges[n];
            let mut slice = slice_para(&base, *start, *end, &self.w);
            let lead = if n == 0 { base.lead.clone() } else { "\n".to_string() };
            if line == &slice.dialect {
                out.push_str(&lead);
                out.push_str(&slice_xml(&slice));
            } else {
                let xml = apply_edit(&slice, line, self);
                out.push_str(&lead);
                out.push_str(&xml);
                slice.dialect = line.clone();
            }
        }
        out
    }

    fn emit_join(&mut self, parts: &[Anchor], line: &str, note: &mut Option<String>) -> String {
        if let Some(xml) = self.restore_join(parts, line) {
            return xml;
        }
        let mut paras = Vec::new();
        for part in parts {
            match part {
                Anchor::Block(id) | Anchor::Fragment { id, .. } => {
                    if let Some(Block::Para(p)) = self.blocks.iter().find(|b| b.id() == *id) {
                        paras.push(p.clone());
                    }
                }
                Anchor::Join(inner) => {
                    let _ = inner;
                }
                Anchor::Fresh => {}
            }
        }
        if paras.is_empty() {
            return self.emit_fresh(line, note);
        }
        if paras.iter().any(|p| p.complex) {
            push_note(note, "rewrote a paragraph that contained Word features ee doesn't edit");
            return fresh_xml(line, &paras[0].lead, self);
        }
        let mut merged = paras[0].clone();
        for extra in &paras[1..] {
            if let Some(Piece::Run(_)) = extra.pieces.iter().find(|p| matches!(p, Piece::Run(_))) {
                merged.pieces.extend(extra.pieces.clone());
            } else {
                merged.pieces.extend(extra.pieces.clone());
            }
        }
        merged.dialect = encode(merged.prefix, &run_refs(&merged.pieces));
        merged.raw.clear();
        if line == merged.dialect {
            return format!("{}{}", merged.lead, assemble(&merged));
        }
        format!("{}{}", merged.lead, apply_edit(&merged, line, self))
    }

    fn restore_join(&mut self, parts: &[Anchor], line: &str) -> Option<String> {
        let mut frags = Vec::new();
        for part in parts {
            let Anchor::Fragment { id, parent, start, end } = part else {
                return None;
            };
            frags.push((*id, parent.clone(), *start, *end));
        }
        let id = frags[0].0;
        let parent = frags[0].1.clone();
        if frags.iter().any(|f| f.0 != id || f.1 != parent) {
            return None;
        }
        frags.sort_by_key(|f| f.2);
        if frags[0].2 != 0 {
            return None;
        }
        let mut at = 0;
        for f in &frags {
            if f.2 != at {
                return None;
            }
            at = f.3;
        }
        if at != parent.chars().count() {
            return None;
        }
        let para = match self.blocks.iter().find(|b| b.id() == id)? {
            Block::Para(p) => p.clone(),
            _ => return None,
        };
        if para.complex || para.dialect != parent {
            return None;
        }
        let mut concat = String::new();
        for f in &frags {
            concat.push_str(&slice_para(&para, f.2, f.3, &self.w).dialect);
        }
        (concat == line).then(|| format!("{}{}", para.lead, para.raw))
    }

    fn pack(&self, document: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in &self.parts {
            write_part(&mut out, name, bytes, opts)?;
        }
        write_part(&mut out, &self.doc_path, document, opts)?;
        if !self.root_rels.is_empty() {
            write_part(&mut out, "_rels/.rels", self.root_rels.as_bytes(), opts)?;
        }
        if !self.rels_xml.is_empty() {
            write_part(&mut out, &self.rels_path, self.rels_xml.as_bytes(), opts)?;
        }
        if let (Some(path), Some(xml)) = (&self.numbering_path, &self.numbering_xml) {
            write_part(&mut out, path, xml.as_bytes(), opts)?;
        }
        if !self.content_types.is_empty() {
            write_part(&mut out, "[Content_Types].xml", self.content_types.as_bytes(), opts)?;
        }
        Ok(out.finish().map_err(zerr)?.into_inner())
    }

    fn adopt_saved(&mut self, built: &str, lines: &[String]) {
        let Ok((_, inner, _)) = split_body(built, &self.w) else {
            return;
        };
        let mut elems = Vec::new();
        for top in split_top(&inner) {
            if let Top::Elem { name, raw, lead } = top {
                if !local_is(&name, "sectPr") {
                    elems.push((lead, name, raw));
                }
            }
        }
        if elems.len() != lines.len() {
            return;
        }
        let old = self.anchors.clone();
        let mut new_anchors = Vec::new();
        for (i, (lead, name, raw)) in elems.into_iter().enumerate() {
            let id = match old.get(i).cloned().unwrap_or(Anchor::Fresh) {
                Anchor::Block(id) => {
                    self.refresh_block(id, lead, &name, raw);
                    id
                }
                Anchor::Join(parts) => self.join_reused(&parts, &raw).unwrap_or_else(|| self.push_block(lead, &name, raw)),
                _ => self.push_block(lead, &name, raw),
            };
            new_anchors.push(Anchor::Block(id));
        }
        self.anchors = new_anchors;
        self.anchored_lines = lines.to_vec();
    }

    fn refresh_block(&mut self, id: u64, lead: String, name: &str, raw: String) {
        let w = self.w.clone();
        let r = self.r.clone();
        let links = self.links.clone();
        let kinds = self.num_kind.clone();
        let Some(block) = self.blocks.iter_mut().find(|b| b.id() == id) else {
            self.blocks.push(classify(id, lead, raw, name, &w, &r, &links, &kinds));
            return;
        };
        match block {
            Block::Frozen { .. } => {}
            Block::Para(para) if para.raw == raw => {}
            Block::Para(para) => *para = parse_para(id, lead, raw, &w, &r, &links, &kinds),
        }
    }

    fn push_block(&mut self, lead: String, name: &str, raw: String) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let w = self.w.clone();
        let r = self.r.clone();
        let links = self.links.clone();
        let kinds = self.num_kind.clone();
        self.blocks.push(classify(id, lead, raw, name, &w, &r, &links, &kinds));
        id
    }

    fn join_reused(&self, parts: &[Anchor], raw: &str) -> Option<u64> {
        for part in parts {
            let id = match part {
                Anchor::Block(id) | Anchor::Fragment { id, .. } => *id,
                _ => continue,
            };
            let same = self.blocks.iter().find(|b| b.id() == id).is_some_and(|b| match b {
                Block::Para(p) => p.raw == raw,
                Block::Frozen { raw: frozen, .. } => frozen == raw,
            });
            if same {
                return Some(id);
            }
        }
        None
    }
}

fn push_note(slot: &mut Option<String>, note: &str) {
    match slot {
        Some(s) if s.contains(note) => {}
        Some(s) => {
            s.push_str("; ");
            s.push_str(note);
        }
        None => *slot = Some(note.to_string()),
    }
}

fn apply_edit(para: &Para, line: &str, session: &mut Session) -> String {
    let parsed = parse(line);
    let new_runs = glyphs_to_runs(&parsed.glyphs);
    let old = run_refs(&para.pieces);
    let w = session.w.clone();
    let r = session.r.clone();
    let mut ppr = para.ppr.clone();
    if parsed.prefix != para.prefix {
        ppr = rewrite_ppr(&ppr, parsed.prefix, &w, session);
    }
    if old.len() == new_runs.len() && old.iter().zip(&new_runs).all(|(a, b)| a.marks == b.0) {
        let mut pieces = para.pieces.clone();
        let mut n = 0;
        for piece in &mut pieces {
            if let Piece::Run(run) = piece {
                if run.text != new_runs[n].1 {
                    run.raw = replace_text(&run.raw, &new_runs[n].1, &w);
                    run.text = new_runs[n].1.clone();
                }
                n += 1;
            }
        }
        let updated = Para { ppr, pieces, ..para.clone() };
        return assemble(&updated);
    }
    let old_atoms = atoms_of(&old);
    let new_atoms = new_runs
        .iter()
        .flat_map(|(marks, text)| text.chars().map(|ch| Atom { ch, marks: marks.clone(), run: None, rpr: String::new() }))
        .collect::<Vec<_>>();
    let pre = old_atoms.iter().zip(&new_atoms).take_while(|(a, b)| a.ch == b.ch && a.marks == b.marks).count();
    let suf = suffix_eq(&old_atoms, &new_atoms, pre);
    let inherit_run = inherit(&old_atoms, pre, suf);
    let inherit_rpr = inherit_run.and_then(|ri| old.get(ri)).map(|r| r.rpr.clone()).unwrap_or_default();
    let inherit_marks = inherit_run.and_then(|ri| old.get(ri)).map(|r| r.marks.clone());
    let mut out_atoms = Vec::new();
    out_atoms.extend(old_atoms[..pre].iter().cloned());
    for atom in new_atoms.iter().take(new_atoms.len() - suf).skip(pre) {
        let mut atom = atom.clone();
        atom.rpr = inherit_rpr.clone();
        if inherit_marks.as_ref() == Some(&atom.marks) {
            atom.run = inherit_run;
        }
        out_atoms.push(atom);
    }
    if suf > 0 {
        out_atoms.extend(old_atoms[old_atoms.len() - suf..].iter().cloned());
    }
    let groups = group_atoms(&out_atoms);
    let mut pieces = Vec::new();
    let mut last_run: Option<usize> = None;
    let mut broke = false;
    for group in &groups {
        if let Some(ri) = group.run {
            if last_run == Some(ri.saturating_sub(1)) && !broke && exclusive(&groups, ri) {
                pieces.extend(gaps_between(&para.pieces, last_run.unwrap(), ri));
            }
            if exclusive(&groups, ri) && group.marks == old[ri].marks {
                let mut run = old[ri].clone();
                if run.text != group.text {
                    run.raw = replace_text(&run.raw, &group.text, &w);
                    run.text = group.text.clone();
                }
                pieces.push(Piece::Run(run));
            } else {
                pieces.push(Piece::Run(synthesized(group, &old[ri].rpr, &w, &r, session)));
            }
            last_run = Some(ri);
            broke = false;
        } else {
            pieces.push(Piece::Run(synthesized(group, &group.rpr, &w, &r, session)));
            broke = true;
        }
    }
    assemble(&Para { ppr, pieces, raw: String::new(), ..para.clone() })
}

struct Atom {
    ch: char,
    marks: Marks,
    run: Option<usize>,
    rpr: String,
}

impl Clone for Atom {
    fn clone(&self) -> Self {
        Self { ch: self.ch, marks: self.marks.clone(), run: self.run, rpr: self.rpr.clone() }
    }
}

fn atoms_of(runs: &[Run]) -> Vec<Atom> {
    let mut out = Vec::new();
    for (ri, run) in runs.iter().enumerate() {
        for ch in run.text.chars() {
            out.push(Atom { ch, marks: run.marks.clone(), run: Some(ri), rpr: run.rpr.clone() });
        }
    }
    out
}

fn suffix_eq(old: &[Atom], new: &[Atom], pre: usize) -> usize {
    let mut n = 0;
    while n < old.len() - pre && n < new.len() - pre {
        let a = &old[old.len() - 1 - n];
        let b = &new[new.len() - 1 - n];
        if a.ch != b.ch || a.marks != b.marks {
            break;
        }
        n += 1;
    }
    n
}

fn inherit(old: &[Atom], pre: usize, suf: usize) -> Option<usize> {
    if pre < old.len() && old.len() - suf > pre {
        let run = old[pre].run;
        if old[pre..old.len() - suf].iter().all(|a| a.run == run) {
            return run;
        }
        return None;
    }
    if pre > 0 && suf > 0 && old[pre - 1].run == old[old.len() - suf].run {
        return old[pre - 1].run;
    }
    if pre > 0 && suf == 0 && pre == old.len() {
        return old[pre - 1].run;
    }
    if pre == 0 && suf > 0 {
        return old[old.len() - suf].run;
    }
    None
}

struct Group {
    text: String,
    marks: Marks,
    run: Option<usize>,
    rpr: String,
}

fn group_atoms(atoms: &[Atom]) -> Vec<Group> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < atoms.len() {
        let marks = atoms[i].marks.clone();
        let run = atoms[i].run;
        let rpr = atoms[i].rpr.clone();
        let mut text = String::new();
        while i < atoms.len() && atoms[i].marks == marks && atoms[i].run == run && atoms[i].rpr == rpr {
            text.push(atoms[i].ch);
            i += 1;
        }
        out.push(Group { text, marks, run, rpr });
    }
    out
}

fn exclusive(groups: &[Group], ri: usize) -> bool {
    groups.iter().filter(|g| g.run == Some(ri)).count() == 1
}

fn gaps_between(pieces: &[Piece], from: usize, to: usize) -> Vec<Piece> {
    let mut seen = 0;
    let mut out = Vec::new();
    let mut collecting = false;
    for piece in pieces {
        match piece {
            Piece::Run(_) => {
                if seen == from {
                    collecting = true;
                }
                seen += 1;
                if seen == to {
                    break;
                }
                if collecting && seen > from {
                    collecting = false;
                }
            }
            Piece::Gap(g) if collecting => out.push(Piece::Gap(g.clone())),
            _ => {}
        }
    }
    out
}

fn synthesized(group: &Group, rpr: &str, w: &str, r: &str, session: &mut Session) -> Run {
    let rpr = apply_marks(rpr, &group.marks, w);
    let mut raw = run_element(w, &rpr, &group.text);
    if let Some(url) = &group.marks.link {
        let id = session.ensure_link(url);
        raw = format!("<{w}:hyperlink {r}:id=\"{id}\">{raw}</{w}:hyperlink>");
    }
    Run { raw, text: group.text.clone(), marks: group.marks.clone(), rpr }
}

fn glyphs_to_runs(glyphs: &[Glyph]) -> Vec<(Marks, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < glyphs.len() {
        let marks = glyphs[i].marks.clone();
        let mut text = String::new();
        while i < glyphs.len() && glyphs[i].marks == marks {
            text.push(glyphs[i].ch);
            i += 1;
        }
        out.push((marks, text));
    }
    out
}

fn fresh_xml(line: &str, lead: &str, session: &mut Session) -> String {
    let parsed = parse(line);
    let w = session.w.clone();
    let r = session.r.clone();
    let ppr = rewrite_ppr("", parsed.prefix, &w, session);
    let runs = glyphs_to_runs(&parsed.glyphs);
    let mut pieces = Vec::new();
    for (marks, text) in &runs {
        pieces.push(Piece::Run(synthesized(
            &Group { text: text.clone(), marks: marks.clone(), run: None, rpr: String::new() },
            "",
            &w,
            &r,
            session,
        )));
    }
    let para = Para {
        id: 0,
        lead: lead.to_string(),
        raw: String::new(),
        open: format!("<{w}:p>"),
        ppr,
        pieces,
        close: format!("</{w}:p>"),
        dialect: line.to_string(),
        prefix: parsed.prefix,
        complex: false,
    };
    format!("{lead}{}", assemble(&para))
}

fn assemble(para: &Para) -> String {
    if !para.raw.is_empty() && para.pieces.is_empty() && para.ppr.is_empty() {
        return para.raw.clone();
    }
    let mut s = para.open.clone();
    s.push_str(&para.ppr);
    for piece in &para.pieces {
        match piece {
            Piece::Gap(g) => s.push_str(g),
            Piece::Run(run) => s.push_str(&run.raw),
        }
    }
    s.push_str(&para.close);
    s
}

fn slice_xml(para: &Para) -> String {
    if !para.raw.is_empty() {
        return para.raw.clone();
    }
    assemble(para)
}

fn slice_para(para: &Para, start: usize, end: usize, w: &str) -> Para {
    let (_dialect, slots) = map_para(para);
    let mut keep: Vec<(usize, usize, usize)> = Vec::new();
    for slot in slots.iter().take(end).skip(start) {
        if let Slot::Char { run, at } = slot {
            if let Some(last) = keep.last_mut() {
                if last.0 == *run && last.2 == *at {
                    last.2 = at + 1;
                    continue;
                }
            }
            keep.push((*run, *at, at + 1));
        }
    }
    let runs = run_refs(&para.pieces);
    let empty = keep.is_empty();
    let mut pieces = Vec::new();
    let mut last = None;
    for (ri, from, to) in keep {
        if last == Some(ri.saturating_sub(1)) {
            pieces.extend(gaps_between(&para.pieces, last.unwrap(), ri));
        }
        let run = &runs[ri];
        let chars: Vec<char> = run.text.chars().collect();
        if from == 0 && to == chars.len() {
            pieces.push(Piece::Run(run.clone()));
        } else {
            let text: String = chars[from..to].iter().collect();
            let raw = replace_text(&run_element(w, &run.rpr, &text), &text, w);
            let wrapped = if run.marks.link.is_some() && local_is(&elem_name(&run.raw), "hyperlink") {
                let open_end = quote_end(&run.raw).unwrap_or(0);
                let open = run.raw[..open_end].to_string();
                format!("{open}{raw}</{}:hyperlink>", w)
            } else {
                raw
            };
            pieces.push(Piece::Run(Run { raw: wrapped, text, marks: run.marks.clone(), rpr: run.rpr.clone() }));
        }
        last = Some(ri);
    }
    let encoded = if empty { String::new() } else { encode(para.prefix, &run_refs(&pieces)) };
    Para {
        raw: String::new(),
        pieces: if empty { Vec::new() } else { pieces },
        dialect: encoded,
        prefix: if empty { Prefix::None } else { para.prefix },
        ppr: if empty { String::new() } else { para.ppr.clone() },
        ..para.clone()
    }
}

#[derive(Clone)]
enum Slot {
    Mark,
    Char { run: usize, at: usize },
}

fn map_para(para: &Para) -> (String, Vec<Slot>) {
    let runs = run_refs(&para.pieces);
    let mut s = match para.prefix {
        Prefix::None => String::new(),
        Prefix::Heading(n) => format!("{} ", "#".repeat(n as usize)),
        Prefix::Bullet => "- ".to_string(),
        Prefix::Number => "1. ".to_string(),
        Prefix::Quote => "> ".to_string(),
    };
    let mut slots = vec![Slot::Mark; s.chars().count()];
    let mut i = 0;
    while i < runs.len() {
        let marks = &runs[i].marks;
        let mut j = i + 1;
        while j < runs.len() && &runs[j].marks == marks {
            j += 1;
        }
        let open = open_marks(marks);
        for _ in open.chars() {
            slots.push(Slot::Mark);
        }
        s.push_str(&open);
        for ri in i..j {
            for (at, ch) in runs[ri].text.chars().enumerate() {
                if matches!(ch, '\\' | '*' | '~' | '[') {
                    s.push('\\');
                    slots.push(Slot::Mark);
                }
                s.push(ch);
                slots.push(Slot::Char { run: ri, at });
            }
        }
        let close = close_marks(marks);
        for _ in close.chars() {
            slots.push(Slot::Mark);
        }
        s.push_str(&close);
        i = j;
    }
    debug_assert_eq!(s.chars().count(), slots.len());
    (s, slots)
}

fn open_marks(marks: &Marks) -> String {
    let mut s = String::new();
    if marks.link.is_some() {
        s.push('[');
    }
    if marks.strike {
        s.push_str("~~");
    }
    if marks.bold {
        s.push_str("**");
    }
    if marks.italic {
        s.push('*');
    }
    s
}

fn close_marks(marks: &Marks) -> String {
    let mut s = String::new();
    if marks.italic {
        s.push('*');
    }
    if marks.bold {
        s.push_str("**");
    }
    if marks.strike {
        s.push_str("~~");
    }
    if let Some(url) = &marks.link {
        s.push_str("](");
        s.push_str(&percent_encode(url));
        s.push(')');
    }
    s
}

fn rewrite_ppr(ppr: &str, prefix: Prefix, w: &str, session: &mut Session) -> String {
    let inner = if ppr.is_empty() {
        String::new()
    } else {
        let (_, inner, _) = unwrap(ppr);
        remove_locals(&inner, &["pStyle", "numPr", "outlineLvl"])
    };
    let mut add = String::new();
    match prefix {
        Prefix::Heading(n) => add = format!("<{w}:pStyle {w}:val=\"Heading{n}\"/>"),
        Prefix::Quote => add = format!("<{w}:pStyle {w}:val=\"Quote\"/>"),
        Prefix::Bullet => {
            let id = session.ensure_list(true);
            add = format!("<{w}:numPr><{w}:ilvl {w}:val=\"0\"/><{w}:numId {w}:val=\"{id}\"/></{w}:numPr>");
        }
        Prefix::Number => {
            let id = session.ensure_list(false);
            add = format!("<{w}:numPr><{w}:ilvl {w}:val=\"0\"/><{w}:numId {w}:val=\"{id}\"/></{w}:numPr>");
        }
        Prefix::None => {}
    }
    if add.is_empty() && inner.trim().is_empty() {
        return String::new();
    }
    format!("<{w}:pPr>{add}{inner}</{w}:pPr>")
}

fn remove_locals(inner: &str, names: &[&str]) -> String {
    let mut out = String::new();
    for top in split_top(inner) {
        match top {
            Top::Gap(g) => out.push_str(&g),
            Top::Elem { name, raw, lead } => {
                if names.iter().any(|n| local_is(&name, n)) {
                    continue;
                }
                out.push_str(&lead);
                out.push_str(&raw);
            }
        }
    }
    out
}

impl Session {
    fn ensure_list(&mut self, bullet: bool) -> String {
        if bullet {
            if let Some(id) = &self.bullet_id {
                return id.clone();
            }
        } else if let Some(id) = &self.number_id {
            return id.clone();
        }
        self.install_numbering();
        if bullet { self.bullet_id.clone() } else { self.number_id.clone() }.unwrap_or_else(|| "1".to_string())
    }

    fn install_numbering(&mut self) {
        if self.numbering_xml.is_none() {
            self.numbering_xml = Some(blank_numbering(&self.w));
            self.numbering_path = Some("word/numbering.xml".to_string());
            self.numbering_dirty = true;
            self.bullet_id = Some("1".to_string());
            self.number_id = Some("2".to_string());
            self.num_kind = vec![("1".into(), true), ("2".into(), false)];
            if !self.rels_xml.contains("numbering") {
                self.rels_xml = insert_rel(&self.rels_xml, &format!("rId{}", self.next_rid), "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering", "numbering.xml", false);
                self.next_rid += 1;
                self.rels_dirty = true;
            }
            if !self.content_types.contains("numbering") {
                self.content_types = insert_override(&self.content_types, "/word/numbering.xml", "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml");
                self.types_dirty = true;
            }
            return;
        }
        let xml = self.numbering_xml.clone().unwrap();
        let w = self.w.clone();
        if self.bullet_id.is_none() {
            let id = fresh_num_id(&xml);
            self.numbering_xml = Some(append_num(&xml, &w, &id, true));
            self.numbering_dirty = true;
            self.bullet_id = Some(id);
        }
        if self.number_id.is_none() {
            let xml = self.numbering_xml.clone().unwrap();
            let id = fresh_num_id(&xml);
            self.numbering_xml = Some(append_num(&xml, &w, &id, false));
            self.numbering_dirty = true;
            self.number_id = Some(id);
        }
    }

    fn ensure_link(&mut self, url: &str) -> String {
        if let Some((id, _)) = self.links.iter().find(|(_, u)| u == url) {
            return id.clone();
        }
        let id = format!("rId{}", self.next_rid);
        self.next_rid += 1;
        self.rels_xml = insert_rel(
            &self.rels_xml,
            &id,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink",
            url,
            true,
        );
        self.rels_dirty = true;
        self.links.push((id.clone(), url.to_string()));
        id
    }
}

fn align_anchors(old_anchors: &[Anchor], old_lines: &[String], new_lines: &[String]) -> Vec<Anchor> {
    let matches = match_identical(old_lines, new_lines);
    let mut used_o = vec![false; old_lines.len()];
    let mut used_n = vec![false; new_lines.len()];
    for &(o, n) in &matches {
        used_o[o] = true;
        used_n[n] = true;
    }
    let mut result = vec![Anchor::Fresh; new_lines.len()];
    for &(o, n) in &matches {
        result[n] = old_anchors.get(o).cloned().unwrap_or(Anchor::Fresh);
    }
    let unmatched_old: Vec<usize> = (0..old_lines.len()).filter(|i| !used_o[*i]).collect();
    let unmatched_new: Vec<usize> = (0..new_lines.len()).filter(|i| !used_n[*i]).collect();
    let mut oi = 0;
    let mut ni = 0;
    while oi < unmatched_old.len() || ni < unmatched_new.len() {
        if oi < unmatched_old.len() && ni < unmatched_new.len() {
            let o = unmatched_old[oi];
            if let Some(k) = split_len(&old_lines[o], &unmatched_new[ni..], new_lines) {
                let anchor = old_anchors.get(o).cloned().unwrap_or(Anchor::Fresh);
                let mut at = 0;
                for part in 0..k {
                    let n = unmatched_new[ni + part];
                    let len = new_lines[n].chars().count();
                    result[n] = fragment_anchor(&anchor, &old_lines[o], at, at + len);
                    at += len;
                }
                oi += 1;
                ni += k;
                continue;
            }
            if let Some(k) = join_len(&new_lines[unmatched_new[ni]], &unmatched_old[oi..], old_lines) {
                let parts = (0..k).map(|t| old_anchors.get(unmatched_old[oi + t]).cloned().unwrap_or(Anchor::Fresh)).collect();
                result[unmatched_new[ni]] = Anchor::Join(parts);
                oi += k;
                ni += 1;
                continue;
            }
            result[unmatched_new[ni]] = old_anchors.get(o).cloned().unwrap_or(Anchor::Fresh);
            oi += 1;
            ni += 1;
            continue;
        }
        if ni < unmatched_new.len() {
            result[unmatched_new[ni]] = Anchor::Fresh;
            ni += 1;
            continue;
        }
        oi += 1;
    }
    result
}

fn fragment_anchor(anchor: &Anchor, old_line: &str, start: usize, end: usize) -> Anchor {
    match anchor {
        Anchor::Block(id) => Anchor::Fragment { id: *id, parent: old_line.to_string(), start, end },
        Anchor::Fragment { id, parent, start: base, .. } => Anchor::Fragment { id: *id, parent: parent.clone(), start: base + start, end: base + (end - start) },
        other => other.clone(),
    }
}

fn split_len(old: &str, new_idx: &[usize], new_lines: &[String]) -> Option<usize> {
    let mut acc = String::new();
    for (k, idx) in new_idx.iter().enumerate() {
        if k > 0 && *idx != new_idx[0] + k {
            return None;
        }
        acc.push_str(&new_lines[*idx]);
        if k >= 1 && acc == old {
            return Some(k + 1);
        }
        if acc.chars().count() > old.chars().count() {
            return None;
        }
    }
    None
}

fn join_len(new: &str, old_idx: &[usize], old_lines: &[String]) -> Option<usize> {
    let mut acc = String::new();
    for (k, idx) in old_idx.iter().enumerate() {
        if k > 0 && *idx != old_idx[0] + k {
            return None;
        }
        acc.push_str(&old_lines[*idx]);
        if k >= 1 && acc == new {
            return Some(k + 1);
        }
        if acc.chars().count() > new.chars().count() {
            return None;
        }
    }
    None
}

fn match_identical(old: &[String], new: &[String]) -> Vec<(usize, usize)> {
    let mut oc: Vec<(&str, Vec<usize>)> = Vec::new();
    for (i, line) in old.iter().enumerate() {
        if let Some((_, v)) = oc.iter_mut().find(|(s, _)| *s == line.as_str()) {
            v.push(i);
        } else {
            oc.push((line.as_str(), vec![i]));
        }
    }
    let mut nc: Vec<(&str, Vec<usize>)> = Vec::new();
    for (i, line) in new.iter().enumerate() {
        if let Some((_, v)) = nc.iter_mut().find(|(s, _)| *s == line.as_str()) {
            v.push(i);
        } else {
            nc.push((line.as_str(), vec![i]));
        }
    }
    let mut used_o = vec![false; old.len()];
    let mut used_n = vec![false; new.len()];
    let mut matches = Vec::new();
    for (i, line) in new.iter().enumerate() {
        let os = oc.iter().find(|(s, _)| *s == line.as_str()).map(|(_, v)| v.clone()).unwrap_or_default();
        let ns = nc.iter().find(|(s, _)| *s == line.as_str()).map(|(_, v)| v.clone()).unwrap_or_default();
        if os.len() == 1 && ns.len() == 1 && !used_o[os[0]] && !used_n[i] {
            used_o[os[0]] = true;
            used_n[i] = true;
            matches.push((os[0], i));
        }
    }
    let oi: Vec<usize> = (0..old.len()).filter(|i| !used_o[*i]).collect();
    let ni: Vec<usize> = (0..new.len()).filter(|i| !used_n[*i]).collect();
    for (a, b) in lcs(&oi, &ni, old, new) {
        matches.push((a, b));
    }
    matches
}

fn lcs(oi: &[usize], ni: &[usize], old: &[String], new: &[String]) -> Vec<(usize, usize)> {
    let n = oi.len();
    let m = ni.len();
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for a in 0..n {
        for b in 0..m {
            dp[a + 1][b + 1] = if old[oi[a]] == new[ni[b]] { dp[a][b] + 1 } else { dp[a + 1][b].max(dp[a][b + 1]) };
        }
    }
    let mut out = Vec::new();
    let (mut a, mut b) = (n, m);
    while a > 0 && b > 0 {
        if old[oi[a - 1]] == new[ni[b - 1]] {
            out.push((oi[a - 1], ni[b - 1]));
            a -= 1;
            b -= 1;
        } else if dp[a - 1][b] >= dp[a][b - 1] {
            a -= 1;
        } else {
            b -= 1;
        }
    }
    out.reverse();
    out
}

fn is_complex(raw: &str) -> bool {
    const NAMES: &[&str] = &[
        "drawing", "pict", "object", "fldChar", "instrText", "fldSimple", "commentRangeStart", "commentRangeEnd", "commentReference", "del", "ins", "moveFrom", "moveTo", "sdt",
        "footnoteReference", "endnoteReference", "sym", "AlternateContent", "br", "cr", "tab", "ruby", "oMath",
    ];
    NAMES.iter().any(|n| has_local(raw, n))
}

fn has_local(xml: &str, local: &str) -> bool {
    for prefix in [":", "<"] {
        let pat = if prefix == ":" { format!(":{local}") } else { format!("<{local}") };
        let mut from = 0;
        while let Some(p) = xml[from..].find(&pat) {
            let at = from + p + pat.len();
            let boundary = xml.as_bytes().get(at).copied().unwrap_or(b'>');
            if matches!(boundary, b'>' | b'/' | b' ' | b'\n' | b'\r' | b'\t') {
                return true;
            }
            from = at;
        }
    }
    false
}

fn marks_of(rpr: &str, w: &str) -> Marks {
    Marks { bold: toggled(rpr, w, "b"), italic: toggled(rpr, w, "i"), strike: toggled(rpr, w, "strike") || toggled(rpr, w, "dstrike"), link: None }
}

fn toggled(xml: &str, w: &str, local: &str) -> bool {
    let patterns = [format!("<{w}:{local}"), format!("<{local}")];
    for pat in patterns {
        let mut from = 0;
        while let Some(p) = xml[from..].find(&pat) {
            let at = from + p + pat.len();
            let boundary = xml.as_bytes().get(at).copied().unwrap_or(b'>');
            if !matches!(boundary, b'>' | b'/' | b' ' | b'\n' | b'\r' | b'\t') {
                from = at;
                continue;
            }
            let tag_end = quote_end(&xml[from + p..]).map(|n| from + p + n).unwrap_or(xml.len());
            let tag = &xml[from + p..tag_end];
            let off = attr_get(tag, &format!("{w}:val")).or_else(|| attr_get(tag, "val")).is_some_and(|v| matches!(v.as_str(), "0" | "false" | "off"));
            return !off;
        }
    }
    false
}

fn apply_marks(rpr: &str, marks: &Marks, w: &str) -> String {
    let mut inner = if rpr.is_empty() { String::new() } else { unwrap(rpr).1 };
    inner = remove_locals(&inner, &["b", "bCs", "i", "iCs", "strike", "dstrike"]);
    let mut add = String::new();
    if marks.bold {
        add.push_str(&format!("<{w}:b/>"));
    }
    if marks.italic {
        add.push_str(&format!("<{w}:i/>"));
    }
    if marks.strike {
        add.push_str(&format!("<{w}:strike/>"));
    }
    if add.is_empty() && inner.trim().is_empty() {
        return String::new();
    }
    format!("<{w}:rPr>{add}{inner}</{w}:rPr>")
}

fn extract_rpr(raw: &str, w: &str) -> String {
    let needle = format!("<{w}:rPr");
    let Some(at) = raw.find(&needle).or_else(|| raw.find("<rPr")) else {
        return String::new();
    };
    let end_tag = format!("</{w}:rPr>");
    if let Some(rel) = raw[at..].find(&end_tag) {
        return raw[at..at + rel + end_tag.len()].to_string();
    }
    if let Some(rel) = raw[at..].find("/>") {
        return raw[at..at + rel + 2].to_string();
    }
    String::new()
}

fn collect_text(raw: &str, w: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    let open = format!("<{w}:t");
    while let Some(at) = rest.find(&open) {
        let after = &rest[at + open.len()..];
        let boundary = after.as_bytes().first().copied().unwrap_or(b'>');
        if !matches!(boundary, b'>' | b'/' | b' ' | b'\n' | b'\r' | b'\t') {
            rest = &rest[at + open.len()..];
            continue;
        }
        let Some(tag_end) = quote_end(after).map(|n| n) else { break };
        let content_from = &after[tag_end..];
        if after[..tag_end].ends_with("/>") {
            rest = content_from;
            continue;
        }
        let close = format!("</{w}:t>");
        let Some(stop) = content_from.find(&close) else { break };
        out.push_str(&decode(&content_from[..stop]));
        rest = &content_from[stop + close.len()..];
    }
    out
}

fn replace_text(raw: &str, text: &str, w: &str) -> String {
    let open = format!("<{w}:t");
    let close = format!("</{w}:t>");
    let Some(at) = raw.find(&open) else {
        let elem = text_elem(w, text);
        if let Some(end) = raw.rfind(&format!("</{w}:r>")) {
            let mut s = String::new();
            s.push_str(&raw[..end]);
            s.push_str(&elem);
            s.push_str(&raw[end..]);
            return s;
        }
        return run_element(w, "", text);
    };
    let after = &raw[at + open.len()..];
    let Some(tag_rel) = quote_end(after) else {
        return raw.to_string();
    };
    let mut tag = after[..tag_rel].trim_end_matches('>').trim_end_matches('/').trim_end().to_string();
    if text_needs_preserve(text) && !tag.contains("xml:space") {
        tag.push_str(" xml:space=\"preserve\"");
    }
    let content_at = at + open.len() + tag_rel;
    let Some(stop) = raw[content_at..].find(&close) else {
        return raw.to_string();
    };
    let mut s = String::new();
    s.push_str(&raw[..at]);
    s.push_str(&open);
    s.push_str(&tag);
    s.push('>');
    s.push_str(&escape_xml(text));
    s.push_str(&close);
    let mut rest = &raw[content_at + stop + close.len()..];
    while let Some(next) = rest.find(&open) {
        s.push_str(&rest[..next]);
        let after = &rest[next + open.len()..];
        let Some(rel) = quote_end(after) else { break };
        if after[..rel].ends_with("/>") {
            rest = &after[rel..];
            continue;
        }
        let content = &after[rel..];
        let Some(end) = content.find(&close) else { break };
        rest = &content[end + close.len()..];
    }
    s.push_str(rest);
    s
}

fn text_needs_preserve(text: &str) -> bool {
    text.starts_with(|c: char| c.is_whitespace()) || text.ends_with(|c: char| c.is_whitespace()) || text.contains("  ")
}

fn text_elem(w: &str, text: &str) -> String {
    if text_needs_preserve(text) {
        format!("<{w}:t xml:space=\"preserve\">{}</{w}:t>", escape_xml(text))
    } else {
        format!("<{w}:t>{}</{w}:t>", escape_xml(text))
    }
}

fn run_element(w: &str, rpr: &str, text: &str) -> String {
    format!("<{w}:r>{rpr}{}</{w}:r>", text_elem(w, text))
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn decode(text: &str) -> String {
    let mut out = String::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'&' {
            if let Some(end) = text[i..].find(';') {
                let entity = &text[i + 1..i + end];
                let ch = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ if let Some(hex) = entity.strip_prefix("#x") => u32::from_str_radix(hex, 16).ok().and_then(char::from_u32),
                    _ if let Some(dec) = entity.strip_prefix('#') => dec.parse::<u32>().ok().and_then(char::from_u32),
                    _ => None,
                };
                if let Some(ch) = ch {
                    out.push(ch);
                    i += end + 1;
                    continue;
                }
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

fn link_target(raw: &str, r: &str, links: &[(String, String)]) -> Option<String> {
    let tag_end = quote_end(raw)?;
    let tag = &raw[..tag_end];
    if let Some(id) = attr_get(tag, &format!("{r}:id")).or_else(|| attr_get(tag, "r:id")) {
        return links.iter().find(|(n, _)| n == &id).map(|(_, u)| u.clone());
    }
    attr_get(tag, "w:anchor").map(|a| format!("#{a}"))
}

fn elem_name(raw: &str) -> String {
    let rest = raw.trim_start_matches('<').trim_start_matches('/');
    rest.split(|c: char| c.is_whitespace() || c == '>' || c == '/').next().unwrap_or("").to_string()
}

#[derive(Clone)]
enum Top {
    Gap(String),
    Elem { name: String, raw: String, lead: String },
}

fn split_top(xml: &str) -> Vec<Top> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut lead = String::new();
    let b = xml.as_bytes();
    while i < b.len() {
        if b[i] != b'<' {
            let start = i;
            while i < b.len() && b[i] != b'<' {
                i += 1;
            }
            lead.push_str(&xml[start..i]);
            continue;
        }
        if xml[i..].starts_with("<!--") {
            let end = xml[i..].find("-->").map(|n| i + n + 3).unwrap_or(xml.len());
            lead.push_str(&xml[i..end]);
            i = end;
            continue;
        }
        if xml[i..].starts_with("<?") {
            let end = xml[i..].find("?>").map(|n| i + n + 2).unwrap_or(xml.len());
            lead.push_str(&xml[i..end]);
            i = end;
            continue;
        }
        if xml[i..].starts_with("</") {
            break;
        }
        let Some(open_end) = quote_end(&xml[i..]).map(|n| i + n) else { break };
        let name = elem_name(&xml[i..open_end]);
        let empty = xml[..open_end].ends_with("/>");
        let raw_end = if empty {
            open_end
        } else {
            match matching_close(xml, open_end, &name) {
                Some(n) => n,
                None => break,
            }
        };
        out.push(Top::Elem { name, raw: xml[i..raw_end].to_string(), lead: std::mem::take(&mut lead) });
        i = raw_end;
    }
    if !lead.is_empty() || i < xml.len() {
        lead.push_str(&xml[i..]);
        if !lead.is_empty() {
            out.push(Top::Gap(lead));
        }
    }
    out
}

fn matching_close(xml: &str, mut i: usize, name: &str) -> Option<usize> {
    let mut depth = 1;
    let b = xml.as_bytes();
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if xml[i..].starts_with("<!--") {
            i = xml[i..].find("-->").map(|n| i + n + 3).unwrap_or(xml.len());
            continue;
        }
        if xml[i..].starts_with("<?") {
            i = xml[i..].find("?>").map(|n| i + n + 2).unwrap_or(xml.len());
            continue;
        }
        let end = quote_end(&xml[i..])? + i;
        let closing = xml[i..].starts_with("</");
        let empty = xml[..end].ends_with("/>");
        let tag_name = elem_name(&xml[i..end]);
        if closing {
            depth -= 1;
            if depth == 0 {
                return Some(end);
            }
            let _ = tag_name;
            let _ = name;
        } else if !empty {
            depth += 1;
        }
        i = end;
    }
    None
}

fn unwrap(raw: &str) -> (String, String, String) {
    let open_end = quote_end(raw).unwrap_or(raw.len());
    if raw[..open_end].ends_with("/>") {
        let name = elem_name(raw);
        return (format!("<{name}>"), String::new(), format!("</{name}>"));
    }
    let close_at = raw.rfind("</").unwrap_or(raw.len());
    (raw[..open_end].to_string(), raw[open_end..close_at].to_string(), raw[close_at..].to_string())
}

fn quote_end(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if b.first().copied() != Some(b'<') && !s.is_empty() {
        // `s` may start at the character after '<' when callers slice that way. Accept both.
    }
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else if c == b'"' || c == b'\'' {
            quote = Some(c);
        } else if c == b'>' {
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

fn attr_get(tag: &str, key: &str) -> Option<String> {
    let mut from = 0;
    while let Some(p) = tag[from..].find(key) {
        let at = from + p;
        let boundary_before = at == 0 || tag.as_bytes()[at - 1].is_ascii_whitespace();
        let after = &tag[at + key.len()..];
        if boundary_before && after.trim_start().starts_with('=') {
            let value = after.trim_start()[1..].trim_start();
            let quote = value.chars().next()?;
            if quote == '"' || quote == '\'' {
                let body = &value[1..];
                let end = body.find(quote)?;
                return Some(decode(&body[..end]));
            }
        }
        from = at + key.len();
    }
    None
}

fn local_is(name: &str, local: &str) -> bool {
    name.rsplit(':').next() == Some(local)
}

fn split_body(xml: &str, w: &str) -> io::Result<(String, String, String)> {
    let needle = format!("<{w}:body");
    let at = xml.find(&needle).or_else(|| xml.find("<body")).ok_or_else(|| invalid("this docx has no body"))?;
    let open_end = quote_end(&xml[at..]).map(|n| at + n).ok_or_else(|| invalid("unclosed body tag"))?;
    let close = format!("</{w}:body>");
    let close_at = xml[open_end..].rfind(&close).map(|n| open_end + n).or_else(|| xml.rfind("</body>")).ok_or_else(|| invalid("unclosed body"))?;
    Ok((xml[..open_end].to_string(), xml[open_end..close_at].to_string(), xml[close_at..].to_string()))
}

fn prefix_for(xml: &str, uri: &str) -> Option<String> {
    let at = xml.find(uri)?;
    let before = &xml[..at];
    let p = before.rfind("xmlns:")?;
    let name = &before[p + 6..];
    let end = name.find('=').unwrap_or(name.len());
    let name = name[..end].trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn office_path(rels: &str) -> Option<String> {
    for raw in rel_elements(rels) {
        let ty = attr_get(&raw, "Type").unwrap_or_default();
        if ty.ends_with("/officeDocument") {
            return attr_get(&raw, "Target").map(|t| t.trim_start_matches('/').to_string());
        }
    }
    None
}

fn numbering_part(rels: &str, rels_path: &str, files: &mut Vec<(String, Vec<u8>)>) -> (Option<String>, Option<String>) {
    for raw in rel_elements(rels) {
        let ty = attr_get(&raw, "Type").unwrap_or_default();
        if ty.ends_with("/numbering") {
            if let Some(target) = attr_get(&raw, "Target") {
                let path = resolve(rels_path, &target);
                let xml = take(files, &path);
                return (Some(path), xml);
            }
        }
    }
    (None, None)
}

fn resolve(rels_path: &str, target: &str) -> String {
    if let Some(stripped) = target.strip_prefix('/') {
        return stripped.to_string();
    }
    let dir = rels_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let base = dir.strip_suffix("/_rels").unwrap_or(dir);
    if base.is_empty() { target.to_string() } else { format!("{base}/{target}") }
}

fn rels_for(doc_path: &str) -> String {
    match doc_path.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{doc_path}.rels"),
    }
}

fn hyperlinks(rels: &str) -> Vec<(String, String)> {
    rel_elements(rels)
        .into_iter()
        .filter(|raw| attr_get(raw, "Type").is_some_and(|t| t.ends_with("/hyperlink")))
        .filter_map(|raw| Some((attr_get(&raw, "Id")?, attr_get(&raw, "Target")?)))
        .collect()
}

fn rel_elements(rels: &str) -> Vec<String> {
    children_of(rels, "Relationships")
        .into_iter()
        .filter_map(|t| match t {
            Top::Elem { raw, .. } => Some(raw),
            _ => None,
        })
        .collect()
}

fn children_of(xml: &str, local: &str) -> Vec<Top> {
    for top in split_top(xml) {
        if let Top::Elem { name, raw, .. } = &top {
            if local_is(name, local) {
                return split_top(&unwrap(raw).1);
            }
        }
    }
    split_top(xml)
}

fn next_rel_id(rels: &str) -> u32 {
    rel_elements(rels)
        .iter()
        .filter_map(|raw| attr_get(raw, "Id"))
        .filter_map(|id| id.strip_prefix("rId")?.parse().ok())
        .max()
        .unwrap_or(0)
        + 1
}

fn list_ids(xml: &str, w: &str) -> (Vec<(String, bool)>, Option<String>, Option<String>) {
    let mut abstracts: Vec<(String, bool)> = Vec::new();
    let mut nums: Vec<(String, String)> = Vec::new();
    for top in children_of(xml, "numbering") {
        let Top::Elem { name, raw, .. } = top else { continue };
        if local_is(&name, "abstractNum") {
            let id = attr_get(&raw[..quote_end(&raw).unwrap_or(raw.len())], &format!("{w}:abstractNumId")).unwrap_or_default();
            let bullet = attr_of_local(&raw, w, "numFmt", "val").is_some_and(|v| v == "bullet");
            abstracts.push((id, bullet));
        } else if local_is(&name, "num") {
            let num_id = attr_get(&raw[..quote_end(&raw).unwrap_or(raw.len())], &format!("{w}:numId")).unwrap_or_default();
            let abs = attr_of_local(&raw, w, "abstractNumId", "val").unwrap_or_default();
            nums.push((num_id, abs));
        }
    }
    let mut kinds = Vec::new();
    let mut bullet_id = None;
    let mut number_id = None;
    for (num, abs) in nums {
        let bullet = abstracts.iter().find(|(id, _)| id == &abs).map(|(_, b)| *b).unwrap_or(false);
        if bullet && bullet_id.is_none() {
            bullet_id = Some(num.clone());
        }
        if !bullet && number_id.is_none() {
            number_id = Some(num.clone());
        }
        kinds.push((num, bullet));
    }
    (kinds, bullet_id, number_id)
}

fn read_zip(bytes: &[u8]) -> io::Result<Vec<(String, Vec<u8>)>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(zerr)?;
    let mut files = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(zerr)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        files.push((name, buf));
    }
    Ok(files)
}

fn take(files: &mut Vec<(String, Vec<u8>)>, name: &str) -> Option<String> {
    let i = files.iter().position(|(n, _)| n == name)?;
    let (_, bytes) = files.remove(i);
    let text = String::from_utf8(bytes).ok()?;
    Some(text.trim_start_matches('\u{feff}').to_string())
}

fn zerr(e: zip::result::ZipError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

fn insert_rel(rels: &str, id: &str, ty: &str, target: &str, external: bool) -> String {
    let mode = if external { " TargetMode=\"External\"" } else { "" };
    let target = target.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;");
    let element = format!("<Relationship Id=\"{id}\" Type=\"{ty}\" Target=\"{target}\"{mode}/>");
    if let Some(at) = rels.rfind("</Relationships>") {
        let mut s = String::new();
        s.push_str(&rels[..at]);
        s.push_str(&element);
        s.push_str(&rels[at..]);
        s
    } else {
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{element}</Relationships>")
    }
}

fn insert_override(types: &str, part: &str, content: &str) -> String {
    let element = format!("<Override PartName=\"{part}\" ContentType=\"{content}\"/>");
    if let Some(at) = types.rfind("</Types>") {
        let mut s = String::new();
        s.push_str(&types[..at]);
        s.push_str(&element);
        s.push_str(&types[at..]);
        s
    } else {
        types.to_string()
    }
}

fn fresh_num_id(xml: &str) -> String {
    let mut max = 0u32;
    for top in children_of(xml, "numbering") {
        let Top::Elem { name, raw, .. } = top else { continue };
        if local_is(&name, "num") {
            if let Some(id) = attr_get(&raw[..quote_end(&raw).unwrap_or(0)], "w:numId").and_then(|s| s.parse().ok()) {
                max = max.max(id);
            }
        }
    }
    (max + 1).to_string()
}

fn append_num(xml: &str, w: &str, id: &str, bullet: bool) -> String {
    let abs = if bullet { "80" } else { "81" };
    let fmt = if bullet { "bullet" } else { "decimal" };
    let text = if bullet { "•" } else { "%1." };
    let block = format!(
        "<{w}:abstractNum {w}:abstractNumId=\"{abs}\"><{w}:lvl {w}:ilvl=\"0\"><{w}:start {w}:val=\"1\"/><{w}:numFmt {w}:val=\"{fmt}\"/><{w}:lvlText {w}:val=\"{text}\"/></{w}:lvl></{w}:abstractNum><{w}:num {w}:numId=\"{id}\"><{w}:abstractNumId {w}:val=\"{abs}\"/></{w}:num>"
    );
    if let Some(at) = xml.rfind(&format!("</{w}:numbering>")) {
        let mut s = String::new();
        s.push_str(&xml[..at]);
        s.push_str(&block);
        s.push_str(&xml[at..]);
        s
    } else {
        xml.to_string()
    }
}

fn blank_numbering(w: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<{w}:numbering xmlns:{w}=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><{w}:abstractNum {w}:abstractNumId=\"0\"><{w}:lvl {w}:ilvl=\"0\"><{w}:start {w}:val=\"1\"/><{w}:numFmt {w}:val=\"bullet\"/><{w}:lvlText {w}:val=\"•\"/></{w}:lvl></{w}:abstractNum><{w}:abstractNum {w}:abstractNumId=\"1\"><{w}:lvl {w}:ilvl=\"0\"><{w}:start {w}:val=\"1\"/><{w}:numFmt {w}:val=\"decimal\"/><{w}:lvlText {w}:val=\"%1.\"/></{w}:lvl></{w}:abstractNum><{w}:num {w}:numId=\"1\"><{w}:abstractNumId {w}:val=\"0\"/></{w}:num><{w}:num {w}:numId=\"2\"><{w}:abstractNumId {w}:val=\"1\"/></{w}:num></{w}:numbering>"
    )
}

fn blank_parts(lines: &[&str]) -> Vec<(String, Vec<u8>)> {
    let w = "w";
    let body: String = lines
        .iter()
        .map(|line| {
            let parsed = parse(line);
            let runs = glyphs_to_runs(&parsed.glyphs);
            let ppr = match parsed.prefix {
                Prefix::Heading(n) => format!("<{w}:pPr><{w}:pStyle {w}:val=\"Heading{n}\"/></{w}:pPr>"),
                Prefix::Bullet => format!("<{w}:pPr><{w}:numPr><{w}:ilvl {w}:val=\"0\"/><{w}:numId {w}:val=\"1\"/></{w}:numPr></{w}:pPr>"),
                Prefix::Number => format!("<{w}:pPr><{w}:numPr><{w}:ilvl {w}:val=\"0\"/><{w}:numId {w}:val=\"2\"/></{w}:numPr></{w}:pPr>"),
                Prefix::Quote => format!("<{w}:pPr><{w}:pStyle {w}:val=\"Quote\"/></{w}:pPr>"),
                Prefix::None => String::new(),
            };
            let mut inner = String::new();
            for (marks, text) in runs {
                let rpr = apply_marks("", &marks, w);
                let mut raw = run_element(w, &rpr, &text);
                if marks.link.is_some() {
                    raw = format!("<{w}:hyperlink r:id=\"rId4\">{raw}</{w}:hyperlink>");
                }
                inner.push_str(&raw);
            }
            format!("<{w}:p>{ppr}{inner}</{w}:p>")
        })
        .collect();
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<{w}:document xmlns:{w}=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><{w}:body>{body}<{w}:sectPr><{w}:pgSz {w}:w=\"12240\" {w}:h=\"15840\"/></{w}:sectPr></{w}:body></{w}:document>"
    );
    let types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/></Types>";
    let root = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";
    let doc_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/></Relationships>";
    let styles = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><{w}:styles xmlns:{w}=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><{w}:style {w}:styleId=\"Normal\" {w}:type=\"paragraph\"><{w}:name {w}:val=\"Normal\"/></{w}:style><{w}:style {w}:styleId=\"Heading1\" {w}:type=\"paragraph\"><{w}:name {w}:val=\"heading 1\"/></{w}:style><{w}:style {w}:styleId=\"Heading2\" {w}:type=\"paragraph\"><{w}:name {w}:val=\"heading 2\"/></{w}:style><{w}:style {w}:styleId=\"Heading3\" {w}:type=\"paragraph\"><{w}:name {w}:val=\"heading 3\"/></{w}:style><{w}:style {w}:styleId=\"Heading4\" {w}:type=\"paragraph\"><{w}:name {w}:val=\"heading 4\"/></{w}:style><{w}:style {w}:styleId=\"Heading5\" {w}:type=\"paragraph\"><{w}:name {w}:val=\"heading 5\"/></{w}:style><{w}:style {w}:styleId=\"Heading6\" {w}:type=\"paragraph\"><{w}:name {w}:val=\"heading 6\"/></{w}:style><{w}:style {w}:styleId=\"Quote\" {w}:type=\"paragraph\"><{w}:name {w}:val=\"Quote\"/></{w}:style><{w}:style {w}:styleId=\"Hyperlink\" {w}:type=\"character\"><{w}:name {w}:val=\"Hyperlink\"/></{w}:style></{w}:styles>");
    vec![
        ("[Content_Types].xml".into(), types.into()),
        ("_rels/.rels".into(), root.into()),
        ("word/document.xml".into(), document.into()),
        ("word/_rels/document.xml.rels".into(), doc_rels.into()),
        ("word/styles.xml".into(), styles.into()),
        ("word/numbering.xml".into(), blank_numbering(w).into()),
    ]
}

fn pack_parts(parts: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in parts {
        out.start_file(name, opts).unwrap();
        out.write_all(bytes).unwrap();
    }
    out.finish().unwrap().into_inner()
}

pub(crate) fn from_lines(lines: &[String]) -> io::Result<(Vec<String>, Session)> {
    let (_, mut session) = blank();
    let mut lines = if lines.is_empty() { vec![String::new()] } else { lines.to_vec() };
    let _ = session.save(&mut lines)?;
    Ok((lines, session))
}

fn write_part(out: &mut ZipWriter<Cursor<Vec<u8>>>, name: &str, data: &[u8], opts: SimpleFileOptions) -> io::Result<()> {
    out.start_file(name, opts).map_err(zerr)?;
    out.write_all(data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Cursor, Read, Write};
    use std::path::PathBuf;

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipArchive, ZipWriter};

    use crate::document::Document;
    use crate::keys::Action;
    use crate::state::EditorState;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("ee-docx-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self, name: &str) -> String {
            self.0.join(name).to_string_lossy().into_owned()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn package(document: &str) -> Vec<u8> {
        let types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>";
        let root = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";
        let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>";
        let mut out = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("[Content_Types].xml", types.as_bytes()),
            ("_rels/.rels", root.as_bytes()),
            ("word/document.xml", document.as_bytes()),
            ("word/_rels/document.xml.rels", rels.as_bytes()),
        ] {
            out.start_file(name, opts).unwrap();
            out.write_all(bytes).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    fn write_docx(path: &str, document: &str) {
        fs::write(path, package(document)).unwrap();
    }

    fn document_xml(bytes: &[u8]) -> String {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut file = archive.by_name("word/document.xml").unwrap();
        let mut s = String::new();
        file.read_to_string(&mut s).unwrap();
        s
    }

    const SAMPLE: &str = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>",
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Chapter</w:t></w:r></w:p>",
        "<w:p><w:r><w:rPr><w:color w:val=\"FF0000\"/></w:rPr><w:t xml:space=\"preserve\">Hello </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>world</w:t></w:r></w:p>",
        "<w:p><w:r><w:rPr><w:sz w:val=\"40\"/></w:rPr><w:t>Keep</w:t></w:r></w:p>",
        "<w:p><w:r><w:t>a * b</w:t></w:r></w:p>",
        "<w:p><w:r><w:rPr><w:b/><w:sz w:val=\"32\"/></w:rPr><w:t>Title</w:t></w:r></w:p>",
        "<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr></w:body></w:document>",
    );

    const KEEP: &str = "<w:p><w:r><w:rPr><w:sz w:val=\"40\"/></w:rPr><w:t>Keep</w:t></w:r></w:p>";
    const STAR: &str = "<w:p><w:r><w:t>a * b</w:t></w:r></w:p>";
    const HELLO: &str = "<w:p><w:r><w:rPr><w:color w:val=\"FF0000\"/></w:rPr><w:t xml:space=\"preserve\">Hello </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>world</w:t></w:r></w:p>";

    #[test]
    fn opening_a_docx_projects_paragraphs_and_saving_untouched_is_the_same_bytes() {
        let dir = Scratch::new("same");
        let path = dir.path("sample.docx");
        write_docx(&path, SAMPLE);
        let original = fs::read(&path).unwrap();
        let mut doc = Document::from_file(&path).unwrap();
        assert_eq!(doc.lines, vec!["# Chapter", "Hello **world**", "Keep", "a \\* b", "**Title**"]);
        doc.write_to(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn a_typo_in_a_coloured_run_keeps_that_colour_and_the_other_paragraph() {
        let dir = Scratch::new("typo");
        let path = dir.path("sample.docx");
        write_docx(&path, SAMPLE);
        let mut doc = Document::from_file(&path).unwrap();
        doc.cursor = (1, 0);
        doc.insert_char('X');
        doc.write_to(&path).unwrap();
        let xml = document_xml(&fs::read(&path).unwrap());
        assert!(xml.contains("XHello"), "{xml}");
        assert!(xml.contains("w:color w:val=\"FF0000\""), "{xml}");
        assert!(xml.contains(KEEP), "{xml}");
        assert!(xml.contains(STAR), "{xml}");
    }

    #[test]
    fn moving_a_line_keeps_each_paragraphs_formatting() {
        let dir = Scratch::new("move");
        let path = dir.path("sample.docx");
        write_docx(&path, SAMPLE);
        let mut doc = Document::from_file(&path).unwrap();
        doc.cursor = (1, 0);
        doc.move_line_down();
        doc.write_to(&path).unwrap();
        let xml = document_xml(&fs::read(&path).unwrap());
        let hello = xml.find(HELLO).expect("coloured paragraph");
        let keep = xml.find(KEEP).expect("sized paragraph");
        assert!(keep < hello, "the sized paragraph moved above the coloured one");
    }

    #[test]
    fn marking_a_word_bold_keeps_its_colour() {
        let dir = Scratch::new("bold");
        let path = dir.path("sample.docx");
        write_docx(&path, SAMPLE);
        let mut doc = Document::from_file(&path).unwrap();
        let len = doc.lines[1].chars().count();
        doc.cursor = (1, len);
        doc.selection = Some(((1, 0), (1, len)));
        doc.insert_text("**Hello** **world**");
        doc.write_to(&path).unwrap();
        let xml = document_xml(&fs::read(&path).unwrap());
        let at = xml.find(">Hello<").expect("hello text");
        let window = &xml[at.saturating_sub(120)..at];
        assert!(window.contains("FF0000"), "{window}");
        assert!(window.contains("<w:b"), "{window}");
        assert!(xml.contains(KEEP), "{xml}");
    }

    #[test]
    fn editing_a_comment_rewrites_that_paragraph_and_leaves_the_neighbour() {
        let dir = Scratch::new("comment");
        let path = dir.path("c.docx");
        let document = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>",
            "<w:p><w:commentRangeStart w:id=\"1\"/><w:r><w:t>Note</w:t></w:r><w:commentRangeEnd w:id=\"1\"/></w:p>",
            "<w:p><w:r><w:rPr><w:sz w:val=\"40\"/></w:rPr><w:t>Keep</w:t></w:r></w:p>",
            "<w:sectPr/></w:body></w:document>",
        );
        write_docx(&path, document);
        let mut state = EditorState::new();
        state.tabs[0] = Document::from_file(&path).unwrap();
        assert_eq!(state.tabs[0].lines[0], "Note");
        state.apply(Action::InsertChar('!'));
        assert!(state.status.contains("rewrote"), "{}", state.status);
        state.apply(Action::SaveAll);
        let xml = document_xml(&fs::read(&path).unwrap());
        assert!(!xml.contains("comment"), "{xml}");
        assert!(xml.contains("!Note"), "{xml}");
        assert!(xml.contains(KEEP), "{xml}");
        assert!(state.status.contains("rewrote"), "{}", state.status);
    }

    #[test]
    fn typing_on_a_table_leaves_the_table_and_does_not_dirty_the_file() {
        let dir = Scratch::new("table");
        let path = dir.path("t.docx");
        let document = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>",
            "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>CELLDATA</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
            "<w:sectPr/></w:body></w:document>",
        );
        write_docx(&path, document);
        let original = fs::read(&path).unwrap();
        let mut state = EditorState::new();
        state.tabs[0] = Document::from_file(&path).unwrap();
        assert_eq!(state.tabs[0].lines, vec!["[table]"]);
        state.apply(Action::InsertChar('Z'));
        assert_eq!(state.tabs[0].lines, vec!["[table]"]);
        assert!(!state.tabs[0].dirty);
        assert!(state.status.contains("table"), "{}", state.status);
        state.apply(Action::SaveAll);
        let saved = fs::read(&path).unwrap();
        assert_eq!(saved, original);
        assert!(document_xml(&saved).contains("CELLDATA"));
    }

    #[test]
    fn a_new_docx_round_trips_a_heading_bold_text_and_a_list() {
        let dir = Scratch::new("new");
        let path = dir.path("notes.docx");
        let mut state = EditorState::new();
        state.open_path(&path);
        for c in "# Title".chars() {
            state.apply(Action::InsertChar(c));
        }
        state.apply(Action::Newline);
        for c in "**bold**".chars() {
            state.apply(Action::InsertChar(c));
        }
        state.apply(Action::Newline);
        for c in "- item".chars() {
            state.apply(Action::InsertChar(c));
        }
        state.apply(Action::SaveAll);
        assert!(!state.status.contains("failed"), "{}", state.status);
        let again = Document::from_file(&path).unwrap();
        assert_eq!(again.lines, vec!["# Title", "**bold**", "- item"]);
        let xml = document_xml(&fs::read(&path).unwrap());
        assert!(xml.contains("Heading1"), "{xml}");
        assert!(xml.contains("<w:b"), "{xml}");
        assert!(xml.contains("numId"), "{xml}");
    }

    #[test]
    fn splitting_a_coloured_word_keeps_its_colour_and_joining_restores_the_file() {
        let dir = Scratch::new("split");
        let path = dir.path("sample.docx");
        write_docx(&path, SAMPLE);
        let original = fs::read(&path).unwrap();

        let mut doc = Document::from_file(&path).unwrap();
        let w = doc.lines[1].chars().position(|c| c == 'w').unwrap();
        doc.cursor = (1, w);
        doc.newline();
        doc.settle_docx();
        assert_eq!(doc.lines[2], "**world**");
        assert_eq!(doc.lines[2].chars().nth(doc.cursor.1), Some('w'));
        doc.undo();

        doc.cursor = (1, 3);
        doc.newline();
        doc.write_to(&dir.path("halves.docx")).unwrap();
        let xml = document_xml(&fs::read(&dir.path("halves.docx")).unwrap());
        assert!(xml.matches("w:color w:val=\"FF0000\"").count() >= 2, "{xml}");
        assert!(xml.contains(">Hel<"), "{xml}");
        assert!(xml.contains(">lo <") || xml.contains("lo "), "{xml}");
        assert!(xml.contains("<w:b"), "{xml}");
        assert!(xml.contains(KEEP), "{xml}");

        let mut doc = Document::from_file(&path).unwrap();
        doc.cursor = (1, 3);
        doc.newline();
        doc.backspace();
        doc.write_to(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
    }
}
