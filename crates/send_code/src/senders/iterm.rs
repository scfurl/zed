use gpui::App;

pub fn send(text: &str, use_bracketed_paste: bool, cx: &mut App) {
    let text = text.to_string();

    cx.spawn(async move |_cx| {
        let text_to_send = if use_bracketed_paste {
            format!("\x1b[200~{}\x1b[201~", text)
        } else {
            text
        };

        // Chunk into 1000-char segments (iTerm can stall on large inputs)
        let chunk_size = 1000;
        let bytes = text_to_send.as_bytes();
        for chunk in bytes.chunks(chunk_size) {
            let chunk_str = String::from_utf8_lossy(chunk);
            let escaped = escape_applescript(&chunk_str);
            let script = format!(
                r#"tell application "iTerm"
    tell current session of current window
        write text "{escaped}" without newline
    end tell
end tell"#
            );

            if let Err(e) = smol::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
                .await
            {
                log::error!("send_code/iterm: osascript failed: {}", e);
                return;
            }

            smol::Timer::after(std::time::Duration::from_millis(100)).await;
        }

        // Commit with a newline
        let newline_script = r#"tell application "iTerm"
    tell current session of current window
        write text ""
    end tell
end tell"#;

        if let Err(e) = smol::process::Command::new("osascript")
            .arg("-e")
            .arg(newline_script)
            .output()
            .await
        {
            log::error!("send_code/iterm: osascript newline failed: {}", e);
        }
    })
    .detach();
}

fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
