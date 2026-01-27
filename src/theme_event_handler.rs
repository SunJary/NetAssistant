use gpui::*;
use gpui_component::theme::{Theme, ThemeRegistry};
use log::info;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ThemeEvent {
    SystemThemeChanged(bool),
}

impl Global for ThemeEventHandler {}

pub struct ThemeEventHandler {
    event_sender: Option<mpsc::UnboundedSender<ThemeEvent>>,
    event_receiver: Option<mpsc::UnboundedReceiver<ThemeEvent>>,
    is_dark_mode: bool,
}

impl ThemeEventHandler {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            event_sender: Some(sender),
            event_receiver: Some(receiver),
            is_dark_mode: false,
        }
    }

    pub fn is_dark_mode(&self) -> bool {
        self.is_dark_mode
    }

    pub fn toggle_theme(&mut self) {
        self.is_dark_mode = !self.is_dark_mode;
        info!(
            "手动切换主题: {}",
            if self.is_dark_mode { "Dark" } else { "Light" }
        );
    }

    pub fn update_from_system_theme(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            use crate::theme_detector::ThemeDetector;
            let detector = ThemeDetector::new();
            let system_is_dark = detector.is_dark_mode();

            if self.is_dark_mode != system_is_dark {
                self.is_dark_mode = system_is_dark;
                info!(
                    "系统主题变化，更新为: {}",
                    if system_is_dark { "Dark" } else { "Light" }
                );
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            if self.is_dark_mode {
                self.is_dark_mode = false;
            }
        }
    }

    pub fn start_listener(&mut self) {
        #[cfg(target_os = "windows")]
        {
            let event_sender = self.event_sender.clone();

            std::thread::spawn(move || {
                use crate::theme_detector::ThemeDetector;
                let mut last_is_dark = ThemeDetector::new().is_dark_mode();

                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));

                    let current_is_dark = ThemeDetector::new().is_dark_mode();
                    if current_is_dark != last_is_dark {
                        last_is_dark = current_is_dark;

                        if let Some(sender) = event_sender.clone() {
                            let _ = sender.send(ThemeEvent::SystemThemeChanged(current_is_dark));
                        }
                    }
                }
            });
        }
    }

    pub fn handle_events(&mut self) -> bool {
        let mut events = Vec::new();
        let mut need_notify = false;

        if let Some(ref mut receiver) = self.event_receiver {
            while let Ok(event) = receiver.try_recv() {
                events.push(event);
            }
        }

        for event in events {
            match event {
                ThemeEvent::SystemThemeChanged(is_dark) => {
                    if self.is_dark_mode != is_dark {
                        self.is_dark_mode = is_dark;
                        info!(
                            "系统主题变化，更新为: {}",
                            if is_dark { "Dark" } else { "Light" }
                        );
                        need_notify = true;
                    }
                }
            }
        }

        need_notify
    }
}

pub fn apply_theme(is_dark_mode: bool, cx: &mut App) {
    let theme_name = if is_dark_mode {
        SharedString::from("Default Dark")
    } else {
        SharedString::from("Default Light")
    };

    info!("应用主题: {}", theme_name);

    if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
        Theme::global_mut(cx).apply_config(&theme);
        info!("主题已应用: {}", theme_name);
    } else {
        info!("主题 {} 未找到", theme_name);
    }
}
