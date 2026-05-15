use std::ops::Range;

use language::{BufferSnapshot, Point};

/// Find the eval region enclosing `cursor_point` using the language's
/// eval.scm query. Returns the tightest matching capture containing the cursor.
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

/// Return all top-level eval regions in order.
/// "Top-level" = no other eval region fully contains this one.
/// Used for GotoNextEval / GotoPrevEval navigation.
pub fn all_evals(buffer: &BufferSnapshot) -> Vec<Range<Point>> {
    let mut syntax_matches = buffer.matches(0..buffer.len(), |grammar| {
        grammar.eval_config.as_ref().map(|c| &c.query)
    });

    let configs: Vec<_> = syntax_matches
        .grammars()
        .iter()
        .map(|grammar| grammar.eval_config.as_ref())
        .collect();

    let mut ranges: Vec<Range<usize>> = Vec::new();

    while let Some(mat) = syntax_matches.peek() {
        if let Some(config) = &configs[mat.grammar_index] {
            for capture in mat.captures.iter() {
                if capture.index == config.eval_capture_ix {
                    ranges.push(capture.node.byte_range());
                }
            }
        }
        syntax_matches.advance();
    }

    // Keep only top-level ranges (remove any contained within another)
    ranges.sort_by_key(|r| r.start);
    let mut top_level: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = top_level.last() {
            if range.start >= last.end {
                top_level.push(range);
            }
        } else {
            top_level.push(range);
        }
    }

    top_level
        .into_iter()
        .map(|r| {
            let start = buffer.offset_to_point(r.start);
            let end = buffer.offset_to_point(r.end);
            start..end
        })
        .collect()
}
