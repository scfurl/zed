use std::ops::Range;

use language::{BufferSnapshot, Language, Point};

use crate::code_getter::expand_paragraph;

/// Expand the cursor position to a language-aware block range.
/// Tries eval.scm tree-sitter query first, falls back to heuristics.
pub fn expand_block(
    buffer: &BufferSnapshot,
    cursor: Point,
    language: &Language,
) -> Range<Point> {
    // 1. Try eval.scm query first
    if let Some(range) = crate::eval::find_eval_at(buffer, cursor) {
        return range;
    }

    // 2. Legacy heuristic fallback
    let name = language.name();
    match name.as_ref() {
        "Python" => expand_python(buffer, cursor),
        "R" => expand_r(buffer, cursor),
        "Julia" => expand_julia(buffer, cursor),
        "Markdown" => expand_markdown(buffer, cursor),
        _ => expand_default(buffer, cursor),
    }
}

// ---------------------------------------------------------------------------
// Python: indentation-based expansion + # %% cell markers + decorators
// ---------------------------------------------------------------------------

fn expand_python(buffer: &BufferSnapshot, cursor: Point) -> Range<Point> {
    let max_row = buffer.max_point().row;

    // First check for # %% cell markers (jupytext cells)
    if let Some(range) = expand_jupytext_cell(buffer, cursor) {
        return range;
    }

    // If cursor is on a top-level comment (indent 0), return just the comment block.
    // Comments inside function bodies (indented) are part of the block.
    if !buffer.is_line_blank(cursor.row) && is_comment_line(buffer, cursor.row, "#") {
        if line_indent(buffer, cursor.row) == 0 {
            return expand_contiguous_comments(buffer, cursor.row, "#");
        }
    }

    // Skip blank lines at cursor — find first non-blank line
    let mut start_row = cursor.row;
    while start_row <= max_row && buffer.is_line_blank(start_row) {
        start_row += 1;
    }
    if start_row > max_row {
        return expand_paragraph(buffer, cursor);
    }

    let base_indent = line_indent(buffer, start_row);

    // At top level (base_indent == 0), comments are boundaries.
    // Inside function bodies (base_indent > 0), comments are part of the block.
    let comments_are_boundaries = base_indent == 0;

    // Expand upward: find the top of this indentation block
    let mut block_start = start_row;
    let mut found_parent = false;
    if block_start > 0 {
        let mut row = block_start - 1;
        loop {
            if !buffer.is_line_blank(row) {
                if is_comment_line(buffer, row, "#") {
                    if comments_are_boundaries {
                        break;
                    }
                    // Inside a body: include comments at same or deeper indent
                    let comment_indent = line_indent(buffer, row);
                    if comment_indent >= base_indent {
                        block_start = row;
                    } else {
                        break;
                    }
                } else {
                    let indent = line_indent(buffer, row);
                    if indent < base_indent {
                        // This is the parent statement (def, class, if, for, etc.)
                        block_start = row;
                        found_parent = true;
                        // Continue upward to include decorators
                        while block_start > 0 {
                            let prev = block_start - 1;
                            let prev_text = line_text(buffer, prev);
                            let trimmed = prev_text.trim_start();
                            if trimmed.starts_with('@') {
                                block_start = prev;
                            } else {
                                break;
                            }
                        }
                        break;
                    } else {
                        block_start = row;
                    }
                }
            }
            if row == 0 {
                break;
            }
            row -= 1;
        }
    }

    // If we found a parent, comments inside the body should be included
    let comments_are_boundaries = comments_are_boundaries && !found_parent;

    // Expand downward: include lines with greater-or-equal indentation or blank lines
    let mut block_end = start_row;
    let mut row = start_row + 1;
    while row <= max_row {
        if buffer.is_line_blank(row) {
            // Look ahead: if the next non-blank line continues the block, include it
            let mut next = row + 1;
            while next <= max_row && buffer.is_line_blank(next) {
                next += 1;
            }
            if next <= max_row && line_indent(buffer, next) >= base_indent {
                // Skip comments at boundary level
                if comments_are_boundaries && is_comment_line(buffer, next, "#") {
                    break;
                }
                block_end = next;
                row = next + 1;
                continue;
            }
            break;
        }
        if is_comment_line(buffer, row, "#") {
            if comments_are_boundaries {
                break;
            }
            // Inside a body: include comments at same or deeper indent
            let comment_indent = line_indent(buffer, row);
            if comment_indent >= base_indent {
                block_end = row;
                row += 1;
                continue;
            }
            break;
        }
        let indent = line_indent(buffer, row);
        if indent >= base_indent {
            block_end = row;
            row += 1;
        } else {
            break;
        }
    }

    Point::new(block_start, 0)..Point::new(block_end, buffer.line_len(block_end))
}

