use crate::db::FileEntry;
use crate::scanner::ScanBatch;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetLogicalDrives, FILE_ATTRIBUTE_DIRECTORY, FILE_GENERIC_READ, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{
    FSCTL_ENUM_USN_DATA, FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, MFT_ENUM_DATA_V0,
    READ_USN_JOURNAL_DATA_V0, USN_RECORD_V2,
};
use windows::Win32::System::IO::DeviceIoControl;

static NEXT_MFT_ID: AtomicU64 = AtomicU64::new(1_000_000);

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct UsnJournalData {
    pub usn_journal_id: u64,
    pub lowest_valid_usn: i64,
    pub highest_usn: i64,
    pub maximum_size: u64,
    pub allocation_delta: u64,
}

pub struct UsnMonitor {
    drive_letter: char,
}

impl UsnMonitor {
    pub fn new(drive_letter: char) -> Self {
        Self { drive_letter }
    }

    pub fn query_journal(&self) -> Result<UsnJournalData, String> {
        let volume_path = format!("\\\\.\\{}:", self.drive_letter);
        let wide: Vec<u16> = volume_path.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            let handle = CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
                HANDLE(std::ptr::null_mut()),
            );

            let handle = match handle {
                Ok(h) => h,
                Err(e) => return Err(format!("Failed to open volume {}: {:?}", volume_path, e)),
            };

            if handle.is_invalid() {
                return Err(format!("Invalid handle for volume {}", volume_path));
            }

            let mut journal_data = UsnJournalData::default();
            let mut bytes_returned: u32 = 0;

            let result = DeviceIoControl(
                handle,
                FSCTL_QUERY_USN_JOURNAL,
                None,
                0,
                Some(&mut journal_data as *mut _ as *mut _),
                std::mem::size_of::<UsnJournalData>() as u32,
                Some(&mut bytes_returned),
                None,
            );

            let _ = CloseHandle(handle);

            if result.is_ok() {
                Ok(journal_data)
            } else {
                Err("DeviceIoControl FSCTL_QUERY_USN_JOURNAL failed (requires Admin)".to_string())
            }
        }
    }
}

pub fn get_available_drives() -> Vec<char> {
    let mut drives = Vec::new();
    unsafe {
        let mask = GetLogicalDrives();
        for i in 0..26 {
            if (mask & (1 << i)) != 0 {
                let drive_char = (b'A' + i as u8) as char;
                drives.push(drive_char);
            }
        }
    }
    drives
}

