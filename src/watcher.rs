use crate::db::{Database, FileEntry};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_ACTION_ADDED, FILE_ACTION_MODIFIED,
    FILE_ACTION_REMOVED, FILE_ACTION_RENAMED_NEW_NAME, FILE_ACTION_RENAMED_OLD_NAME,
    FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FILE_NOTIFY_INFORMATION,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};

static WATCHER_ID: AtomicU64 = AtomicU64::new(100_000_000);

pub struct DriveWatcher {
    root: PathBuf,
    db: Arc<Mutex<Database>>,
    changed_flag: Arc<AtomicBool>,
}

impl DriveWatcher {
    pub fn new(root: PathBuf, db: Arc<Mutex<Database>>, changed_flag: Arc<AtomicBool>) -> Self {
        Self {
            root,
            db,
            changed_flag,
        }
    }

    pub fn start(self) {
        thread::spawn(move || {
            let root_str = self.root.to_string_lossy();
            let wide_path: Vec<u16> = root_str.encode_utf16().chain(std::iter::once(0)).collect();

            unsafe {
                let handle = CreateFileW(
                    PCWSTR(wide_path.as_ptr()),
                    FILE_LIST_DIRECTORY.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    None,
                    OPEN_EXISTING,
                    windows::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS,
                    windows::Win32::Foundation::HANDLE(std::ptr::null_mut()),
                );

                let handle = match handle {
                    Ok(h) if !h.is_invalid() => h,
                    _ => return,
                };

                let notify_filter = FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_SIZE
                    | FILE_NOTIFY_CHANGE_LAST_WRITE;

                let mut buffer = vec![0u8; 65536];
                let mut bytes_returned: u32 = 0;

                loop {
                    let success = ReadDirectoryChangesW(
                        handle,
                        buffer.as_mut_ptr() as *mut _,
                        buffer.len() as u32,
                        true,
                        notify_filter,
                        Some(&mut bytes_returned),
                        None,
                        None,
                    );

                    if success.is_err() || bytes_returned == 0 {
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }

                    let mut offset = 0usize;
                    let mut has_changes = false;

                    while offset < bytes_returned as usize {
                        let info = &*(buffer.as_ptr().add(offset) as *const FILE_NOTIFY_INFORMATION);
                        let name_len = (info.FileNameLength / 2) as usize;
                        let name_slice = std::slice::from_raw_parts(info.FileName.as_ptr(), name_len);
                        let rel_path = String::from_utf16_lossy(name_slice);
                        let full_path = self.root.join(&rel_path);

                        match info.Action {
                            FILE_ACTION_ADDED | FILE_ACTION_MODIFIED | FILE_ACTION_RENAMED_NEW_NAME => {
                                if let Ok(meta) = fs::metadata(&full_path) {
                                    let is_dir = meta.is_dir();
                                    let size = if is_dir { 0 } else { meta.len() };
                                    let modified = meta
                                        .modified()
                                        .ok()
                                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                        .map(|d| d.as_secs())
                                        .unwrap_or(0);
                                    let file_name = full_path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("")
                                        .to_string();

                                    let entry = FileEntry {
                                        id: WATCHER_ID.fetch_add(1, Ordering::Relaxed),
                                        name: file_name,
                                        path: full_path.to_string_lossy().to_string(),
                                        size,
                                        is_dir,
                                        modified,
                                    };

                                    let mut db = self.db.lock().unwrap();
                                    db.update_or_insert(entry);
                                    has_changes = true;
                                }
                            }
                            FILE_ACTION_REMOVED | FILE_ACTION_RENAMED_OLD_NAME => {
                                let path_str = full_path.to_string_lossy();
                                let mut db = self.db.lock().unwrap();
                                db.remove_by_path(&path_str);
                                has_changes = true;
                            }
                            _ => {}
                        }

                        if info.NextEntryOffset == 0 {
                            break;
                        }
                        offset += info.NextEntryOffset as usize;
                    }

                    if has_changes {
                        self.changed_flag.store(true, Ordering::Relaxed);
                        let db = self.db.lock().unwrap();
                        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
                        let p = PathBuf::from(appdata).join("Magnolia").join("magnolia.db");
                        let _ = db.save_to_file(p);
                    }
                }
            }
        });
    }
}