// ---------------------------------------------------------------------------
// R: pipe continuation, bracket balancing, roxygen
// ---------------------------------------------------------------------------

fn expand_r(buffer: &BufferSnapshot, cursor: Point) -> Range<Point> {
    let max_row = buffer.max_point().row;

    // If cursor is on a comment line, check if it's embedded in an open bracket expression.
    // If so, expand the whole expression. If not, return just the comment block.
    if !buffer.is_line_blank(cursor.row) && is_comment_line(buffer, cursor.row, "#") {
        if !is_inside_brackets_r(buffer, cursor.row) {
            return expand_contiguous_comments(buffer, cursor.row, "#");
        }
        // Fall through — comment is embedded in a bracketed expression
    }

    // --- Backward scan ---
    let mut start_row = cursor.row;
    while start_row > 0 {
        let prev = start_row - 1;

        if buffer.is_line_blank(prev) {
            break;
        }

        let prev_text = line_text(buffer, prev);
        let trimmed = prev_text.trim();

        if trimmed.starts_with('#') {
            // Hit a comment block. Find the first code line above it.
            let mut comment_top = prev;
            while comment_top > 0 && is_comment_line(buffer, comment_top - 1, "#") {
                comment_top -= 1;
            }
            if comment_top > 0 {
                let above_text = line_text(buffer, comment_top - 1);
                let above_trimmed = above_text.trim();
                if !above_trimmed.is_empty()
                    && !above_trimmed.starts_with('#')
                    && (line_continues_r(above_trimmed) || !brackets_balanced(above_trimmed))
                {
                    // Expression continues above the comment — include it
                    start_row = comment_top - 1;
                    continue;
                }
            }
            // Comment is a boundary
            break;
        }

        if line_continues_r(trimmed) || !brackets_balanced(trimmed) {
            start_row = prev;
            continue;
        }

        // Check if current line (skipping comments) starts with a pipe
        let mut curr = start_row;
        while curr <= max_row && is_comment_line(buffer, curr, "#") {
            curr += 1;
        }
        if curr <= max_row {
            let curr_text = line_text(buffer, curr);
            let curr_trimmed = curr_text.trim_start();
            if curr_trimmed.starts_with("%>%")
                || curr_trimmed.starts_with("|>")
                || curr_trimmed.starts_with('+')
            {
                start_row = prev;
                continue;
            }
        }

        break;
    }

    // --- Forward scan with cumulative bracket depth ---
    // Recompute from start_row so we know when we're inside brackets
    let mut end_row = start_row;
    let mut fwd_bracket_depth: i32 = 0;

    let mut row = start_row;
    while row <= max_row {
        let text = line_text(buffer, row);
        let trimmed = text.trim();

        if trimmed.starts_with('#') {
            if fwd_bracket_depth > 0 {
                // Inside brackets — include comment
                end_row = row;
                row += 1;
                continue;
            }
            // Not inside brackets — comment is a boundary, stop
            break;
        }

        if buffer.is_line_blank(row) {
            // Blank line: stop if brackets are balanced
            if fwd_bracket_depth <= 0 {
                break;
            }
            end_row = row;
            row += 1;
            continue;
        }

        // Count brackets in this code line
        for ch in trimmed.chars() {
            match ch {
                '(' | '[' | '{' => fwd_bracket_depth += 1,
                ')' | ']' | '}' => fwd_bracket_depth -= 1,
                _ => {}
            }
        }

        end_row = row;

        // If brackets are still open or line continues, keep going
        if fwd_bracket_depth > 0 || line_continues_r(trimmed) {
            row += 1;
            continue;
        }

        // Check if the next non-comment line starts with a pipe continuation
        let mut next = row + 1;
        while next <= max_row && is_comment_line(buffer, next, "#") {
            next += 1;
        }
        if next <= max_row && !buffer.is_line_blank(next) {
            let next_text = line_text(buffer, next);
            let next_trimmed = next_text.trim_start();
            if next_trimmed.starts_with("%>%")
                || next_trimmed.starts_with("|>")
                || next_trimmed.starts_with('+')
            {
                // Include any intervening comment lines
                end_row = next;
                row = next + 1;
                continue;
            }
        }

        break;
    }

    Point::new(start_row, 0)..Point::new(end_row, buffer.line_len(end_row))
}

