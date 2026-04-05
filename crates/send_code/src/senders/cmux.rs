use gpui::App;

pub fn send(
    text: &str,
    use_bracketed_paste: bool,
    chunk_size: usize,
    surface: Option<&str>,
    cx: &mut App,
) {
    let text = text.to_string();
    let chunk_size = chunk_size.max(1);
    let surface = surface.map(|s| s.to_string());

    cx.spawn(async move |_cx| {
        let cmux_bin = resolve_cmux();

        if use_bracketed_paste {
            // Send bracketed paste start
            send_cmux_text(&cmux_bin, "\x1b[200~", surface.as_deref()).await;
        }

        // Chunk the text
        let bytes = text.as_bytes();
        for chunk in bytes.chunks(chunk_size) {
            let chunk_str = String::from_utf8_lossy(chunk);
            send_cmux_text(&cmux_bin, &chunk_str, surface.as_deref()).await;
        }

        if use_bracketed_paste {
            // Send bracketed paste end
            send_cmux_text(&cmux_bin, "\x1b[201~", surface.as_deref()).await;
            // Send ESC to finalize
            send_cmux_text(&cmux_bin, "\x1b", surface.as_deref()).await;
        } else {
            // Send enter key
            send_cmux_key(&cmux_bin, "enter", surface.as_deref()).await;
        }
    })
    .detach();
}

async fn send_cmux_text(cmux_bin: &str, text: &str, surface: Option<&str>) {
    let mut cmd = smol::process::Command::new(cmux_bin);
    cmd.arg("send").arg(text);
    if let Some(s) = surface {
        cmd.arg("--surface").arg(s);
    }
    if let Err(e) = cmd.output().await {
        log::error!("send_code/cmux: send failed: {}", e);
    }
}

async fn send_cmux_key(cmux_bin: &str, key: &str, surface: Option<&str>) {
    let mut cmd = smol::process::Command::new(cmux_bin);
    cmd.arg("send-key").arg(key);
    if let Some(s) = surface {
        cmd.arg("--surface").arg(s);
    }
    if let Err(e) = cmd.output().await {
        log::error!("send_code/cmux: send-key failed: {}", e);
    }
}

fn resolve_cmux() -> String {
    // Check common Homebrew locations since Zed may not have them in PATH
    for path in &["/opt/homebrew/bin/cmux", "/usr/local/bin/cmux"] {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }
    "cmux".to_string()
}
