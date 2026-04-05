use gpui::App;

pub fn send(text: &str, cx: &mut App) {
    let text = text.to_string();

    cx.spawn(async move |_cx| {
        let escaped = escape_applescript(&text);
        let script = format!(
            r#"tell application "Terminal"
    do script "{escaped}" in front window
end tell"#
        );

        if let Err(e) = smol::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .await
        {
            log::error!("send_code/terminal_app: osascript failed: {}", e);
        }
    })
    .detach();
}

fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
