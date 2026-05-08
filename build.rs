use std::fs;

fn main() {
    if std::env::var_os("CARGO_FEATURE_NAPI").is_some() {
        napi_build::setup();
    }

    // Relay ct2rs / ctranslate2 versions from Cargo.lock into compile-time
    // env vars consumed by `Version` in src/lib.rs. Empty strings if absent —
    // see PLAN_skeleton.md R1 (Fallback A / B).
    println!("cargo:rerun-if-changed=Cargo.lock");
    let ct2rs = read_lock_version("ct2rs").unwrap_or_default();
    let ctranslate2 = read_lock_version("ctranslate2").unwrap_or_default();
    println!("cargo:rustc-env=CADMUS_DEP_CT2RS_VERSION={ct2rs}");
    println!("cargo:rustc-env=CADMUS_DEP_CTRANSLATE2_VERSION={ctranslate2}");
}

fn read_lock_version(pkg: &str) -> Option<String> {
    let lock = fs::read_to_string("Cargo.lock").ok()?;
    let mut current_name: Option<String> = None;
    for line in lock.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("name = ") {
            current_name = Some(rest.trim_matches('"').to_string());
        } else if let Some(rest) = trimmed.strip_prefix("version = ") {
            if current_name.as_deref() == Some(pkg) {
                return Some(rest.trim_matches('"').to_string());
            }
        }
    }
    None
}
