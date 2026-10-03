/// How many screen rows a source line occupies.
/// The caret can sit one past the last character, so a line whose length is a
/// positive multiple of `width` takes a row that holds only that caret.
pub fn rows_for(len: usize, width: usize) -> usize {
    if width == 0 {
        1
    } else {
        len / width + 1
    }
}

/// Screen rows for already-rendered cells (a preview line, which has no caret).
pub fn display_rows(cells: usize, width: usize) -> usize {
    if width == 0 || cells == 0 {
        1
    } else {
        (cells + width - 1) / width
    }
}

/// Move `(line, col)` by `delta` visual rows.
/// `width == 0` moves by source lines and keeps the source column.
/// Otherwise the visual column from the start of the call is kept, including
/// across a shorter row in the same call. A later call starts from wherever
/// the caret actually landed, so each caret keeps its own column.
pub fn shift(n: usize, len_of: impl Fn(usize) -> usize, mut line: usize, mut col: usize, delta: isize, width: usize) -> (usize, usize) {
    if n == 0 {
        return (0, 0);
    }
    line = line.min(n - 1);
    col = col.min(len_of(line));
    if delta == 0 || width == 0 {
        if width == 0 && delta != 0 {
            let target = line.saturating_add_signed(delta).min(n - 1);
            return (target, col.min(len_of(target)));
        }
        return (line, col);
    }
    let goal = col % width;
    let mut left = delta.unsigned_abs();
    let down = delta > 0;
    while left > 0 {
        let len = len_of(line);
        let vrow = col / width;
        let rows = rows_for(len, width);
        if down {
            if vrow + 1 < rows {
                col = ((vrow + 1) * width + goal).min(len);
                left -= 1;
            } else if line + 1 < n {
                line += 1;
                col = goal.min(len_of(line));
                left -= 1;
            } else {
                break;
            }
        } else if vrow > 0 {
            col = ((vrow - 1) * width + goal).min(len);
            left -= 1;
        } else if line > 0 {
            line -= 1;
            let len = len_of(line);
            let last = rows_for(len, width) - 1;
            col = (last * width + goal).min(len);
            left -= 1;
        } else {
            break;
        }
    }
    (line, col)
}

#[derive(Clone, Debug)]
pub struct Map {
    pub starts: Vec<usize>,
    pub total: usize,
}

impl Map {
    pub fn from_counts(counts: &[usize]) -> Self {
        let mut starts = Vec::with_capacity(counts.len());
        let mut at = 0;
        for &n in counts {
            starts.push(at);
            at += n.max(1);
        }
        Self { starts, total: at }
    }

    pub fn max_top(&self, height: usize) -> usize {
        self.total.saturating_sub(height.max(1))
    }

    pub fn rows_of(&self, line: usize) -> usize {
        let start = self.starts[line];
        self.starts.get(line + 1).copied().unwrap_or(self.total) - start
    }

    pub fn line_at(&self, visual: usize) -> Option<(usize, usize)> {
        if self.starts.is_empty() || visual >= self.total {
            return None;
        }
        let line = match self.starts.binary_search(&visual) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        Some((line, visual - self.starts[line]))
    }

    pub fn source_range(&self, top: usize, height: usize) -> std::ops::Range<usize> {
        if height == 0 || self.starts.is_empty() || self.total == 0 {
            return 0..0;
        }
        let last_visual = self.total - 1;
        let Some((first, _)) = self.line_at(top.min(last_visual)) else {
            return 0..0;
        };
        let Some((last, _)) = self.line_at(top.saturating_add(height - 1).min(last_visual)) else {
            return 0..0;
        };
        first..last + 1
    }

