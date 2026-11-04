#![windows_subsystem = "windows"]

use chrono::{DateTime, Local};
use eframe::egui;
use magnolia::db::Database;
use magnolia::ntfs::{
    get_available_drives, scan_mft, start_downloads_watcher, start_usn_monitor, FileSystemChange,
};
use magnolia::query::{QueryEngine, QueryOptions};
use magnolia::scanner::scan_directory_fast;
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

struct RenameDialogState {
    entry_id: u64,
    old_full_path: String,
    new_name: String,
    error_msg: Option<String>,
}

struct MagnoliaApp {
    db: Arc<Mutex<Database>>,
    search_query: String,
    options: QueryOptions,
    matched_results: Vec<usize>,
    status_text: String,
    is_scanning: Arc<AtomicBool>,
    db_file_path: PathBuf,
    sort_column: SortColumn,
    sort_asc: bool,
    selected_id: Option<u64>,
    fs_rx: std::sync::mpsc::Receiver<FileSystemChange>,
    _watcher_stop: Arc<AtomicBool>,
    icon_cache: std::collections::HashMap<String, egui::TextureHandle>,
    col_width_name: f32,
    col_width_path: f32,
    col_width_size: f32,
    col_width_date: f32,
    rename_modal: Option<RenameDialogState>,
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

        let (fs_tx, fs_rx) = std::sync::mpsc::channel();
        let watcher_stop = Arc::new(AtomicBool::new(false));

        let drives = get_available_drives();
        for &d in &drives {
            start_usn_monitor(d, watcher_stop.clone(), fs_tx.clone());
        }
        start_downloads_watcher(watcher_stop.clone(), fs_tx);

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
            selected_id: None,
            fs_rx,
            _watcher_stop: watcher_stop,
            icon_cache: std::collections::HashMap::new(),
            col_width_name: 260.0,
            col_width_path: 380.0,
            col_width_size: 90.0,
            col_width_date: 150.0,
            rename_modal: None,
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
            let mut all_batches = Vec::new();

            for &drive in &drives {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                // Coba pembacaan MFT langsung (super cepat jika elevated Admin)
                match scan_mft(drive, stop_flag.clone()) {
                    Ok(batch) => {
                        all_batches.push(batch);
                    }
                    Err(_) => {
                        // Fallback cepat: parallel drive traversal
                        let root = PathBuf::from(format!("{}:\\", drive));
                        if root.exists() {
                            let batch = scan_directory_fast(root, stop_flag.clone());
                            all_batches.push(batch);
                        }
                    }
                }
            }

            {
                let mut db = db_clone.lock().unwrap();
                for batch in all_batches {
                    db.add_batch(batch.folders, batch.entries);
                }
                db.deduplicate();
                let _ = db.save_to_file(&save_path);
            }

