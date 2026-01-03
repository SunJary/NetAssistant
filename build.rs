fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-arg=/SUBSYSTEM:WINDOWS");
        println!("cargo:rustc-link-arg=/ENTRY:mainCRTStartup");
        
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
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