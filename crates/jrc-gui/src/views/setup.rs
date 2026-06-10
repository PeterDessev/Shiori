//! First-run setup: download dictionary and frequency data.

use eframe::egui;

use crate::app::{JrcGui, Phase};

impl JrcGui {
    pub fn show_setup(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.heading("Japanese Reading Companion");
                ui.add_space(12.0);
                ui.label("First run: the JMdict dictionary (~11 MB download) and a word \
                          frequency list need to be fetched and imported.");
                ui.label(format!("Data directory: {}", self.data_dir.display()));
                ui.add_space(16.0);

                match self.phase {
                    Phase::NeedsData => {
                        if ui.button("Download reference data").clicked() {
                            self.start_download(ctx);
                        }
                    }
                    Phase::Downloading => {
                        ui.spinner();
                        ui.add_space(8.0);
                    }
                    _ => {}
                }

                for line in &self.progress {
                    ui.label(line);
                }

                if let Some(error) = self.error.clone() {
                    ui.add_space(10.0);
                    ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
                    if ui.small_button("dismiss").clicked() {
                        self.error = None;
                    }
                }
            });
        });
    }
}
