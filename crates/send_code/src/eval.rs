use std::ops::Range;

use language::{BufferSnapshot, Point};

pub fn find_comment_block_at(buffer: &BufferSnapshot, cursor_point: Point) -> Option<Range<Point>> {
    if buffer.is_line_blank(cursor_point.row) {
        return None;
    }

    let language = buffer.language_at(cursor_point)?;
    let comment_prefix =
        comment_prefix_for_line(buffer, cursor_point.row, &language.config().line_comments)?;
    Some(contiguous_comment_block(
        buffer,
        cursor_point.row,
        &comment_prefix,
    ))
}

/// Find the eval region enclosing `cursor_point` using the language's
/// eval.scm query. Returns the tightest `@eval` capture containing the cursor.
pub fn find_eval_at(buffer: &BufferSnapshot, cursor_point: Point) -> Option<Range<Point>> {
    let cursor_offset = buffer.point_to_offset(cursor_point);

    let mut syntax_matches = buffer.matches(0..buffer.len(), |grammar| {
        grammar.eval_config.as_ref().map(|c| &c.query)
    });

    let configs: Vec<_> = syntax_matches
        .grammars()
        .iter()
        .map(|grammar| grammar.eval_config.as_ref())
        .collect();

    let mut best: Option<Range<usize>> = None;

    while let Some(mat) = syntax_matches.peek() {
        if let Some(config) = &configs[mat.grammar_index] {
            for capture in mat.captures.iter() {
                if capture.index == config.eval_capture_ix {
                    let range = capture.node.byte_range();
                    if range.start <= cursor_offset && cursor_offset <= range.end {
                        let is_tighter =
                            best.as_ref().map(|b| range.len() < b.len()).unwrap_or(true);
                        if is_tighter {
                            best = Some(range);
                        }
                    }
                }
            }
        }
        syntax_matches.advance();
    }

    best.map(|r| {
        let start = buffer.offset_to_point(r.start);
        let end = buffer.offset_to_point(r.end);
        start..end
    })
}

/// Fallback for Markdown buffers: when the cursor sits inside a fenced code
/// block, return the injected content range. Lets a single SendEvalAtCursor
/// press grab a full ```python``` block from a Markdown file without needing
/// a per-language eval.scm to be loaded for Markdown.
pub fn find_markdown_injection_at(
    buffer: &BufferSnapshot,
    cursor_point: Point,
) -> Option<Range<Point>> {
    let outer = buffer.language()?;
    if outer.name().as_ref() != "Markdown" {
        return None;
    }

    let cursor_offset = buffer.point_to_offset(cursor_point);
    let (content_range, _) = buffer
        .injections_intersecting_range(cursor_offset..cursor_offset)
        .next()?;
    let start = buffer.offset_to_point(content_range.start);
    let end = buffer.offset_to_point(content_range.end);
    Some(start..end)
}

fn comment_prefix_for_line(
    buffer: &BufferSnapshot,
    row: u32,
    comment_prefixes: &[std::sync::Arc<str>],
) -> Option<String> {
    let line = line_text(buffer, row);
    let trimmed = line.trim_start();

    comment_prefixes
        .iter()
        .map(|prefix| prefix.trim_end())
        .filter(|prefix| !prefix.is_empty() && trimmed.starts_with(prefix))
        .max_by_key(|prefix| prefix.len())
        .map(ToOwned::to_owned)
}

fn contiguous_comment_block(buffer: &BufferSnapshot, row: u32, prefix: &str) -> Range<Point> {
    let max_row = buffer.max_point().row;

    let mut start_row = row;
    while start_row > 0 && line_is_comment(buffer, start_row - 1, prefix) {
        start_row -= 1;
    }

    let mut end_row = row;
    while end_row < max_row && line_is_comment(buffer, end_row + 1, prefix) {
        end_row += 1;
    }

    Point::new(start_row, 0)..Point::new(end_row, buffer.line_len(end_row))
}

fn line_is_comment(buffer: &BufferSnapshot, row: u32, prefix: &str) -> bool {
    let line = line_text(buffer, row);
    line.trim_start().starts_with(prefix)
}

