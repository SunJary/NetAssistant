fn main() {
    let version = std::env::var("BUILD_VERSION")
        .ok()
        .unwrap_or_else(|| {
            chrono::Utc::now().format("%Y%m%d").to_string()
        });
    
    println!("cargo:rustc-env=APP_VERSION={}", version);

    #[cfg(target_os = "windows")]
    {
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
        res.set("FileDescription", "NetAssistant 网络调试工具");
        res.set("LegalCopyright", "Copyright (c) 2024 sunjary");
        res.set("InternalName", "NetAssistant");
        res.set("OriginalFilename", "NetAssistant.exe");

        if let Err(e) = res.compile() {
            eprintln!("Failed to compile Windows resources: {}", e);
        }
    }
}
