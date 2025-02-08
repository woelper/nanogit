#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use anyhow::{Context, Result};
use eframe::egui::{self, Id, Response, Sense, Stroke, Ui, WidgetText};
use egui_notify::Toasts;
use egui_phosphor::regular::*;
use log::{debug, info};
use nanogit::{RepoCache, Status};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn main() -> eframe::Result {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native(
        "NanoGit",
        options,
        Box::new(|cc| Ok(Box::new(GitApp::new(cc)))),
    )
}

#[derive(Serialize, Deserialize)]

struct GitApp {
    #[serde(skip)]
    repo: Option<RepoCache>,
    // The root of the repo, for reopening on the next run
    repo_root: Option<PathBuf>,
    commit_message: String,
    #[serde(skip)]
    toasts: Toasts,
    selected_file: Option<usize>,
}

impl Default for GitApp {
    fn default() -> Self {
        Self {
            repo: None,
            repo_root: None,
            commit_message: Default::default(),
            toasts: Toasts::default(),
            selected_file: None,
        }
    }
}

impl GitApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fd = egui::FontDefinitions::default();

        egui_phosphor::add_to_fonts(&mut fd, egui_phosphor::Variant::Regular);

        cc.egui_ctx.set_fonts(fd);

        if let Some(storage) = cc.storage {
            let mut state =
                eframe::get_value::<GitApp>(storage, eframe::APP_KEY).unwrap_or_default();
            info!("storage present {:?}", state.repo_root);

            if let Some(root) = state.repo_root.as_ref() {
                state.repo = RepoCache::open(&root).ok();
                _ = state.repo.as_ref().map(|r| r.refresh());
                return state;
            }
        }
        Self::default()
    }
}

