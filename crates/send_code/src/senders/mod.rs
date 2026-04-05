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
    workspace: Option<&WeakEntity<Workspace>>,
    cx: &mut App,
) {
    let use_bracketed = settings.bracketed_paste && text.contains('\n');

    match target {
        "zed_terminal" => {
            if let Some(ws) = workspace {
                zed_terminal::send(text, use_bracketed, ws, cx);
            } else {
                log::warn!("send_code: no workspace available for zed_terminal target");
            }
        }
        "ghostty" => {
            ghostty::send(text, use_bracketed, settings.ghostty_chunk_size, cx);
        }
        "iterm" => {
            iterm::send(text, use_bracketed, cx);
        }
        "terminal_app" => {
            terminal_app::send(text, cx);
        }
        "cmux" => {
            cmux::send(
                text,
                use_bracketed,
                settings.cmux_chunk_size,
                settings.cmux_surface.as_deref(),
                cx,
            );
        }
        "tmux" => {
            tmux::send(text, use_bracketed, settings.tmux_target.as_deref(), cx);
        }
        _ => {
            log::error!("send_code: unknown target \"{}\"", target);
        }
    }
}
