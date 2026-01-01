//! Dialog Components
//!
//! Modal dialogs for settings, session conflicts, and warnings.

use crate::app::{GameInfo, Settings, ServerInfo, ServerStatus, UiAction, SettingChange};
use crate::app::session::ActiveSessionInfo;

/// Render the settings modal
pub fn render_settings_modal(
    ctx: &egui::Context,
    settings: &Settings,
    servers: &[ServerInfo],
    selected_server_index: usize,
    auto_server_selection: bool,
    ping_testing: bool,
    actions: &mut Vec<UiAction>,
) {
    let modal_width = 500.0;
    let modal_height = 600.0;

    // Dark overlay
    egui::Area::new(egui::Id::new("settings_overlay"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            #[allow(deprecated)]
            let screen_rect = ctx.screen_rect();
            ui.allocate_response(screen_rect.size(), egui::Sense::click());
            ui.painter().rect_filled(
                screen_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180),
            );
        });

    // Modal window
    #[allow(deprecated)]
    let screen_rect = ctx.screen_rect();
    let modal_pos = egui::pos2(
        (screen_rect.width() - modal_width) / 2.0,
        (screen_rect.height() - modal_height) / 2.0,
    );

    egui::Area::new(egui::Id::new("settings_modal"))
        .fixed_pos(modal_pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(28, 28, 35))
                .corner_radius(12.0)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 75)))
                .inner_margin(egui::Margin::same(20))
                .show(ui, |ui| {
                    ui.set_min_size(egui::vec2(modal_width, modal_height));

                    // Header with close button
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Settings")
                                .size(20.0)
                                .strong()
                                .color(egui::Color32::WHITE)
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let close_btn = egui::Button::new(
                                egui::RichText::new("✕")
                                    .size(16.0)
                                    .color(egui::Color32::WHITE)
                            )
                            .fill(egui::Color32::TRANSPARENT)
                            .corner_radius(4.0);

                            if ui.add(close_btn).clicked() {
                                actions.push(UiAction::ToggleSettingsModal);
                            }
                        });
                    });

                    ui.add_space(15.0);
                    ui.separator();
                    ui.add_space(15.0);

                    egui::ScrollArea::vertical()
                        .max_height(modal_height - 100.0)
                        .show(ui, |ui| {
                            // === Stream Settings Section ===
                            ui.label(
                                egui::RichText::new("Stream Settings")
                                    .size(16.0)
                                    .strong()
                                    .color(egui::Color32::WHITE)
                            );
                            ui.add_space(15.0);

                            egui::Grid::new("settings_grid")
                                .num_columns(2)
                                .spacing([20.0, 12.0])
                                .min_col_width(100.0)
                                .show(ui, |ui| {
                                    // Resolution dropdown
                                    ui.label(
                                        egui::RichText::new("Resolution")
                                            .size(13.0)
                                            .color(egui::Color32::GRAY)
                                    );
                                    egui::ComboBox::from_id_salt("resolution_combo")
                                        .selected_text(&settings.resolution)
                                        .width(180.0)
                                        .show_ui(ui, |ui| {
                                            for res in crate::app::config::RESOLUTIONS {
                                                if ui.selectable_label(settings.resolution == res.0, format!("{} ({})", res.0, res.1)).clicked() {
                                                    actions.push(UiAction::UpdateSetting(SettingChange::Resolution(res.0.to_string())));
                                                }
                                            }
                                        });
                                    ui.end_row();

                                    // FPS dropdown
                                    ui.label(
                                        egui::RichText::new("FPS")
                                            .size(13.0)
                                            .color(egui::Color32::GRAY)
                                    );
                                    egui::ComboBox::from_id_salt("fps_combo")
                                        .selected_text(format!("{} FPS", settings.fps))
                                        .width(180.0)
                                        .show_ui(ui, |ui| {
                                            for fps in crate::app::config::FPS_OPTIONS {
                                                if ui.selectable_label(settings.fps == *fps, format!("{} FPS", fps)).clicked() {
                                                    actions.push(UiAction::UpdateSetting(SettingChange::Fps(*fps)));
                                                }
                                            }
                                        });
                                    ui.end_row();

                                    // Codec dropdown
                                    ui.label(
                                        egui::RichText::new("Video Codec")
                                            .size(13.0)
                                            .color(egui::Color32::GRAY)
                                    );
                                    egui::ComboBox::from_id_salt("codec_combo")
                                        .selected_text(settings.codec.display_name())
                                        .width(180.0)
                                        .show_ui(ui, |ui| {
                                            for codec in crate::app::config::VideoCodec::all() {
                                                if ui.selectable_label(settings.codec == *codec, codec.display_name()).clicked() {
                                                    actions.push(UiAction::UpdateSetting(SettingChange::Codec(*codec)));
                                                }
                                            }
                                        });
                                    ui.end_row();

                                    // Max Bitrate slider
                                    ui.label(
                                        egui::RichText::new("Max Bitrate")
                                            .size(13.0)
                                            .color(egui::Color32::GRAY)
                                    );
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{} Mbps", settings.max_bitrate_mbps))
                                                .size(13.0)
                                                .color(egui::Color32::WHITE)
                                        );
                                        ui.label(
                                            egui::RichText::new("(200 = unlimited)")
                                                .size(10.0)
                                                .color(egui::Color32::GRAY)
                                        );
                                    });
                                    ui.end_row();
                                });

                            ui.add_space(25.0);
                            ui.separator();
                            ui.add_space(15.0);

                            // === Server Region Section ===
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Server Region")
                                        .size(16.0)
                                        .strong()
                                        .color(egui::Color32::WHITE)
                                );

                                ui.add_space(20.0);

                                // Ping test button
                                let ping_btn_text = if ping_testing { "Testing..." } else { "Test Ping" };
                                let ping_btn = egui::Button::new(
                                    egui::RichText::new(ping_btn_text)
                                        .size(11.0)
                                        .color(egui::Color32::WHITE)
                                )
                                .fill(if ping_testing {
                                    egui::Color32::from_rgb(80, 80, 100)
                                } else {
                                    egui::Color32::from_rgb(60, 120, 60)
                                })
                                .corner_radius(4.0);

                                if ui.add_sized([80.0, 24.0], ping_btn).clicked() && !ping_testing {
                                    actions.push(UiAction::StartPingTest);
                                }

                                if ping_testing {
                                    ui.spinner();
                                }
                            });
                            ui.add_space(10.0);

                            // Server dropdown with Auto option and best server highlighted
                            let selected_text = if auto_server_selection {
                                // Find best server for display
                                let best = servers.iter()
                                    .filter(|s| s.status == ServerStatus::Online && s.ping_ms.is_some())
                                    .min_by_key(|s| s.ping_ms.unwrap_or(9999));
                                if let Some(best_server) = best {
                                    format!("Auto: {} ({}ms)", best_server.name, best_server.ping_ms.unwrap_or(0))
                                } else {
                                    "Auto (Best Ping)".to_string()
                                }
                            } else {
                                servers.get(selected_server_index)
                                    .map(|s| {
                                        if let Some(ping) = s.ping_ms {
                                            format!("{} ({}ms)", s.name, ping)
                                        } else {
                                            s.name.clone()
                                        }
                                    })
                                    .unwrap_or_else(|| "Select a server...".to_string())
                            };

                            egui::ComboBox::from_id_salt("server_combo")
                                .selected_text(selected_text)
                                .width(300.0)
                                .show_ui(ui, |ui| {
                                    // Auto option at the top
                                    let auto_label = {
                                        let best = servers.iter()
                                            .filter(|s| s.status == ServerStatus::Online && s.ping_ms.is_some())
                                            .min_by_key(|s| s.ping_ms.unwrap_or(9999));
                                        if let Some(best_server) = best {
                                            format!("✨ Auto: {} ({}ms)", best_server.name, best_server.ping_ms.unwrap_or(0))
                                        } else {
                                            "✨ Auto (Best Ping)".to_string()
                                        }
                                    };

                                    if ui.selectable_label(auto_server_selection, auto_label).clicked() {
                                        actions.push(UiAction::SetAutoServerSelection(true));
                                    }

                                    ui.separator();
                                    ui.add_space(5.0);

                                    // Group by region
                                    let regions = ["Europe", "North America", "Canada", "Asia-Pacific", "Other"];
                                    for region in regions {
                                        let region_servers: Vec<_> = servers
                                            .iter()
                                            .enumerate()
                                            .filter(|(_, s)| s.region == region)
                                            .collect();

                                        if region_servers.is_empty() {
                                            continue;
                                        }

                                        ui.label(
                                            egui::RichText::new(region)
                                                .size(11.0)
                                                .strong()
                                                .color(egui::Color32::from_rgb(118, 185, 0))
                                        );

                                        for (idx, server) in region_servers {
                                            let is_selected = !auto_server_selection && idx == selected_server_index;
                                            let ping_text = match server.status {
                                                ServerStatus::Online => {
                                                    server.ping_ms.map(|p| format!(" ({}ms)", p)).unwrap_or_default()
                                                }
                                                ServerStatus::Testing => " (testing...)".to_string(),
                                                ServerStatus::Offline => " (offline)".to_string(),
                                                ServerStatus::Unknown => "".to_string(),
                                            };

                                            let label = format!("  {}{}", server.name, ping_text);
                                            if ui.selectable_label(is_selected, label).clicked() {
                                                actions.push(UiAction::SelectServer(idx));
                                            }
                                        }

                                        ui.add_space(5.0);
                                    }
                                });

                            ui.add_space(20.0);
                        });
                });
        });
}

