//! Custom theme for OpenNow
//!
//! Dark theme matching the GFN aesthetic.

use iced_core::Color;

/// OpenNow dark color palette
pub mod colors {
    use super::Color;
    
    pub const BACKGROUND: Color = Color::from_rgb(0.078, 0.078, 0.118); // #141420
    pub const SURFACE: Color = Color::from_rgb(0.098, 0.098, 0.137); // #191923
    pub const SURFACE_HOVER: Color = Color::from_rgb(0.137, 0.137, 0.176); // #23232d
    pub const PRIMARY: Color = Color::from_rgb(0.467, 0.784, 0.196); // #76c832 (GFN green)
    pub const PRIMARY_HOVER: Color = Color::from_rgb(0.533, 0.847, 0.263); // #88d843
    pub const TEXT: Color = Color::from_rgb(0.933, 0.933, 0.933); // #eeeeee
    pub const TEXT_DIM: Color = Color::from_rgb(0.6, 0.6, 0.6); // #999999
    pub const ERROR: Color = Color::from_rgb(0.9, 0.3, 0.3); // red
    pub const BORDER: Color = Color::from_rgb(0.2, 0.2, 0.25); // subtle border
}
