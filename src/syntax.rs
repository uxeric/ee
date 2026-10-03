pub const MAX_LINES: usize = 100_000;
pub const MAX_BYTES: usize = 5 * 1024 * 1024;
#[cfg_attr(not(feature = "lang-bash"), allow(dead_code))]
pub const BASH_MAX_LINES: usize = 1_500;
pub const TOO_LARGE: &str = "file too large for syntax colours";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "syntax"), allow(dead_code))]
pub enum Role {
    Keyword,
    String,
    Escape,
    Constant,
    Comment,
    Function,
    Type,
    Builtin,
    Property,
    Tag,
    Punctuation,
    Plain,
}

#[cfg(feature = "syntax")]
const PREFIXES: &[(&str, Role)] = &[
    ("string.special.key", Role::Escape),
    ("string.escape", Role::Escape),
    ("string.special", Role::String),
    ("string", Role::String),
    ("keyword", Role::Keyword),
    ("escape", Role::Escape),
    ("number", Role::Constant),
    ("constant.builtin", Role::Constant),
    ("constant", Role::Constant),
    ("boolean", Role::Constant),
    ("comment", Role::Comment),
    ("function.method", Role::Function),
    ("function.macro", Role::Function),
    ("function.builtin", Role::Function),
    ("function", Role::Function),
    ("constructor", Role::Function),
    ("type.builtin", Role::Type),
    ("type", Role::Type),
    ("module", Role::Type),
    ("attribute", Role::Type),
    ("variable.builtin", Role::Builtin),
    ("variable.parameter", Role::Property),
    ("property.definition", Role::Property),
    ("property", Role::Property),
    ("label", Role::Property),
    ("tag", Role::Tag),
    ("operator", Role::Punctuation),
    ("punctuation", Role::Punctuation),
    ("embedded", Role::Punctuation),
    ("variable", Role::Plain),
];

#[cfg(feature = "syntax")]
pub fn role_of(capture: &str) -> Role {
    let name = capture.trim_start_matches('@');
    let mut best: Option<(usize, Role)> = None;
    for (prefix, role) in PREFIXES {
        let hit = name == *prefix || name.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('.'));
        if hit && best.is_none_or(|(len, _)| prefix.len() > len) {
            best = Some((prefix.len(), *role));
        }
    }
    best.map(|(_, role)| role).unwrap_or(Role::Plain)
}

pub fn too_large(lines: &[String]) -> bool {
    lines.len() > MAX_LINES || lines.iter().map(|l| l.len() + 1).sum::<usize>() > MAX_BYTES
}

#[cfg(not(feature = "syntax"))]
mod engine {
    use super::Role;
    use std::ops::Range;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Lang {}

    impl Lang {
        pub fn for_file(_name: &str, _first_line: &str) -> Option<Self> {
            None
        }

        pub fn from_fence(_info: &str) -> Option<Self> {
            None
        }
    }

    pub struct Highlighter;

    impl Highlighter {
        pub fn new(_lang: Lang, _lines: &[String]) -> Option<Self> {
            None
        }

        pub fn update(&mut self, _lines: &[String]) {}

        pub fn roles(&self, rows: Range<usize>) -> Vec<Vec<(Range<usize>, Role)>> {
            vec![Vec::new(); rows.end.saturating_sub(rows.start)]
        }
    }
}

#[cfg(feature = "syntax")]
mod engine {
    use super::{refuses, role_of, Role, MAX_BYTES};
    use std::cell::RefCell;
    use std::ops::Range;
    use tree_sitter::{InputEdit, Parser, Point, Query, QueryCursor, StreamingIterator, Tree};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Lang {
        #[cfg(feature = "lang-json")]
        Json,
        #[cfg(feature = "lang-javascript")]
        JavaScript,
        #[cfg(feature = "lang-typescript")]
        TypeScript,
        #[cfg(feature = "lang-typescript")]
        Tsx,
        #[cfg(feature = "lang-rust")]
        Rust,
        #[cfg(feature = "lang-bash")]
        Bash,
        #[cfg(feature = "lang-java")]
        Java,
        #[cfg(feature = "lang-toml")]
        Toml,
        #[cfg(feature = "lang-yaml")]
        Yaml,
        #[cfg(feature = "lang-python")]
        Python,
        #[cfg(feature = "lang-css")]
        Css,
        #[cfg(feature = "lang-html")]
        Html,
        #[cfg(feature = "lang-go")]
        Go,
    }

    fn ext_lang(ext: &str) -> Option<Lang> {
        match ext {
            #[cfg(feature = "lang-json")]
            "json" | "jsonc" => Some(Lang::Json),
            #[cfg(feature = "lang-javascript")]
            "js" | "mjs" | "cjs" | "jsx" => Some(Lang::JavaScript),
            #[cfg(feature = "lang-typescript")]
            "ts" | "mts" | "cts" => Some(Lang::TypeScript),
            #[cfg(feature = "lang-typescript")]
            "tsx" => Some(Lang::Tsx),
            #[cfg(feature = "lang-rust")]
            "rs" => Some(Lang::Rust),
            #[cfg(feature = "lang-bash")]
            "sh" | "bash" | "zsh" => Some(Lang::Bash),
            #[cfg(feature = "lang-java")]
            "java" => Some(Lang::Java),
            #[cfg(feature = "lang-toml")]
            "toml" => Some(Lang::Toml),
            #[cfg(feature = "lang-yaml")]
            "yaml" | "yml" => Some(Lang::Yaml),
            #[cfg(feature = "lang-python")]
            "py" | "pyi" | "pyw" => Some(Lang::Python),
            #[cfg(feature = "lang-css")]
            "css" => Some(Lang::Css),
            #[cfg(feature = "lang-html")]
            "html" | "htm" | "xhtml" => Some(Lang::Html),
            #[cfg(feature = "lang-go")]
            "go" => Some(Lang::Go),
            _ => None,
        }
    }

