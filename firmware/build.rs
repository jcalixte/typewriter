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
    // Pointing rerun-if-changed at a file that never exists forces this
    // script to rerun on every build, keeping BUILD_TIME fresh.
    println!("cargo:rerun-if-changed=.force-build-stamp");
}
