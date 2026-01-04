//! Session Screen
//!
//! Renders the session loading/connecting screen.

use crate::app::{GameInfo, UiAction};

/// Render the session screen (loading/connecting state)
pub fn render_session_screen(
    ctx: &egui::Context,
    selected_game: &Option<GameInfo>,
    status_message: &str,
    error_message: &Option<String>,
    actions: &mut Vec<UiAction>
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);

            // Game title
            if let Some(ref game) = selected_game {
                ui.label(
                    egui::RichText::new(&game.title)
                        .size(28.0)
                        .strong()
                        .color(egui::Color32::WHITE)
                );
            }

            ui.add_space(40.0);

            // Spinner
            ui.spinner();

            ui.add_space(20.0);

            // Status
            ui.label(
                egui::RichText::new(status_message)
                    .size(16.0)
                    .color(egui::Color32::LIGHT_GRAY)
            );

            // Error message
            if let Some(ref error) = error_message {
                ui.add_space(20.0);
                ui.label(
                    egui::RichText::new(error)
                        .size(14.0)
                        .color(egui::Color32::from_rgb(255, 100, 100))
                );
            }

            ui.add_space(40.0);

            // Cancel button
            if ui.button("Cancel").clicked() {
                actions.push(UiAction::StopStreaming);
            }
        });
    });
}
