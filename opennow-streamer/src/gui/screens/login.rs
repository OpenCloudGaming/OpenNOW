//! Login Screen
//!
//! Renders the login/provider selection screen.

use crate::app::UiAction;
use crate::auth::LoginProvider;

/// Render the login screen with provider selection
pub fn render_login_screen(
    ctx: &egui::Context,
    login_providers: &[LoginProvider],
    selected_provider_index: usize,
    status_message: &str,
    is_loading: bool,
    actions: &mut Vec<UiAction>
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        let available_height = ui.available_height();
        let content_height = 400.0;
        let top_padding = ((available_height - content_height) / 2.0).max(40.0);

        ui.vertical_centered(|ui| {
            ui.add_space(top_padding);

            // Logo/Title with gradient-like effect
            ui.label(
                egui::RichText::new("OpenNOW")
                    .size(48.0)
                    .color(egui::Color32::from_rgb(118, 185, 0)) // NVIDIA green
                    .strong()
            );

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("GeForce NOW Client")
                    .size(14.0)
                    .color(egui::Color32::from_rgb(150, 150, 150))
            );

            ui.add_space(60.0);

            // Login card container
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(30, 30, 40))
                .corner_radius(12.0)
                .inner_margin(egui::Margin { left: 40, right: 40, top: 30, bottom: 30 })
                .show(ui, |ui| {
                    ui.set_min_width(320.0);

                    ui.vertical(|ui| {
                        // Region selection label - centered
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new("Select Region")
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(180, 180, 180))
                            );
                        });

                        ui.add_space(10.0);

                        // Provider dropdown - centered using horizontal with spacing
                        ui.horizontal(|ui| {
                            let available_width = ui.available_width();
                            let combo_width = 240.0;
                            let padding = (available_width - combo_width) / 2.0;
                            ui.add_space(padding.max(0.0));

                            let selected_name = login_providers.get(selected_provider_index)
                                .map(|p| p.login_provider_display_name.as_str())
                                .unwrap_or("NVIDIA (Global)");

                            egui::ComboBox::from_id_salt("provider_select")
                                .selected_text(selected_name)
                                .width(combo_width)
                                .show_ui(ui, |ui| {
                                    for (i, provider) in login_providers.iter().enumerate() {
                                        let is_selected = i == selected_provider_index;
                                        if ui.selectable_label(is_selected, &provider.login_provider_display_name).clicked() {
                                            actions.push(UiAction::SelectProvider(i));
                                        }
                                    }
                                });
                        });

                        ui.add_space(25.0);

                        // Login button or loading state - centered
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            if is_loading {
                                ui.add_space(10.0);
                                ui.spinner();
                                ui.add_space(12.0);
                                ui.label(
                                    egui::RichText::new("Opening browser...")
                                        .size(13.0)
                                        .color(egui::Color32::from_rgb(118, 185, 0))
                                );
                                ui.add_space(5.0);
                                ui.label(
                                    egui::RichText::new("Complete login in your browser")
                                        .size(11.0)
                                        .color(egui::Color32::GRAY)
                                );
                            } else {
                                let login_btn = egui::Button::new(
                                    egui::RichText::new("Sign In")
                                        .size(15.0)
                                        .color(egui::Color32::WHITE)
                                        .strong()
                                )
                                .fill(egui::Color32::from_rgb(118, 185, 0))
                                .corner_radius(6.0);

                                if ui.add_sized([240.0, 42.0], login_btn).clicked() {
                                    actions.push(UiAction::StartLogin);
                                }

                                ui.add_space(15.0);

                                ui.label(
                                    egui::RichText::new("Sign in with your NVIDIA account")
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(120, 120, 120))
                                );
                            }
                        });
                    });
                });

            ui.add_space(20.0);

            // Status message (if any)
            if !status_message.is_empty() && status_message != "Welcome to OpenNOW" {
                ui.label(
                    egui::RichText::new(status_message)
                        .size(11.0)
                        .color(egui::Color32::from_rgb(150, 150, 150))
                );
            }

            ui.add_space(40.0);

            // Footer info
            ui.label(
                egui::RichText::new("Alliance Partners can select their region above")
                    .size(10.0)
                    .color(egui::Color32::from_rgb(80, 80, 80))
            );
        });
    });
}
