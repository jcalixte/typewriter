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
    // The TW_REMOTE_URL / TW_GH_USER / TW_PAT / TW_AUTHOR_* vars back Spike 7's
    // on-device push (src/bin/git_push.rs). env!() embeds a value only in a
    // binary that references it, so the editor binary carries none of these.
    // NOTE: TW_PAT ends up in the git_push image — fine for the bench spike, but
    // a product must not bake the PAT into flash (ADR-005).
    for var in [
        "TW_WIFI_SSID",
        "TW_WIFI_PASS",
        "TW_REMOTE_URL",
        "TW_GH_USER",
        "TW_PAT",
        "TW_AUTHOR_NAME",
        "TW_AUTHOR_EMAIL",
    ] {
        let val = std::env::var(var).unwrap_or_default();
        println!("cargo:rustc-env={var}={val}");
        println!("cargo:rerun-if-env-changed={var}");
    }

    // A git-feature build with an empty publish config used to be refused here
    // (env!() only bakes what `just` dotenv-loads from firmware/.env; a bare
    // `cargo build --features git` silently produced a firmware whose `:sync`
    // could never work — bit the 2026-07-13 flash). Since the runtime conf
    // (v0.9 onboarding slice 0) the card's /sd/typoena.conf overrides the
    // baked values per field, so an unbaked git build is legitimate — it just
    // needs a provisioned card. Warn instead of panic: the dev-flash foot-gun
    // stays visible, the card-provisioned path stays buildable.
    if std::env::var("CARGO_FEATURE_GIT").is_ok() {
        let missing: Vec<&str> = ["TW_WIFI_SSID", "TW_REMOTE_URL", "TW_GH_USER", "TW_PAT"]
            .into_iter()
            .filter(|v| std::env::var(v).map_or(true, |val| val.is_empty()))
            .collect();
        if !missing.is_empty() {
            println!(
                "cargo:warning=git build without baked publish config ({} unset/empty): \
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
