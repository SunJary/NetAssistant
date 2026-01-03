use gpui::*;
use gpui_component::Root;

mod app;
mod config;
mod ui;
mod message;

use app::NetAssistantApp;

#[tokio::main]
async fn main() {
    let app = Application::new();

    app.run(move |cx| {
        // 必须在使用任何 GPUI Component 功能之前调用
        gpui_component::init(cx);

        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point { x: px(100.0), y: px(100.0) },
                        size: gpui::Size { width: px(1200.0), height: px(800.0) },
                    })),
                    titlebar: Some(TitlebarOptions {
                        title: Some("NetAssistant - 多协议网络调试工具".into()),
                        appears_transparent: false,
                        traffic_light_position: None,
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| NetAssistantApp::new(window, cx));
                    // 窗口的第一层应该是 Root
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}