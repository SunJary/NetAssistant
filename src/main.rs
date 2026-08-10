use gpui::*;
use gpui_component::TitleBar;
use log::{error, info};
use simple_logger::SimpleLogger;
use std::borrow::Cow;

// 导入配置存储
use crate::config::storage::ConfigStorage;

// 导入自定义资产
use crate::assets::{CustomAssets, Fonts};

mod app;
mod assets;
mod config;
mod core;
mod custom_icons;
mod export;
mod log_writer;
mod message;
mod network;
mod stress;
mod ui;
mod utils;
mod theme_manager;
mod theme_event_handler;
mod update_checker;

use app::NetAssistantApp;
use theme_manager::ThemeManager;
use theme_event_handler::{ThemeEventHandler, apply_theme};

fn main() {
    // 初始化日志
    // - debug 构建: DEBUG 级别（cosmic_text 过滤到 Info 避免字体回退噪音）
    // - release 构建: ERROR 级别（输出干净，只显示错误）
    // - 环境变量 NETASSISTANT_LOG 可覆盖（如 NETASSISTANT_LOG=debug 调试 release 版）
    let default_level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Error
    };
    let level = std::env::var("NETASSISTANT_LOG")
        .ok()
        .and_then(|v| v.parse::<log::LevelFilter>().ok())
        .unwrap_or(default_level);

    SimpleLogger::new()
        .with_level(level)
        .with_module_level("cosmic_text", log::LevelFilter::Info)
        .with_utc_timestamps()
        .init()
        .unwrap();

    info!("=== 应用程序启动 (日志级别: {}) ===", level);

    // 手动创建 tokio runtime（替代 #[tokio::main]）
    // 目的：在 app.run() 返回后能用 shutdown_timeout 给后台任务优雅退出时间，
    // 并用 std::process::exit(0) 兜底终止 GPUI dispatcher 等非 tokio 线程
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("创建 tokio runtime 失败");

    // 进入 runtime context，让 app.run 闭包内能调用 tokio::runtime::Handle::current()
    let _guard = runtime.enter();

    run_app();

    // app.run() 已返回：GPUI 已销毁窗口和 entity，Drop 已清理连接
    info!("=== app.run 已返回，准备关闭 tokio runtime ===");

    // 给后台任务 3 秒优雅退出时间：
    // - 日志文件 close/flush
    // - 网络 task 通过 cancel_token 协作取消后退出
    // - 压测引擎 stop 后的清理
    // 超时后 tokio 强制取消剩余 task
    runtime.shutdown_timeout(std::time::Duration::from_secs(3));

    // 兜底：强制退出进程
    // 终止 GPUI dispatcher 等非 tokio 线程（dispatcher 偶尔在窗口销毁后不退出消息循环，导致进程残留）
    // OS 会回收所有资源（socket、文件句柄、内存）
    info!("=== 强制退出进程 ===");
    std::process::exit(0);
}