            is_scanning_flag.store(false, Ordering::Relaxed);
        });
    }

    fn update_search(&mut self) {
        let q_engine = QueryEngine::new(&self.search_query, self.options.clone());
        let db_guard = self.db.lock().unwrap();

        let entries = &db_guard.entries;
        let n_threads = 4;
        let chunk_size = (entries.len() + n_threads - 1).max(1) / n_threads;

        let mut results: Vec<usize> = Vec::new();
        if chunk_size > 0 && entries.len() > 1000 {
            std::thread::scope(|s| {
                let mut handles = Vec::new();
                for (t_idx, chunk) in entries.chunks(chunk_size).enumerate() {
                    let offset = t_idx * chunk_size;
                    let q = &q_engine;
                    let db = &db_guard;
                    let handle = s.spawn(move || {
                        let mut matched = Vec::new();
                        for (i, e) in chunk.iter().enumerate() {
                            let folder = db.get_folder(e.parent_id);
                            if q.matches(e, folder) {
                                matched.push(offset + i);
                                if matched.len() >= 1000 {
                                    break;
                                }
                            }
                        }
                        matched
                    });
                    handles.push(handle);
                }
                for h in handles {
                    if let Ok(matched) = h.join() {
                        results.extend(matched);
                        if results.len() >= 2000 {
                            break;
                        }
                    }
                }
            });
        } else {
            for (i, e) in entries.iter().enumerate() {
                let folder = db_guard.get_folder(e.parent_id);
                if q_engine.matches(e, folder) {
                    results.push(i);
                    if results.len() >= 2000 {
                        break;
                    }
                }
            }
        }

        match self.sort_column {
            SortColumn::Name => {
                if self.sort_asc {
                    results.sort_by(|&a, &b| {
                        db_guard.entries[a].name.cmp(&db_guard.entries[b].name)
                    });
                } else {
                    results.sort_by(|&a, &b| {
                        db_guard.entries[b].name.cmp(&db_guard.entries[a].name)
                    });
                }
            }
            SortColumn::Path => {
                if self.sort_asc {
                    results.sort_by(|&a, &b| {
                        let fa = db_guard.get_folder(db_guard.entries[a].parent_id);
                        let fb = db_guard.get_folder(db_guard.entries[b].parent_id);
                        fa.cmp(fb).then_with(|| {
                            db_guard.entries[a].name.cmp(&db_guard.entries[b].name)
                        })
                    });
                } else {
                    results.sort_by(|&a, &b| {
                        let fa = db_guard.get_folder(db_guard.entries[a].parent_id);
                        let fb = db_guard.get_folder(db_guard.entries[b].parent_id);
                        fb.cmp(fa).then_with(|| {
                            db_guard.entries[b].name.cmp(&db_guard.entries[a].name)
                        })
                    });
                }
            }
            SortColumn::Size => {
                if self.sort_asc {
                    results.sort_by_key(|&i| db_guard.entries[i].size);
                } else {
                    results.sort_by_key(|&i| std::cmp::Reverse(db_guard.entries[i].size));
                }
            }
            SortColumn::Date => {
                if self.sort_asc {
                    results.sort_by_key(|&i| db_guard.entries[i].modified);
                } else {
                    results.sort_by_key(|&i| std::cmp::Reverse(db_guard.entries[i].modified));
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
            let batch = scan_directory_fast(folder, stop_flag);
            {
                let mut db = db_clone.lock().unwrap();
                db.add_batch(batch.folders, batch.entries);
                db.deduplicate();
                let _ = db.save_to_file(&save_path);
            }

            is_scanning_flag.store(false, Ordering::Relaxed);
        });
    }
}

fn header_button(ui: &mut egui::Ui, text: &str, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::click());
    let bg_color = if resp.hovered() {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    ui.painter().rect_filled(rect, 2.0, bg_color);
    ui.painter().text(
        egui::pos2(rect.min.x + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(13.0),
        ui.visuals().text_color(),
    );
    resp
}

fn column_divider(ui: &mut egui::Ui, width: &mut f32) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(6.0, 24.0), egui::Sense::click_and_drag());
    if resp.dragged() {
        *width = (*width + resp.drag_delta().x).max(50.0);
    }
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
        ui.painter().rect_filled(rect, 1.0, ui.visuals().widgets.active.bg_fill);
    } else {
        let x = rect.center().x;
        ui.painter().line_segment(
            [egui::pos2(x, rect.min.y + 4.0), egui::pos2(x, rect.max.y - 4.0)],
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
    }
}