/// Check if a row is inside an unclosed bracket expression by scanning all code
/// lines above it (ignoring comments) for cumulative bracket balance.
fn is_inside_brackets_r(buffer: &BufferSnapshot, row: u32) -> bool {
    let mut depth: i32 = 0;
    for r in 0..row {
        let text = line_text(buffer, r);
        let trimmed = text.trim();
        if !trimmed.starts_with('#') && !trimmed.is_empty() {
            for ch in trimmed.chars() {
                match ch {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => depth -= 1,
                    _ => {}
                }
            }
        }
    }
    depth > 0
}

fn line_continues_r(trimmed: &str) -> bool {
    trimmed.ends_with("%>%")
        || trimmed.ends_with("|>")
        || trimmed.ends_with('+')
        || trimmed.ends_with('\\')
        || trimmed.ends_with(',')
        || trimmed.ends_with('=')
}

fn brackets_balanced(text: &str) -> bool {
    let mut depth: i32 = 0;
    for ch in text.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    depth == 0
}

// ---------------------------------------------------------------------------
// Julia: keyword pairs (function/end, begin/end, etc.)
// ---------------------------------------------------------------------------

fn expand_julia(buffer: &BufferSnapshot, cursor: Point) -> Range<Point> {
    let max_row = buffer.max_point().row;

    // If cursor is on a comment line, expand to contiguous comment lines only
    if !buffer.is_line_blank(cursor.row) && is_comment_line(buffer, cursor.row, "#") {
        return expand_contiguous_comments(buffer, cursor.row, "#");
    }

    // First check for # %% cell markers
    if let Some(range) = expand_jupytext_cell(buffer, cursor) {
        return range;
    }

    // Try outline-based expansion first
    if let Some(range) = buffer.outline_range_containing(cursor..cursor) {
        return range;
    }

    // Fallback: scan for keyword/end pairs
    let keywords = ["function", "begin", "for", "while", "if", "let", "do", "module", "struct", "macro"];

    // Scan backward to find a keyword at or before cursor (skip comment lines)
    let mut keyword_row = None;
    let mut row = cursor.row;
    loop {
        let text = line_text(buffer, row);
        let trimmed = text.trim();
        // Skip comment lines — keywords may be above them
        if !trimmed.starts_with('#') {
            if keywords.iter().any(|kw| {
                trimmed.starts_with(kw)
                    && trimmed[kw.len()..].starts_with(|c: char| c.is_whitespace() || c == '(')
            }) {
                keyword_row = Some(row);
                break;
            }
        }
        if row == 0 {
            break;
        }
        row -= 1;
    }

    if let Some(start) = keyword_row {
        // Scan forward for the matching `end`
        let mut depth = 0;
        for row in start..=max_row {
            let text = line_text(buffer, row);
            let trimmed = text.trim();

            // Skip comment lines inside blocks
            if trimmed.starts_with('#') {
                continue;
            }

            if keywords.iter().any(|kw| {
                trimmed.starts_with(kw)
                    && trimmed[kw.len()..].starts_with(|c: char| c.is_whitespace() || c == '(')
            }) {
                depth += 1;
            }
            if trimmed == "end" || trimmed.starts_with("end ") || trimmed.starts_with("end\t") {
                depth -= 1;
                if depth <= 0 {
                    return Point::new(start, 0)..Point::new(row, buffer.line_len(row));
                }
            }
        }
    }

    // Ultimate fallback: paragraph
    expand_paragraph(buffer, cursor)
}

// ---------------------------------------------------------------------------
// Markdown: fenced code block via injection ranges
// ---------------------------------------------------------------------------

