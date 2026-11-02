use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetLogicalDrives, FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::FSCTL_QUERY_USN_JOURNAL;
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

