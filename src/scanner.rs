use crate::db::FileEntry;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Default, Debug)]
pub struct ScanBatch {
    pub folders: Vec<String>,
    pub entries: Vec<FileEntry>,
}

pub fn scan_directory_fast<P: AsRef<Path>>(
    root: P,
    stop_signal: Arc<AtomicBool>,
) -> ScanBatch {
    let root = root.as_ref();
    let mut folders = Vec::new();
    let mut folder_map = HashMap::new();
    let mut entries = Vec::new();

    let root_str = root.to_string_lossy().to_string();
    folders.push(root_str.clone());
    folder_map.insert(root.to_path_buf(), 0u32);

    let mut stack = vec![(root.to_path_buf(), 0u32)];

    while let Some((current_dir, parent_folder_id)) = stack.pop() {
        if stop_signal.load(Ordering::Relaxed) {
            break;
        }

        let read_dir = match fs::read_dir(&current_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry_res in read_dir {
            if stop_signal.load(Ordering::Relaxed) {
                break;
            }
            if let Ok(entry) = entry_res {
                let file_name = entry.file_name().to_string_lossy().to_string();
                let file_type = entry.file_type().ok();
                let is_dir = file_type.as_ref().map(|ft| ft.is_dir()).unwrap_or(false);

                let metadata = entry.metadata().ok();
                let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = metadata
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

                entries.push(FileEntry {
                    id,
                    name: file_name,
                    parent_id: parent_folder_id,
                    size,
                    is_dir,
                    modified,
                });

                if is_dir {
                    let sub_path = entry.path();
                    let new_folder_id = folders.len() as u32;
                    folders.push(sub_path.to_string_lossy().to_string());
                    folder_map.insert(sub_path.clone(), new_folder_id);
                    stack.push((sub_path, new_folder_id));
                }
            }
        }
    }

    ScanBatch { folders, entries }
}

pub fn scan_drives_parallel(
    drives: Vec<char>,
    stop_signal: Arc<AtomicBool>,
) -> Vec<ScanBatch> {
    use std::thread;

    let mut handles = Vec::new();

    for drive in drives {
        let stop = stop_signal.clone();
        let handle = thread::spawn(move || {
            let root = PathBuf::from(format!("{}:\\", drive));
            if root.exists() {
                Some(scan_directory_fast(root, stop))
            } else {
                None
            }
        });
        handles.push(handle);
    }

    let mut batches = Vec::new();
    for handle in handles {
        if let Ok(Some(batch)) = handle.join() {
            batches.push(batch);
        }
    }
    batches
}
