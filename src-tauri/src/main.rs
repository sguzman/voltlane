#[cfg(target_os = "linux")]
fn configure_wayland_env() {
    let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    if !is_wayland {
        return;
    }

    // Work around WebKitGTK + Wayland protocol errors seen on some drivers/compositors.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: This runs at process startup before any threads are spawned.
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    configure_wayland_env();
    voltlane_tauri_lib::run();
}
