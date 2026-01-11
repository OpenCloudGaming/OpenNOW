//! Stats Panel Overlay
//!
//! Bottom-left stats display matching the web client style.
//! Includes throttling to reduce CPU usage - stats update every 200ms instead of every frame.

use egui::{Align2, Color32, FontId, RichText};
use crate::media::StreamStats;
use crate::app::StatsPosition;
use std::time::{Duration, Instant};

/// Interval between stats updates (200ms = 5 updates per second)
/// This dramatically reduces CPU usage while still providing responsive feedback
const STATS_UPDATE_INTERVAL: Duration = Duration::from_millis(200);

/// Stats panel overlay with throttled updates
pub struct StatsPanel {
    pub visible: bool,
    pub position: StatsPosition,
    /// Cached stats for throttled rendering
    cached_stats: Option<StreamStats>,
    /// Last time stats were updated
    last_update: Instant,
}

impl StatsPanel {
    pub fn new() -> Self {
        Self {
            visible: true,
            position: StatsPosition::BottomLeft,
            cached_stats: None,
            last_update: Instant::now(),
        }
    }

    /// Check if stats need to be updated (throttled to STATS_UPDATE_INTERVAL)
    fn should_update(&self) -> bool {
        self.last_update.elapsed() >= STATS_UPDATE_INTERVAL
    }

    /// Update cached stats if the throttle interval has passed
    /// Returns true if stats were updated (UI needs repaint)
    pub fn update_stats(&mut self, stats: &StreamStats) -> bool {
        if self.should_update() {
            self.cached_stats = Some(stats.clone());
            self.last_update = Instant::now();
            true
        } else {
            false
        }
    }

    /// Render the stats panel using cached stats
    /// This avoids recalculating the display every frame
    pub fn render(&self, ctx: &egui::Context, stats: &StreamStats) {
        if !self.visible {
            return;
        }

        // Use cached stats if available, otherwise use provided stats
        let display_stats = self.cached_stats.as_ref().unwrap_or(stats);

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
                        let res_text = if display_stats.resolution.is_empty() {
                            "Connecting...".to_string()
                        } else {
                            format!("{} @ {} fps", display_stats.resolution, display_stats.fps as u32)
                        };

                        ui.label(
                            RichText::new(res_text)
                                .font(FontId::monospace(13.0))
                                .color(Color32::WHITE)
                        );

                        // Codec, HDR status, and bitrate
                        if !display_stats.codec.is_empty() {
                            let hdr_indicator = if display_stats.is_hdr {
                                " • HDR"
                            } else {
                                ""
                            };
                            let hdr_color = if display_stats.is_hdr {
                                Color32::from_rgb(255, 180, 0) // Orange/gold for HDR
                            } else {
                                Color32::LIGHT_GRAY
                            };

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} • {:.1} Mbps",
                                        display_stats.codec,
                                        display_stats.bitrate_mbps
                                    ))
                                    .font(FontId::monospace(11.0))
                                    .color(Color32::LIGHT_GRAY)
                                );
                                if display_stats.is_hdr {
                                    ui.label(
                                        RichText::new("HDR")
                                            .font(FontId::monospace(11.0))
                                            .color(hdr_color)
                                    );
                                }
                            });
                        }

                        // Network RTT (round-trip time)
                        if display_stats.rtt_ms > 0.0 {
                            let rtt_color = if display_stats.rtt_ms < 30.0 {
                                Color32::GREEN
                            } else if display_stats.rtt_ms < 60.0 {
                                Color32::YELLOW
                            } else {
                                Color32::RED
                            };

                            ui.label(
                                RichText::new(format!("RTT: {:.0}ms", display_stats.rtt_ms))
                                .font(FontId::monospace(11.0))
                                .color(rtt_color)
                            );
                        } else {
                            ui.label(
                                RichText::new("RTT: N/A")
                                .font(FontId::monospace(11.0))
                                .color(Color32::GRAY)
                            );
                        }

                        // Packet loss
                        if display_stats.packet_loss > 0.1 {
                            let loss_color = if display_stats.packet_loss < 1.0 {
                                Color32::YELLOW
                            } else {
                                Color32::RED
                            };

                            ui.label(
                                RichText::new(format!(
                                    "Packet Loss: {:.2}%",
                                    display_stats.packet_loss
                                ))
                                .font(FontId::monospace(11.0))
                                .color(loss_color)
                            );
                        }

                        // Decode, render, and input latency
                        if display_stats.decode_time_ms > 0.0 || display_stats.render_time_ms > 0.0 {
                            ui.label(
                                RichText::new(format!(
                                    "Decode: {:.1}ms • Render: {:.1}ms",
                                    display_stats.decode_time_ms,
                                    display_stats.render_time_ms
                                ))
                                .font(FontId::monospace(10.0))
                                .color(Color32::GRAY)
                            );
                        }

                        // Input latency (client-side only)
                        if display_stats.input_latency_ms > 0.0 {
                            let input_color = if display_stats.input_latency_ms < 5.0 {
                                Color32::GREEN
                            } else if display_stats.input_latency_ms < 10.0 {
                                Color32::YELLOW
                            } else {
                                Color32::RED
                            };

                            ui.label(
                                RichText::new(format!(
                                    "Input: {:.1}ms",
                                    display_stats.input_latency_ms
                                ))
                                .font(FontId::monospace(10.0))
                                .color(input_color)
                            );
                        }

                        // Frame stats
                        if display_stats.frames_received > 0 {
                            ui.label(
                                RichText::new(format!(
                                    "Frames: {} rx, {} dec, {} drop",
                                    display_stats.frames_received,
                                    display_stats.frames_decoded,
                                    display_stats.frames_dropped
                                ))
                                .font(FontId::monospace(10.0))
                                .color(Color32::DARK_GRAY)
                            );
                        }

                        // GPU and server info
                        if !display_stats.gpu_type.is_empty() || !display_stats.server_region.is_empty() {
                            let info = format!(
                                "{}{}{}",
                                display_stats.gpu_type,
                                if !display_stats.gpu_type.is_empty() && !display_stats.server_region.is_empty() { " • " } else { "" },
                                display_stats.server_region
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
