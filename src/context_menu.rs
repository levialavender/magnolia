use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::POINT;
use windows::Win32::System::Com::IBindCtx;
use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
use windows::Win32::UI::Shell::{
    BHID_SFUIObject, IContextMenu, IShellItem, SHCreateItemFromParsingName, CMF_EXPLORE,
    CMF_NORMAL, CMINVOKECOMMANDINFO,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, GetCursorPos, GetForegroundWindow, PostMessageW,
    SetForegroundWindow, TrackPopupMenu, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_NULL,
};

pub fn show_native_context_menu<P: AsRef<Path>>(file_path: P) {
    let path = file_path.as_ref();
    let path_str = path.to_string_lossy();
    let wide_path: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();

    let mut pt = POINT::default();
    let _ = unsafe { GetCursorPos(&mut pt) };
    let active_hwnd_raw = unsafe { GetForegroundWindow().0 as usize };

    // Spawn on a dedicated STA thread initialized with OleInitialize for Shell COM/drag-drop/clipboard
    std::thread::spawn(move || unsafe {
        let _ = OleInitialize(None);
        let active_hwnd = windows::Win32::Foundation::HWND(active_hwnd_raw as *mut _);

        let item_res: windows::core::Result<IShellItem> =
            SHCreateItemFromParsingName(PCWSTR(wide_path.as_ptr()), None::<&IBindCtx>);

        if let Ok(item) = item_res {
            let menu_res: windows::core::Result<IContextMenu> = item.BindToHandler(None, &BHID_SFUIObject);
            if let Ok(context_menu) = menu_res {
                if let Ok(hmenu) = CreatePopupMenu() {
                    if !hmenu.is_invalid() {
                        let flags = CMF_NORMAL | CMF_EXPLORE;
                        let _ = context_menu.QueryContextMenu(hmenu, 0, 1, 0x7FFF, flags);

                        // SetForegroundWindow is strictly required by Windows for TrackPopupMenu
                        if !active_hwnd.is_invalid() {
                            let _ = SetForegroundWindow(active_hwnd);
                        }

                        let cmd = TrackPopupMenu(
                            hmenu,
                            TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
                            pt.x,
                            pt.y,
                            0,
                            active_hwnd,
                            None,
                        );

                        // PostMessage WM_NULL forces the menu message queue to flush properly
                        if !active_hwnd.is_invalid() {
                            let _ = PostMessageW(active_hwnd, WM_NULL, None, None);
                        }

                        if cmd.0 > 0 {
                            let mut invoke_info = CMINVOKECOMMANDINFO::default();
                            invoke_info.cbSize = std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32;
                            invoke_info.fMask = 0;
                            invoke_info.hwnd = active_hwnd;
                            invoke_info.lpVerb = windows::core::PCSTR((cmd.0 - 1) as usize as *const u8);
                            invoke_info.nShow = 1;
                            let _ = context_menu.InvokeCommand(&invoke_info);
                        }
                        let _ = DestroyMenu(hmenu);
                    }
                }
            }
        }

        OleUninitialize();
    });
}


