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
    //
    // The TW_REMOTE_URL / TW_GH_USER / TW_TOKEN / TW_AUTHOR_* vars back the
    // product firmware's git push config (src/infrastructure/net.rs) as the
    // `BAKED_*` fallbacks for the card's typoena.conf. env!() bakes a value only
    // into a binary that references it, so the bench bins carry none of these.
    // NOTE: a baked TW_TOKEN lands in the flash image — fine for a personal dev
    // flash, but a shipped product must not bake the token (ADR-005); the
    // card-provisioned typoena.conf is the real path.
    for var in [
        "TW_WIFI_SSID",
        "TW_WIFI_PASS",
        "TW_REMOTE_URL",
        "TW_GH_USER",
        "TW_TOKEN",
        "TW_PAT", // legacy spelling — kept so the spike bins still compile
        "TW_AUTHOR_NAME",
        "TW_AUTHOR_EMAIL",
    ] {
        let val = std::env::var(var).unwrap_or_default();
        println!("cargo:rustc-env={var}={val}");
        println!("cargo:rerun-if-env-changed={var}");
    }
    // The product firmware reads TW_TOKEN; honor a legacy TW_PAT-only .env by
    // re-emitting it under the new name (an explicit TW_TOKEN wins above).
    if std::env::var("TW_TOKEN").map_or(true, |v| v.is_empty()) {
        if let Ok(legacy) = std::env::var("TW_PAT") {
            println!("cargo:rustc-env=TW_TOKEN={legacy}");
        }
    }

    // A full build with an empty push config used to be refused here
    // (env!() only bakes what `just` dotenv-loads from firmware/.env; a bare
    // `cargo build --features full` silently produced a firmware whose `:gp` /
    // `:gl` could never work — bit the 2026-07-13 flash). Since the runtime conf
    // (v0.9 onboarding slice 0) the card's /sd/typoena.conf overrides the
    // baked values per field, so an unbaked full build is legitimate — it just
    // needs a provisioned card. Warn instead of panic: the dev-flash foot-gun
    // stays visible, the card-provisioned path stays buildable.
    if std::env::var("CARGO_FEATURE_FULL").is_ok() {
        let missing: Vec<&str> = ["TW_WIFI_SSID", "TW_REMOTE_URL", "TW_GH_USER", "TW_TOKEN"]
            .into_iter()
            .filter(|v| {
                let set = |k: &str| std::env::var(k).is_ok_and(|val| !val.is_empty());
                !set(v) && !(*v == "TW_TOKEN" && set("TW_PAT"))
            })
            .collect();
        if !missing.is_empty() {
            println!(
                "cargo:warning=git build without baked push config ({} unset/empty): \
                 the device needs a provisioned typoena.conf on the card, or build \
                 through `just build` to bake firmware/.env.",
                missing.join(", ")
            );
        }
    }

    // Pointing rerun-if-changed at a file that never exists forces this
    // script to rerun on every build, keeping BUILD_TIME fresh.
    println!("cargo:rerun-if-changed=.force-build-stamp");
}
