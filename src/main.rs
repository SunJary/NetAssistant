use gpui::*;
use log::info;
use simple_logger::SimpleLogger;

// 导入自定义资产
use crate::assets::CustomAssets;

mod app;
mod assets;
mod config;
mod core;
mod custom_icons;
mod message;
mod network;
mod ui;
mod utils;
mod theme_manager;
mod theme_event_handler;

use app::NetAssistantApp;
use theme_manager::ThemeManager;
use theme_event_handler::{ThemeEventHandler, apply_theme};

#[tokio::main]
async fn main() {
    // 初始化日志
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug) // 提高日志级别到 Debug，确保所有日志都能显示
        .with_utc_timestamps()
        .init()
        .unwrap();

    info!("=== 应用程序启动 ===");
    let app = Application::new().with_assets(CustomAssets::new());
    info!("=== Application::new() 创建成功 ===");

    app.run(move |cx| {
        info!("=== 进入 app.run 回调 ===");
        // 必须在使用任何 GPUI Component 功能之前调用
        gpui_component::init(cx);
        info!("=== gpui_component::init() 完成 ===");

        // 初始化主题管理器
        let mut theme_manager = ThemeManager::new();
        theme_manager.init(cx);
        info!("=== 主题管理器初始化完成 ===");

        let bounds = Bounds {
            origin: Point {
                x: px(100.0),
                y: px(100.0),
            },
            size: gpui::Size {
                width: px(1200.0),
                height: px(800.0),
            },
        };
        
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("NetAssistant".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                ..Default::default()
            },
            |window, cx| {
                info!("=== 进入 open_window 回调 ===");
                // 创建应用实例
                let app = cx.new(|cx| NetAssistantApp::new(window, cx));
                
                // 初始化主题处理器
                let theme_handler = ThemeEventHandler::new();
                cx.set_global(theme_handler);
                
                // 注册GPUI窗口主题变化监听
                window.observe_window_appearance(move |window, cx| {
                    info!("=== 应用级别主题变化回调被调用 ===");
                    let is_dark = window.appearance() == gpui::WindowAppearance::Dark;
                    info!("检测到主题变化: is_dark = {}", is_dark);
                    apply_theme(is_dark, cx);
                    cx.global_mut::<ThemeEventHandler>().set_is_dark_mode(is_dark);
                    info!("=== 应用级别主题变化回调处理完成 ===");
                })
                .detach();
                
                // 初始化主题状态（根据当前窗口主题）
                let is_dark = window.appearance() == gpui::WindowAppearance::Dark;
                cx.global_mut::<ThemeEventHandler>().set_is_dark_mode(is_dark);
                apply_theme(is_dark, cx);
                
                // 使用 gpui_component::Root 包装应用
                cx.new(|cx| gpui_component::Root::new(app, window, cx))
            },
        )
        .unwrap();
        
        info!("=== open_window 调用完成 ===");
    });
    info!("=== app.run 调用完成 ===");
}