fn expand_markdown(buffer: &BufferSnapshot, cursor: Point) -> Range<Point> {
    let cursor_offset = buffer.point_to_offset(cursor);

    // Use injections_intersecting_range to find fenced code block content
    for (content_range, _language) in
        buffer.injections_intersecting_range(cursor_offset..cursor_offset)
    {
        let start = buffer.offset_to_point(content_range.start);
        let end = buffer.offset_to_point(content_range.end);
        return start..end;
    }

    // Fallback: paragraph
    expand_paragraph(buffer, cursor)
}

// ---------------------------------------------------------------------------
// Default: outline_range_containing + paragraph fallback
// ---------------------------------------------------------------------------

fn expand_default(buffer: &BufferSnapshot, cursor: Point) -> Range<Point> {
    if let Some(range) = buffer.outline_range_containing(cursor..cursor) {
        return range;
    }
    expand_paragraph(buffer, cursor)
}

// ---------------------------------------------------------------------------
// Shared utilities
// ---------------------------------------------------------------------------

/// Detect # %% (jupytext) cell markers and return the cell range containing the cursor.
fn expand_jupytext_cell(buffer: &BufferSnapshot, cursor: Point) -> Option<Range<Point>> {
    let max_row = buffer.max_point().row;

    let Some(language) = buffer.language() else {
        return None;
    };

    let default_scope = language.default_scope();
    let comment_prefixes = default_scope.line_comment_prefixes();
    if comment_prefixes.is_empty() {
        return None;
    }

    let jupytext_prefixes: Vec<String> = comment_prefixes
        .iter()
        .map(|prefix| format!("{prefix}%%"))
        .collect();

    let is_cell_marker = |row: u32| -> bool {
        jupytext_prefixes.iter().any(|prefix| {
            buffer.contains_str_at(Point::new(row, 0), prefix)
        })
    };

    // Scan backward to find the cell start marker
    let mut cell_start = None;
    let mut row = cursor.row;
    loop {
        if is_cell_marker(row) {
            cell_start = Some(row);
            break;
        }
        if row == 0 {
            break;
        }
        row -= 1;
    }

    let cell_start = cell_start?;

    // Scan forward to find the next cell marker (end of this cell)
    let mut cell_end = max_row;
    for row in (cell_start + 1)..=max_row {
        if is_cell_marker(row) {
            cell_end = row - 1;
            break;
        }
    }

    // Trim trailing blank lines
    while cell_end > cell_start && buffer.is_line_blank(cell_end) {
        cell_end -= 1;
    }

    // The content starts after the marker line
    let content_start = cell_start + 1;
    if content_start > cell_end {
        return None;
    }

    Some(Point::new(content_start, 0)..Point::new(cell_end, buffer.line_len(cell_end)))
}

/// Check if a line is a comment (starts with the given prefix after whitespace).
fn is_comment_line(buffer: &BufferSnapshot, row: u32, prefix: &str) -> bool {
    let text = line_text(buffer, row);
    text.trim_start().starts_with(prefix)
}

/// Expand to contiguous comment lines around the given row.
fn expand_contiguous_comments(buffer: &BufferSnapshot, row: u32, prefix: &str) -> Range<Point> {
    let max_row = buffer.max_point().row;

    let mut start_row = row;
    while start_row > 0 && is_comment_line(buffer, start_row - 1, prefix) {
        start_row -= 1;
    }

    let mut end_row = row;
    while end_row < max_row && is_comment_line(buffer, end_row + 1, prefix) {
        end_row += 1;
    }

    Point::new(start_row, 0)..Point::new(end_row, buffer.line_len(end_row))
}

fn line_indent(buffer: &BufferSnapshot, row: u32) -> u32 {
    let text = line_text(buffer, row);
    text.chars().take_while(|c| c.is_whitespace()).count() as u32
}

