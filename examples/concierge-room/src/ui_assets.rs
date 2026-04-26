//! `rust-embed` wrapper around the SPA bundle produced by `web/dist`.
//!
//! The build script does not auto-run Vite; producing the bundle is a
//! manual step (see `web/README.md`). When the bundle is missing,
//! `Asset::get("index.html")` returns the fallback `index.html` we ship
//! at the crate root so the demo still serves a functional UI.

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/web/dist"]
#[include = "*"]
pub struct Asset;
