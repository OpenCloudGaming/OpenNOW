//! OpenNow Streamer Library
//!
//! Core components for the native GeForce NOW streaming client.

#![recursion_limit = "256"]

pub mod api;
pub mod app;
pub mod auth;
pub mod gui_iced;
pub mod input;
pub mod media;
pub mod utils;
pub mod webrtc;

// Use iced-based GUI
pub use gui_iced as gui;

pub use app::{App, AppState};