/// Render the session conflict dialog
pub fn render_session_conflict_dialog(
    ctx: &egui::Context,
    active_sessions: &[ActiveSessionInfo],
    pending_game: Option<&GameInfo>,
    actions: &mut Vec<UiAction>,
) {
    let modal_width = 500.0;
    let modal_height = 300.0;

    egui::Area::new(egui::Id::new("session_conflict_overlay"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            #[allow(deprecated)]
            let screen_rect = ctx.screen_rect();
            ui.allocate_response(screen_rect.size(), egui::Sense::click());
            ui.painter().rect_filled(
                screen_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200),
            );
        });

    #[allow(deprecated)]
    let screen_rect = ctx.screen_rect();
    let modal_pos = egui::pos2(
        (screen_rect.width() - modal_width) / 2.0,
        (screen_rect.height() - modal_height) / 2.0,
    );

    egui::Area::new(egui::Id::new("session_conflict_modal"))
        .fixed_pos(modal_pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(28, 28, 35))
                .corner_radius(12.0)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 75)))
                .inner_margin(egui::Margin::same(20))
                .show(ui, |ui| {
                    ui.set_min_size(egui::vec2(modal_width, modal_height));

                    ui.label(
                        egui::RichText::new("⚠ Active Session Detected")
                            .size(20.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 200, 80))
                    );

                    ui.add_space(15.0);

                    if let Some(session) = active_sessions.first() {
                        ui.label(
                            egui::RichText::new("You have an active GFN session running:")
                                .size(14.0)
                                .color(egui::Color32::LIGHT_GRAY)
                        );

                        ui.add_space(10.0);

                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(40, 40, 50))
                            .corner_radius(8.0)
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("App ID:")
                                            .size(13.0)
                                            .color(egui::Color32::GRAY)
                                    );
                                    ui.label(
                                        egui::RichText::new(format!("{}", session.app_id))
                                            .size(13.0)
                                            .color(egui::Color32::WHITE)
                                    );
                                });

                                if let Some(ref gpu) = session.gpu_type {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("GPU:")
                                                .size(13.0)
                                                .color(egui::Color32::GRAY)
                                        );
                                        ui.label(
                                            egui::RichText::new(gpu)
                                                .size(13.0)
                                                .color(egui::Color32::WHITE)
                                        );
                                    });
                                }

                                if let Some(ref res) = session.resolution {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Resolution:")
                                                .size(13.0)
                                                .color(egui::Color32::GRAY)
                                        );
                                        ui.label(
                                            egui::RichText::new(format!("{} @ {}fps", res, session.fps.unwrap_or(60)))
                                                .size(13.0)
                                                .color(egui::Color32::WHITE)
                                        );
                                    });
                                }

                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Status:")
                                            .size(13.0)
                                            .color(egui::Color32::GRAY)
                                    );
                                    let status_text = match session.status {
                                        2 => "Ready",
                                        3 => "Running",
                                        _ => "Unknown",
                                    };
                                    ui.label(
                                        egui::RichText::new(status_text)
                                            .size(13.0)
                                            .color(egui::Color32::from_rgb(118, 185, 0))
                                    );
                                });
                            });

                        ui.add_space(15.0);

                        if pending_game.is_some() {
                            ui.label(
                                egui::RichText::new("GFN only allows one session at a time. You can either:")
                                    .size(13.0)
                                    .color(egui::Color32::LIGHT_GRAY)
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("What would you like to do?")
                                    .size(13.0)
                                    .color(egui::Color32::LIGHT_GRAY)
                            );
                        }

                        ui.add_space(15.0);

                        ui.vertical_centered(|ui| {
                            let resume_btn = egui::Button::new(
                                egui::RichText::new("Resume Existing Session")
                                    .size(14.0)
                                    .color(egui::Color32::WHITE)
                            )
                            .fill(egui::Color32::from_rgb(118, 185, 0))
                            .min_size(egui::vec2(200.0, 35.0));

                            if ui.add(resume_btn).clicked() {
                                actions.push(UiAction::ResumeSession(session.clone()));
                            }

                            ui.add_space(8.0);

                            if let Some(game) = pending_game {
                                let terminate_btn = egui::Button::new(
                                    egui::RichText::new(format!("End Session & Launch \"{}\"", game.title))
                                        .size(14.0)
                                        .color(egui::Color32::WHITE)
                                )
                                .fill(egui::Color32::from_rgb(220, 60, 60))
                                .min_size(egui::vec2(200.0, 35.0));

                                if ui.add(terminate_btn).clicked() {
                                    actions.push(UiAction::TerminateAndLaunch(session.session_id.clone(), game.clone()));
                                }

                                ui.add_space(8.0);
                            }

                            let cancel_btn = egui::Button::new(
                                egui::RichText::new("Cancel")
                                    .size(14.0)
                                    .color(egui::Color32::LIGHT_GRAY)
                            )
                            .fill(egui::Color32::from_rgb(60, 60, 75))
                            .min_size(egui::vec2(200.0, 35.0));

                            if ui.add(cancel_btn).clicked() {
                                actions.push(UiAction::CloseSessionConflict);
                            }
                        });
                    }
                });
        });
}

