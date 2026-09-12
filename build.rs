//! Build script for Shinobi.
//!
//! Building the Angular UI is **opt-in** so that `cargo build`, `cargo test`
//! and `cargo clippy` never touch npm or the network:
//!
//! ```bash
//! SHINOBI_BUILD_FRONTEND=1 cargo build --release   # builds frontend/ into static/
//! cargo build --release                            # API only, uses existing static/ if present
//! ```
//!
//! When the UI is not built the server still runs and serves the JSON API;
//! requests for `/` simply return 404 (documented in the README).

use std::path::Path;
use std::process::Command;

fn env_requested() -> bool {
    matches!(
        std::env::var("SHINOBI_BUILD_FRONTEND").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

fn main() {
    // Re-run the build script only when this switch changes (plus the usual
    // rebuild on build.rs itself).
    println!("cargo:rerun-if-env-changed=SHINOBI_BUILD_FRONTEND");

    if !env_requested() {
        println!(
            "cargo:warning=frontend build skipped (set SHINOBI_BUILD_FRONTEND=1 to build the Angular UI)"
        );
        return;
    }

    let frontend_dir = Path::new("frontend");
    if !frontend_dir.join("package.json").exists() {
        println!("cargo:warning=frontend/package.json not found; skipping UI build");
        return;
    }

    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/angular.json");

    if !frontend_dir.join("node_modules").exists() {
        println!("cargo:warning=installing frontend dependencies (npm install)");
        let ok = Command::new("npm")
            .args(["install", "--legacy-peer-deps"])
            .current_dir(frontend_dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            println!("cargo:warning=npm install failed; static/ UI not rebuilt");
            return;
        }
    }

    println!("cargo:warning=building Angular frontend (ng build)");
    let ok = Command::new("npx")
        .args(["ng", "build"])
        .current_dir(frontend_dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if ok {
        println!("cargo:warning=Angular build completed successfully");
    } else {
        println!("cargo:warning=ng build failed; keeping previous static/ UI if any");
    }
}
