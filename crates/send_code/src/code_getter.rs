use std::sync::Arc;

use editor::Editor;
use gpui::App;
use language::{Language, Point};

use crate::block_expander;

pub struct CodePayload {
    pub text: String,
    pub language: Option<Arc<Language>>,
    pub advance_to: Option<Point>,
}

pub enum GetCodeMode {
    Auto { advance: bool },
    Line,
    File,
}

pub fn get_code(editor: &Editor, mode: GetCodeMode, cx: &mut App) -> Option<CodePayload> {
    let multibuffer = editor.buffer().clone();
    let buffer = multibuffer.read(cx).as_singleton()?;
    let snapshot = buffer.read(cx).snapshot();

    match mode {
        GetCodeMode::Auto { advance } => get_code_auto(editor, advance, cx),
        GetCodeMode::Line => get_code_line(editor, cx),
        GetCodeMode::File => get_code_file(editor, &snapshot, cx),
    }
}

/// Languages that have block-aware expansion (SendCode expands to full block).
/// Returns true if the language has an eval.scm query or is a language with
/// heuristic block expansion (Python, R, Julia).
/// All other languages fall back to single-line behavior for SendCode.
fn supports_block_expansion(language: &Language) -> bool {
    // Check if the language has an eval.scm tree-sitter query
    if language.grammar().is_some_and(|g| g.eval_config.is_some()) {
        return true;
    }
    // Heuristic fallback languages
    matches!(language.name().as_ref(), "Python" | "R" | "Julia")
}

fn get_code_auto(editor: &Editor, advance: bool, cx: &mut App) -> Option<CodePayload> {
    let multibuffer = editor.buffer().clone();
    let buffer = multibuffer.read(cx).as_singleton()?;
    let snapshot = buffer.read(cx).snapshot();

    let display_snapshot = editor.display_snapshot(cx);
    let selection = editor.selections.newest_adjusted(&display_snapshot);
    let range = selection.range();

    let start = range.start;
    let end = range.end;

    let language = snapshot.language_at(start).cloned();

    if start != end {
        // Non-empty selection: send selected text
        let text: String = snapshot.text_for_range(start..end).collect();
        let advance_to = if advance {
            let next_row = end.row + 1;
            if next_row <= snapshot.max_point().row {
                Some(Point::new(next_row, 0))
            } else {
                None
            }
        } else {
            None
        };

        Some(CodePayload {
            text: ensure_trailing_newline(text),
            language,
            advance_to,
        })
    } else if language
        .as_ref()
        .is_some_and(|l| supports_block_expansion(l))
    {
        // Blank line: send just a newline and advance (check before block expansion)
        if snapshot.is_line_blank(start.row) {
            return Some(blank_line_payload(language, advance, start.row, &snapshot));
        }

        if language.as_ref().is_some_and(|language| {
            block_expander::is_standalone_comment_line(&snapshot, start.row, language)
        }) {
            return None;
        }

        // Block-aware language (Python, R, Julia): expand to block
        let cursor = start;
        let block_range =
            block_expander::expand_block(&snapshot, cursor, language.as_ref().unwrap());

        let text: String = snapshot
            .text_for_range(block_range.start..block_range.end)
            .collect();
        if text.trim().is_empty() {
            return Some(blank_line_payload(language, advance, start.row, &snapshot));
        }

        let advance_to = if advance {
            let next_row = block_range.end.row + 1;
            if next_row <= snapshot.max_point().row {
                Some(Point::new(next_row, 0))
            } else {
                None
            }
        } else {
            None
        };

        Some(CodePayload {
            text: ensure_trailing_newline(text),
            language,
            advance_to,
        })
    } else {
        // All other languages: send current line only (like SendLine)
        let row = start.row;
        let line_end = Point::new(row, snapshot.line_len(row));
        let text: String = snapshot
            .text_for_range(Point::new(row, 0)..line_end)
            .collect();

        if text.trim().is_empty() {
            return Some(blank_line_payload(language, advance, row, &snapshot));
        }

        let advance_to = if advance {
            let next_row = row + 1;
            if next_row <= snapshot.max_point().row {
                Some(Point::new(next_row, 0))
            } else {
                None
            }
        } else {
            None
        };

        Some(CodePayload {
            text: ensure_trailing_newline(text),
            language,
            advance_to,
        })
    }
}

