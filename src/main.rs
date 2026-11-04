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
            let row_height = 20.0;
            let total_rows = self.matched_results.len();

            let avail_width = ui.available_width();
            let size_width = 80.0;
            let date_width = 140.0;
            let spacing = 6.0;
            let remaining = (avail_width - size_width - date_width - (spacing * 3.0) - 16.0).max(200.0);
            let name_width = remaining * 0.4;
            let path_width = remaining * 0.6;

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = spacing;
                if ui.add_sized([name_width, 22.0], egui::Button::new("Name ⬍")).clicked() {
                    if self.sort_column == SortColumn::Name {
                        self.sort_asc = !self.sort_asc;
                    } else {
                        self.sort_column = SortColumn::Name;
                        self.sort_asc = true;
                    }
                    self.update_search();
                }
                if ui.add_sized([path_width, 22.0], egui::Button::new("Path ⬍")).clicked() {
                    if self.sort_column == SortColumn::Path {
                        self.sort_asc = !self.sort_asc;
                    } else {
                        self.sort_column = SortColumn::Path;
                        self.sort_asc = true;
                    }
                    self.update_search();
                }
                if ui.add_sized([size_width, 22.0], egui::Button::new("Size ⬍")).clicked() {
                    if self.sort_column == SortColumn::Size {
                        self.sort_asc = !self.sort_asc;
                    } else {
                        self.sort_column = SortColumn::Size;
                        self.sort_asc = true;
                    }
                    self.update_search();
                }
                if ui.add_sized([date_width, 22.0], egui::Button::new("Date Modified ⬍")).clicked() {
                    if self.sort_column == SortColumn::Date {
                        self.sort_asc = !self.sort_asc;
                    } else {
                        self.sort_column = SortColumn::Date;
                        self.sort_asc = true;
                    }
                    self.update_search();
                }
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
                                    ui.spacing_mut().item_spacing.x = spacing;
                                    let name_resp = ui.allocate_ui_with_layout(
                                        egui::vec2(name_width, row_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.spacing_mut().item_spacing.x = 6.0;
                                            let img_resp = if let Some(tex) = icon_tex {
                                                ui.add(
                                                    egui::Image::new((tex.id(), egui::vec2(16.0, 16.0)))
                                                        .sense(egui::Sense::click()),
                                                )
                                            } else {
                                                ui.allocate_response(egui::vec2(16.0, 16.0), egui::Sense::hover())
                                            };
                                            let label_resp = ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&entry_name).color(text_color),
                                                )
                                                .sense(egui::Sense::click())
                                                .truncate(),
                                            );
                                            img_resp | label_resp
                                        },
                                    ).inner;

                                    let path_resp = ui.add_sized(
                                        [path_width, row_height],
                                        egui::Label::new(
                                            egui::RichText::new(&entry_path).color(text_color),
                                        )
                                        .sense(egui::Sense::click())
                                        .truncate(),
                                    );
                                    let size_resp = ui.add_sized(
                                        [size_width, row_height],
                                        egui::Label::new(
                                            egui::RichText::new(&size_text).color(text_color),
                                        )
                                        .sense(egui::Sense::click()),
                                    );
                                    let date_resp = ui.add_sized(
                                        [date_width, row_height],
                                        egui::Label::new(
                                            egui::RichText::new(&date_text).color(text_color),
                                        )
                                        .sense(egui::Sense::click()),
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
                                }

                                // Built-in In-App Context Menu via egui (Dijamin 100% responsif & lancar)
                                row_resp.context_menu(|ui| {
                                    ui.set_min_width(220.0);
                                    if ui.button("🚀 Open File").clicked() {
                                        let _ = open_file_default(&entry_path);
                                        ui.close_menu();
                                    }
                                    if ui.button("📁 Open in Explorer").clicked() {
                                        let _ = open_in_explorer(&entry_path);
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    if ui.button("📋 Copy Full Path").clicked() {
                                        ui.output_mut(|o| o.copied_text = entry_path.clone());
                                        ui.close_menu();
                                    }
                                    if ui.button("📄 Copy File Name").clicked() {
                                        ui.output_mut(|o| o.copied_text = entry_name.clone());
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    if ui.button("🌐 Windows Context Menu (System)").clicked() {
                                        magnolia::context_menu::show_native_context_menu(&entry_path);
                                        ui.close_menu();
                                    }
                                    if ui.button("ℹ️ Properties").clicked() {
                                        let _ = open_file_properties(&entry_path);
                                        ui.close_menu();
                                    }
                                });
                            }
                        }
                    }
                });

            if let Some(id) = clicked_id {
                self.selected_id = Some(id);
            }
        });
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





