// Modified for EsanCraft; original notices retained. See MODIFICATIONS.md.
//! Windows only: embed the app icon and version info (VERSIONINFO) into `esancraft.exe`, so it
//! shows in Explorer, the taskbar, the Start menu and Alt-Tab.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `ESANCRAFT_REQUIRE_WINRES=1` turns it
//! into an error (for release builds).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/esancraft.ico");
    println!("cargo:rerun-if-env-changed=ESANCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/esancraft.ico")
        .set("ProductName", "EsanCraft")
        .set("FileDescription", "EsanCraft digital audio workstation")
        .set("CompanyName", "EsanCraft contributors")
        .set("LegalCopyright", "Copyright (c) the SoundCraft authors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "esancraft.exe")
        .set("InternalName", "esancraft");
    if let Err(e) = res.compile() {
        if std::env::var_os("ESANCRAFT_REQUIRE_WINRES").is_some() {
            eprintln!("embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=esancraft.exe built without icon/version resources: {e}");
    }
}
