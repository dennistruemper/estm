fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let timestamp = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%d %H:%M:%S UTC"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=ESTM_BUILD_TIMESTAMP={timestamp}");

    tauri_build::build()
}
