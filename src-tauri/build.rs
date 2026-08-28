// LoLShorts Tauri Build Script
// Simple build configuration - no external dependencies required

fn main() {
    for name in [
        "SUPABASE_URL",
        "SUPABASE_ANON_KEY",
        "YOUTUBE_CLIENT_ID",
        "YOUTUBE_CLIENT_SECRET",
        "YOUTUBE_REDIRECT_URI",
        "TAURI_UPDATER_PUBKEY",
        "SENTRY_DSN",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    embed_comctl32_v6_manifest_in_test_binaries();

    // Run Tauri build
    tauri_build::build()
}

/// Give Cargo's test/bench binaries the same ComCtl32 v6 manifest the app binary
/// gets from `tauri_build`.
///
/// The dialog plugin's dependency chain statically imports
/// `comctl32!TaskDialogIndirect` (plus the window-subclassing helpers), and only
/// ComCtl32 v6 exports them — the v5.82 copy in System32 does not. A binary that
/// imports them without requesting the v6 side-by-side assembly fails to load at
/// all: `cargo test` dies with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139) before
/// running a single test, which reads like a mysterious test failure rather than a
/// link problem. The app binary is unaffected because tauri_build embeds its own
/// manifest.
fn embed_comctl32_v6_manifest_in_test_binaries() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }

    let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let manifest = std::path::Path::new(&manifest_dir).join("comctl32-v6.manifest");
    if !manifest.exists() {
        println!(
            "cargo:warning=comctl32-v6.manifest not found at {}; \
             `cargo test` may fail to launch with STATUS_ENTRYPOINT_NOT_FOUND",
            manifest.display()
        );
        return;
    }

    println!("cargo:rerun-if-changed=comctl32-v6.manifest");

    // Two different targets need two different treatments.
    //
    // Targets under `tests/` get the manifest directly — they carry no manifest of
    // their own, so embedding one is the straightforward fix.
    for target in ["tests", "benches"] {
        println!("cargo:rustc-link-arg-{}=/MANIFEST:EMBED", target);
        println!(
            "cargo:rustc-link-arg-{}=/MANIFESTINPUT:{}",
            target,
            manifest.display()
        );
    }

    // The lib's own `#[cfg(test)]` harness — where almost all of the suite lives — is
    // not a "test" target as far as Cargo is concerned, and there is no link-arg
    // channel that reaches it alone. Applying `/MANIFESTINPUT` to every target is not
    // an option either: the app binary already carries tauri's manifest resource and
    // a second one makes the linker fail with LNK1123.
    //
    // Delay-loading comctl32 sidesteps the manifest question entirely. The four
    // imported symbols are resolved on first call instead of at load, so a harness
    // that never opens a dialog never touches them, and the app — which does have the
    // v6 manifest active at runtime — resolves them normally when it needs them.
    println!("cargo:rustc-link-arg=/DELAYLOAD:comctl32.dll");
    println!("cargo:rustc-link-arg=delayimp.lib");
}