pub fn scan_mft(drive: char, stop_signal: Arc<AtomicBool>) -> Result<ScanBatch, String> {
    let volume_path = format!("\\\\.\\{}:", drive);
    let wide: Vec<u16> = volume_path.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let handle = CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
            HANDLE(std::ptr::null_mut()),
        );

        let handle = match handle {
            Ok(h) => h,
            Err(e) => return Err(format!("Access denied for volume {}: {:?}", volume_path, e)),
        };

        if handle.is_invalid() {
            return Err(format!("Invalid handle for volume {}", volume_path));
        }

        let mut journal_data = UsnJournalData::default();
        let mut bytes_returned: u32 = 0;

        let query_res = DeviceIoControl(
            handle,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some(&mut journal_data as *mut _ as *mut _),
            std::mem::size_of::<UsnJournalData>() as u32,
            Some(&mut bytes_returned),
            None,
        );

        if query_res.is_err() {
            let _ = CloseHandle(handle);
            return Err("FSCTL_QUERY_USN_JOURNAL failed (requires Administrator privilege)".to_string());
        }

        let mut enum_data = MFT_ENUM_DATA_V0 {
            StartFileReferenceNumber: 0,
            LowUsn: 0,
            HighUsn: journal_data.highest_usn,
        };

        let mut buffer = vec![0u8; 64 * 1024];

        struct RawMftEntry {
            parent_ref: u64,
            name: String,
            is_dir: bool,
            modified: u64,
        }

        let mut raw_entries: HashMap<u64, RawMftEntry> = HashMap::with_capacity(200_000);

        loop {
            if stop_signal.load(Ordering::Relaxed) {
                break;
            }

            let mut out_bytes = 0u32;
            let ok = DeviceIoControl(
                handle,
                FSCTL_ENUM_USN_DATA,
                Some(&enum_data as *const _ as *const _),
                std::mem::size_of::<MFT_ENUM_DATA_V0>() as u32,
                Some(buffer.as_mut_ptr() as *mut _),
                buffer.len() as u32,
                Some(&mut out_bytes),
                None,
            );

            if ok.is_err() || out_bytes <= 8 {
                break;
            }

            let next_usn = *(buffer.as_ptr() as *const u64);
            enum_data.StartFileReferenceNumber = next_usn;

            let mut offset = 8usize;
            while offset + std::mem::size_of::<USN_RECORD_V2>() <= out_bytes as usize {
                let rec = &*(buffer.as_ptr().add(offset) as *const USN_RECORD_V2);
                let rec_len = rec.RecordLength as usize;
                if rec_len == 0 {
                    break;
                }

                let name_offset = offset + rec.FileNameOffset as usize;
                let name_chars = (rec.FileNameLength / 2) as usize;
                if name_offset + (name_chars * 2) <= out_bytes as usize {
                    let name_slice = std::slice::from_raw_parts(
                        buffer.as_ptr().add(name_offset) as *const u16,
                        name_chars,
                    );
                    let name = String::from_utf16_lossy(name_slice);
                    let is_dir = (rec.FileAttributes & FILE_ATTRIBUTE_DIRECTORY.0) != 0;
                    let file_ref = rec.FileReferenceNumber;
                    let parent_ref = rec.ParentFileReferenceNumber;

                    let modified = if rec.TimeStamp > 116444736000000000 {
                        ((rec.TimeStamp - 116444736000000000) / 10000000) as u64
                    } else {
                        0
                    };

                    raw_entries.insert(
                        file_ref,
                        RawMftEntry {
                            parent_ref,
                            name,
                            is_dir,
                            modified,
                        },
                    );
                }

                offset += rec_len;
            }
        }

        let _ = CloseHandle(handle);

        let root_dir_str = format!("{}:\\", drive);
        let mut folders = Vec::new();
        folders.push(root_dir_str);

        let mut dir_to_folder_id: HashMap<u64, u32> = HashMap::new();
        dir_to_folder_id.insert(0x0005000000000005, 0);
        dir_to_folder_id.insert(5, 0);

        for (&file_ref, entry) in &raw_entries {
            if entry.is_dir && !dir_to_folder_id.contains_key(&file_ref) {
                let mut path_components = vec![entry.name.clone()];
                let mut cur_parent = entry.parent_ref;
                let mut loop_count = 0;

                while loop_count < 100 {
                    loop_count += 1;
                    if let Some(&id) = dir_to_folder_id.get(&cur_parent) {
                        let base = &folders[id as usize];
                        let mut full = base.clone();
                        if !full.ends_with('\\') {
                            full.push('\\');
                        }
                        for comp in path_components.iter().rev() {
                            full.push_str(comp);
                            full.push('\\');
                        }
                        if full.ends_with('\\') && full.len() > 3 {
                            full.pop();
                        }
                        let new_id = folders.len() as u32;
                        folders.push(full);
                        dir_to_folder_id.insert(file_ref, new_id);
                        break;
                    } else if let Some(parent_entry) = raw_entries.get(&cur_parent) {
                        path_components.push(parent_entry.name.clone());
                        cur_parent = parent_entry.parent_ref;
                    } else {
                        break;
                    }
                }
            }
        }

        let mut entries = Vec::with_capacity(raw_entries.len());
        for (_file_ref, entry) in raw_entries {
            let parent_id = *dir_to_folder_id.get(&entry.parent_ref).unwrap_or(&0);
            let id = NEXT_MFT_ID.fetch_add(1, Ordering::Relaxed);
            entries.push(FileEntry {
                id,
                name: entry.name,
                parent_id,
                size: 0,
                is_dir: entry.is_dir,
                modified: entry.modified,
            });
        }

        Ok(ScanBatch { folders, entries })
    }
}

pub const USN_REASON_FILE_CREATE: u32 = 0x00000100;
pub const USN_REASON_FILE_DELETE: u32 = 0x00000200;
pub const USN_REASON_RENAME_OLD_NAME: u32 = 0x00001000;
pub const USN_REASON_RENAME_NEW_NAME: u32 = 0x00002000;

#[derive(Debug, Clone)]
pub enum FileSystemChange {
    CreatedOrModified {
        drive: char,
        name: String,
        parent_folder: String,
        is_dir: bool,
    },
    Deleted {
        drive: char,
        name: String,
    },
}