    pub fn cursor_row(&self, line: usize, col: usize, width: usize) -> usize {
        let local = if width == 0 { 0 } else { col / width };
        self.starts.get(line).copied().unwrap_or(0) + local
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lens(lines: &[&str]) -> Vec<usize> {
        lines.iter().map(|s| s.chars().count()).collect()
    }

    fn go(lines: &[&str], line: usize, col: usize, delta: isize, width: usize) -> (usize, usize) {
        let lens = lens(lines);
        shift(lens.len(), |i| lens[i], line, col, delta, width)
    }

    #[test]
    fn row_counts_cover_the_caret_past_the_last_character() {
        let cases = [
            (0, 10, 1),
            (1, 10, 1),
            (9, 10, 1),
            (10, 10, 2),
            (11, 10, 2),
            (20, 10, 3),
            (4, 0, 1),
            (0, 1, 1),
            (1, 1, 2),
            (3, 1, 4),
        ];
        for (len, width, rows) in cases {
            assert_eq!(rows_for(len, width), rows, "len {len} width {width}");
        }
        assert_eq!(display_rows(0, 10), 1);
        assert_eq!(display_rows(10, 10), 1, "a preview line has no caret row");
        assert_eq!(display_rows(11, 10), 2);
    }

    #[test]
    fn every_column_round_trips_through_its_visual_row_including_a_multi_byte_char() {
        let text = "é中abc";
        let len = text.chars().count();
        assert_eq!(len, 5, "each scalar is one column, matching the caret");
        for width in [1usize, 2, 3, 5, 8] {
            for col in 0..=len {
                let (row, visual) = (col / width, col % width);
                assert!(row < rows_for(len, width), "col {col} width {width}");
                assert_eq!(row * width + visual, col);
                let map = Map::from_counts(&[rows_for(len, width)]);
                let (line, local) = map.line_at(row).unwrap();
                assert_eq!((line, local), (0, row));
                let hit = (local * width + visual).min(len);
                assert_eq!(hit, col);
            }
        }
    }

    #[test]
    fn vertical_movement_steps_through_a_wrapped_line_then_the_next_one() {
        let line = "0123456789abcdefghij";
        assert_eq!(line.chars().count(), 20);
        assert_eq!(go(&[line, "short"], 0, 3, 1, 10), (0, 13), "same visual column on the next row");
        assert_eq!(go(&[line, "short"], 0, 13, 1, 10), (0, 20), "the last row holds the caret at the end");
        assert_eq!(go(&[line, "short"], 0, 20, 1, 10), (1, 0), "the next source line keeps the visual column");
        assert_eq!(go(&[line, "short"], 0, 8, 1, 10), (0, 18));
        assert_eq!(go(&[line, "short"], 0, 18, 1, 10), (0, 20), "the blank caret row clamps a column that does not fit");
        let short_end = "0123456789abcde";
        assert_eq!(short_end.chars().count(), 15);
        assert_eq!(go(&[short_end, "zzzzzzzzzz"], 0, 8, 1, 10), (0, 15));
        assert_eq!(go(&[short_end, "zzzzzzzzzz"], 0, 15, 1, 10), (1, 5), "a short final row hands its real column to the next line");
        assert_eq!(go(&[line, "zzzzzzzzzz"], 1, 4, -1, 10), (0, 20));
        assert_eq!(go(&[line, "zzzzzzzzzz"], 0, 20, -1, 10), (0, 10), "up from the blank caret row uses that row's column, which is 0");
        assert_eq!(go(&[line, "short"], 0, 0, -1, 10), (0, 0));
        assert_eq!(go(&[line], 0, 20, 5, 10), (0, 20), "nowhere further to go");
    }

    #[test]
    fn one_move_keeps_the_visual_column_across_a_short_line_and_a_later_call_does_not() {
        let lines = ["0123456789abcdefghij", "ab", "0123456789abcdefghij"];
        assert_eq!(go(&lines, 0, 15, 3, 10), (2, 5), "page across the short line and back to column 5");
        assert_eq!(go(&lines, 0, 15, 1, 10), (0, 20), "first down lands on the end, which this width draws at column 0");
        assert_eq!(go(&lines, 0, 20, 1, 10), (1, 0), "the next down starts from that column");
        assert_eq!(go(&lines, 1, 0, 1, 10), (2, 0));
    }

    #[test]
    fn width_zero_still_moves_by_source_lines_and_keeps_the_source_column() {
        assert_eq!(go(&["abcdef", "ab", "abcdef"], 0, 5, 2, 0), (2, 5));
        assert_eq!(go(&["ab", "cdef"], 1, 4, -1, 0), (0, 2));
        assert_eq!(go(&["ab"], 0, 1, 0, 8), (0, 1));
        assert_eq!(go(&[], 3, 3, 1, 4), (0, 0));
    }

    #[test]
    fn the_map_scrolls_inside_a_wrapped_line_and_names_the_source_lines_on_screen() {
        let counts = [rows_for(25, 10), rows_for(0, 10), rows_for(4, 10)];
        assert_eq!(counts, [3, 1, 1]);
        let map = Map::from_counts(&counts);
        assert_eq!(map.total, 5);
        assert_eq!(map.max_top(2), 3);
        assert_eq!(map.line_at(0), Some((0, 0)));
        assert_eq!(map.line_at(2), Some((0, 2)), "the caret row of the long line");
        assert_eq!(map.line_at(3), Some((1, 0)));
        assert_eq!(map.line_at(5), None);
        assert_eq!(map.cursor_row(0, 25, 10), 2);
        assert_eq!(map.cursor_row(0, 0, 10), 0);
        assert_eq!(map.source_range(1, 3), 0..2, "rows 1, 2 and 3 cover the long line and the blank one");
        assert_eq!(map.source_range(4, 2), 2..3);
        assert_eq!(Map::from_counts(&[]).source_range(0, 4), 0..0);
    }
}
