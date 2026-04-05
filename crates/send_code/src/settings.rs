use collections::HashMap;
use gpui::App;
use settings::{RegisterSetting, Settings};

#[derive(Debug, Default, Clone, RegisterSetting)]
pub struct SendCodeSettings {
    pub enabled: bool,
    pub debug: bool,
    pub target: String,
    pub bracketed_paste: bool,
    pub ghostty_chunk_size: usize,
    pub cmux_chunk_size: usize,
    pub cmux_surface: Option<String>,
    pub tmux_target: Option<String>,
    pub language_targets: HashMap<String, String>,
}

impl SendCodeSettings {
    pub fn enabled(cx: &App) -> bool {
        Self::get_global(cx).enabled
    }
}

impl Settings for SendCodeSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let sc = content.send_code.as_ref();
        match sc {
            Some(sc) => Self {
                enabled: sc.enabled.unwrap_or(true),
                debug: sc.debug.unwrap_or(false),
                target: sc.target.clone().unwrap_or_else(|| "zed_terminal".into()),
                bracketed_paste: sc.bracketed_paste.unwrap_or(true),
                ghostty_chunk_size: sc.ghostty_chunk_size.unwrap_or(1000),
                cmux_chunk_size: sc.cmux_chunk_size.unwrap_or(200),
                cmux_surface: sc.cmux_surface.clone(),
                tmux_target: sc.tmux_target.clone(),
                language_targets: sc.language_targets.clone().unwrap_or_default(),
            },
            None => Self {
                enabled: true,
                debug: false,
                target: "zed_terminal".into(),
                bracketed_paste: true,
                ghostty_chunk_size: 1000,
                cmux_chunk_size: 200,
                cmux_surface: None,
                tmux_target: None,
                language_targets: HashMap::default(),
            },
        }
    }
}

pub use settings::settings_content::SendCodeSettingsContent;
