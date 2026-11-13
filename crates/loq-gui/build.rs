//! Embed the Tauri frontend at build time.
//!
//! `tauri_build` only tracks `tauri.conf.json`/capabilities for rebuilds, so
//! we additionally watch the built frontend directory. Otherwise a local
//! `cargo build --release -p loq-gui` would embed a stale `dist/` and the
//! shipped binary would show an older UI after `bun run build` updates it.
//!
//! `frontendDist` in tauri.conf.json is `../../ui/dist` relative to the crate
//! directory, so we watch that absolute path.
fn main() {
    let dist = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ui/dist");
    println!("cargo:rerun-if-changed={}", dist);

    tauri_build::build();
}