    fn fence_lang(word: &str) -> Option<Lang> {
        match word {
            #[cfg(feature = "lang-json")]
            "json" | "jsonc" => Some(Lang::Json),
            #[cfg(feature = "lang-javascript")]
            "js" | "javascript" | "jsx" | "mjs" => Some(Lang::JavaScript),
            #[cfg(feature = "lang-typescript")]
            "ts" | "typescript" => Some(Lang::TypeScript),
            #[cfg(feature = "lang-typescript")]
            "tsx" => Some(Lang::Tsx),
            #[cfg(feature = "lang-rust")]
            "rust" | "rs" => Some(Lang::Rust),
            #[cfg(feature = "lang-bash")]
            "sh" | "bash" | "shell" | "zsh" | "shellscript" => Some(Lang::Bash),
            #[cfg(feature = "lang-java")]
            "java" => Some(Lang::Java),
            #[cfg(feature = "lang-toml")]
            "toml" => Some(Lang::Toml),
            #[cfg(feature = "lang-yaml")]
            "yaml" | "yml" => Some(Lang::Yaml),
            #[cfg(feature = "lang-python")]
            "python" | "py" | "python3" => Some(Lang::Python),
            #[cfg(feature = "lang-css")]
            "css" => Some(Lang::Css),
            #[cfg(feature = "lang-html")]
            "html" | "htm" => Some(Lang::Html),
            #[cfg(feature = "lang-go")]
            "go" | "golang" => Some(Lang::Go),
            _ => None,
        }
    }

    #[cfg(feature = "lang-python")]
    fn is_python(name: &str) -> bool {
        let Some(rest) = name.strip_prefix("python") else { return false };
        rest.is_empty() || rest.starts_with(|c: char| c.is_ascii_digit())
    }

    fn shebang(line: &str) -> Option<Lang> {
        let rest = line.trim().strip_prefix("#!")?;
        let mut parts = rest.split_whitespace();
        let prog = parts.next()?.rsplit('/').next()?;
        let name = if prog == "env" { parts.next()? } else { prog };
        match name {
            #[cfg(feature = "lang-bash")]
            "bash" | "sh" | "zsh" => Some(Lang::Bash),
            #[cfg(feature = "lang-javascript")]
            "node" => Some(Lang::JavaScript),
            #[cfg(feature = "lang-python")]
            name if is_python(name) => Some(Lang::Python),
            _ => None,
        }
    }

    impl Lang {
        pub fn for_file(name: &str, first_line: &str) -> Option<Self> {
            let file = std::path::Path::new(name).file_name().and_then(|s| s.to_str()).unwrap_or(name);
            let lower = file.to_ascii_lowercase();
            if matches!(lower.as_str(), ".bashrc" | ".bash_profile" | ".bash_aliases" | ".zshrc" | ".zprofile" | ".profile" | "pkgbuild") {
                #[cfg(feature = "lang-bash")]
                return Some(Lang::Bash);
                #[cfg(not(feature = "lang-bash"))]
                return None;
            }
            #[cfg(feature = "lang-toml")]
            if lower == "cargo.lock" {
                return Some(Lang::Toml);
            }
            if let Some((stem, ext)) = lower.rsplit_once('.') {
                if !stem.is_empty() {
                    if let Some(lang) = ext_lang(ext) {
                        return Some(lang);
                    }
                }
            }
            shebang(first_line)
        }

        pub fn from_fence(info: &str) -> Option<Self> {
            let word = info.split_whitespace().next()?.to_ascii_lowercase();
            fence_lang(&word)
        }

