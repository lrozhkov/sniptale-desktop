use std::{env, path::PathBuf, process::Command};

fn main() {
    if let Err(error) = run() {
        println!("cargo:warning={error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    println!("cargo:rerun-if-env-changed=SNIPTALE_WINDOWS_RESOURCES");
    for path in [
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
        Ok("check-skip") => Ok(()),
        Ok("require") => compile_resources(),
        _ => Err("Windows artifacts require SNIPTALE_WINDOWS_RESOURCES=require".to_owned()),
    }
}

fn compile_resources() -> Result<(), String> {
    let root = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| "Cargo manifest path is missing".to_owned())?,
    );
    let output = PathBuf::from(
        env::var_os("OUT_DIR").ok_or_else(|| "Cargo output path is missing".to_owned())?,
    );
    let resource = output.join("sniptale.res");
    let status = Command::new("rc.exe")
        .arg("/nologo")
        .arg(format!("/fo{}", resource.display()))
        .arg(root.join("resources/windows/sniptale.rc"))
        .status()
        .map_err(|_| "Windows artifact build requires rc.exe".to_owned())?;
    if !status.success() {
        return Err("rc.exe failed to compile Sniptale resources".to_owned());
    }
    println!("cargo:rustc-link-arg={}", resource.display());
    Ok(())
}
