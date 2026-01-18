//! GUI Module
//!
//! Window management, rendering, and stats overlay.

pub mod image_cache;
mod renderer;
pub mod screens;
mod shaders;
mod stats_panel;

pub use image_cache::{get_image, request_image, update_cache};
pub use renderer::Renderer;
pub use stats_panel::StatsPanel;
