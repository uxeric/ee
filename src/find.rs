fn chars_eq(a: char, b: char, case_sensitive: bool) -> bool {
    a == b || (!case_sensitive && a.to_lowercase().eq(b.to_lowercase()))
}

pub fn find_all(lines: &[String], query: &str, case_sensitive: bool) -> Vec<(usize, usize, usize)> {
    let q: Vec<char> = query.chars().collect();
    let mut out = Vec::new();
    if q.is_empty() {
        return out;
    }
    for (l, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i + q.len() <= chars.len() {
            if chars[i..i + q.len()]
                .iter()
                .zip(&q)
                .all(|(a, b)| chars_eq(*a, *b, case_sensitive))
            {
                out.push((l, i, i + q.len()));
                i += q.len();
            } else {
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(|s| s.to_string()).collect()
    }

    #[test]
    fn finds_matches_across_lines_without_overlap() {
        let l = lines("abab\nxx ab\naaa");
        assert_eq!(find_all(&l, "ab", true), vec![(0, 0, 2), (0, 2, 4), (1, 3, 5)]);
        assert_eq!(find_all(&l, "aa", true), vec![(2, 0, 2)]);
        assert!(find_all(&l, "", true).is_empty());
    }

    #[test]
    fn case_toggle_and_multibyte_columns() {
        let l = lines("Été été ÉTÉ");
        assert_eq!(find_all(&l, "été", true), vec![(0, 4, 7)]);
        assert_eq!(find_all(&l, "été", false), vec![(0, 0, 3), (0, 4, 7), (0, 8, 11)]);
    }
}
