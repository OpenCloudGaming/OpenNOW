//! Iced-based GUI Module
//!
//! GPU-accelerated retained-mode UI using iced.
//! Replaces egui for much lower CPU usage.

mod renderer;
mod controls;
mod screens;
mod shaders;
mod theme;

pub mod image_cache;

pub use renderer::{Renderer, EventResponse};
pub use controls::{Controls, Message};
