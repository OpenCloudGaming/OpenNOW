//! Screen Components
//!
//! UI screens and dialogs for the application.

mod login;
mod session;

pub use login::render_login_screen;
pub use session::render_session_screen;

use crate::app::config::{ColorQuality, FPS_OPTIONS, RESOLUTIONS};
use crate::app::session::ActiveSessionInfo;
use crate::app::{GameInfo, ServerInfo, SettingChange, Settings, UiAction};

/// Render the settings modal with bitrate slider and other options
/// Render the settings modal with bitrate slider and other options
pub fn render_settings_modal(
    ctx: &egui::Context,
    settings: &Settings,
    servers: &[ServerInfo],
    selected_server_index: usize,
    auto_server_selection: bool,
    ping_testing: bool,
    subscription: Option<&crate::app::SubscriptionInfo>,
    actions: &mut Vec<UiAction>,
) {
    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(false)
        .fixed_size([500.0, 450.0]) // Increased size for cleaner layout
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(8.0);

                // === Video Settings Section ===
                ui.heading(egui::RichText::new("Video").color(egui::Color32::from_rgb(118, 185, 0)));
                ui.add_space(8.0);

                egui::Grid::new("video_settings_grid")
                    .num_columns(2)
                    .spacing([24.0, 16.0])
                    .show(ui, |ui| {
                        // Max Bitrate
                        ui.label("Max Bitrate")
                            .on_hover_text("Controls the maximum bandwidth usage for video streaming.\nHigher values improve quality but require a stable, fast internet connection.");
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let mut bitrate = settings.max_bitrate_mbps as f32;
                                let slider = egui::Slider::new(&mut bitrate, 10.0..=200.0)
                                    .show_value(false)
                                    .step_by(5.0);
                                if ui.add(slider).changed() {
                                    actions.push(UiAction::UpdateSetting(SettingChange::MaxBitrate(bitrate as u32)));
                                }
                                ui.label(egui::RichText::new(format!("{} Mbps", settings.max_bitrate_mbps)).strong());
                            });
                            ui.label(egui::RichText::new("Recommend: 50-75 Mbps for most users").size(10.0).weak());
                        });
                        ui.end_row();

                        // Resolution
                        ui.label("Resolution")
                            .on_hover_text("The resolution of the video stream.");
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let current_display = RESOLUTIONS.iter()
                                .find(|(res, _)| *res == settings.resolution)
                                .map(|(_, name)| *name)
                                .unwrap_or(&settings.resolution);

                            egui::ComboBox::from_id_salt("resolution_combo")
                                .selected_text(current_display)
                                .show_ui(ui, |ui| {
                                    // Use entitled resolutions if available
                                    if let Some(sub) = subscription {
                                        if !sub.entitled_resolutions.is_empty() {
                                            // 1. Deduplicate unique resolutions
                                            let mut unique_resolutions = std::collections::HashSet::new();
                                            let mut resolutions = Vec::new();

                                            // Sort by width then height descending first
                                            let mut sorted_res = sub.entitled_resolutions.clone();
                                            sorted_res.sort_by(|a, b| b.width.cmp(&a.width).then(b.height.cmp(&a.height)));

                                            for res in sorted_res {
                                                let key = (res.width, res.height);
                                                if unique_resolutions.contains(&key) {
                                                    continue;
                                                }
                                                unique_resolutions.insert(key);
                                                resolutions.push(res);
                                            }

                                            // 2. Group by Aspect Ratio
                                            let mut groups: std::collections::BTreeMap<String, Vec<crate::app::types::EntitledResolution>> = std::collections::BTreeMap::new();

                                            for res in resolutions {
                                                let ratio = res.width as f32 / res.height as f32;
                                                let category = if (ratio - 16.0/9.0).abs() < 0.05 {
                                                    "16:9 Standard"
                                                } else if (ratio - 16.0/10.0).abs() < 0.05 {
                                                    "16:10 Widescreen"
                                                } else if (ratio - 21.0/9.0).abs() < 0.05 {
                                                    "21:9 Ultrawide"
                                                } else if (ratio - 32.0/9.0).abs() < 0.05 {
                                                    "32:9 Super Ultrawide"
                                                } else if (ratio - 4.0/3.0).abs() < 0.05 {
                                                    "4:3 Legacy"
                                                } else {
                                                    "Other"
                                                };

                                                groups.entry(category.to_string()).or_default().push(res);
                                            }

                                            // Define preferred order of categories
                                            let order = ["16:9 Standard", "16:10 Widescreen", "21:9 Ultrawide", "32:9 Super Ultrawide", "4:3 Legacy", "Other"];

                                            for category in order.iter() {
                                                if let Some(res_list) = groups.get(*category) {
                                                    ui.heading(*category);
                                                    for res in res_list {
                                                        let res_str = format!("{}x{}", res.width, res.height);

                                                        // Friendly name logic
                                                        let name = match (res.width, res.height) {
                                                            (1280, 720) => "720p (HD)".to_string(),
                                                            (1920, 1080) => "1080p (FHD)".to_string(),
                                                            (2560, 1440) => "1440p (QHD)".to_string(),
                                                            (3840, 2160) => "4K (UHD)".to_string(),
                                                            (2560, 1080) => "2560x1080 (Ultrawide)".to_string(),
                                                            (3440, 1440) => "3440x1440 (Ultrawide)".to_string(),
                                                            (w, h) => format!("{}x{}", w, h),
                                                        };

                                                        if ui.selectable_label(settings.resolution == res_str, name).clicked() {
                                                            actions.push(UiAction::UpdateSetting(SettingChange::Resolution(res_str)));
                                                        }
                                                    }
                                                    ui.separator();
                                                }
                                            }
                                            return;
                                        }
                                    }

                                    // Fallback to static list
                                    for (res, name) in RESOLUTIONS {
                                        if ui.selectable_label(settings.resolution == *res, *name).clicked() {
                                            actions.push(UiAction::UpdateSetting(SettingChange::Resolution(res.to_string())));
                                        }
                                    }
                                });
                        });
                        ui.end_row();

                        // Frame Rate
                        ui.label("Frame Rate")
                             .on_hover_text("Target frame rate for the stream.\nHigh FPS requires more bandwidth and decoder power.");
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            egui::ComboBox::from_id_salt("fps_combo")
                                .selected_text(format!("{} FPS", settings.fps))
                                .show_ui(ui, |ui| {
                                    // Use entitled FPS for the current resolution if available
                                    if let Some(sub) = subscription {
                                        if !sub.entitled_resolutions.is_empty() {
                                            let (w, h) = crate::app::types::parse_resolution(&settings.resolution);

                                            // Find max FPS for this resolution
                                            let mut available_fps = Vec::new();
                                            for res in &sub.entitled_resolutions {
                                                if res.width == w && res.height == h {
                                                    available_fps.push(res.fps);
                                                }
                                            }

                                            // Also include global max FPS just in case resolution match fails
                                            // or if we want to allow users to force lower FPS
                                            if available_fps.is_empty() {
                                                // Fallback to all entitled FPS
                                                for res in &sub.entitled_resolutions {
                                                     available_fps.push(res.fps);
                                                }
                                            }

                                            available_fps.sort();
                                            available_fps.dedup();

                                            if !available_fps.is_empty() {
                                                for fps in available_fps {
                                                    if ui.selectable_label(settings.fps == fps, format!("{} FPS", fps)).clicked() {
                                                        actions.push(UiAction::UpdateSetting(SettingChange::Fps(fps)));
                                                    }
                                                }
                                                return;
                                            }
                                        }
                                    }

                                    // Fallback to static list
                                    for &fps in FPS_OPTIONS {
                                        if ui.selectable_label(settings.fps == fps, format!("{} FPS", fps)).clicked() {
                                            actions.push(UiAction::UpdateSetting(SettingChange::Fps(fps)));
                                        }
                                    }
                                });
                        });
                        ui.end_row();

                        // Video Codec
                        ui.label("Video Codec")
                             .on_hover_text("Compression standard used for video.\nAV1 and H.265 (HEVC) offer better quality than H.264 at the same bitrate, but require compatible hardware.");
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let codec_text = match settings.codec {
                                crate::app::VideoCodec::H264 => "H.264",
                                crate::app::VideoCodec::H265 => "H.265 (HEVC)",
                                crate::app::VideoCodec::AV1 => "AV1",
                            };
                            egui::ComboBox::from_id_salt("codec_combo")
                                .selected_text(codec_text)
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(matches!(settings.codec, crate::app::VideoCodec::H264), "H.264").clicked() {
                                        actions.push(UiAction::UpdateSetting(SettingChange::Codec(crate::app::VideoCodec::H264)));
                                    }
                                    if ui.selectable_label(matches!(settings.codec, crate::app::VideoCodec::H265), "H.265 (HEVC)").clicked() {
                                        actions.push(UiAction::UpdateSetting(SettingChange::Codec(crate::app::VideoCodec::H265)));
                                    }
                                    if crate::media::is_av1_hardware_supported() {
                                        if ui.selectable_label(matches!(settings.codec, crate::app::VideoCodec::AV1), "AV1").clicked() {
                                            actions.push(UiAction::UpdateSetting(SettingChange::Codec(crate::app::VideoCodec::AV1)));
                                        }
                                    } else {
                                        ui.label(egui::RichText::new("AV1 unavailable (HW unsupported)").weak());
                                    }
                                });
                        });
                        ui.end_row();

                        // Video Decoder - Hidden for now, always use Native DXVA
                        // TODO: Re-enable when other backends are ready
                        // ui.label("Video Decoder")
                        //      .on_hover_text("The hardware/software backend used to decode the video stream.\nUsually 'D3D11' or 'Vulkan' on Windows.");
                        // ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        //     egui::ComboBox::from_id_salt("decoder_combo")
                        //         .selected_text(settings.decoder_backend.as_str())
                        //         .show_ui(ui, |ui| {
                        //             for backend in crate::media::get_supported_decoder_backends() {
                        //                 if ui.selectable_label(settings.decoder_backend == backend, backend.as_str()).clicked() {
                        //                     actions.push(UiAction::UpdateSetting(SettingChange::DecoderBackend(backend)));
                        //                 }
                        //             }
                        //         });
                        // });
                        // ui.end_row();

                        // Color Quality
                        ui.label("Color Quality")
                             .on_hover_text("Color bit depth and chroma subsampling.\n\n• 4:2:0 - Standard chroma, lower bandwidth\n• 4:4:4 - Full chroma, better for text/UI (requires HEVC)\n• 8-bit - Standard dynamic range\n• 10-bit - HDR capable, smoother gradients");
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            egui::ComboBox::from_id_salt("color_quality_combo")
                                .selected_text(settings.color_quality.display_name())
                                .show_ui(ui, |ui| {
                                    for &quality in ColorQuality::all() {
                                        let label = format!("{}", quality.display_name());
                                        let tooltip = quality.description();
                                        if ui.selectable_label(settings.color_quality == quality, &label)
                                            .on_hover_text(tooltip)
                                            .clicked()
                                        {
                                            actions.push(UiAction::UpdateSetting(SettingChange::ColorQuality(quality)));
                                        }
                                    }
                                });
                        });
                        ui.end_row();

                        // HDR Mode
                        ui.label("HDR Mode")
                             .on_hover_text("Enable High Dynamic Range for supported displays.\nRequires 10-bit color and HEVC/AV1 codec.\nWill auto-switch settings when enabled.");
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let mut hdr_enabled = settings.hdr_enabled;
                            if ui.add(egui::Checkbox::new(&mut hdr_enabled, "Enable HDR")).changed() {
                                actions.push(UiAction::UpdateSetting(SettingChange::Hdr(hdr_enabled)));
                            }
                            if settings.hdr_enabled {
                                ui.label(egui::RichText::new("(10-bit + HEVC required)").size(10.0).weak());
                            }
                        });
                        ui.end_row();
                    });

                ui.add_space(20.0);
                ui.separator();
                ui.add_space(8.0);

                // === Server Settings Section ===
                ui.heading(egui::RichText::new("Server & Network").color(egui::Color32::from_rgb(118, 185, 0)));
                ui.add_space(8.0);

                egui::Grid::new("server_settings_grid")
                    .num_columns(2)
                    .spacing([24.0, 16.0])
                    .show(ui, |ui| {
                        // Auto Selection
                        ui.label("Server Selection")
                             .on_hover_text("Choose a specific GeForce NOW server or let the client automatically pick the best one.");

                        ui.vertical(|ui| {
                            let mut auto_select = auto_server_selection;
                            if ui.checkbox(&mut auto_select, "Auto-select best server").on_hover_text("Automatically selects the server with the lowest ping.").changed() {
                                actions.push(UiAction::SetAutoServerSelection(auto_select));
                            }

                            if !auto_server_selection && !servers.is_empty() {
                                ui.add_space(4.0);
                                let current_server = servers.get(selected_server_index)
                                    .map(|s| format!("{} ({}ms)", s.name, s.ping_ms.unwrap_or(0)))
                                    .unwrap_or_else(|| "Select server".to_string());

                                egui::ComboBox::from_id_salt("server_combo")
                                    .selected_text(current_server)
                                    .width(250.0)
                                    .show_ui(ui, |ui| {
                                        for (i, server) in servers.iter().enumerate() {
                                            let ping_str = server.ping_ms
                                                .map(|p| format!(" ({}ms)", p))
                                                .unwrap_or_default();
                                            let label = format!("{}{}", server.name, ping_str);
                                            if ui.selectable_label(i == selected_server_index, label).clicked() {
                                                actions.push(UiAction::SelectServer(i));
                                            }
                                        }
                                    });
                            }
                        });
                        ui.end_row();

                        // Network Test
                        if !auto_server_selection && !servers.is_empty() {
                            ui.label("Network Test")
                                 .on_hover_text("Measure latency to available servers.");
                            ui.horizontal(|ui| {
                                if ping_testing {
                                    ui.spinner();
                                    ui.label("Testing ping...");
                                } else if ui.button("Test Ping").clicked() {
                                    actions.push(UiAction::StartPingTest);
                                }
                            });
                            ui.end_row();
                        }
                    });

                ui.add_space(24.0);

                // Buttons row
                ui.horizontal(|ui| {
                    // Reset button on the left
                    if ui.button(egui::RichText::new("Reset to Defaults").size(14.0).color(egui::Color32::from_rgb(200, 80, 80))).clicked() {
                        actions.push(UiAction::ResetSettings);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui::RichText::new("Close").size(16.0)).clicked() {
                            actions.push(UiAction::ToggleSettingsModal);
                        }
                    });
                });

                ui.add_space(8.0);
            });
        });
}

