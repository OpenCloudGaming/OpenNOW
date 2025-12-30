//! Stats Panel Overlay
//!
//! Bottom-left stats display matching the web client style.

use egui::{Align2, Color32, FontId, RichText};
use crate::media::StreamStats;
use crate::app::StatsPosition;

/// Stats panel overlay
pub struct StatsPanel {
    pub visible: bool,
    pub position: StatsPosition,
}

impl StatsPanel {
    pub fn new() -> Self {
        Self {
            visible: true,
            position: StatsPosition::BottomLeft,
        }
    }

    /// Render the stats panel
    pub fn render(&self, ctx: &egui::Context, stats: &StreamStats) {
        if !self.visible {
            return;
        }

        let (anchor, offset) = match self.position {
            StatsPosition::BottomLeft => (Align2::LEFT_BOTTOM, [10.0, -10.0]),
            StatsPosition::BottomRight => (Align2::RIGHT_BOTTOM, [-10.0, -10.0]),
            StatsPosition::TopLeft => (Align2::LEFT_TOP, [10.0, 10.0]),
            StatsPosition::TopRight => (Align2::RIGHT_TOP, [-10.0, 10.0]),
        };

        egui::Area::new(egui::Id::new("stats_panel"))
            .anchor(anchor, offset)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                    .corner_radius(4.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.set_min_width(200.0);

                        // Resolution and FPS
                        let res_text = if stats.resolution.is_empty() {
                            "Connecting...".to_string()
                        } else {
                            format!("{} @ {} fps", stats.resolution, stats.fps as u32)
                        };

                        ui.label(
                            RichText::new(res_text)
                                .font(FontId::monospace(13.0))
                                .color(Color32::WHITE)
                        );

                        // Codec and bitrate
                        if !stats.codec.is_empty() {
                            ui.label(
                                RichText::new(format!(
                                    "{} • {:.1} Mbps",
                                    stats.codec,
                                    stats.bitrate_mbps
                                ))
                                .font(FontId::monospace(11.0))
                                .color(Color32::LIGHT_GRAY)
                            );
                        }

                        // Latency and packet loss
                        let latency_color = if stats.latency_ms < 30.0 {
                            Color32::GREEN
                        } else if stats.latency_ms < 60.0 {
                            Color32::YELLOW
                        } else {
                            Color32::RED
                        };

                        ui.label(
                            RichText::new(format!(
                                "Latency: {:.0} ms",
                                stats.latency_ms
                            ))
                            .font(FontId::monospace(11.0))
                            .color(latency_color)
                        );

                        if stats.packet_loss > 0.0 {
                            let loss_color = if stats.packet_loss < 1.0 {
                                Color32::YELLOW
                            } else {
                                Color32::RED
                            };

                            ui.label(
                                RichText::new(format!(
                                    "Packet Loss: {:.1}%",
                                    stats.packet_loss
                                ))
                                .font(FontId::monospace(11.0))
                                .color(loss_color)
                            );
                        }

                        // Decode and render times
                        if stats.decode_time_ms > 0.0 || stats.render_time_ms > 0.0 {
                            ui.label(
                                RichText::new(format!(
                                    "Decode: {:.1} ms • Render: {:.1} ms",
                                    stats.decode_time_ms,
                                    stats.render_time_ms
                                ))
                                .font(FontId::monospace(10.0))
                                .color(Color32::GRAY)
                            );
                        }

                        // Frame stats
                        if stats.frames_received > 0 {
                            ui.label(
                                RichText::new(format!(
                                    "Frames: {} rx, {} dec, {} drop",
                                    stats.frames_received,
                                    stats.frames_decoded,
                                    stats.frames_dropped
                                ))
                                .font(FontId::monospace(10.0))
                                .color(Color32::DARK_GRAY)
                            );
                        }

                        // GPU and server info
                        if !stats.gpu_type.is_empty() || !stats.server_region.is_empty() {
                            let info = format!(
                                "{}{}{}",
                                stats.gpu_type,
                                if !stats.gpu_type.is_empty() && !stats.server_region.is_empty() { " • " } else { "" },
                                stats.server_region
                            );

                            ui.label(
                                RichText::new(info)
                                    .font(FontId::monospace(10.0))
                                    .color(Color32::DARK_GRAY)
                            );
                        }
                    });
            });
    }

    /// Toggle visibility
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Set position
    pub fn set_position(&mut self, position: StatsPosition) {
        self.position = position;
    }
}

impl Default for StatsPanel {
    fn default() -> Self {
        Self::new()
    }
}
