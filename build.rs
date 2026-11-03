use std::{env, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        println!("cargo:warning={error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    println!("cargo:rerun-if-env-changed=SNIPTALE_WINDOWS_RESOURCES");
    println!("cargo:rustc-check-cfg=cfg(sniptale_check_only)");
    for path in [
        "ui/dist",
        "tauri.conf.json",
        "capabilities",
        "resources/windows/sniptale.rc",
        "resources/windows/sniptale-native-agent.manifest",
        "resources/windows/icons/sniptale.ico",
        "resources/windows/icons/sniptale-recording.ico",
        "resources/windows/icons/sniptale-paused.ico",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    match env::var("SNIPTALE_WINDOWS_RESOURCES").as_deref() {
        Ok("check-skip") => {
            println!("cargo:rustc-cfg=sniptale_check_only");
            Ok(())
        }
        Ok("require") => prepare_tauri(),
        _ => Err("Windows artifacts require SNIPTALE_WINDOWS_RESOURCES=require".to_owned()),
    }
}

/// Native artifacts generate the complete context, ACLs, and Windows resources.
fn prepare_tauri() -> Result<(), String> {
    const COMMANDS: &[&str] = &[
        "desktop_capture_snapshot",
        "desktop_capture_command",
        "desktop_preferences_snapshot",
        "desktop_preferences_save",
        "desktop_settings_command",
        "desktop_close_settings",
        "desktop_open_settings",
        "desktop_panel_frame",
        "desktop_close_panel",
        "desktop_drag_panel",
        "desktop_prompt_snapshot",
        "desktop_prompt_reply",
        "desktop_prompt_dismiss",
    ];
    if env::var_os("TAURI_CONFIG").is_some() {
        return Err("Packaged builds require the canonical Tauri configuration".to_owned());
    }
    let root = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| "Cargo manifest path is missing".to_owned())?,
    );
    let icons = root
        .join("resources/windows/icons")
        .to_string_lossy()
        .replace('\\', "/");
    let extra_resources = include_str!("resources/windows/sniptale.rc")
        .replace("\"icons/", &format!("\"{icons}/"))
        // The helper embeds the canonical manifest once through WindowsAttributes.
        .replace("1 RT_MANIFEST \"sniptale-native-agent.manifest\"", "");
    let windows = tauri_build::WindowsAttributes::new()
        .app_manifest(include_str!(
            "resources/windows/sniptale-native-agent.manifest"
        ))
        .append_rc_content(extra_resources);
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS))
        .windows_attributes(windows)
        .codegen(tauri_build::CodegenContext::new());
    tauri_build::try_build(attributes)
        .map_err(|error| format!("Tauri build preparation failed: {error:#}"))
}
