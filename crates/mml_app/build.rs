//! Embeds the application icon in the Windows executable, so Explorer,
//! the taskbar and shortcuts show it instead of a generic program icon.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon.ico");
        // A missing resource compiler must not break the build: the
        // window icon is set at run time anyway.
        if let Err(error) = resource.compile() {
            println!("cargo:warning=icon not embedded in the executable: {error}");
        }
    }
}
