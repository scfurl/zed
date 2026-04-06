use gpui::App;

pub fn send(text: &str, use_bracketed_paste: bool, chunk_size: usize, cx: &mut App) {
    let text = text.to_string();
    let chunk_size = chunk_size.max(1);

    cx.spawn(async move |_cx| {
        if use_bracketed_paste {
            // Wrap entire text in bracketed paste sequences and send as one block.
            let text_to_send = format!("\x1b[200~{}\x1b[201~", text);
            send_text_chunked(&text_to_send, chunk_size).await;
            send_enter().await;
        } else {
            // Send line-by-line so REPLs that don't support bracketed paste
            // (like R) process each line correctly.
            let trimmed = text.trim_end_matches('\n');
            let lines: Vec<&str> = trimmed.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                send_text_chunked(line, chunk_size).await;
                send_enter().await;
                // Small delay between lines so the REPL can process each one.
                if i < lines.len() - 1 {
                    smol::Timer::after(std::time::Duration::from_millis(50)).await;
                }
            }
            // If original text ended with multiple newlines (e.g. Python block terminator),
            // send extra Enter(s).
            let trailing_newlines = text.len() - text.trim_end_matches('\n').len();
            if trailing_newlines > 1 {
                for _ in 0..trailing_newlines - 1 {
                    send_enter().await;
                    smol::Timer::after(std::time::Duration::from_millis(50)).await;
                }
            }
        }
    })
    .detach();
}

async fn send_text_chunked(text: &str, chunk_size: usize) {
    let bytes = text.as_bytes();
    for chunk in bytes.chunks(chunk_size) {
        let chunk_str = String::from_utf8_lossy(chunk);
        let escaped = escape_applescript(&chunk_str);
        let script = format!(
            r#"tell application "Ghostty"
    input text "{escaped}" to focused terminal of selected tab of front window
end tell"#
        );

        let result = smol::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .await;

        match result {
            Err(e) => {
                log::error!("send_code/ghostty: osascript failed: {}", e);
                return;
            }
            Ok(output) if !output.status.success() => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log::error!("send_code/ghostty: osascript error: {}", stderr);
                return;
            }
            _ => {}
        }

        // Small delay between chunks
        if bytes.len() > chunk_size {
            smol::Timer::after(std::time::Duration::from_millis(100)).await;
        }
    }
}

async fn send_enter() {
    let script = r#"tell application "Ghostty"
    send key "enter" to focused terminal of selected tab of front window
end tell"#;

    match smol::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .await
    {
        Err(e) => {
            log::error!("send_code/ghostty: osascript enter key failed: {}", e);
        }
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("send_code/ghostty: osascript enter key error: {}", stderr);
        }
        _ => {}
    }
}

fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
