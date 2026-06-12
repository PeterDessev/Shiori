//! First-run setup: download dictionary and frequency data.

use eframe::egui;

use crate::app::{Phase, ShioriGui};

impl ShioriGui {
    pub fn show_setup(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.heading("Shiori");
                ui.add_space(12.0);
                ui.label(
                    "Reference data needs to be fetched and imported: the JMdict \
                     dictionary (~11 MB), a word frequency list, kanji data with \
                     stroke order (~5 MB), and JLPT vocabulary lists. Steps \
                     already imported are skipped.",
                );
                ui.label(format!("Data directory: {}", self.data_dir.display()));
                ui.add_space(16.0);

                match self.phase {
                    Phase::NeedsData => {
                        if ui.button("Download reference data").clicked() {
                            self.start_download(ctx);
                        }
                        ui.add_space(8.0);
                        if ui
                            .button("Continue without dictionary")
                            .on_hover_text(
                                "Import, read, and review without definitions; \
                                 you can retry the download any time",
                            )
                            .clicked()
                        {
                            self.phase = Phase::Ready;
                            self.refresh_caches();
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
