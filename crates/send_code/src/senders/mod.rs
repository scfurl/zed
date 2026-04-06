mod cmux;
mod ghostty;
mod iterm;
mod terminal_app;
mod tmux;
mod zed_terminal;

use gpui::{App, WeakEntity};
use workspace::Workspace;

use crate::settings::SendCodeSettings;

pub fn send_to_target(
    text: &str,
    target: &str,
    settings: &SendCodeSettings,
    language_name: Option<&str>,
    workspace: Option<&WeakEntity<Workspace>>,
    cx: &mut App,
) {
    let is_multiline = text.contains('\n') && text.trim_end_matches('\n').contains('\n');
    let use_bracketed = settings.bracketed_paste
        && is_multiline
        && language_supports_bracketed_paste(language_name);

    // Python's REPL (IPython/cpython) needs an extra blank line after multi-line
    // blocks (def, class, for, if, etc.) to signal "end of block".
    let text = if is_multiline && matches!(language_name, Some("Python")) {
        let mut t = text.to_string();
        if !t.ends_with("\n\n") {
            if t.ends_with('\n') {
                t.push('\n');
            } else {
                t.push_str("\n\n");
            }
        }
        t
    } else {
        text.to_string()
    };

    match target {
        "zed_terminal" => {
            if let Some(ws) = workspace {
                zed_terminal::send(&text, use_bracketed, ws, cx);
            } else {
                log::warn!("send_code: no workspace available for zed_terminal target");
            }
        }
        "ghostty" => {
            ghostty::send(&text, use_bracketed, settings.ghostty_chunk_size, cx);
        }
        "iterm" => {
            iterm::send(&text, use_bracketed, cx);
        }
        "terminal_app" => {
            terminal_app::send(&text, cx);
        }
        "cmux" => {
            cmux::send(
                &text,
                use_bracketed,
                settings.cmux_chunk_size,
                settings.cmux_surface.as_deref(),
                cx,
            );
        }
        "tmux" => {
            tmux::send(&text, use_bracketed, settings.tmux_target.as_deref(), cx);
        }
        _ => {
            log::error!("send_code: unknown target \"{}\"", target);
        }
    }
}

/// Returns true if the given language's REPL is known to support bracketed paste.
/// R's readline does not handle bracketed paste sequences by default.
fn language_supports_bracketed_paste(language_name: Option<&str>) -> bool {
    match language_name {
        Some("Python") => true,
        Some("Julia") => true,
        Some("R") => false,
        // Default: assume no bracketed paste for safety
        _ => false,
    }
}
