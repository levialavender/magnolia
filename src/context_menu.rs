use std::path::Path;
use std::sync::Once;
use windows::core::{w, Interface, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Com::IBindCtx;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
use windows::Win32::UI::Shell::{
    BHID_SFUIObject, CMINVOKECOMMANDINFO, IContextMenu, IContextMenu2, IContextMenu3, IShellItem,
    SHCreateItemFromParsingName, CMF_CANRENAME, CMF_EXPLORE, CMF_NORMAL, GCS_VERBA,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, GetCursorPos,
    GetForegroundWindow, GetWindowLongPtrW, PostMessageW, RegisterClassW, SetForegroundWindow,
    SetWindowLongPtrW, TrackPopupMenu, CS_OWNDC, GWLP_USERDATA, TPM_LEFTALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR,
    WM_NULL, WNDCLASSW, WS_POPUP,
};

static REGISTER_CLASS_ONCE: Once = Once::new();
const MENU_CLASS_NAME: windows::core::PCWSTR = w!("MagnoliaShellMenuHost");

struct MenuContext {
    cm2: Option<IContextMenu2>,
    cm3: Option<IContextMenu3>,
}

unsafe extern "system" fn menu_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MenuContext;
    if !ptr.is_null() {
        let ctx = &*ptr;
        if let Some(ref cm3) = ctx.cm3 {
            let mut lresult = LRESULT(0);
            if cm3.HandleMenuMsg2(msg, wparam, lparam, Some(&mut lresult)).is_ok() {
                return lresult;
            }
        } else if let Some(ref cm2) = ctx.cm2 {
            if matches!(msg, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
                if cm2.HandleMenuMsg(msg, wparam, lparam).is_ok() {
                    return LRESULT(0);
                }
            }
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

pub fn show_native_context_menu<P: AsRef<Path>, F: FnOnce() + Send + 'static>(
    file_path: P,
    on_rename: F,
) {
    let path = file_path.as_ref();
    let path_str = path.to_string_lossy();
    let wide_path: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();

    let mut pt = POINT::default();
    let _ = unsafe { GetCursorPos(&mut pt) };
    let main_hwnd_raw = unsafe { GetForegroundWindow().0 as usize };

    // Dedicated STA thread initialized with OleInitialize for Shell COM/drag-drop/clipboard
    std::thread::spawn(move || unsafe {
        let _ = OleInitialize(None);
        let main_hwnd = HWND(main_hwnd_raw as *mut _);

        let hinstance = GetModuleHandleW(None).unwrap();

        REGISTER_CLASS_ONCE.call_once(|| {
            let wnd_class = WNDCLASSW {
                style: CS_OWNDC,
                lpfnWndProc: Some(menu_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: MENU_CLASS_NAME,
                ..Default::default()
            };
            let _ = RegisterClassW(&wnd_class);
        });

        let thread_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            MENU_CLASS_NAME,
            w!("MagnoliaShellMenuWindow"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            HWND(std::ptr::null_mut()),
            None,
            hinstance,
            None,
        );

        let thread_hwnd = match thread_hwnd {
            Ok(h) => h,
            Err(_) => {
                OleUninitialize();
                return;
            }
        };

        let item_res: windows::core::Result<IShellItem> =
            SHCreateItemFromParsingName(PCWSTR(wide_path.as_ptr()), None::<&IBindCtx>);

        if let Ok(item) = item_res {
            let menu_res: windows::core::Result<IContextMenu> = item.BindToHandler(None, &BHID_SFUIObject);
            if let Ok(context_menu) = menu_res {
                let cm2: Option<IContextMenu2> = context_menu.cast().ok();
                let cm3: Option<IContextMenu3> = context_menu.cast().ok();

                let mut ctx = MenuContext { cm2, cm3 };
                SetWindowLongPtrW(thread_hwnd, GWLP_USERDATA, &mut ctx as *mut _ as isize);

                if let Ok(hmenu) = CreatePopupMenu() {
                    if !hmenu.is_invalid() {
                        let flags = CMF_NORMAL | CMF_EXPLORE | CMF_CANRENAME;
                        let _ = context_menu.QueryContextMenu(hmenu, 0, 1, 0x7FFF, flags);

                        let _ = SetForegroundWindow(thread_hwnd);

                        let cmd = TrackPopupMenu(
                            hmenu,
                            TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
                            pt.x,
                            pt.y,
                            0,
                            thread_hwnd,
                            None,
                        );

                        let _ = PostMessageW(thread_hwnd, WM_NULL, None, None);

                        if cmd.0 > 0 {
                            let mut buf = [0u8; 64];
                            let is_rename = context_menu
                                .GetCommandString(
                                    (cmd.0 - 1) as usize,
                                    GCS_VERBA,
                                    None,
                                    windows::core::PSTR(buf.as_mut_ptr()),
                                    buf.len() as u32,
                                )
                                .ok()
                                .map(|_| {
                                    let s = windows::core::PCSTR(buf.as_ptr()).to_string().unwrap_or_default();
                                    s.eq_ignore_ascii_case("rename")
                                })
                                .unwrap_or(false);

                            if is_rename {
                                on_rename();
                            } else {
                                let mut invoke_info = CMINVOKECOMMANDINFO::default();
                                invoke_info.cbSize = std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32;
                                invoke_info.fMask = 0;
                                invoke_info.hwnd = main_hwnd;
                                invoke_info.lpVerb = windows::core::PCSTR((cmd.0 - 1) as usize as *const u8);
                                invoke_info.nShow = 1;
                                let _ = context_menu.InvokeCommand(&invoke_info);
                            }
                        }
                        let _ = DestroyMenu(hmenu);
                    }
                }
            }
        }

        let _ = DestroyWindow(thread_hwnd);
        OleUninitialize();
    });
}


