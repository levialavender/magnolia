use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetLogicalDrives, FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL};
use windows::Win32::System::IO::DeviceIoControl;

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct UsnJournalData {
    pub usn_journal_id: u64,
    pub lowest_valid_usn: i64,
    pub highest_usn: i64,
    pub maximum_size: u64,
    pub allocation_delta: u64,
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct ReadUsnJournalDataV0 {
    pub start_usn: i64,
    pub reason_mask: u32,
    pub return_only_on_close: u32,
    pub timeout: u64,
    pub bytes_to_wait_for: u64,
    pub usn_journal_id: u64,
}

#[derive(Debug, Clone)]
pub enum UsnChange {
    AddedOrModified(String),
    Removed(String),
}

pub struct UsnMonitor {
    drive_letter: char,
    journal_id: u64,
    next_usn: i64,
}

impl UsnMonitor {
    pub fn new(drive_letter: char) -> Result<Self, String> {
        let mut monitor = Self {
            drive_letter,
            journal_id: 0,
            next_usn: 0,
        };
        let journal = monitor.query_journal()?;
        monitor.journal_id = journal.usn_journal_id;
        monitor.next_usn = journal.highest_usn;
        Ok(monitor)
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

    pub fn read_recent_changes(&mut self) -> Result<Vec<UsnChange>, String> {
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
                Err(e) => return Err(format!("Failed to open volume: {:?}", e)),
            };

            let read_opts = ReadUsnJournalDataV0 {
                start_usn: self.next_usn,
                reason_mask: 0xFFFFFFFF,
                return_only_on_close: 0,
                timeout: 0,
                bytes_to_wait_for: 0,
                usn_journal_id: self.journal_id,
            };

            let mut buffer = vec![0u8; 65536];
            let mut bytes_returned: u32 = 0;

            let res = DeviceIoControl(
                handle,
                FSCTL_READ_USN_JOURNAL,
                Some(&read_opts as *const _ as *const _),
                std::mem::size_of::<ReadUsnJournalDataV0>() as u32,
                Some(buffer.as_mut_ptr() as *mut _),
                buffer.len() as u32,
                Some(&mut bytes_returned),
                None,
            );

            let _ = CloseHandle(handle);

            if res.is_err() || bytes_returned < 8 {
                return Ok(Vec::new());
            }

            // First 8 bytes of output is next USN
            let next_usn = *(buffer.as_ptr() as *const i64);
            self.next_usn = next_usn;

            let mut changes = Vec::new();
            let mut offset = 8usize;

            while offset + 60 <= bytes_returned as usize {
                let rec_len = *(buffer.as_ptr().add(offset) as *const u32) as usize;
                if rec_len == 0 || offset + rec_len > bytes_returned as usize {
                    break;
                }

                let reason = *(buffer.as_ptr().add(offset + 40) as *const u32);
                let fn_len = *(buffer.as_ptr().add(offset + 56) as *const u16) as usize;
                let fn_offset = *(buffer.as_ptr().add(offset + 58) as *const u16) as usize;

                if offset + fn_offset + fn_len <= bytes_returned as usize && fn_len > 0 {
                    let name_slice = std::slice::from_raw_parts(
                        buffer.as_ptr().add(offset + fn_offset) as *const u16,
                        fn_len / 2,
                    );
                    let file_name = String::from_utf16_lossy(name_slice);

                    // USN reason masks:
                    // 0x00000001 (DATA_OVERWRITE)
                    // 0x00000002 (DATA_EXTEND)
                    // 0x00000100 (FILE_CREATE)
                    // 0x00000200 (FILE_DELETE)
                    // 0x00002000 (RENAME_NEW_NAME)
                    // 0x00001000 (RENAME_OLD_NAME)
                    // 0x80000000 (CLOSE)
                    if (reason & 0x00000200) != 0 || (reason & 0x00001000) != 0 {
                        changes.push(UsnChange::Removed(file_name));
                    } else if (reason & 0x00000100) != 0 || (reason & 0x00002000) != 0 || (reason & 0x80000000) != 0 {
                        changes.push(UsnChange::AddedOrModified(file_name));
                    }
                }

                offset += rec_len;
            }

            Ok(changes)
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

