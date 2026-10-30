/// Pretty-print a JSON buffer when the saved text is packed onto fewer lines
/// than `serde_json`'s pretty form. `None` means there is nothing to offer:
/// the text is not one JSON value, or it already uses at least as many lines.
pub fn pretty_lines(lines: &[String]) -> Option<Vec<String>> {
    let raw = lines.join("\n");
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let pretty = serde_json::to_string_pretty(&value).ok()?;
    if pretty.lines().count() <= trimmed.lines().count() {
        return None;
    }
    let mut out: Vec<String> = pretty.split('\n').map(str::to_string).collect();
    if raw.ends_with('\n') {
        out.push(String::new());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_of(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_string).collect()
    }

    #[test]
    fn only_a_packed_object_or_array_is_offered_and_the_trailing_newline_stays() {
        let packed = pretty_lines(&lines_of("{\"a\":1,\"b\":[2,3]}\n")).unwrap();
        assert!(packed.len() > 2, "{packed:?}");
        assert_eq!(packed.last().map(String::as_str), Some(""));
        let body = packed[..packed.len() - 1].join("\n");
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed, serde_json::json!({"a": 1, "b": [2, 3]}));

        let no_nl = pretty_lines(&lines_of("[1,2]")).unwrap();
        assert!(no_nl.len() > 1, "{no_nl:?}");
        assert_ne!(no_nl.last().map(String::as_str), Some(""));

        let already = serde_json::to_string_pretty(&serde_json::json!({"a": 1})).unwrap();
        let long = serde_json::to_string_pretty(&serde_json::json!({"msg": "x".repeat(80)})).unwrap();
        let silent = [
            &already,
            &format!("{already}\n"),
            "{\n    \"a\": 1\n}\n",
            "{",
            "{ // note\n  \"a\": 1\n}\n",
            "{\"a\":1}\n{\"b\":2}\n",
            "1\n",
            "\"hi\"\n",
            "true\n",
            "{}\n",
            "[]\n",
            "",
            "\n",
            &long,
        ];
        for text in silent {
            assert!(pretty_lines(&lines_of(text)).is_none(), "{text:?}");
        }
    }
}
