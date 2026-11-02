use crate::db::FileEntry;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn scan_directory<P: AsRef<Path>>(
    root: P,
    stop_signal: Arc<AtomicBool>,
    on_entry: impl Fn(FileEntry) + Send + Sync + 'static,
) {
    let root = root.as_ref();
    let mut stack = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        if stop_signal.load(Ordering::Relaxed) {
            break;
        }

        let entries = match fs::read_dir(&current) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry_res in entries {
            if stop_signal.load(Ordering::Relaxed) {
                break;
            }
            if let Ok(entry) = entry_res {
                let path = entry.path();
                let is_dir = path.is_dir();
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                let metadata = entry.metadata().ok();
                let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = metadata
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

                on_entry(FileEntry {
                    id,
                    name: file_name,
                    path: path.to_string_lossy().to_string(),
                    size,
                    is_dir,
                    modified,
                });

                if is_dir {
                    stack.push(path);
                }
            }
        }
    }
}