/// 运行 GPUI 应用
fn run_app() {
    let app = gpui_platform::application().with_assets(CustomAssets::new());
    info!("=== Application::new() 创建成功 ===");

    app.run(move |cx| {
        info!("=== 进入 app.run 回调 ===");
        // 必须在使用任何 GPUI Component 功能之前调用
        gpui_component::init(cx);
        info!("=== gpui_component::init() 完成 ===");

        // 加载嵌入的 JetBrains Mono 等宽字体
        // 非英文字符（如中文）由系统字体自动回退渲染
        let font_data: Vec<Cow<'static, [u8]>> = Fonts::iter()
            .filter_map(|path| Fonts::get(&path).map(|f| Cow::Owned(f.data.to_vec())))
            .collect();
        match cx.text_system().add_fonts(font_data) {
            Ok(_) => info!("=== JetBrains Mono 字体加载完成 ==="),
            Err(e) => error!("=== 字体加载失败: {:?} ===", e),
        }

        // 初始化主题管理器
        let mut theme_manager = ThemeManager::new();
        theme_manager.init(cx);
        info!("=== 主题管理器初始化完成 ===");

        // 加载窗口配置
        let window_bounds = match ConfigStorage::new() {
            Ok(storage) => {
                if let Some((x, y, width, height)) = storage.load_window_bounds() {
                    info!("=== 从配置加载窗口尺寸: {}x{} @ ({}, {}) ===", width, height, x, y);
                    // 确保窗口在可见区域内，至少x和y坐标为0
                    let visible_x = x.max(0.0);
                    let visible_y = y.max(0.0);
                    
                    if visible_x != x || visible_y != y {
                        info!("=== 调整窗口位置到可见区域: {}x{} @ ({}, {}) ===", width, height, visible_x, visible_y);
                    }
                    
                    Bounds {
                        origin: Point {
                            x: px(visible_x as f32),
                            y: px(visible_y as f32),
                        },
                        size: gpui::Size {
                            width: px(width as f32),
                            height: px(height as f32),
                        },
                    }
                } else {
                    info!("=== 使用默认窗口尺寸 ===");
                    // 使用默认窗口尺寸
                    Bounds {
                        origin: Point {
                            x: px(100.0),
                            y: px(100.0),
                        },
                        size: gpui::Size {
                            width: px(900.0),
                            height: px(600.0),
                        },
                    }
                }
            },
            Err(e) => {
                error!("=== 加载配置失败，使用默认窗口尺寸: {:?} ===", e);
                // 使用默认窗口尺寸
                Bounds {
                    origin: Point {
                        x: px(100.0),
                        y: px(100.0),
                    },
                    size: gpui::Size {
                        width: px(900.0),
                        height: px(600.0),
                    },
                }
            },
        };
        
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                window_min_size: Some(gpui::Size { width: px(600.0), height: px(300.0) }),
                titlebar: Some(TitleBar::title_bar_options()),
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
                cx.new(|cx| {
                    // 监听窗口大小变化，实现响应式布局和窗口配置保存
                    let app_clone = app.clone();
                    
                    cx.observe_window_bounds(window, move |_, window, cx| {
                        // 获取窗口内容大小和位置
                        let window_bounds = window.bounds();
                        let content_size = window_bounds.size;
                        let origin = window_bounds.origin;
                        
                        // 设置响应式断点（800px）
                        let threshold = px(800.0);
                        
                        // 根据窗口宽度自动隐藏/显示侧边栏
                        app_clone.update(cx, |app, cx| {
                            if content_size.width < threshold && !app.sidebar_collapsed {
                                app.sidebar_collapsed = true;
                                cx.notify();
                            } else if content_size.width >= threshold && app.sidebar_collapsed {
                                app.sidebar_collapsed = false;
                                cx.notify();
                            }
                            
                            // 计算并更新消息容器宽度
                            let sidebar_width = app.sidebar_width.unwrap_or(px(200.0));
                            // 连接信息面板的宽度特性：
                            // - 默认宽度为父容器的1/4
                            // - 最小宽度：40rem (160px)
                            // - 最大宽度：64rem (256px)
                            let parent_content_width = if app.sidebar_collapsed {
                                content_size.width - px(16.0)
                            } else {
                                content_size.width - sidebar_width - px(16.0)
                            };
                            // 根据连接信息面板的实际宽度特性计算可用宽度
                            let connection_info_width = parent_content_width * 0.25;
                            let connection_info_width = connection_info_width.max(px(160.0)).min(px(256.0));
                            let available_width = parent_content_width - connection_info_width;
                            let message_width = if available_width > px(0.0) {
                                available_width
                            } else {
                                px(800.0)
                            };
                            app.message_container_width = Some(message_width);
                            
                            // 不需要清空缓存，因为消息级别缓存已有宽度判断逻辑
                            // 当宽度变化超过 10px 时会自动重新计算
                        });
                        
                        // 保存窗口配置
                        if let Ok(mut storage) = ConfigStorage::new() {
                            let x = (origin.x / gpui::px(1.0)) as f64;
                            let y = (origin.y / gpui::px(1.0)) as f64;
                            let width = (content_size.width / gpui::px(1.0)) as f64;
                            let height = (content_size.height / gpui::px(1.0)) as f64;
                            
                            // 检查窗口位置是否有效（防止窗口被关闭时保存无效位置）
                            if x > -1000.0 && y > -1000.0 && x < 32768.0 && y < 32768.0 {
                                storage.save_window_bounds(Some(x), Some(y), width, height);
                            } else {
                                // 只保存窗口尺寸，不保存无效位置
                                storage.save_window_bounds(None, None, width, height);
                            }
                        }
                    })
                    .detach();
                    
                    gpui_component::Root::new(app, window, cx)
                })
            },
        )
        .unwrap();
        
        info!("=== open_window 调用完成 ===");
    });
}
