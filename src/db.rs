use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: u64,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Database {
    pub entries: Vec<FileEntry>,
}

impl Database {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn add(&mut self, entry: FileEntry) {
        self.entries.push(entry);
    }

    pub fn add_batch(&mut self, new_entries: Vec<FileEntry>) {
        self.entries.extend(new_entries);
    }

    pub fn deduplicate(&mut self) {
        self.entries.sort_by(|a, b| a.path.cmp(&b.path));
        self.entries.dedup_by(|a, b| a.path == b.path);
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
        db.add(FileEntry {
            id: 1,
            name: "test.txt".to_string(),
            path: "C:\\test.txt".to_string(),
            size: 100,
            is_dir: false,
            modified: 12345,
        });
        assert_eq!(db.len(), 1);
    }

    #[test]
    fn test_save_load() {
        let tmp = std::env::temp_dir().join("magnolia_test.db");
        let mut db = Database::new();
        db.add(FileEntry {
            id: 1,
            name: "halo.rs".to_string(),
            path: "D:\\code\\halo.rs".to_string(),
            size: 512,
            is_dir: false,
            modified: 999,
        });
        db.save_to_file(&tmp).unwrap();
        let loaded = Database::load_from_file(&tmp).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.entries[0].name, "halo.rs");
        let _ = std::fs::remove_file(tmp);
    }
}
