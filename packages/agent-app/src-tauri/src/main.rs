// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    prepare_linux_wayland_ime_env();
    agent_app_lib::run()
}

/// On Wayland, a global `GTK_IM_MODULE=fcitx` forces the fcitx GTK IM module.
/// In WebKitGTK (Tauri/wry) that path mis-positions the candidate window in
/// nested layouts (offset grows toward the bottom of the window). Clearing the
/// variable lets GTK use native `zwp_text_input_v3` instead. Skip when the user
/// already forced X11 via `GDK_BACKEND=x11`.
fn prepare_linux_wayland_ime_env() {
    #[cfg(target_os = "linux")]
    {
        let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let forced_x11 = std::env::var_os("GDK_BACKEND").is_some_and(|v| v == "x11");
        if on_wayland && !forced_x11 {
            // Before GTK/Tauri init so the IM module choice is not already locked in.
            std::env::remove_var("GTK_IM_MODULE");
        }
    }
}