        pub(crate) fn language(self) -> tree_sitter::Language {
            match self {
                #[cfg(feature = "lang-json")]
                Lang::Json => tree_sitter_json::LANGUAGE.into(),
                #[cfg(feature = "lang-javascript")]
                Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
                #[cfg(feature = "lang-typescript")]
                Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                #[cfg(feature = "lang-typescript")]
                Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
                #[cfg(feature = "lang-rust")]
                Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
                #[cfg(feature = "lang-bash")]
                Lang::Bash => tree_sitter_bash::LANGUAGE.into(),
                #[cfg(feature = "lang-java")]
                Lang::Java => tree_sitter_java::LANGUAGE.into(),
                #[cfg(feature = "lang-toml")]
                Lang::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
                #[cfg(feature = "lang-yaml")]
                Lang::Yaml => tree_sitter_yaml::LANGUAGE.into(),
                #[cfg(feature = "lang-python")]
                Lang::Python => tree_sitter_python::LANGUAGE.into(),
                #[cfg(feature = "lang-css")]
                Lang::Css => tree_sitter_css::LANGUAGE.into(),
                #[cfg(feature = "lang-html")]
                Lang::Html => tree_sitter_html::LANGUAGE.into(),
                #[cfg(feature = "lang-go")]
                Lang::Go => tree_sitter_go::LANGUAGE.into(),
                #[cfg(not(any(
                    feature = "lang-json",
                    feature = "lang-javascript",
                    feature = "lang-typescript",
                    feature = "lang-rust",
                    feature = "lang-bash",
                    feature = "lang-java",
                    feature = "lang-toml",
                    feature = "lang-yaml",
                    feature = "lang-python",
                    feature = "lang-css",
                    feature = "lang-html",
                    feature = "lang-go"
                )))]
                _ => match self {},
            }
        }

        pub(crate) fn queries(self) -> String {
            match self {
                #[cfg(feature = "lang-json")]
                Lang::Json => tree_sitter_json::HIGHLIGHTS_QUERY.to_string(),
                #[cfg(feature = "lang-javascript")]
                Lang::JavaScript => format!("{}\n{}", tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY),
                #[cfg(feature = "lang-typescript")]
                Lang::TypeScript => format!("{}\n{}", tree_sitter_typescript::HIGHLIGHTS_QUERY, tree_sitter_javascript::HIGHLIGHT_QUERY),
                #[cfg(feature = "lang-typescript")]
                Lang::Tsx => format!(
                    "{}\n{}\n{}",
                    tree_sitter_typescript::HIGHLIGHTS_QUERY,
                    tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                    tree_sitter_javascript::HIGHLIGHT_QUERY
                ),
                #[cfg(feature = "lang-rust")]
                Lang::Rust => tree_sitter_rust::HIGHLIGHTS_QUERY.to_string(),
                #[cfg(feature = "lang-bash")]
                Lang::Bash => tree_sitter_bash::HIGHLIGHT_QUERY.to_string(),
                #[cfg(feature = "lang-java")]
                Lang::Java => tree_sitter_java::HIGHLIGHTS_QUERY.to_string(),
                #[cfg(feature = "lang-toml")]
                Lang::Toml => tree_sitter_toml_ng::HIGHLIGHTS_QUERY.to_string(),
                #[cfg(feature = "lang-yaml")]
                Lang::Yaml => tree_sitter_yaml::HIGHLIGHTS_QUERY.to_string(),
                #[cfg(feature = "lang-python")]
                Lang::Python => tree_sitter_python::HIGHLIGHTS_QUERY.to_string(),
                #[cfg(feature = "lang-css")]
                Lang::Css => tree_sitter_css::HIGHLIGHTS_QUERY.to_string(),
                #[cfg(feature = "lang-html")]
                Lang::Html => tree_sitter_html::HIGHLIGHTS_QUERY.to_string(),
                #[cfg(feature = "lang-go")]
                Lang::Go => tree_sitter_go::HIGHLIGHTS_QUERY.to_string(),
                #[cfg(not(any(
                    feature = "lang-json",
                    feature = "lang-javascript",
                    feature = "lang-typescript",
                    feature = "lang-rust",
                    feature = "lang-bash",
                    feature = "lang-java",
                    feature = "lang-toml",
                    feature = "lang-yaml",
                    feature = "lang-python",
                    feature = "lang-css",
                    feature = "lang-html",
                    feature = "lang-go"
                )))]
                _ => match self {},
            }
        }
    }

    struct Grammar {
        query: Query,
        roles: Vec<Role>,
    }

    fn with_grammar<T>(lang: Lang, f: impl FnOnce(&Grammar) -> T) -> Option<T> {
        thread_local! {
            static CACHE: RefCell<Vec<(Lang, Option<Grammar>)>> = RefCell::new(Vec::new());
        }
        CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some((_, ready)) = cache.iter().find(|(cached, _)| *cached == lang) {
                return ready.as_ref().map(f);
            }
            let loaded = Query::new(&lang.language(), &lang.queries()).ok().map(|query| {
                let roles = query.capture_names().iter().map(|name| role_of(name)).collect();
                Grammar { query, roles }
            });
            cache.push((lang, loaded));
            cache.last().and_then(|(_, ready)| ready.as_ref().map(f))
        })
    }

    pub struct Highlighter {
        lang: Lang,
        parser: Parser,
        tree: Tree,
        source: String,
        lines: Vec<String>,
        starts: Vec<usize>,
    }

    impl Highlighter {
        pub fn new(lang: Lang, lines: &[String]) -> Option<Self> {
            if refuses(lang, lines) {
                return None;
            }
            let mut parser = Parser::new();
            parser.set_language(&lang.language()).ok()?;
            let source = join(lines);
            if source.len() > MAX_BYTES {
                return None;
            }
            let tree = parser.parse(&source, None)?;
            Some(Self { lang, parser, tree, source, lines: lines.to_vec(), starts: line_starts(lines) })
        }

        pub fn update(&mut self, lines: &[String]) {
            if lines == self.lines.as_slice() {
                return;
            }
            let new_source = join(lines);
            if let Some(edit) = input_edit(&self.lines, &self.source, lines, &new_source) {
                self.tree.edit(&edit);
                if let Some(tree) = self.parser.parse(&new_source, Some(&self.tree)) {
                    self.tree = tree;
                } else if let Some(tree) = self.parser.parse(&new_source, None) {
                    self.tree = tree;
                }
            }
            self.source = new_source;
            self.lines = lines.to_vec();
            self.starts = line_starts(&self.lines);
        }

        pub fn roles(&self, rows: Range<usize>) -> Vec<Vec<(Range<usize>, Role)>> {
            let n = rows.end.saturating_sub(rows.start);
            let mut painted: Vec<Vec<Role>> = (0..n)
                .map(|i| {
                    let line = rows.start + i;
                    if line < self.lines.len() {
                        vec![Role::Plain; self.lines[line].chars().count()]
                    } else {
                        Vec::new()
                    }
                })
                .collect();
            if n == 0 || self.source.is_empty() {
                return compress(&painted);
            }
            let Some(()) = with_grammar(self.lang, |grammar| {
                let start_byte = self.starts.get(rows.start).copied().unwrap_or(self.source.len()).min(self.source.len());
                let end_byte = if rows.end >= self.lines.len() {
                    self.source.len()
                } else {
                    self.starts.get(rows.end).copied().unwrap_or(self.source.len())
                };
                let end_byte = end_byte.max(start_byte).min(self.source.len());
                if start_byte >= self.source.len() {
                    return;
                }
                let mut cursor = QueryCursor::new();
                cursor.set_byte_range(start_byte..end_byte.max(start_byte + 1).min(self.source.len().max(start_byte + 1)));
                let mut caps = cursor.captures(&grammar.query, self.tree.root_node(), self.source.as_bytes());
                let mut raw = Vec::new();
                while let Some((mat, idx)) = caps.next() {
                    let idx = *idx;
                    let capture = mat.captures()[idx];
                    let node = capture.node;
                    raw.push(Cap {
                        id: node.id(),
                        start: node.start_byte(),
                        end: node.end_byte(),
                        role: grammar.roles.get(capture.index as usize).copied().unwrap_or(Role::Plain),
                    });
                }
                let mut collapsed = Vec::new();
                for cap in raw {
                    if collapsed.last().is_some_and(|prev: &Cap| prev.id == cap.id) {
                        collapsed.pop();
                    }
                    if cap.end > cap.start {
                        collapsed.push(cap);
                    }
                }
                paint(&mut painted, &self.lines, &self.starts, rows.start, &self.source, &collapsed);
            }) else {
                return vec![Vec::new(); n];
            };
            compress(&painted)
        }
    }

    struct Cap {
        id: usize,
        start: usize,
        end: usize,
        role: Role,
    }

    fn paint(painted: &mut [Vec<Role>], lines: &[String], starts: &[usize], row0: usize, source: &str, caps: &[Cap]) {
        let mut role_stack: Vec<Role> = Vec::new();
        let mut end_stack: Vec<usize> = Vec::new();
        let mut byte = 0usize;
        let mut i = 0usize;
        let len = source.len();
        while i < caps.len() || end_stack.last().copied().unwrap_or(0) > byte {
            let next = caps.get(i).map(|c| c.start).unwrap_or(len);
            if let Some(&end) = end_stack.last() {
                if end <= byte {
                    end_stack.pop();
                    role_stack.pop();
                    continue;
                }
                if end <= next {
                    let role = role_stack.last().copied().unwrap_or(Role::Plain);
                    apply_bytes(painted, lines, starts, row0, byte, end, role);
                    byte = end;
                    end_stack.pop();
                    role_stack.pop();
                    continue;
                }
            }
            if i >= caps.len() {
                break;
            }
            let cap = &caps[i];
            i += 1;
            if byte < cap.start {
                let role = role_stack.last().copied().unwrap_or(Role::Plain);
                apply_bytes(painted, lines, starts, row0, byte, cap.start, role);
                byte = cap.start;
            }
            if cap.end > byte {
                role_stack.push(cap.role);
                end_stack.push(cap.end);
            }
        }
    }

    fn apply_bytes(painted: &mut [Vec<Role>], lines: &[String], starts: &[usize], row0: usize, from: usize, to: usize, role: Role) {
        if from >= to || role == Role::Plain || starts.is_empty() {
            return;
        }
        let start_line = starts.partition_point(|&s| s <= from).saturating_sub(1);
        let end_line = starts.partition_point(|&s| s < to).saturating_sub(1);
        for line in start_line..=end_line {
            if line >= lines.len() || line < row0 || line - row0 >= painted.len() {
                continue;
            }
            let origin = starts[line];
            let len = lines[line].len();
            let local_from = from.max(origin).saturating_sub(origin).min(len);
            let local_to = to.min(origin + len).saturating_sub(origin);
            if local_from >= local_to {
                continue;
            }
            let cs = char_at(&lines[line], local_from);
            let ce = char_at(&lines[line], local_to);
            for slot in painted[line - row0].iter_mut().take(ce).skip(cs) {
                *slot = role;
            }
        }
    }

    fn char_at(line: &str, byte: usize) -> usize {
        let byte = byte.min(line.len());
        let byte = (0..=byte).rev().find(|b| line.is_char_boundary(*b)).unwrap_or(0);
        line[..byte].chars().count()
    }

    fn compress(painted: &[Vec<Role>]) -> Vec<Vec<(Range<usize>, Role)>> {
        painted
            .iter()
            .map(|roles| {
                let mut out = Vec::new();
                let mut i = 0;
                while i < roles.len() {
                    let role = roles[i];
                    let start = i;
                    while i < roles.len() && roles[i] == role {
                        i += 1;
                    }
                    if role != Role::Plain {
                        out.push((start..i, role));
                    }
                }
                out
            })
            .collect()
    }

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

    fn point_of(lines: &[String], line: usize) -> Point {
        if lines.is_empty() {
            return Point { row: 0, column: 0 };
        }
        if line >= lines.len() {
            Point { row: lines.len() - 1, column: lines.last().map(|l| l.len()).unwrap_or(0) }
        } else {
            Point { row: line, column: 0 }
        }
    }

    fn byte_of(starts: &[usize], src_len: usize, line: usize, len: usize) -> usize {
        if line >= len { src_len } else { starts.get(line).copied().unwrap_or(src_len) }
    }

    fn input_edit(old_lines: &[String], old_src: &str, new_lines: &[String], new_src: &str) -> Option<InputEdit> {
        let mut prefix = 0;
        while prefix < old_lines.len() && prefix < new_lines.len() && old_lines[prefix] == new_lines[prefix] {
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < old_lines.len() - prefix && suffix < new_lines.len() - prefix && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix] {
            suffix += 1;
        }
        if prefix == old_lines.len() && prefix == new_lines.len() {
            return None;
        }
        let old_starts = line_starts(old_lines);
        let new_starts = line_starts(new_lines);
        let old_end_line = old_lines.len() - suffix;
        let new_end_line = new_lines.len() - suffix;
        let mut start_byte = byte_of(&old_starts, old_src.len(), prefix, old_lines.len());
        let old_end_byte = byte_of(&old_starts, old_src.len(), old_end_line, old_lines.len());
        let new_end_byte = byte_of(&new_starts, new_src.len(), new_end_line, new_lines.len());
        let mut start_position = if prefix >= old_lines.len() {
            point_of(old_lines, old_lines.len())
        } else {
            Point { row: prefix, column: 0 }
        };
        if suffix == 0 && prefix > 0 && prefix < old_lines.len() {
            start_byte = old_starts[prefix - 1] + old_lines[prefix - 1].len();
            start_position = Point { row: prefix - 1, column: old_lines[prefix - 1].len() };
        }
        let old_end_position = if old_end_line >= old_lines.len() {
            point_of(old_lines, old_lines.len())
        } else {
            Point { row: old_end_line, column: 0 }
        };
        let new_end_position = if new_end_line >= new_lines.len() {
            point_of(new_lines, new_lines.len())
        } else {
            Point { row: new_end_line, column: 0 }
        };
        Some(InputEdit { start_byte, old_end_byte, new_end_byte, start_position, old_end_position, new_end_position })
    }
}