/// Render session conflict dialog when user has active sessions
pub fn render_session_conflict_dialog(
    ctx: &egui::Context,
    active_sessions: &[ActiveSessionInfo],
    pending_game: Option<&GameInfo>,
    actions: &mut Vec<UiAction>,
) {
    egui::Window::new("Active Session")
        .collapsible(false)
        .resizable(false)
        .fixed_size([400.0, 250.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);

                ui.label(
                    egui::RichText::new("You have an active session")
                        .size(18.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );

                ui.add_space(15.0);

                // Show active session info
                if let Some(session) = active_sessions.first() {
                    ui.label(
                        egui::RichText::new(format!("Session ID: {}", &session.session_id))
                            .size(14.0)
                            .color(egui::Color32::from_rgb(118, 185, 0)),
                    );

                    ui.add_space(5.0);

                    if let Some(ref server_ip) = session.server_ip {
                        ui.label(
                            egui::RichText::new(format!("Server: {}", server_ip))
                                .size(12.0)
                                .color(egui::Color32::GRAY),
                        );
                    }
                }

                ui.add_space(25.0);

                ui.horizontal(|ui| {
                    // Resume existing session
                    let resume_btn =
                        egui::Button::new(egui::RichText::new("Resume Session").size(14.0))
                            .fill(egui::Color32::from_rgb(70, 130, 70))
                            .min_size(egui::vec2(130.0, 35.0));

                    if ui.add(resume_btn).clicked() {
                        if let Some(session) = active_sessions.first() {
                            actions.push(UiAction::ResumeSession(session.clone()));
                        }
                        actions.push(UiAction::CloseSessionConflict);
                    }

                    ui.add_space(10.0);

                    // Terminate and start new
                    if let Some(game) = pending_game {
                        let new_btn =
                            egui::Button::new(egui::RichText::new("Start New Game").size(14.0))
                                .fill(egui::Color32::from_rgb(130, 70, 70))
                                .min_size(egui::vec2(130.0, 35.0));

                        if ui.add(new_btn).clicked() {
                            if let Some(session) = active_sessions.first() {
                                actions.push(UiAction::TerminateAndLaunch(
                                    session.session_id.clone(),
                                    game.clone(),
                                ));
                            }
                            actions.push(UiAction::CloseSessionConflict);
                        }
                    }
                });

                ui.add_space(15.0);

                // Cancel
                if ui.button("Cancel").clicked() {
                    actions.push(UiAction::CloseSessionConflict);
                }
            });
        });
}

