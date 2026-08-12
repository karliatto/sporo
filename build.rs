fn main() {
    check_xtensa_linker_available();

    // linkall.x must be the last linker script.
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

/// Fail with an actionable message rather than a wall of linker errors when the
/// espup environment hasn't been sourced.
#[cfg(unix)]
fn check_xtensa_linker_available() {
    println!("cargo:rerun-if-env-changed=PATH");

    let linker = std::env::var("CARGO_TARGET_XTENSA_ESP32_NONE_ELF_LINKER")
        .unwrap_or_else(|_| "xtensa-esp32-elf-gcc".to_string());

    if std::process::Command::new(&linker)
        .arg("--version")
        .output()
        .is_ok()
    {
        return;
    }

    let export_file = std::env::var("HOME")
        .map(|home| format!("{home}/export-esp.sh"))
        .unwrap_or_else(|_| "$HOME/export-esp.sh".to_string());

    panic!(
        "Xtensa linker `{linker}` was not found in PATH.\n\n\
         Run `source {export_file}` first, or `espup install` if that file does not exist.\n\
         See https://github.com/esp-rs/espup#environment-variables-setup"
    );
}

#[cfg(not(unix))]
fn check_xtensa_linker_available() {}