impl eframe::App for GitApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        debug!("Saved state {:?}", self.repo_root);

        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.toasts.show(ctx);

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open repository").clicked() {
                        match open_repo() {
                            Ok(r) => {
                                self.repo = Some(r);
                                let repo = self.repo.as_ref().expect("This repo must exist");
                                if let Err(e) = repo.refresh() {
                                    self.toasts.error(e.to_string());
                                }
                                self.repo_root = Some(repo.get_root());
                            }
                            Err(e) => {
                                self.toasts.error(format!("{e}"));
                            }
                        }
                        ui.close_menu();
                    }

                    if let Some(repo) = self.repo.as_mut() {
                        if ui.button("Refresh").clicked() {
                            if let Err(e) = repo.refresh() {
                                self.toasts.error(e.to_string());
                            }
                        }
                    }
                });
            });
            if let Some(repo) = &self.repo {
                if !repo.is_local_refreshed() {
                    ui.spinner();
                }
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // if let Some(repo) = self.repo.as_mut() {
            //     if ui.button("Status").clicked() {
            //         if let Err(e) = repo.get_status() {
            //             self.toasts.error(e.to_string());
            //         }
            //     }
            // }

            if let Some(repo) = &self.repo {
                ui.vertical_centered_justified(|ui| {
                    let any_staged = repo.get_statuses().iter().any(|s| {
                        s.status.is_index_deleted()
                            || s.status.is_index_modified()
                            || s.status.is_index_new()
                            || s.status.is_index_renamed()
                    });

                    ui.add_enabled_ui(any_staged, |ui| {
                        egui::TextEdit::multiline(&mut self.commit_message)
                            .desired_rows(1)
                            .hint_text("Commit message")
                            .desired_width(ui.available_width())
                            .show(ui);

                        ui.add_enabled_ui(!self.commit_message.is_empty(), |ui| {
                            if ui.button("Commit").clicked() {
                                match repo.commit() {
                                    Ok(_) => self.commit_message.clear(),
                                    Err(e) => {
                                        self.toasts.error(format!("{e}"));
                                    }
                                }
                            }
                        });
                    });
                });

                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::CollapsingHeader::new("Changes")
                        .default_open(true)
                        .show(ui, |ui| {
                            for (i, status) in repo.get_statuses().iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let row_rect = ui.available_rect_before_wrap();

                                    if ui.rect_contains_pointer(row_rect) {
                                        ui.painter().rect(
                                            row_rect,
                                            0.,
                                            ui.style().visuals.widgets.hovered.bg_fill,
                                            Stroke::NONE,
                                            // StrokeKind::Middle,
                                        );
                                    }

                                    if ui.interact(row_rect, Id::new(i), Sense::click()).clicked() {
                                        info!("Clicked {i}, selected {:?}", self.selected_file);
                                        if Some(i) == self.selected_file {
                                            self.selected_file = None;
                                        } else {
                                            self.selected_file = Some(i);
                                            if let Ok(diff) = repo.diff(&status.path) {
                                                info!("diff {diff}");
                                                ui.ctx().data_mut(|w| {
                                                    w.insert_temp("diff".into(), diff)
                                                });
                                            }
                                        }
                                    }

                                    if Some(i) == self.selected_file {
                                        ui.painter().rect(
                                            row_rect,
                                            0.,
                                            ui.style().visuals.widgets.active.bg_fill,
                                            Stroke::NONE,
                                            // StrokeKind::Middle,
                                        );
                                    }

                                    unselected_label(
                                        status
                                            .path
                                            .file_name()
                                            .map(|f| f.to_string_lossy().to_string())
                                            .unwrap_or_default(),
                                        ui,
                                    )
                                    .on_hover_text(format!("{:?}", status.status));

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            unselected_label(status_text(status.status), ui);

                                            if ui.rect_contains_pointer(row_rect) {
                                                if status.status.is_index_new()
                                                    || status.status.is_index_modified()
                                                {
                                                    if ui.button(MINUS).clicked() {
                                                        _ = repo.unstage(&status.path);
                                                    }
                                                }

                                                if status.status.is_wt_new()
                                                    || status.status.is_wt_modified()
                                                {
                                                    if ui.button(PLUS).clicked() {
                                                        _ = repo.stage(&status.path);
                                                    }
                                                }
                                            }
                                        },
                                    );

                                    ui.end_row();
                                });
                            }
                        });

                    if self.selected_file.is_some() {
                        ui.collapsing("Diff", |ui| {
                            if let Some(diff) =
                                ui.ctx().data(|r| r.get_temp::<String>("diff".into()))
                            {
                                // ui.label(diff);

                                use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
                                let mut diff = diff;

                                let syntax = Syntax {
                                    language: "diff",
                                    case_sensitive: false,
                                    comment: "//",
                                    comment_multiline: ["SDsD", "dsdssd"],
                                    hyperlinks: Default::default(),
                                    keywords: std::collections::BTreeSet::from(["+"]),
                                    types: std::collections::BTreeSet::from(["-"]),
                                    special: Default::default(),
                                };

                                CodeEditor::default()
                                    .id_source("code editor")
                                    .with_fontsize(14.0)
                                    .with_theme(ColorTheme::SONOKAI)
                                    .with_syntax(Syntax::shell())
                                    .with_syntax(syntax)
                                    .with_numlines(true)
                                    .show(ui, &mut diff);
                            }
                        });
                    }

                    ui.collapsing("Log", |ui| {
                        for logitem in repo.get_log() {
                            ui.horizontal(|ui| {
                                ui.label("Name");
                                ui.label(logitem.name);
                                ui.label("email");
                                ui.label(logitem.email);
                            });
                            ui.horizontal(|ui| {
                                ui.label(logitem.message);
                            });
                            ui.separator();
                        }
                    });
                });
            }
        });
    }
}

fn open_repo() -> Result<RepoCache> {
    let folder = rfd::FileDialog::new().pick_folder().context("No folder")?;
    info!("Opening: {}", folder.display());
    let repo = RepoCache::open(&folder)?;
    Ok(repo)
}

/// Just a helper for unselected labels
fn unselected_label(text: impl Into<WidgetText>, ui: &mut Ui) -> Response {
    ui.add(egui::Label::new(text).selectable(false))
}

fn status_text(status: Status) -> &'static str {
    if status.is_conflicted() {
        return "!";
    }

    if status.is_index_modified() || status.is_wt_modified() {
        return "M";
    }

    if status.is_index_new() || status.is_wt_new() {
        return "U";
    }

    if status.is_index_deleted() || status.is_wt_deleted() {
        return "D";
    }

    if status.is_index_typechange() || status.is_wt_typechange() {
        return "A";
    }

    "?"
}
