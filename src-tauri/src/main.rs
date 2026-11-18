// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bolke_app_lib::CliArgs;
use clap::Parser;

fn main() {
    // Let users pick GTK's display backend even inside the AppImage, whose
    // launch hook forces GDK_BACKEND=x11 over the user's own value (#1729,
    // #2034; hook workaround for tauri-apps/tauri#8541). Runs first so GTK
    // never sees the forced value.
    #[cfg(target_os = "linux")]
    bolke_app_lib::apply_bolke_gdk_backend();

    bolke_app_lib::webkit_renderer::apply();

    #[cfg(target_os = "windows")]
    {
        // Avoid overlay/capture layer crashes (#2049). Set before backend
        // initialization, preserving user overrides.
        if std::env::var_os("VK_LOADER_LAYERS_DISABLE").is_none()
            && !bolke_app_lib::env_flag_enabled("BOLKE_KEEP_VULKAN_IMPLICIT_LAYERS")
        {
            std::env::set_var("VK_LOADER_LAYERS_DISABLE", "~implicit~");
        }
    }

    #[cfg(target_os = "linux")]
    bolke_app_lib::disable_gamescope_wsi_layer();

    // Transcription worker: runs transcribe.cpp in isolation and never touches
    // Tauri, the CLI, or single-instance handling. Checked after the env setup
    // above so the worker inherits it (e.g. the Vulkan layer opt-out).
    if bolke_app_lib::engine_supervisor::is_worker_invocation() {
        std::process::exit(bolke_app_lib::engine_supervisor::run_worker());
    }

    // Voice separator for meetings: runs on its own, like the worker above.
    if bolke_app_lib::speakers::is_worker_invocation() {
        std::process::exit(bolke_app_lib::speakers::run_worker());
    }

    let cli_args = CliArgs::parse();
    bolke_app_lib::run(cli_args)
}
