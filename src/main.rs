#![windows_subsystem = "windows"]

use chrono::{DateTime, Local};
use eframe::egui;
use magnolia::db::{Database, FileEntry};
use magnolia::ntfs::get_available_drives;
use magnolia::query::{QueryEngine, QueryOptions};
use magnolia::scanner::scan_directory;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn load_app_icon() -> Option<egui::IconData> {
    let png_bytes = include_bytes!("../assets/icon.png");
    if let Ok(image) = image::load_from_memory(png_bytes) {
        let image = image.to_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        Some(egui::IconData {
            rgba,
            width,
            height,
        })
    } else {
        None
    }
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([950.0, 620.0])
        .with_min_inner_size([500.0, 350.0])
        .with_title("Magnolia");

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Magnolia",
        options,
        Box::new(|_cc| Ok(Box::new(MagnoliaApp::new()))),
    )
}

struct MagnoliaApp {
    db: Arc<Mutex<Database>>,
    search_query: String,
    options: QueryOptions,
    matched_results: Vec<FileEntry>,
    status_text: String,
    is_scanning: Arc<AtomicBool>,
    db_file_path: PathBuf,
    sort_column: SortColumn,
    sort_asc: bool,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum SortColumn {
    Name,
    Path,
    Size,
    Date,
}

impl MagnoliaApp {
    fn new() -> Self {
        let app_dir = dirs_or_local();
        let db_file_path = app_dir.join("magnolia.db");

        let db = if db_file_path.exists() {
            Database::load_from_file(&db_file_path).unwrap_or_else(|_| Database::new())
        } else {
            Database::new()
        };

        let db_arc = Arc::new(Mutex::new(db));
        let count = db_arc.lock().unwrap().len();

        let mut app = Self {
            db: db_arc,
            search_query: String::new(),
            options: QueryOptions::default(),
            matched_results: Vec::new(),
            status_text: if count > 0 {
                format!("Instant ready: {} items loaded from cache.", count)
            } else {
                "First launch: Auto-indexing all drives in background...".to_string()
            },
            is_scanning: Arc::new(AtomicBool::new(false)),
            db_file_path,
            sort_column: SortColumn::Name,
            sort_asc: true,
        };

        app.update_search();

        // Otomatis index semua drive jika database kosong saat pertama kali dibuka
        if count == 0 {
            app.trigger_scan_all_drives();
        }

        app
    }

    fn trigger_scan_all_drives(&mut self) {
        if self.is_scanning.load(Ordering::Relaxed) {
            return;
        }

        self.is_scanning.store(true, Ordering::Relaxed);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let is_scanning_flag = self.is_scanning.clone();
        let db_clone = self.db.clone();
        let save_path = self.db_file_path.clone();

        thread::spawn(move || {
            let drives = get_available_drives();
            let buffer = Arc::new(Mutex::new(Vec::new()));

            for drive in drives {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                let root = PathBuf::from(format!("{}:\\", drive));
                if root.exists() {
                    let buf_inner = buffer.clone();
                    scan_directory(root, stop_flag.clone(), move |entry| {
                        buf_inner.lock().unwrap().push(entry);
                    });
                }
            }

            let entries = std::mem::take(&mut *buffer.lock().unwrap());
            {
                let mut db = db_clone.lock().unwrap();
                db.add_batch(entries);
                db.deduplicate();
                let _ = db.save_to_file(&save_path);
            }

            is_scanning_flag.store(false, Ordering::Relaxed);
        });
    }

    fn update_search(&mut self) {
        let q_engine = QueryEngine::new(&self.search_query, self.options.clone());
        let db_guard = self.db.lock().unwrap();

        let mut results: Vec<FileEntry> = db_guard
            .entries
            .iter()
            .filter(|e| q_engine.matches(e))
            .take(5000)
            .cloned()
            .collect();

        match self.sort_column {
            SortColumn::Name => {
                if self.sort_asc {
                    results.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                } else {
                    results.sort_by(|a, b| b.name.to_lowercase().cmp(&a.name.to_lowercase()));
                }
            }
            SortColumn::Path => {
                if self.sort_asc {
                    results.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
                } else {
                    results.sort_by(|a, b| b.path.to_lowercase().cmp(&a.path.to_lowercase()));
                }
            }
            SortColumn::Size => {
                if self.sort_asc {
                    results.sort_by_key(|e| e.size);
                } else {
                    results.sort_by_key(|e| std::cmp::Reverse(e.size));
                }
            }
            SortColumn::Date => {
                if self.sort_asc {
                    results.sort_by_key(|e| e.modified);
                } else {
                    results.sort_by_key(|e| std::cmp::Reverse(e.modified));
                }
            }
        }

        self.matched_results = results;
        self.status_text = format!(
            "Showing {} matches out of {} total items.",
            self.matched_results.len(),
            db_guard.len()
        );
    }