impl eframe::App for MagnoliaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut fs_updated = false;
        while let Ok(change) = self.fs_rx.try_recv() {
            let mut db = self.db.lock().unwrap();
            match change {
                FileSystemChange::CreatedOrModified {
                    drive: _,
                    name,
                    parent_folder,
                    is_dir,
                } => {
                    let parent_id = db.add_folder(parent_folder);
                    let id = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(0);
                    db.add(magnolia::db::FileEntry {
                        id,
                        name,
                        parent_id,
                        size: 0,
                        is_dir,
                        modified: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
                    });
                    fs_updated = true;
                }
                FileSystemChange::Deleted { drive: _, name } => {
                    let before = db.entries.len();
                    db.entries.retain(|e| e.name != name);
                    if db.entries.len() != before {
                        fs_updated = true;
                    }
                }
            }
        }

        if fs_updated {
            self.update_search();
            ctx.request_repaint();
        }

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
            let row_height = 22.0;
            let total_rows = self.matched_results.len();

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if header_button(ui, "Name ⬍", self.col_width_name).clicked() {
                    if self.sort_column == SortColumn::Name {
                        self.sort_asc = !self.sort_asc;
                    } else {
                        self.sort_column = SortColumn::Name;
                        self.sort_asc = true;
                    }
                    self.update_search();
                }
                column_divider(ui, &mut self.col_width_name);

                if header_button(ui, "Path ⬍", self.col_width_path).clicked() {
                    if self.sort_column == SortColumn::Path {
                        self.sort_asc = !self.sort_asc;
                    } else {
                        self.sort_column = SortColumn::Path;
                        self.sort_asc = true;
                    }
                    self.update_search();
                }
                column_divider(ui, &mut self.col_width_path);

                if header_button(ui, "Size ⬍", self.col_width_size).clicked() {
                    if self.sort_column == SortColumn::Size {
                        self.sort_asc = !self.sort_asc;
                    } else {
                        self.sort_column = SortColumn::Size;
                        self.sort_asc = true;
                    }
                    self.update_search();
                }
                column_divider(ui, &mut self.col_width_size);

                if header_button(ui, "Date Modified ⬍", self.col_width_date).clicked() {
                    if self.sort_column == SortColumn::Date {
                        self.sort_asc = !self.sort_asc;
                    } else {
                        self.sort_column = SortColumn::Date;
                        self.sort_asc = true;
                    }
                    self.update_search();
                }
                column_divider(ui, &mut self.col_width_date);
            });

            ui.separator();

            // Pre-cache Windows icons for visible rows
            {
                let db_guard = self.db.lock().unwrap();
                for &idx in self.matched_results.iter().take(200) {
                    if let Some(entry) = db_guard.entries.get(idx) {
                        let ext_key = if entry.is_dir {
                            "__folder__".to_string()
                        } else {
                            std::path::Path::new(&entry.name)
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|e| e.to_lowercase())
                                .unwrap_or_else(|| "__file__".to_string())
                        };

                        if !self.icon_cache.contains_key(&ext_key) {
                            let hicon = magnolia::icon::get_windows_icon(entry.is_dir, &ext_key);
                            let img = hicon
                                .and_then(|h| magnolia::icon::hicon_to_color_image(h, 16, 16))
                                .unwrap_or_else(|| magnolia::icon::get_fallback_icon(entry.is_dir));
                            let tex = ctx.load_texture(
                                format!("win_icon_{}", ext_key),
                                img,
                                egui::TextureOptions::default(),
                            );
                            self.icon_cache.insert(ext_key, tex);
                        }
                    }
                }
            }

            let mut clicked_id = None;

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show_rows(ui, row_height, total_rows, |ui, row_range| {
                    let db_guard = self.db.lock().unwrap();
                    for row in row_range {
                        if let Some(&idx) = self.matched_results.get(row) {
                            if let Some(entry) = db_guard.entries.get(idx) {
                                let entry_id = entry.id;
                                let ext_key = if entry.is_dir {
                                    "__folder__".to_string()
                                } else {
                                    std::path::Path::new(&entry.name)
                                        .extension()
                                        .and_then(|e| e.to_str())
                                        .map(|e| e.to_lowercase())
                                        .unwrap_or_else(|| "__file__".to_string())
                                };

                                let entry_path = db_guard.get_full_path(entry);
                                let entry_name = entry.name.clone();
                                let size_text = format_size(entry.size, entry.is_dir);
                                let date_text = format_timestamp(entry.modified);

                                let mut row_rect = ui.available_rect_before_wrap();
                                row_rect.set_height(row_height);

                                let is_hovered = ui.rect_contains_pointer(row_rect);
                                let is_selected = self.selected_id == Some(entry_id);

                                // Visual highlight: selection > hover
                                if is_selected {
                                    ui.painter().rect_filled(
                                        row_rect,
                                        2.0,
                                        ui.visuals().selection.bg_fill,
                                    );
                                } else if is_hovered {
                                    ui.painter().rect_filled(
                                        row_rect,
                                        2.0,
                                        ui.visuals().faint_bg_color,
                                    );
                                }

                                let text_color = if is_selected {
                                    ui.visuals().selection.stroke.color
                                } else {
                                    ui.visuals().text_color()
                                };

                                let icon_tex = self.icon_cache.get(&ext_key).or_else(|| {
                                    self.icon_cache.get(if entry.is_dir { "__folder__" } else { "__file__" })
                                });

                                let row_resp = ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;

                                    // 1. Kolom Name
                                    let (name_rect, name_resp) = ui.allocate_exact_size(
                                        egui::vec2(self.col_width_name, row_height),
                                        egui::Sense::click(),
                                    );
                                    let icon_rect = egui::Rect::from_min_size(
                                        egui::pos2(name_rect.min.x + 4.0, name_rect.min.y + (row_height - 16.0) / 2.0),
                                        egui::vec2(16.0, 16.0),
                                    );
                                    if let Some(tex) = icon_tex {
                                        ui.painter().image(
                                            tex.id(),
                                            icon_rect,
                                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                            egui::Color32::WHITE,
                                        );
                                    }
                                    let name_clip = egui::Rect::from_min_max(
                                        egui::pos2(name_rect.min.x + 24.0, name_rect.min.y),
                                        egui::pos2(name_rect.max.x - 4.0, name_rect.max.y),
                                    );
                                    ui.painter().with_clip_rect(name_clip).text(
                                        egui::pos2(name_rect.min.x + 24.0, name_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        &entry_name,
                                        egui::FontId::proportional(13.0),
                                        text_color,
                                    );

                                    // Spacer 6.0px persis sejajar divider header
                                    ui.allocate_exact_size(egui::vec2(6.0, row_height), egui::Sense::hover());

                                    // 2. Kolom Path (Align Left)
                                    let (path_rect, path_resp) = ui.allocate_exact_size(
                                        egui::vec2(self.col_width_path, row_height),
                                        egui::Sense::click(),
                                    );
                                    let path_clip = egui::Rect::from_min_max(
                                        egui::pos2(path_rect.min.x + 4.0, path_rect.min.y),
                                        egui::pos2(path_rect.max.x - 4.0, path_rect.max.y),
                                    );
                                    ui.painter().with_clip_rect(path_clip).text(
                                        egui::pos2(path_rect.min.x + 4.0, path_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        &entry_path,
                                        egui::FontId::proportional(13.0),
                                        text_color,
                                    );

                                    // Spacer 6.0px
                                    ui.allocate_exact_size(egui::vec2(6.0, row_height), egui::Sense::hover());

                                    // 3. Kolom Size (Align Left)
                                    let (size_rect, size_resp) = ui.allocate_exact_size(
                                        egui::vec2(self.col_width_size, row_height),
                                        egui::Sense::click(),
                                    );
                                    let size_clip = egui::Rect::from_min_max(
                                        egui::pos2(size_rect.min.x + 4.0, size_rect.min.y),
                                        egui::pos2(size_rect.max.x - 4.0, size_rect.max.y),
                                    );
                                    ui.painter().with_clip_rect(size_clip).text(
                                        egui::pos2(size_rect.min.x + 4.0, size_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        &size_text,
                                        egui::FontId::proportional(13.0),
                                        text_color,
                                    );

                                    // Spacer 6.0px
                                    ui.allocate_exact_size(egui::vec2(6.0, row_height), egui::Sense::hover());

                                    // 4. Kolom Date (Align Left)
                                    let (date_rect, date_resp) = ui.allocate_exact_size(
                                        egui::vec2(self.col_width_date, row_height),
                                        egui::Sense::click(),
                                    );
                                    let date_clip = egui::Rect::from_min_max(
                                        egui::pos2(date_rect.min.x + 4.0, date_rect.min.y),
                                        egui::pos2(date_rect.max.x - 4.0, date_rect.max.y),
                                    );
                                    ui.painter().with_clip_rect(date_clip).text(
                                        egui::pos2(date_rect.min.x + 4.0, date_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        &date_text,
                                        egui::FontId::proportional(13.0),
                                        text_color,
                                    );

                                    name_resp | path_resp | size_resp | date_resp
                                }).inner;

                                if row_resp.clicked() {
                                    clicked_id = Some(entry_id);
                                    // Klik satu kali: seleksi baris saja
                                }

                                if row_resp.double_clicked() {
                                    clicked_id = Some(entry_id);
                                    // Klik dua kali: buka file langsung
                                    let _ = open_file_default(&entry_path);
                                }

                                if row_resp.secondary_clicked() {
                                    clicked_id = Some(entry_id);
                                    // Klik kanan: langsung buka Windows Explorer Context Menu (IContextMenu/2/3)
                                    magnolia::context_menu::show_native_context_menu(&entry_path);
                                }
                            }
                        }
                    }
                });

            if let Some(id) = clicked_id {
                self.selected_id = Some(id);
            }

            // Keyboard shortcut F2: Rename file yang sedang diseleksi
            if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
                if let Some(id) = self.selected_id {
                    let db_guard = self.db.lock().unwrap();
                    if let Some(entry) = db_guard.entries.iter().find(|e| e.id == id) {
                        let full_p = db_guard.get_full_path(entry);
                        self.rename_modal = Some(RenameDialogState {
                            entry_id: id,
                            old_full_path: full_p,
                            new_name: entry.name.clone(),
                            error_msg: None,
                        });
                    }
                }
            }
        });

        if let Some(mut state) = self.rename_modal.take() {
            let mut should_cancel = false;
            let mut should_rename = false;

            egui::Window::new("✏️ Rename File")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("Path: {}", state.old_full_path)).weak());
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label("New Name:");
                        let text_resp = ui.add(
                            egui::TextEdit::singleline(&mut state.new_name)
                                .desired_width(320.0),
                        );
                        if text_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            should_rename = true;
                        }
                    });

                    if let Some(ref err) = state.error_msg {
                        ui.add_space(4.0);
                        ui.colored_label(egui::Color32::from_rgb(255, 80, 80), err);
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("💾 Save").clicked() {
                            should_rename = true;
                        }
                        if ui.button("Cancel").clicked() {
                            should_cancel = true;
                        }
                    });
                });

            if should_rename {
                let old_p = std::path::PathBuf::from(&state.old_full_path);
                if let Some(parent) = old_p.parent() {
                    let new_p = parent.join(&state.new_name);
                    match std::fs::rename(&old_p, &new_p) {
                        Ok(()) => {
                            {
                                let mut db = self.db.lock().unwrap();
                                if let Some(e) = db.entries.iter_mut().find(|e| e.id == state.entry_id) {
                                    e.name = state.new_name.clone();
                                }
                            }
                            self.update_search();
                        }
                        Err(e) => {
                            state.error_msg = Some(format!("Rename failed: {}", e));
                            self.rename_modal = Some(state);
                        }
                    }
                }
            } else if !should_cancel {
                self.rename_modal = Some(state);
            }
        }
    }
}

