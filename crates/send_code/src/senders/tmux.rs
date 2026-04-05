use gpui::App;

pub fn send(text: &str, use_bracketed_paste: bool, target: Option<&str>, cx: &mut App) {
    let text = text.to_string();
    let target = target.map(|s| s.to_string());

    cx.spawn(async move |_cx| {
        if use_bracketed_paste {
            // Send bracketed paste start escape sequence
            send_tmux_keys("\x1b[200~", target.as_deref()).await;
        }

        // Chunk text into 200-char segments
        let chunk_size = 200;
        let bytes = text.as_bytes();
        for chunk in bytes.chunks(chunk_size) {
            let chunk_str = String::from_utf8_lossy(chunk);
            // tmux set-buffer + paste-buffer approach
            let mut set_cmd = smol::process::Command::new("tmux");
            set_cmd.arg("set-buffer").arg("--").arg(chunk_str.as_ref());
            if let Err(e) = set_cmd.output().await {
                log::error!("send_code/tmux: set-buffer failed: {}", e);
                return;
            }

            let mut paste_cmd = smol::process::Command::new("tmux");
            paste_cmd.arg("paste-buffer").arg("-d");
            if let Some(ref t) = target {
                paste_cmd.arg("-t").arg(t);
            }
            if let Err(e) = paste_cmd.output().await {
                log::error!("send_code/tmux: paste-buffer failed: {}", e);
                return;
            }
        }

        if use_bracketed_paste {
            // Send bracketed paste end escape sequence
            send_tmux_keys("\x1b[201~", target.as_deref()).await;
        } else {
            // Send Enter
            send_tmux_keys("Enter", target.as_deref()).await;
        }
    })
    .detach();
}

async fn send_tmux_keys(keys: &str, target: Option<&str>) {
    let mut cmd = smol::process::Command::new("tmux");
    cmd.arg("send-keys");
    if let Some(t) = target {
        cmd.arg("-t").arg(t);
    }
    cmd.arg(keys);
    if let Err(e) = cmd.output().await {
        log::error!("send_code/tmux: send-keys failed: {}", e);
    }
}