/// Render AV1 hardware warning dialog
pub fn render_av1_warning_dialog(ctx: &egui::Context, actions: &mut Vec<UiAction>) {
    egui::Window::new("AV1 Not Supported")
        .collapsible(false)
        .resizable(false)
        .fixed_size([400.0, 180.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(15.0);

                ui.label(
                    egui::RichText::new("⚠ AV1 Hardware Decoding Not Available")
                        .size(16.0)
                        .strong()
                        .color(egui::Color32::from_rgb(255, 180, 50))
                );

                ui.add_space(15.0);

                ui.label(
                    egui::RichText::new("Your GPU does not support AV1 hardware decoding.\nAV1 requires an NVIDIA RTX 30 series or newer GPU.")
                        .size(13.0)
                        .color(egui::Color32::LIGHT_GRAY)
                );

                ui.add_space(20.0);

                ui.horizontal(|ui| {
                    if ui.button("Switch to H.265").clicked() {
                        actions.push(UiAction::UpdateSetting(SettingChange::Codec(crate::app::VideoCodec::H265)));
                        actions.push(UiAction::CloseAV1Warning);
                    }

                    ui.add_space(10.0);

                    if ui.button("Close").clicked() {
                        actions.push(UiAction::CloseAV1Warning);
                    }
                });
            });
        });
}

