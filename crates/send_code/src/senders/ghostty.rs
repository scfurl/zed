use gpui::App;

pub fn send(text: &str, use_bracketed_paste: bool, chunk_size: usize, cx: &mut App) {
    let text = text.to_string();
    let chunk_size = chunk_size.max(1);

    cx.spawn(async move |_cx| {
        let text_to_send = if use_bracketed_paste {
            format!("\x1b[200~{}\x1b[201~", text)
        } else {
            text
        };

        // Chunk the text to avoid AppleScript stalling on large inputs
        let bytes = text_to_send.as_bytes();
        for chunk in bytes.chunks(chunk_size) {
            let chunk_str = String::from_utf8_lossy(chunk);
            let escaped = escape_applescript(&chunk_str);
            let script = format!(
                r#"tell application "Ghostty"
    tell front window
        tell selected tab
            tell focused terminal
                input text "{escaped}"
            end tell
        end tell
    end tell
end tell"#
            );

            let result = smol::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
                .await;

            if let Err(e) = result {
                log::error!("send_code/ghostty: osascript failed: {}", e);
                return;
            }

            // Small delay between chunks
            smol::Timer::after(std::time::Duration::from_millis(100)).await;
        }

        // Send Enter key
        let enter_script = r#"tell application "Ghostty"
    tell front window
        tell selected tab
            tell focused terminal
                send key "enter"
            end tell
        end tell
    end tell
end tell"#;

        if let Err(e) = smol::process::Command::new("osascript")
            .arg("-e")
            .arg(enter_script)
            .output()
            .await
        {
            log::error!("send_code/ghostty: osascript enter key failed: {}", e);
        }
    })
    .detach();
}

fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
