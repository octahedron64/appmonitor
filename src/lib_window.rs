use std::sync::LazyLock;
use fxhash::FxHashSet;
use windows::Win32::{
    Foundation::{FALSE, LPARAM, TRUE}, Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute}, System::Threading::{OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW}, UI::{Input::KeyboardAndMouse::IsWindowEnabled, WindowsAndMessaging::{GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible}, }
};
use windows_core::{BOOL, PCWSTR, PWSTR};

use crate::lib_common::{Handle, WSTR};

use super::*;

const WINLIST_IGNORE: [&str; 2] = ["Progman", "Internet Explorer_Hidden"];
static WINLIST_IGNORE_U16: LazyLock<[Vec<u16>; 2]> = LazyLock::new(||{
    [WINLIST_IGNORE[0].encode_utf16().collect(), WINLIST_IGNORE[1].encode_utf16().collect(),]
});

#[derive(Clone, Debug)]
pub struct WindowInfo{}

impl WindowInfo {
    pub extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if !unsafe { IsWindowVisible(hwnd).into() } || !unsafe { IsWindowEnabled(hwnd).into() } {
            return TRUE
        }

        let mut b = FALSE;
        let r = unsafe {DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED,
            &mut b as *mut _ as _, std::mem::size_of::<BOOL>() as u32) }; 
        if r.is_ok() && b.into() { return TRUE }

        let mut buf = [0u16; 512];

        let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
        if len == 0 { return TRUE }

        let len = unsafe { GetClassNameW(hwnd, &mut buf) };
        for title_ignore in WINLIST_IGNORE_U16.iter() {
            if title_ignore.eq(&buf[..len as usize]) { return TRUE }
        }

        let h = unsafe { &mut *(lparam.0 as *mut FxHashSet<usize>) };
        h.insert(hwnd.0.addr());
        TRUE
    }

    pub extern "system" fn enum_window_mine(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let mut buf = [0u16; 64];
        let ret_len = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
        if ret_len > 0 {
            let (mine, myclass) =  unsafe { &mut *(lparam.0 as *mut (&mut Result<HWND>, &PCWSTR)) };
            if buf[..ret_len].eq(unsafe { myclass.as_wide() }) {
                **mine = Ok(hwnd);
                return FALSE
            }
        }
        TRUE
    }

    pub fn window_process_name(hwnd: HWND) -> String {
        let mut pid = 0u32;
        let r = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if r == 0 { return String::default() }

        let hp = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.unwrap_or_default()); // auto drop resource
        if hp.0.is_invalid() { return String::default() }

        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        if unsafe { QueryFullProcessImageNameW(hp.0, PROCESS_NAME_FORMAT(0), PWSTR::from_raw(&mut buf as _), &mut len) }.is_err() {
            return String::default()
        }
        if len == 0 { return String::default() }

        let s = WSTR::from_slice_to_string(&buf, len as usize);
        let idx = if let Some(i) = s.rfind("\\") { if i < s.len() { i + 1 }  else { i } }else { 0 };
        s[idx..].to_string()
    }
}