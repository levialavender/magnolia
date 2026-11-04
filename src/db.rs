use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    pub id: u64,
    pub name: String,
    pub parent_id: u32,
    pub size: u64,
    pub is_dir: bool,
    pub modified: u64,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Database {
    pub folder_paths: Vec<String>,
    pub entries: Vec<FileEntry>,
}

impl Database {
    pub fn new() -> Self {
        Self {
            folder_paths: Vec::new(),
            entries: Vec::new(),
        }
    }

    pub fn get_folder(&self, parent_id: u32) -> &str {
        self.folder_paths
            .get(parent_id as usize)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn get_full_path(&self, entry: &FileEntry) -> String {
        let folder = self.get_folder(entry.parent_id);
        if folder.is_empty() {
            entry.name.clone()
        } else if folder.ends_with('\\') || folder.ends_with('/') {
            format!("{}{}", folder, entry.name)
        } else {
            format!("{}\\{}", folder, entry.name)
        }
    }

    pub fn add(&mut self, entry: FileEntry) {
        self.entries.push(entry);
    }

    pub fn add_folder(&mut self, path: String) -> u32 {
        if let Some(pos) = self.folder_paths.iter().position(|p| p == &path) {
            pos as u32
        } else {
            let id = self.folder_paths.len() as u32;
            self.folder_paths.push(path);
            id
        }
    }

    pub fn add_batch(&mut self, folders: Vec<String>, new_entries: Vec<FileEntry>) {
        if self.folder_paths.is_empty() {
            self.folder_paths = folders;
            self.entries.extend(new_entries);
        } else {
            let mut id_map = Vec::with_capacity(folders.len());
            for f in folders {
                let id = self.add_folder(f);
                id_map.push(id);
            }
            for mut e in new_entries {
                if let Some(&new_parent_id) = id_map.get(e.parent_id as usize) {
                    e.parent_id = new_parent_id;
                }
                self.entries.push(e);
            }
        }
    }

    pub fn deduplicate(&mut self) {
        self.entries
            .sort_by(|a, b| a.parent_id.cmp(&b.parent_id).then_with(|| a.name.cmp(&b.name)));
        self.entries
            .dedup_by(|a, b| a.parent_id == b.parent_id && a.name == b.name);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let path = path.as_ref();
        let tmp_path = path.with_extension("db.tmp");
        let encoded = bincode::serialize(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&tmp_path, encoded)?;
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
        std::fs::rename(tmp_path, path)?;
        Ok(())
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let data = std::fs::read(path)?;
        let db = bincode::deserialize(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(db)
    }

    pub fn sort_by_name(&mut self, ascending: bool) {
        if ascending {
            self.entries.sort_by(|a, b| a.name.cmp(&b.name));
        } else {
            self.entries.sort_by(|a, b| b.name.cmp(&a.name));
        }
    }

    pub fn sort_by_size(&mut self, ascending: bool) {
        if ascending {
            self.entries.sort_by(|a, b| a.size.cmp(&b.size));
        } else {
            self.entries.sort_by(|a, b| b.size.cmp(&a.size));
        }
    }

    pub fn sort_by_date(&mut self, ascending: bool) {
        if ascending {
            self.entries.sort_by(|a, b| a.modified.cmp(&b.modified));
        } else {
            self.entries.sort_by(|a, b| b.modified.cmp(&a.modified));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_len() {
        let mut db = Database::new();
        assert!(db.is_empty());
        let folder_id = db.add_folder("C:\\".to_string());
        db.add(FileEntry {
            id: 1,
            name: "test.txt".to_string(),
            parent_id: folder_id,
            size: 100,
            is_dir: false,
            modified: 12345,
        });
        assert_eq!(db.len(), 1);
        assert_eq!(db.get_full_path(&db.entries[0]), "C:\\test.txt");
    }

    #[test]
    fn test_save_load() {
        let tmp = std::env::temp_dir().join("magnolia_test.db");
        let mut db = Database::new();
        let folder_id = db.add_folder("D:\\code".to_string());
        db.add(FileEntry {
            id: 1,
            name: "halo.rs".to_string(),
            parent_id: folder_id,
            size: 512,
            is_dir: false,
            modified: 999,
        });
        db.save_to_file(&tmp).unwrap();
        // Test overwrite save on existing file
        db.save_to_file(&tmp).unwrap();
        let loaded = Database::load_from_file(&tmp).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.entries[0].name, "halo.rs");
        assert_eq!(loaded.get_full_path(&loaded.entries[0]), "D:\\code\\halo.rs");
        let _ = std::fs::remove_file(tmp);
    }
}
