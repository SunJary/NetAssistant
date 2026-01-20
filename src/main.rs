use gpui::*;
use gpui_component_assets::Assets;
use log::info;
use simple_logger::SimpleLogger;

mod app;
mod config;
mod message;
mod ui;
mod utils;

use app::NetAssistantApp;

#[tokio::main]
async fn main() {
    // 初始化日志
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug) // 提高日志级别到 Debug，确保所有日志都能显示
        .with_utc_timestamps()
        .init()
        .unwrap();

    info!("=== 应用程序启动 ===");
    let app = Application::new().with_assets(Assets);
    info!("=== Application::new() 创建成功 ===");

    app.run(move |cx| {
        info!("=== 进入 app.run 回调 ===");
        // 必须在使用任何 GPUI Component 功能之前调用
        gpui_component::init(cx);
        info!("=== gpui_component::init() 完成 ===");

        cx.spawn(async move |cx| {
            info!("=== 进入 spawn 异步任务 ===");
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point {
                            x: px(100.0),
                            y: px(100.0),
                        },
                        size: gpui::Size {
                            width: px(1200.0),
                            height: px(800.0),
                        },
                    })),
                    titlebar: Some(TitlebarOptions {
                        title: Some("NetAssistant - 多协议网络调试工具".into()),
                        appears_transparent: false,
                        traffic_light_position: None,
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    info!("=== 进入 open_window 回调 ===");
                    // 创建应用实例
                    let app = cx.new(|cx| NetAssistantApp::new(window, cx));
                    // 使用 gpui_component::Root 包装应用
                    cx.new(|cx| gpui_component::Root::new(app, window, cx))
                },
            )?;
            info!("=== open_window 调用完成 ===");

            Ok::<_, anyhow::Error>(())
        })
        .detach();
        info!("=== spawn 任务已 detach ===");
    });
    info!("=== app.run 调用完成 ===");
}
