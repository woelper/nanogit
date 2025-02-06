#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)] // it's an example

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use eframe::egui;
use git2::{Repository, Status, StatusOptions};
use log::{error, info};

struct FileStatus {
    path: PathBuf,
    status: Status,
}

struct RepoCache {
    pub repo: Arc<Mutex<Repository>>,
    statuses: Arc<Mutex<Vec<FileStatus>>>,
}

impl RepoCache {
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::open(path)?;
        Ok(Self {
            repo: Arc::new(Mutex::new(repo)),
            statuses: Arc::new(Mutex::new(vec![])),
        })
    }

    pub fn stage(&self, path: &Path) -> Result<()> {
        let mut index = self.repo.lock().unwrap().index()?;
        index.add_path(path)?;
        index.write()?;
        Ok(())
    }

    pub fn unstage(&self, path: &Path) -> Result<()> {
        let mut index = self.repo.lock().unwrap().index()?;
        index.remove_path(path)?;
        index.write()?;
        Ok(())
    }

    pub fn get_status(&self) -> Result<()> {
        let repo = self.repo.clone();
        let r_statuses = self.statuses.clone();

        std::thread::spawn(move || {
            let mut status_opts = StatusOptions::new();
            status_opts
                .include_untracked(true) // Show untracked files
                .recurse_untracked_dirs(true); // Show untracked files within dirs

            // Get the status of all files in the repo
            // let statuses = repo.lock().unwrap().statuses(Some(&mut status_opts)).unwrap();
            let binding = repo.lock().unwrap();
            let statuses = binding.statuses(Some(&mut status_opts)).unwrap();

            // Iterate through each file's status
            r_statuses.lock().unwrap().clear();
            for entry in statuses.iter() {
                let path = entry.path().unwrap_or("<none>");
                info!("{path}");
                // You can check various bits in `status`:
                // - INDEX_* for staged changes
                // - WT_* for working tree changes (untracked, modified, etc.)
                r_statuses.lock().unwrap().push(FileStatus {
                    path: PathBuf::from(path),
                    status: entry.status(),
                });
            }
        });

        Ok(())
    }
}

fn main() -> eframe::Result {
    std::env::set_var("RUST_LOG", "debug");

    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native(
        "bait",
        options,
        Box::new(|cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::<MyApp>::default())
        }),
    )
}

struct MyApp {
    repo: Option<RepoCache>,
}

impl Default for MyApp {
    fn default() -> Self {
        Self { repo: None }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.button("Load").clicked() {
                match open_repo() {
                    Ok(r) => {
                        self.repo = Some(r);
                    }
                    Err(e) => error!("{e}"),
                }
            }

            if let Some(repo) = self.repo.as_mut() {
                if ui.button("Status").clicked() {
                    let s = repo.get_status();
                    info!("{:?}", s);
                    _ = repo.get_status();
                }
            }

            if let Some(repo) = &self.repo {
                ui.label("Repo opened.");
                egui::Grid::new("id_salt")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for s in &*repo.statuses.lock().unwrap() {
                            ui.label(
                                s.path
                                    .file_name()
                                    .map(|f| f.to_string_lossy().to_string())
                                    .unwrap_or_default(),
                            );
                            ui.label(format!("{:?}", s.status));
                            if s.status.is_index_new() {
                                if ui.button("-").clicked() {
                                    _ = repo.unstage(&s.path);
                                    _ = repo.get_status();
                                }
                            }
                            if s.status.is_wt_new() {
                                if ui.button("+").clicked() {
                                    _ = repo.stage(&s.path);
                                    _ = repo.get_status();
                                }
                            }

                            ui.end_row();
                        }
                    });
            }
        });
    }
}

use anyhow::{Context, Result};

fn open_repo() -> Result<RepoCache> {
    let folder = rfd::FileDialog::new().pick_folder().context("No folder")?;
    info!("{}", folder.display());
    let repo = RepoCache::open(&folder)?;
    Ok(repo)
}
