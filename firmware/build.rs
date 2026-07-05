use std::process::Command;

fn main() {
    embuild::espidf::sysenv::output();

    // Stamp the binary so serial output and on-panel text identify the
    // exact build (bring-up lesson: know which build you're diagnosing).
    let git = Command::new("git")
        .args(["describe", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let time = Command::new("date")
        .args(["-u", "+%m-%d %H:%M"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=BUILD_GIT={git}");
    println!("cargo:rustc-env=BUILD_TIME={time}Z");

    // Wi-Fi credentials for the network spikes (6/7) and, later, the runtime.
    // Read at build time and emitted as compile-time env so a binary can pull
    // them in with env!(). Empty when unset: the network spike checks at
    // runtime and prints a clear message, so the *editor* build never has to
    // carry Wi-Fi creds. Source them from firmware/.env (loaded by `just`).
    for var in ["TW_WIFI_SSID", "TW_WIFI_PASS"] {
        let val = std::env::var(var).unwrap_or_default();
        println!("cargo:rustc-env={var}={val}");
        println!("cargo:rerun-if-env-changed={var}");
    }

    // Pointing rerun-if-changed at a file that never exists forces this
    // script to rerun on every build, keeping BUILD_TIME fresh.
    println!("cargo:rerun-if-changed=.force-build-stamp");
}