pub use engine::{Highlighter, Lang};

pub fn refuses(lang: Lang, lines: &[String]) -> bool {
    if too_large(lines) {
        return true;
    }
    #[cfg(feature = "lang-bash")]
    if matches!(lang, Lang::Bash) && lines.len() > BASH_MAX_LINES {
        return true;
    }
    let _ = lang;
    false
}

#[cfg(not(feature = "syntax"))]
#[cfg(test)]
mod off {
    use super::*;

    #[test]
    fn highlighting_is_off_without_the_engine() {
        assert!(Lang::for_file("main.rs", "#!/usr/bin/env node").is_none());
        assert!(Lang::from_fence("rust").is_none());
        assert!(too_large(&vec![String::new(); MAX_LINES + 1]));
    }
}

#[cfg(feature = "syntax")]
#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Range;

    fn lines_of(text: &str) -> Vec<String> {
        text.split('\n').map(|s| s.to_string()).collect()
    }

    fn langs() -> Vec<Lang> {
        let mut out = Vec::new();
        #[cfg(feature = "lang-json")]
        out.push(Lang::Json);
        #[cfg(feature = "lang-javascript")]
        out.push(Lang::JavaScript);
        #[cfg(feature = "lang-typescript")]
        out.push(Lang::TypeScript);
        #[cfg(feature = "lang-typescript")]
        out.push(Lang::Tsx);
        #[cfg(feature = "lang-rust")]
        out.push(Lang::Rust);
        #[cfg(feature = "lang-bash")]
        out.push(Lang::Bash);
        #[cfg(feature = "lang-java")]
        out.push(Lang::Java);
        #[cfg(feature = "lang-toml")]
        out.push(Lang::Toml);
        #[cfg(feature = "lang-yaml")]
        out.push(Lang::Yaml);
        #[cfg(feature = "lang-python")]
        out.push(Lang::Python);
        #[cfg(feature = "lang-css")]
        out.push(Lang::Css);
        #[cfg(feature = "lang-html")]
        out.push(Lang::Html);
        #[cfg(feature = "lang-go")]
        out.push(Lang::Go);
        out
    }

    #[test]
    fn languages_come_from_the_name_the_shebang_and_the_fence() {
        let file = |name, first, lang: Option<Lang>| (name, first, lang, false);
        let fence = |info, lang: Option<Lang>| (info, "", lang, true);
        #[allow(unused_mut)]
        let mut cases = vec![
            file("notes.md", "", None),
            file("notes.markdown", "# hi", None),
            file("a.txt", "", None),
            file("a.txt", "#!/usr/bin/env ruby", None),
            file("Makefile", "", None),
        ];
        #[cfg(feature = "lang-json")]
        cases.extend([file("a.json", "", Some(Lang::Json)), file("a.JSONC", "", Some(Lang::Json)), fence("JSON extra", Some(Lang::Json)), fence("jsonc", Some(Lang::Json))]);
        #[cfg(feature = "lang-javascript")]
        cases.extend([
            file("a.js", "", Some(Lang::JavaScript)),
            file("a.MJS", "", Some(Lang::JavaScript)),
            file("a.cjs", "", Some(Lang::JavaScript)),
            file("a.jsx", "", Some(Lang::JavaScript)),
            file("run", "#!/usr/bin/env node", Some(Lang::JavaScript)),
            file("run", "#!/usr/bin/node", Some(Lang::JavaScript)),
            fence("javascript", Some(Lang::JavaScript)),
            fence("JS", Some(Lang::JavaScript)),
            fence("jsx", Some(Lang::JavaScript)),
            fence("mjs", Some(Lang::JavaScript)),
        ]);
        #[cfg(feature = "lang-typescript")]
        cases.extend([
            file("a.ts", "", Some(Lang::TypeScript)),
            file("a.mts", "", Some(Lang::TypeScript)),
            file("a.CTS", "", Some(Lang::TypeScript)),
            file("a.tsx", "", Some(Lang::Tsx)),
            fence("ts", Some(Lang::TypeScript)),
            fence("typescript", Some(Lang::TypeScript)),
            fence("tsx", Some(Lang::Tsx)),
        ]);
        #[cfg(feature = "lang-rust")]
        cases.extend([file("a.rs", "", Some(Lang::Rust)), file("lib.RS", "", Some(Lang::Rust)), fence("rust", Some(Lang::Rust)), fence("rs", Some(Lang::Rust))]);
        #[cfg(feature = "lang-bash")]
        cases.extend([
            file("a.sh", "", Some(Lang::Bash)),
            file("a.bash", "", Some(Lang::Bash)),
            file("a.zsh", "", Some(Lang::Bash)),
            file(".bashrc", "", Some(Lang::Bash)),
            file(".bash_profile", "", Some(Lang::Bash)),
            file(".bash_aliases", "", Some(Lang::Bash)),
            file(".zshrc", "", Some(Lang::Bash)),
            file(".zprofile", "", Some(Lang::Bash)),
            file(".profile", "", Some(Lang::Bash)),
            file("PKGBUILD", "", Some(Lang::Bash)),
            file("pkgbuild", "", Some(Lang::Bash)),
            file("dir/PKGBUILD", "", Some(Lang::Bash)),
            file("script", "#!/bin/bash", Some(Lang::Bash)),
            file("script", "#!/bin/sh", Some(Lang::Bash)),
            file("script", "#!/usr/bin/bash", Some(Lang::Bash)),
            file("script", "#!/usr/bin/env bash", Some(Lang::Bash)),
            file("script", "#!/usr/bin/env sh", Some(Lang::Bash)),
            file("script", "#!/usr/bin/env zsh", Some(Lang::Bash)),
            fence("sh", Some(Lang::Bash)),
            fence("bash", Some(Lang::Bash)),
            fence("shell", Some(Lang::Bash)),
            fence("zsh", Some(Lang::Bash)),
            fence("shellscript", Some(Lang::Bash)),
        ]);
        #[cfg(feature = "lang-java")]
        cases.extend([file("A.java", "", Some(Lang::Java)), file("A.JAVA", "", Some(Lang::Java)), fence("java", Some(Lang::Java))]);
        #[cfg(feature = "lang-toml")]
        cases.extend([
            file("Cargo.toml", "", Some(Lang::Toml)),
            file("a.TOML", "", Some(Lang::Toml)),
            file("Cargo.lock", "", Some(Lang::Toml)),
            fence("toml", Some(Lang::Toml)),
            fence("TOML", Some(Lang::Toml)),
        ]);
        #[cfg(feature = "lang-yaml")]
        cases.extend([file("a.yaml", "", Some(Lang::Yaml)), file("a.YML", "", Some(Lang::Yaml)), fence("yaml", Some(Lang::Yaml)), fence("yml", Some(Lang::Yaml))]);
        #[cfg(feature = "lang-python")]
        cases.extend([
            file("a.py", "", Some(Lang::Python)),
            file("a.PYI", "", Some(Lang::Python)),
            file("a.pyw", "", Some(Lang::Python)),
            file("run", "#!/usr/bin/env python", Some(Lang::Python)),
            file("run", "#!/usr/bin/python3", Some(Lang::Python)),
            file("run", "#!/usr/bin/python3.13", Some(Lang::Python)),
            fence("python", Some(Lang::Python)),
            fence("py", Some(Lang::Python)),
            fence("python3", Some(Lang::Python)),
        ]);
        #[cfg(feature = "lang-css")]
        cases.extend([file("a.css", "", Some(Lang::Css)), file("a.CSS", "", Some(Lang::Css)), fence("css", Some(Lang::Css))]);
        #[cfg(feature = "lang-html")]
        cases.extend([
            file("a.html", "", Some(Lang::Html)),
            file("a.HTM", "", Some(Lang::Html)),
            file("a.xhtml", "", Some(Lang::Html)),
            fence("html", Some(Lang::Html)),
            fence("htm", Some(Lang::Html)),
        ]);
        #[cfg(feature = "lang-go")]
        cases.extend([file("a.go", "", Some(Lang::Go)), file("a.GO", "", Some(Lang::Go)), fence("go", Some(Lang::Go)), fence("golang", Some(Lang::Go))]);
        for (name, first, lang, is_fence) in cases {
            let got = if is_fence { Lang::from_fence(name) } else { Lang::for_file(name, first) };
            assert_eq!(got, lang, "{name} {first}");
        }
    }

    #[test]
    fn capture_names_map_to_roles_by_their_longest_prefix() {
        let cases = [
            ("keyword", Role::Keyword),
            ("keyword.control", Role::Keyword),
            ("tag", Role::Tag),
            ("tag.attribute", Role::Tag),
            ("string", Role::String),
            ("string.special", Role::String),
            ("string.special.url", Role::String),
            ("string.special.key", Role::Escape),
            ("string.escape", Role::Escape),
            ("escape", Role::Escape),
            ("number", Role::Constant),
            ("constant", Role::Constant),
            ("constant.builtin", Role::Constant),
            ("boolean", Role::Constant),
            ("comment", Role::Comment),
            ("comment.line", Role::Comment),
            ("function", Role::Function),
            ("function.method", Role::Function),
            ("function.macro", Role::Function),
            ("function.builtin", Role::Function),
            ("constructor", Role::Function),
            ("type", Role::Type),
            ("type.builtin", Role::Type),
            ("module", Role::Type),
            ("attribute", Role::Type),
            ("variable.builtin", Role::Builtin),
            ("variable.parameter", Role::Property),
            ("property", Role::Property),
            ("property.definition", Role::Property),
            ("label", Role::Property),
            ("operator", Role::Punctuation),
            ("punctuation.bracket", Role::Punctuation),
            ("punctuation.delimiter", Role::Punctuation),
            ("embedded", Role::Punctuation),
            ("variable", Role::Plain),
            ("variable.other", Role::Plain),
            ("@keyword", Role::Keyword),
            ("not-a-role", Role::Plain),
            ("spell", Role::Plain),
        ];
        for (name, role) in cases {
            assert_eq!(role_of(name), role, "{name}");
        }
    }

    fn expand(lines: &[String], roles: &[Vec<(Range<usize>, Role)>]) -> Vec<Vec<Role>> {
        lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let mut row = vec![Role::Plain; line.chars().count()];
                for (range, role) in &roles[i] {
                    for slot in row.iter_mut().take(range.end).skip(range.start) {
                        *slot = *role;
                    }
                }
                row
            })
            .collect()
    }

    fn oracle(lang: Lang, source: &str) -> Vec<Vec<Role>> {
        use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter as Official};
        let mut config = HighlightConfiguration::new(lang.language(), "sample", &lang.queries(), "", "").unwrap_or_else(|err| panic!("{lang:?}: {err}"));
        let names: Vec<String> = config.names().iter().map(|s| (*s).to_string()).collect();
        config.configure(&names);
        let mut official = Official::new();
        let events = official.highlight(&config, source.as_bytes(), None, None, |_| None).unwrap_or_else(|err| panic!("{lang:?}: {err}"));
        let mut bytes = vec![Role::Plain; source.len()];
        let mut stack = Vec::new();
        for event in events {
            match event.unwrap() {
                HighlightEvent::HighlightStart(Highlight(i)) => stack.push(role_of(&names[i])),
                HighlightEvent::HighlightEnd => {
                    stack.pop();
                }
                HighlightEvent::Source { start, end } => {
                    let role = stack.last().copied().unwrap_or(Role::Plain);
                    if role != Role::Plain {
                        for slot in bytes.iter_mut().take(end).skip(start) {
                            *slot = role;
                        }
                    }
                }
            }
        }
        let mut rows = Vec::new();
        let mut row = Vec::new();
        for (i, ch) in source.char_indices() {
            if ch == '\n' {
                rows.push(std::mem::take(&mut row));
            } else {
                row.push(bytes[i]);
            }
        }
        rows.push(row);
        rows
    }

    fn assert_matches_official(lang: Lang, text: &str) {
        let lines = lines_of(text);
        let hl = Highlighter::new(lang, &lines).unwrap_or_else(|| panic!("{lang:?} did not highlight"));
        let got = expand(&lines, &hl.roles(0..lines.len()));
        let expect = oracle(lang, &hl_source(&lines));
        assert_eq!(got, expect, "{lang:?} roles diverged from tree-sitter-highlight\n{text}");
    }

    fn hl_source(lines: &[String]) -> String {
        let mut source = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                source.push('\n');
            }
            source.push_str(line);
        }
        source
    }

    #[test]
    fn roles_match_tree_sitter_highlight() {
        #[cfg(feature = "lang-rust")]
        assert_matches_official(Lang::Rust, "fn main() {\n    let s = \"a\\nb\"; // note\n    let n = 1;\n}\n");
        #[cfg(feature = "lang-javascript")]
        assert_matches_official(Lang::JavaScript, "function go(name) {\n  const s = `hello\n${name}`;\n  return s; // done\n}\n");
        #[cfg(feature = "lang-typescript")]
        assert_matches_official(Lang::TypeScript, "interface User { name: string }\nfunction greet(user: User): string {\n  return user.name;\n}\n");
        #[cfg(feature = "lang-typescript")]
        assert_matches_official(Lang::Tsx, "function App() {\n  return <Button kind=\"go\">hi</Button>;\n}\n");
        #[cfg(feature = "lang-json")]
        assert_matches_official(Lang::Json, "{\n  \"name\": \"ee\",\n  \"ok\": true,\n  \"n\": 1\n}\n");
        #[cfg(feature = "lang-bash")]
        assert_matches_official(Lang::Bash, "#!/bin/bash\necho \"hi\" # note\nif true; then\n  echo done\nfi\n");
        #[cfg(feature = "lang-java")]
        assert_matches_official(Lang::Java, "class App {\n  int n = 1;\n  String greet() { return \"hi\"; } // note\n}\n");
        #[cfg(feature = "lang-toml")]
        assert_matches_official(Lang::Toml, "[package]\nname = \"ee\" # note\nok = true\nn = 1\n");
        #[cfg(feature = "lang-yaml")]
        assert_matches_official(Lang::Yaml, "name: ee # note\nok: true\nn: 1\nitems:\n  - one\n");
        #[cfg(feature = "lang-python")]
        assert_matches_official(Lang::Python, "def greet(name):\n    s = \"hi\"  # note\n    n = 1\n    return s\n");
        #[cfg(feature = "lang-css")]
        assert_matches_official(Lang::Css, "/* note */\nbody {\n  color: #fff;\n  margin: 0;\n}\n");
        #[cfg(feature = "lang-html")]
        assert_matches_official(Lang::Html, "<!-- note -->\n<p class=\"hi\">hello</p>\n");
        #[cfg(feature = "lang-go")]
        assert_matches_official(Lang::Go, "package main\n\nfunc greet(name string) string {\n    return \"hi\" // note\n}\n");
    }

    #[test]
    fn multi_byte_characters_take_one_column_each() {
        #[cfg(feature = "lang-rust")]
        {
            let text = "let s = \"é😀\";";
            let lines = lines_of(text);
            let hl = Highlighter::new(Lang::Rust, &lines).unwrap();
            let rows = expand(&lines, &hl.roles(0..1));
            let chars: Vec<char> = text.chars().collect();
            assert_eq!(rows[0].len(), chars.len());
            let quote = chars.iter().position(|c| *c == '"').unwrap();
            assert_eq!(rows[0][quote], Role::String);
            assert_eq!(rows[0][quote + 1], Role::String, "é");
            assert_eq!(rows[0][quote + 2], Role::String, "emoji is one column");
            assert_eq!(rows[0][0], Role::Keyword, "let");
        }
    }

    #[test]
    fn a_block_comment_covers_every_line_including_a_window_in_the_middle() {
        #[cfg(feature = "lang-rust")]
        {
            let lines = lines_of("fn main() {\n    /*\n    inner\n    */\n}\n");
            let hl = Highlighter::new(Lang::Rust, &lines).unwrap();
            let all = expand(&lines, &hl.roles(0..lines.len()));
            assert!(all[1].iter().any(|r| *r == Role::Comment), "opener: {:?}", all[1]);
            assert!(all[2].iter().all(|r| *r == Role::Comment), "middle: {:?}", all[2]);
            assert!(all[3].iter().any(|r| *r == Role::Comment), "closer: {:?}", all[3]);
            let middle = expand(&lines[2..3], &hl.roles(2..3));
            assert!(middle[0].iter().all(|r| *r == Role::Comment), "querying only the middle line still sees the comment: {:?}", middle[0]);
        }
    }

    #[test]
    fn files_over_the_size_guard_are_not_highlighted() {
        let lang = langs()[0];
        let mut lines = vec!["fn main() {}".to_string()];
        lines.extend(std::iter::repeat_n(String::new(), MAX_LINES));
        assert!(too_large(&lines));
        assert!(Highlighter::new(lang, &lines).is_none());
        let huge = vec!["x".repeat(MAX_BYTES + 1)];
        assert!(too_large(&huge));
        assert!(Highlighter::new(lang, &huge).is_none());
        let ok = vec![String::new(); MAX_LINES];
        assert!(!too_large(&ok));
    }

    fn mutate(lines: &mut Vec<String>, state: &mut u64) {
        let mut next = || {
            *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            *state
        };
        if lines.is_empty() {
            lines.push(String::new());
        }
        let line = next() as usize % lines.len();
        match next() % 5 {
            0 => {
                let col = next() as usize % (lines[line].chars().count() + 1);
                let ch = ['a', ' ', '_', '1', '{', '}', 'é'][next() as usize % 7];
                let byte: usize = lines[line].chars().take(col).map(|c| c.len_utf8()).sum();
                lines[line].insert(byte, ch);
            }
            1 => {
                let count = lines[line].chars().count();
                if count == 0 {
                    return;
                }
                let col = next() as usize % count;
                let mut chars: Vec<char> = lines[line].chars().collect();
                chars.remove(col);
                lines[line] = chars.into_iter().collect();
            }
            2 => lines.insert(line, "let x = 1;".into()),
            3 => {
                if lines.len() > 1 {
                    lines.remove(line);
                }
            }
            _ => {
                let end = (line + 1 + next() as usize % 3).min(lines.len());
                lines.splice(line..end, std::iter::once(format!("// {}", next() % 1000)));
            }
        }
    }

    #[test]
    fn incremental_edits_match_a_fresh_parse() {
        let samples: Vec<(Lang, Vec<String>)> = vec![
            #[cfg(feature = "lang-rust")]
            (Lang::Rust, lines_of(include_str!("state.rs"))),
            #[cfg(feature = "lang-typescript")]
            (
                Lang::TypeScript,
                lines_of("interface User {\n  name: string;\n  age: number;\n}\ntype Id = string;\nfunction greet(user: User): string {\n  const label = `hello\n${user.name}`;\n  return label; // done\n}\n"),
            ),
        ];
        for (lang, start) in samples {
            let mut lines = start;
            let mut hl = Highlighter::new(lang, &lines).unwrap();
            let mut state = 0x1234_5678_u64;
            for n in 0..200 {
                mutate(&mut lines, &mut state);
                hl.update(&lines);
                let got = hl.roles(0..lines.len());
                let fresh = Highlighter::new(lang, &lines).unwrap().roles(0..lines.len());
                assert_eq!(got, fresh, "{lang:?} diverged after edit {n}");
            }
        }
    }
}