fn line_text(buffer: &BufferSnapshot, row: u32) -> String {
    let start = Point::new(row, 0);
    let end = Point::new(row, buffer.line_len(row));
    buffer.text_for_range(start..end).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{App, prelude::*};
    use indoc::indoc;
    use language::{Buffer, Language, LanguageConfig};
    use std::sync::Arc;

    fn make_language(name: &'static str) -> Arc<Language> {
        Arc::new(Language::new(
            LanguageConfig {
                name: name.into(),
                line_comments: vec!["# ".into()],
                ..Default::default()
            },
            None,
        ))
    }

    fn text_for_range(snapshot: &BufferSnapshot, range: Range<Point>) -> String {
        snapshot.text_for_range(range).collect()
    }

    // -----------------------------------------------------------------------
    // Paragraph expansion (default fallback)
    // -----------------------------------------------------------------------

    #[gpui::test]
    fn test_expand_paragraph_single_block(cx: &mut App) {
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    line 1
                    line 2
                    line 3
                "},
                cx,
            )
        });
        let snapshot = buffer.read(cx).snapshot();

        let range = expand_paragraph(&snapshot, Point::new(1, 0));
        assert_eq!(text_for_range(&snapshot, range), "line 1\nline 2\nline 3");
    }

    #[gpui::test]
    fn test_expand_paragraph_separated_blocks(cx: &mut App) {
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    block 1 line 1
                    block 1 line 2

                    block 2 line 1
                    block 2 line 2
                "},
                cx,
            )
        });
        let snapshot = buffer.read(cx).snapshot();

        // Cursor in first block
        let range = expand_paragraph(&snapshot, Point::new(0, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "block 1 line 1\nblock 1 line 2"
        );

        // Cursor in second block
        let range = expand_paragraph(&snapshot, Point::new(3, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "block 2 line 1\nblock 2 line 2"
        );
    }

    // -----------------------------------------------------------------------
    // Python expansion
    // -----------------------------------------------------------------------

    #[gpui::test]
    fn test_python_indentation_block(cx: &mut App) {
        let lang = make_language("Python");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    def foo():
                        x = 1
                        y = 2
                        return x + y

                    z = 3
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        // Cursor inside the function body
        let range = expand_python(&snapshot, Point::new(1, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "def foo():\n    x = 1\n    y = 2\n    return x + y"
        );
    }

    #[gpui::test]
    fn test_python_decorator(cx: &mut App) {
        let lang = make_language("Python");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    @decorator
                    def bar():
                        pass
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        // Cursor on `pass` line
        let range = expand_python(&snapshot, Point::new(2, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "@decorator\ndef bar():\n    pass"
        );
    }

    #[gpui::test]
    fn test_python_jupytext_cell(cx: &mut App) {
        let lang = make_language("Python");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    # %% Cell 1
                    x = 1
                    y = 2

                    # %% Cell 2
                    z = 3
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        // Cursor in cell 1
        let range = expand_python(&snapshot, Point::new(1, 0));
        assert_eq!(text_for_range(&snapshot, range), "x = 1\ny = 2");

        // Cursor in cell 2
        let range = expand_python(&snapshot, Point::new(5, 0));
        assert_eq!(text_for_range(&snapshot, range), "z = 3");
    }

    // -----------------------------------------------------------------------
    // R expansion
    // -----------------------------------------------------------------------

    #[gpui::test]
    fn test_r_pipe_chain(cx: &mut App) {
        let lang = make_language("R");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    result <- data %>%
                        filter(x > 0) %>%
                        mutate(y = x * 2)
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        let range = expand_r(&snapshot, Point::new(0, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "result <- data %>%\n    filter(x > 0) %>%\n    mutate(y = x * 2)"
        );
    }

    #[gpui::test]
    fn test_r_native_pipe(cx: &mut App) {
        let lang = make_language("R");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    result <- data |>
                        filter(x > 0) |>
                        summarize(n = n())
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        let range = expand_r(&snapshot, Point::new(1, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "result <- data |>\n    filter(x > 0) |>\n    summarize(n = n())"
        );
    }

    #[gpui::test]
    fn test_r_bracket_balancing(cx: &mut App) {
        let lang = make_language("R");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    foo(
                        bar,
                        baz
                    )
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        let range = expand_r(&snapshot, Point::new(0, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "foo(\n    bar,\n    baz\n)"
        );
    }

    #[gpui::test]
    fn test_r_ggplot_plus(cx: &mut App) {
        let lang = make_language("R");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    ggplot(data, aes(x, y)) +
                        geom_point() +
                        theme_minimal()
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        let range = expand_r(&snapshot, Point::new(0, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "ggplot(data, aes(x, y)) +\n    geom_point() +\n    theme_minimal()"
        );
    }

    #[gpui::test]
    fn test_r_multiline_comment_does_not_bleed_into_code(cx: &mut App) {
        let lang = make_language("R");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    # function(test, arg1,
                    # arg1)
                    merged <- inner_join(
                      kg1k %>% select(barcode, status),
                      kg1k_deep %>% select(barcode, status),
                      by = \"barcode\"
                    )
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        // Cursor on the comment line — should only get the comment block
        let range = expand_r(&snapshot, Point::new(0, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "# function(test, arg1,\n# arg1)"
        );

        // Cursor on the `merged` line — should get the code block, not the comments
        let range = expand_r(&snapshot, Point::new(2, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "merged <- inner_join(\n  kg1k %>% select(barcode, status),\n  kg1k_deep %>% select(barcode, status),\n  by = \"barcode\"\n)"
        );
    }

    #[gpui::test]
    fn test_r_single_line_comment_before_code(cx: &mut App) {
        let lang = make_language("R");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    # function(test, arg1, arg1)
                    merged <- inner_join(
                      kg1k %>% select(barcode, status),
                      by = \"barcode\"
                    )
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        // Cursor on the `merged` line — should NOT include the comment above
        let range = expand_r(&snapshot, Point::new(1, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "merged <- inner_join(\n  kg1k %>% select(barcode, status),\n  by = \"barcode\"\n)"
        );
    }

    #[gpui::test]
    fn test_r_embedded_comment_in_brackets(cx: &mut App) {
        let lang = make_language("R");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    merged <- inner_join(
                      kg1k %>% select(barcode, status),
                      # this is a comment inside the call
                      kg1k_deep %>% select(barcode, status),
                      by = \"barcode\"
                    )
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        // Cursor on line after embedded comment — should get the whole expression
        let range = expand_r(&snapshot, Point::new(3, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "merged <- inner_join(\n  kg1k %>% select(barcode, status),\n  # this is a comment inside the call\n  kg1k_deep %>% select(barcode, status),\n  by = \"barcode\"\n)"
        );

        // Cursor on the embedded comment itself — should also get the whole expression
        let range = expand_r(&snapshot, Point::new(2, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "merged <- inner_join(\n  kg1k %>% select(barcode, status),\n  # this is a comment inside the call\n  kg1k_deep %>% select(barcode, status),\n  by = \"barcode\"\n)"
        );
    }

    // -----------------------------------------------------------------------
    // Python comment handling
    // -----------------------------------------------------------------------

    #[gpui::test]
    fn test_python_comment_does_not_bleed_into_code(cx: &mut App) {
        let lang = make_language("Python");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    # some multiline
                    # comment here
                    x = 1
                    y = 2
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        // Cursor on comment — should only get comments
        let range = expand_python(&snapshot, Point::new(0, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "# some multiline\n# comment here"
        );

        // Cursor on code — should not include comments above
        let range = expand_python(&snapshot, Point::new(2, 0));
        assert_eq!(text_for_range(&snapshot, range), "x = 1\ny = 2");
    }

    #[gpui::test]
    fn test_python_comment_inside_function_body(cx: &mut App) {
        let lang = make_language("Python");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    def foo():
                        x = 1
                        # comment inside body
                        y = 2
                        return x + y
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        // Cursor on code after comment — should include the whole function body
        let range = expand_python(&snapshot, Point::new(3, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "def foo():\n    x = 1\n    # comment inside body\n    y = 2\n    return x + y"
        );
    }

    // -----------------------------------------------------------------------
    // Julia comment handling
    // -----------------------------------------------------------------------

    #[gpui::test]
    fn test_julia_comment_does_not_bleed_into_code(cx: &mut App) {
        let lang = make_language("Julia");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    # some comment
                    # another comment
                    x = 1
                    y = 2
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        // Cursor on comment — should only get comments
        let range = expand_julia(&snapshot, Point::new(0, 0));
        assert_eq!(
            text_for_range(&snapshot, range),
            "# some comment\n# another comment"
        );
    }

    #[gpui::test]
    fn test_julia_paragraph_fallback(cx: &mut App) {
        // Without tree-sitter, Julia falls back to paragraph expansion
        let lang = make_language("Julia");
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc! {"
                    x = 1
                    y = 2

                    z = 3
                "},
                cx,
            )
            .with_language(lang, cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        let range = expand_julia(&snapshot, Point::new(0, 0));
        assert_eq!(text_for_range(&snapshot, range), "x = 1\ny = 2");
    }
}
