//! GUI Module
//!
//! Window management, rendering, and stats overlay.

mod renderer;
mod stats_panel;
mod shaders;
pub mod screens;
pub mod image_cache;

pub use renderer::Renderer;
pub use stats_panel::StatsPanel;
pub use image_cache::{get_image, request_image, update_cache};