pub fn start_usn_monitor(
    drive: char,
    stop_signal: Arc<AtomicBool>,
    sender: std::sync::mpsc::Sender<FileSystemChange>,
) {
    std::thread::spawn(move || {
        let volume_path = format!("\\\\.\\{}:", drive);
        let wide: Vec<u16> = volume_path.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            let handle = CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
                HANDLE(std::ptr::null_mut()),
            );

            let handle = match handle {
                Ok(h) => h,
                Err(_) => return,
            };

            if handle.is_invalid() {
                return;
            }

            let mut journal_data = UsnJournalData::default();
            let mut bytes_returned = 0u32;

            let query_res = DeviceIoControl(
                handle,
                FSCTL_QUERY_USN_JOURNAL,
                None,
                0,
                Some(&mut journal_data as *mut _ as *mut _),
                std::mem::size_of::<UsnJournalData>() as u32,
                Some(&mut bytes_returned),
                None,
            );

            if query_res.is_err() {
                let _ = CloseHandle(handle);
                return;
            }

            let mut current_usn = journal_data.highest_usn;
            let journal_id = journal_data.usn_journal_id;
            let mut buffer = vec![0u8; 16 * 1024];

            while !stop_signal.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(500));

                let read_data = READ_USN_JOURNAL_DATA_V0 {
                    StartUsn: current_usn,
                    ReasonMask: 0xFFFFFFFF,
                    ReturnOnlyOnClose: 0,
                    Timeout: 0,
                    BytesToWaitFor: 0,
                    UsnJournalID: journal_id,
                };

                let mut out_bytes = 0u32;
                let ok = DeviceIoControl(
                    handle,
                    FSCTL_READ_USN_JOURNAL,
                    Some(&read_data as *const _ as *const _),
                    std::mem::size_of::<READ_USN_JOURNAL_DATA_V0>() as u32,
                    Some(buffer.as_mut_ptr() as *mut _),
                    buffer.len() as u32,
                    Some(&mut out_bytes),
                    None,
                );

                if ok.is_ok() && out_bytes > 8 {
                    let next_usn = *(buffer.as_ptr() as *const i64);
                    let mut offset = 8usize;

                    while offset + std::mem::size_of::<USN_RECORD_V2>() <= out_bytes as usize {
                        let rec = &*(buffer.as_ptr().add(offset) as *const USN_RECORD_V2);
                        let rec_len = rec.RecordLength as usize;
                        if rec_len == 0 {
                            break;
                        }

                        let name_offset = offset + rec.FileNameOffset as usize;
                        let name_chars = (rec.FileNameLength / 2) as usize;
                        if name_offset + (name_chars * 2) <= out_bytes as usize {
                            let name_slice = std::slice::from_raw_parts(
                                buffer.as_ptr().add(name_offset) as *const u16,
                                name_chars,
                            );
                            let name = String::from_utf16_lossy(name_slice);
                            let is_dir = (rec.FileAttributes & FILE_ATTRIBUTE_DIRECTORY.0) != 0;

                            if (rec.Reason & (USN_REASON_FILE_CREATE | USN_REASON_RENAME_NEW_NAME)) != 0 {
                                let _ = sender.send(FileSystemChange::CreatedOrModified {
                                    drive,
                                    name,
                                    parent_folder: format!("{}:\\", drive),
                                    is_dir,
                                });
                            } else if (rec.Reason & (USN_REASON_FILE_DELETE | USN_REASON_RENAME_OLD_NAME)) != 0 {
                                let _ = sender.send(FileSystemChange::Deleted {
                                    drive,
                                    name,
                                });
                            }
                        }

                        offset += rec_len;
                    }

                    current_usn = next_usn;
                }
            }

            let _ = CloseHandle(handle);
        }
    });
}

pub fn start_downloads_watcher(
    stop_signal: Arc<AtomicBool>,
    sender: std::sync::mpsc::Sender<FileSystemChange>,
) {
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let downloads = std::path::PathBuf::from(userprofile).join("Downloads");
        if downloads.exists() {
            std::thread::spawn(move || {
                let mut known_files: HashMap<String, u64> = HashMap::new();
                if let Ok(entries) = std::fs::read_dir(&downloads) {
                    for e in entries.flatten() {
                        let name = e.file_name().to_string_lossy().to_string();
                        let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                        known_files.insert(name, size);
                    }
                }

                while !stop_signal.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(1000));
                    if let Ok(entries) = std::fs::read_dir(&downloads) {
                        let mut current_files = HashMap::new();
                        for e in entries.flatten() {
                            let name = e.file_name().to_string_lossy().to_string();
                            let is_dir = e.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
                            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                            current_files.insert(name.clone(), size);

                            if !known_files.contains_key(&name) {
                                let _ = sender.send(FileSystemChange::CreatedOrModified {
                                    drive: downloads.to_string_lossy().chars().next().unwrap_or('C'),
                                    name,
                                    parent_folder: downloads.to_string_lossy().to_string(),
                                    is_dir,
                                });
                            }
                        }

                        for name in known_files.keys() {
                            if !current_files.contains_key(name) {
                                let _ = sender.send(FileSystemChange::Deleted {
                                    drive: downloads.to_string_lossy().chars().next().unwrap_or('C'),
                                    name: name.clone(),
                                });
                            }
                        }

                        known_files = current_files;
                    }
                }
            });
        }
    }
}


