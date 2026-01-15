fn main() {
    #[cfg(target_os = "windows")]
    {
        // 根据构建模式设置不同的子系统
        // debug模式使用控制台子系统（有命令行窗口）
        // release模式使用Windows子系统（无命令行窗口）
        if cfg!(debug_assertions) {
            println!("cargo:rustc-link-arg=/SUBSYSTEM:CONSOLE");
        } else {
            println!("cargo:rustc-link-arg=/SUBSYSTEM:WINDOWS");
            println!("cargo:rustc-link-arg=/ENTRY:mainCRTStartup");
        }

        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon/icon.ico");
        res.set_version_info(winres::VersionInfo::PRODUCTVERSION, 0x0001000000000000);
        res.set_version_info(winres::VersionInfo::FILEVERSION, 0x0001000000000000);
        res.set("ProductName", "NetAssistant");
        res.set("CompanyName", "sunjary");
        res.set("FileDescription", "多协议网络调试工具");
        res.set("LegalCopyright", "Copyright (c) 2024 sunjary");

        if let Err(e) = res.compile() {
            eprintln!("Failed to compile Windows resources: {}", e);
        }
    }
}
