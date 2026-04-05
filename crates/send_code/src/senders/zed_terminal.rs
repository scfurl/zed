use gpui::{App, Entity, WeakEntity};
use terminal::Terminal;
use terminal_view::{TerminalView, terminal_panel::TerminalPanel};
use workspace::Workspace;

pub fn send(text: &str, use_bracketed_paste: bool, workspace: &WeakEntity<Workspace>, cx: &mut App) {
    let text = text.to_string();

    let terminal = workspace
        .update(cx, |workspace, cx| find_active_terminal(workspace, cx))
        .ok()
        .flatten();

    let Some(terminal) = terminal else {
        log::warn!("send_code: no active terminal found in terminal panel");
        return;
    };

    let text = text
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .to_string();

    if use_bracketed_paste {
        // Send entire block wrapped in bracketed paste escape sequences.
        // The REPL receives it as a single paste and executes on Enter.
        terminal.update(cx, |terminal, _| {
            let sanitized = text.replace('\x1b', "");
            let paste_text = format!("\x1b[200~{}\x1b[201~", sanitized);
            terminal.input(paste_text.into_bytes());
            terminal.input(b"\r".to_vec());
        });
    } else {
        // Without bracketed paste, send line by line with delays so the
        // REPL can process each line before receiving the next one.
        let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
        cx.spawn({
            let terminal = terminal.downgrade();
            async move |cx| {
                for (i, line) in lines.iter().enumerate() {
                    let line = line.clone();
                    let Ok(_) = terminal.update(cx, |terminal, _| {
                        terminal.input(line.into_bytes());
                        terminal.input(b"\r".to_vec());
                    }) else {
                        break;
                    };
                    // Small delay between lines so the REPL can process each one.
                    // Skip delay after the last line.
                    if i < lines.len() - 1 {
                        smol::Timer::after(std::time::Duration::from_millis(50)).await;
                    }
                }
            }
        })
        .detach();
    }
}

fn find_active_terminal(workspace: &Workspace, cx: &App) -> Option<Entity<Terminal>> {
    let terminal_panel = workspace.panel::<TerminalPanel>(cx)?;
    let active_pane = terminal_panel.read(cx).active_terminal_pane().clone();
    let terminal_view = active_pane
        .read(cx)
        .active_item()
        .and_then(|item| item.downcast::<TerminalView>())?;
    Some(terminal_view.read(cx).terminal().clone())
}