    fn trigger_scan(&mut self, folder: PathBuf) {
        if self.is_scanning.load(Ordering::Relaxed) {
            return;
        }

        self.is_scanning.store(true, Ordering::Relaxed);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let is_scanning_flag = self.is_scanning.clone();
        let db_clone = self.db.clone();
        let save_path = self.db_file_path.clone();

        thread::spawn(move || {
            let buffer = Arc::new(Mutex::new(Vec::new()));
            let buffer_inner = buffer.clone();

            scan_directory(folder, stop_flag, move |entry| {
                let mut b = buffer_inner.lock().unwrap();
                b.push(entry);
            });

            let entries = std::mem::take(&mut *buffer.lock().unwrap());
            {
                let mut db = db_clone.lock().unwrap();
                for e in entries {
                    db.add(e);
                }
                let _ = db.save_to_file(&save_path);
            }

            is_scanning_flag.store(false, Ordering::Relaxed);
        });
    }
}

impl eframe::App for MagnoliaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🔍 Search:").strong());
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Type to search files and folders... (!negate, *.ext)")
                        .desired_width(f32::INFINITY),
                );
                if response.changed() {
                    self.update_search();
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.checkbox(&mut self.options.case_sensitive, "Match Case").changed() {
                    self.update_search();
                }
                if ui.checkbox(&mut self.options.match_path, "Match Path").changed() {
                    self.update_search();
                }
                if ui.checkbox(&mut self.options.regex, "Regex").changed() {
                    self.update_search();
                }

                ui.separator();

                if ui.button("⚡ Index All Drives").clicked() {
                    self.trigger_scan_all_drives();
                }

                if ui.button("📁 Add Folder...").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        self.trigger_scan(folder);
                    }
                }

                if self.is_scanning.load(Ordering::Relaxed) {
                    ui.spinner();
                    ui.label("Indexing in background...");
                    ctx.request_repaint_after(Duration::from_millis(500));
                } else if self.matched_results.is_empty() && self.search_query.is_empty() {
                    // Refresh hasil begitu scan selesai
                    self.update_search();
                }
            });
            ui.add_space(4.0);
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_text);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let row_height = 20.0;
            let total_rows = self.matched_results.len();

            egui::Grid::new("results_header")
                .striped(true)
                .min_col_width(80.0)
                .show(ui, |ui| {
                    if ui.button("Name ⬍").clicked() {
                        if self.sort_column == SortColumn::Name {
                            self.sort_asc = !self.sort_asc;
                        } else {
                            self.sort_column = SortColumn::Name;
                            self.sort_asc = true;
                        }
                        self.update_search();
                    }
                    if ui.button("Path ⬍").clicked() {
                        if self.sort_column == SortColumn::Path {
                            self.sort_asc = !self.sort_asc;
                        } else {
                            self.sort_column = SortColumn::Path;
                            self.sort_asc = true;
                        }
                        self.update_search();
                    }
                    if ui.button("Size ⬍").clicked() {
                        if self.sort_column == SortColumn::Size {
                            self.sort_asc = !self.sort_asc;
                        } else {
                            self.sort_column = SortColumn::Size;
                            self.sort_asc = true;
                        }
                        self.update_search();
                    }
                    if ui.button("Date Modified ⬍").clicked() {
                        if self.sort_column == SortColumn::Date {
                            self.sort_asc = !self.sort_asc;
                        } else {
                            self.sort_column = SortColumn::Date;
                            self.sort_asc = true;
                        }
                        self.update_search();
                    }
                    ui.end_row();
                });

            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show_rows(ui, row_height, total_rows, |ui, row_range| {
                    for row in row_range {
                        if let Some(entry) = self.matched_results.get(row) {
                            let icon = if entry.is_dir { "📁 " } else { "📄 " };
                            ui.horizontal(|ui| {
                                let name_text = format!("{}{}", icon, entry.name);
                                if ui.link(name_text).clicked() {
                                    let _ = open_in_explorer(&entry.path);
                                }
                                ui.label(&entry.path);
                                ui.label(format_size(entry.size, entry.is_dir));
                                ui.label(format_timestamp(entry.modified));
                            });
                        }
                    }
                });
        });
    }
}

fn open_in_explorer(path: &str) -> std::io::Result<()> {
    std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", path))
        .spawn()?;
    Ok(())
}

fn format_size(bytes: u64, is_dir: bool) -> String {
    if is_dir {
        return "<DIR>".to_string();
    }
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_timestamp(secs: u64) -> String {
    if secs == 0 {
        return "-".to_string();
    }
    if let Some(dt) = DateTime::from_timestamp(secs as i64, 0) {
        let local_dt: DateTime<Local> = DateTime::from(dt);
        local_dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        "-".to_string()
    }
}

fn dirs_or_local() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("Magnolia");
        let _ = std::fs::create_dir_all(&p);
        p
    } else {
        PathBuf::from(".")
    }
}