fn line_text(buffer: &BufferSnapshot, row: u32) -> String {
    let start = Point::new(row, 0);
    let end = Point::new(row, buffer.line_len(row));
    buffer.text_for_range(start..end).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, TestAppContext};
    use language::{Buffer, Language, LanguageRegistry};
    use std::sync::Arc;

    fn rust_language_with_eval_query() -> Arc<Language> {
        let language = match Arc::try_unwrap(language::rust_lang()) {
            Ok(language) => language,
            Err(_) => panic!("rust_lang should be uniquely owned in this test"),
        };

        let language = language
            .with_eval_query("(function_item) @eval\n(block) @eval")
            .expect("rust eval query should parse");
        Arc::new(language)
    }

    fn bash_language() -> Arc<Language> {
        Arc::new(Language::new(
            language::LanguageConfig {
                name: "Bash".into(),
                line_comments: vec!["# ".into()],
                ..Default::default()
            },
            Some(tree_sitter_bash::LANGUAGE.into()),
        ))
    }

    fn snapshot_for(
        text: &str,
        language: Arc<Language>,
        language_registry: Option<Arc<LanguageRegistry>>,
        cx: &mut TestAppContext,
    ) -> BufferSnapshot {
        let buffer = cx.new(|cx| {
            let mut buffer = Buffer::local(text, cx);
            if let Some(language_registry) = language_registry {
                buffer.set_language_registry(language_registry);
            }
            buffer.set_language(Some(language), cx);
            buffer
        });
        cx.executor().run_until_parked();
        buffer.read_with(cx, |buffer, _| buffer.snapshot())
    }

    fn text_for_range(buffer: &BufferSnapshot, range: Range<Point>) -> String {
        buffer.text_for_range(range.start..range.end).collect()
    }

    #[gpui::test]
    fn find_eval_at_returns_tightest_capture(cx: &mut TestAppContext) {
        let snapshot = snapshot_for(
            "fn outer() {\n    if true {\n        println!(\"hi\");\n    }\n}\n",
            rust_language_with_eval_query(),
            None,
            cx,
        );

        let range = find_eval_at(&snapshot, Point::new(2, 10))
            .expect("expected cursor inside nested block to match an eval capture");

        assert_eq!(
            text_for_range(&snapshot, range),
            "{\n        println!(\"hi\");\n    }"
        );
    }

    #[gpui::test]
    fn find_markdown_injection_at_returns_fenced_code_block(cx: &mut TestAppContext) {
        let registry = Arc::new(LanguageRegistry::test(cx.background_executor.clone()));
        let markdown_language = language::markdown_lang();
        registry.add(markdown_language.clone());
        registry.add(language::rust_lang());

        let snapshot = snapshot_for(
            "before\n\n```rs\nfn main() {\n    println!(\"hi\");\n}\n```\n\nafter\n",
            markdown_language,
            Some(registry),
            cx,
        );

        let range = find_markdown_injection_at(&snapshot, Point::new(4, 4))
            .expect("expected cursor inside fenced code block to match injection content");

        assert_eq!(
            text_for_range(&snapshot, range),
            "fn main() {\n    println!(\"hi\");\n}\n"
        );
    }

    #[gpui::test]
    fn find_comment_block_at_prefers_contiguous_shell_comments_in_markdown_fence(
        cx: &mut TestAppContext,
    ) {
        let registry = Arc::new(LanguageRegistry::test(cx.background_executor.clone()));
        let markdown_language = language::markdown_lang();
        registry.add(markdown_language.clone());
        registry.add(bash_language());

        let snapshot = snapshot_for(
            "before\n\n```Bash\ncd ~/develop/zed\n\n# One-time setup:\n# git remote add upstream https://github.com/zed-industries/zed.git\n\ngit fetch upstream\ngit checkout main\n```\n\nafter\n",
            markdown_language,
            Some(registry),
            cx,
        );

        let range = find_comment_block_at(&snapshot, Point::new(5, 0))
            .expect("expected shell comment line to return its contiguous comment block");
        assert_eq!(
            text_for_range(&snapshot, range),
            "# One-time setup:\n# git remote add upstream https://github.com/zed-industries/zed.git"
        );

        let range = find_markdown_injection_at(&snapshot, Point::new(5, 0))
            .expect("expected markdown fallback to still match the entire fenced code block");
        assert_eq!(
            text_for_range(&snapshot, range),
            "cd ~/develop/zed\n\n# One-time setup:\n# git remote add upstream https://github.com/zed-industries/zed.git\n\ngit fetch upstream\ngit checkout main\n"
        );
    }
}