/// Render Alliance experimental warning dialog
pub fn render_alliance_warning_dialog(
    ctx: &egui::Context,
    provider_name: &str,
    actions: &mut Vec<UiAction>,
) {
    egui::Window::new("Alliance Partner")
        .collapsible(false)
        .resizable(false)
        .fixed_size([420.0, 200.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);

                // Alliance badge - centered
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 80, 130))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin {
                        left: 14,
                        right: 14,
                        top: 6,
                        bottom: 6,
                    })
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("ALLIANCE")
                                .size(14.0)
                                .color(egui::Color32::from_rgb(100, 180, 255))
                                .strong(),
                        );
                    });

                ui.add_space(12.0);

                ui.label(
                    egui::RichText::new(format!("Welcome to {} via Alliance!", provider_name))
                        .size(17.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );

                ui.add_space(10.0);

                ui.label(
                    egui::RichText::new("Alliance support is still experimental.")
                        .size(14.0)
                        .color(egui::Color32::from_rgb(255, 200, 80)),
                );

                ui.add_space(6.0);

                ui.label(
                    egui::RichText::new(
                        "Please report issues: github.com/zortos293/OpenNOW/issues",
                    )
                    .size(13.0)
                    .color(egui::Color32::LIGHT_GRAY),
                );

                ui.add_space(6.0);

                ui.label(
                    egui::RichText::new(
                        "Note: Feedback from Alliance users is especially valuable!",
                    )
                    .size(12.0)
                    .color(egui::Color32::GRAY)
                    .italics(),
                );

                ui.add_space(12.0);

                let got_it_btn =
                    egui::Button::new(egui::RichText::new("Got it!").size(14.0).strong())
                        .fill(egui::Color32::from_rgb(70, 130, 70))
                        .min_size(egui::vec2(100.0, 32.0));

                if ui.add(got_it_btn).clicked() {
                    actions.push(UiAction::CloseAllianceWarning);
                }
            });
        });
}