fn get_code_line(editor: &Editor, cx: &mut App) -> Option<CodePayload> {
    let multibuffer = editor.buffer().clone();
    let buffer = multibuffer.read(cx).as_singleton()?;
    let snapshot = buffer.read(cx).snapshot();

    let display_snapshot = editor.display_snapshot(cx);
    let selection = editor.selections.newest_adjusted(&display_snapshot);
    let range = selection.range();
    let start = range.start;
    let end = range.end;

    let language = snapshot.language_at(start).cloned();

    if start != end {
        // Non-empty selection: send the entire selection
        let text: String = snapshot.text_for_range(start..end).collect();
        let next_row = end.row + 1;
        let advance_to = if next_row <= snapshot.max_point().row {
            Some(Point::new(next_row, 0))
        } else {
            None
        };

        Some(CodePayload {
            text: ensure_trailing_newline(text),
            language,
            advance_to,
        })
    } else {
        // No selection: send current line
        let row = start.row;

        if snapshot.is_line_blank(row) {
            return Some(blank_line_payload(language, true, row, &snapshot));
        }

        if language.as_ref().is_some_and(|language| {
            block_expander::is_standalone_comment_line(&snapshot, row, language)
        }) {
            return None;
        }

        let line_end = Point::new(row, snapshot.line_len(row));
        let text: String = snapshot
            .text_for_range(Point::new(row, 0)..line_end)
            .collect();

        let next_row = row + 1;
        let advance_to = if next_row <= snapshot.max_point().row {
            Some(Point::new(next_row, 0))
        } else {
            None
        };

        Some(CodePayload {
            text: ensure_trailing_newline(text),
            language,
            advance_to,
        })
    }
}

/// Returns a payload that sends just a newline (Enter) for blank lines.
fn blank_line_payload(
    language: Option<Arc<Language>>,
    advance: bool,
    row: u32,
    snapshot: &language::BufferSnapshot,
) -> CodePayload {
    let advance_to = if advance {
        let next_row = row + 1;
        if next_row <= snapshot.max_point().row {
            Some(Point::new(next_row, 0))
        } else {
            None
        }
    } else {
        None
    };
    CodePayload {
        text: "\n".to_string(),
        language,
        advance_to,
    }
}

fn get_code_file(
    editor: &Editor,
    snapshot: &language::BufferSnapshot,
    cx: &App,
) -> Option<CodePayload> {
    let language = snapshot.language().cloned();
    let language_name = language.as_ref().map(|l| l.name().to_string());

    // Try to get the file path for source commands
    let multibuffer = editor.buffer().clone();
    let buffer = multibuffer.read(cx).as_singleton()?;
    let file_path = buffer
        .read(cx)
        .file()
        .and_then(|f| f.as_local())
        .map(|f| f.abs_path(cx).to_string_lossy().to_string());

    let text = if let Some(path) = file_path {
        match language_name.as_deref() {
            Some("R") => format!(
                "source(\"{}\")\n",
                path.replace('\\', "\\\\").replace('"', "\\\"")
            ),
            Some("Python") => format!(
                "exec(open(\"{}\").read())\n",
                path.replace('\\', "\\\\").replace('"', "\\\"")
            ),
            Some("Julia") => format!(
                "include(\"{}\")\n",
                path.replace('\\', "\\\\").replace('"', "\\\"")
            ),
            _ => {
                // Fallback: send entire file contents
                let text: String = snapshot.text().to_string();
                ensure_trailing_newline(text)
            }
        }
    } else {
        let text: String = snapshot.text().to_string();
        ensure_trailing_newline(text)
    };

    Some(CodePayload {
        text,
        language,
        advance_to: None,
    })
}

/// Expand to contiguous non-blank lines around the cursor (paragraph heuristic).
pub fn expand_paragraph(
    snapshot: &language::BufferSnapshot,
    cursor: Point,
) -> std::ops::Range<Point> {
    let max_row = snapshot.max_point().row;

    // Find start: go up while lines are non-blank
    let mut start_row = cursor.row;
    while start_row > 0 && !snapshot.is_line_blank(start_row - 1) {
        start_row -= 1;
    }

    // Find end: go down while lines are non-blank
    let mut end_row = cursor.row;
    while end_row < max_row && !snapshot.is_line_blank(end_row + 1) {
        end_row += 1;
    }

    Point::new(start_row, 0)..Point::new(end_row, snapshot.line_len(end_row))
}

fn ensure_trailing_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}