fn open_file_default(path: &str) -> std::io::Result<()> {
    // Open file or folder with ShellExecuteW directly: instant 0-delay and zero console window flash
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let wide_open: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let res = ShellExecuteW(
            HWND(std::ptr::null_mut()),
            PCWSTR(wide_open.as_ptr()),
            PCWSTR(wide_path.as_ptr()),
            PCWSTR(std::ptr::null()),
            PCWSTR(std::ptr::null()),
            SW_SHOWNORMAL,
        );
        if res.0 as usize <= 32 {
            let _ = open_in_explorer(path);
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn open_file_properties(path: &str) -> std::io::Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW, SEE_MASK_INVOKEIDLIST};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

    let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let wide_verb: Vec<u16> = "properties".encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let mut info = SHELLEXECUTEINFOW::default();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_INVOKEIDLIST;
        info.hwnd = HWND(std::ptr::null_mut());
        info.lpVerb = PCWSTR(wide_verb.as_ptr());
        info.lpFile = PCWSTR(wide_path.as_ptr());
        info.nShow = SW_SHOW.0;
        let _ = ShellExecuteExW(&mut info);
    }
    Ok(())
}

fn open_in_explorer(path: &str) -> std::io::Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
    use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};

    let path_buf = path.to_string();
    std::thread::spawn(move || {
        let wide: Vec<u16> = path_buf.encode_utf16().chain(std::iter::once(0)).collect();
        let success = unsafe {
            let _ = OleInitialize(None);
            let pidl = ILCreateFromPathW(PCWSTR(wide.as_ptr()));
            if !pidl.is_null() {
                let hr = SHOpenFolderAndSelectItems(pidl, None, 0);
                ILFree(Some(pidl));
                OleUninitialize();
                hr.is_ok()
            } else {
                OleUninitialize();
                false
            }
        };

        if !success {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("explorer.exe")
                .raw_arg(format!("/select,\"{}\"", path_buf))
                .spawn();
        }
    });

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