/// Render the AV1 hardware warning dialog
pub fn render_av1_warning_dialog(
    ctx: &egui::Context,
    actions: &mut Vec<UiAction>,
) {
    let modal_width = 450.0;
    let modal_height = 280.0;

    // Dark overlay
    egui::Area::new(egui::Id::new("av1_warning_overlay"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            #[allow(deprecated)]
            let screen_rect = ctx.screen_rect();
            let overlay_rect = egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                screen_rect.size()
            );

            ui.painter().rect_filled(
                overlay_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200)
            );

            // Modal window
            let modal_pos = egui::pos2(
                (screen_rect.width() - modal_width) / 2.0,
                (screen_rect.height() - modal_height) / 2.0
            );

            egui::Area::new(egui::Id::new("av1_warning_modal"))
                .fixed_pos(modal_pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(35, 35, 45))
                        .corner_radius(12.0)
                        .inner_margin(egui::Margin::same(25))
                        .show(ui, |ui| {
                            ui.set_width(modal_width - 50.0);

                            // Warning icon and title
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("⚠")
                                        .size(28.0)
                                        .color(egui::Color32::from_rgb(255, 180, 0))
                                );
                                ui.add_space(10.0);
                                ui.label(
                                    egui::RichText::new("AV1 Hardware Not Detected")
                                        .size(20.0)
                                        .strong()
                                        .color(egui::Color32::WHITE)
                                );
                            });

                            ui.add_space(20.0);

                            // Warning message
                            ui.label(
                                egui::RichText::new(
                                    "Your system does not appear to have hardware AV1 decoding support. \
                                    AV1 requires:\n\n\
                                    • NVIDIA RTX 30 series or newer\n\
                                    • Intel 11th Gen (Tiger Lake) or newer\n\
                                    • AMD RX 6000 series or newer (Linux)\n\
                                    • Apple M3 or newer (macOS)"
                                )
                                .size(14.0)
                                .color(egui::Color32::LIGHT_GRAY)
                            );

                            ui.add_space(15.0);

                            ui.label(
                                egui::RichText::new(
                                    "Software decoding will be used, which may cause high CPU usage and poor performance."
                                )
                                .size(13.0)
                                .color(egui::Color32::from_rgb(255, 150, 100))
                            );

                            ui.add_space(25.0);

                            // Buttons
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let ok_btn = egui::Button::new(
                                    egui::RichText::new("I Understand")
                                        .size(14.0)
                                        .color(egui::Color32::WHITE)
                                )
                                .fill(egui::Color32::from_rgb(118, 185, 0))
                                .min_size(egui::vec2(130.0, 35.0));

                                if ui.add(ok_btn).clicked() {
                                    actions.push(UiAction::CloseAV1Warning);
                                }
                            });
                        });
                });
        });
}
