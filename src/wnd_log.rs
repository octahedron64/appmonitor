use std::{cell::RefCell, rc::{Rc, Weak}};

use windows::Win32::{Foundation::{FALSE, HWND, LPARAM, LRESULT, TRUE, WPARAM}, Graphics::Gdi::{COLOR_3DFACE, GetSysColorBrush}, UI::{Controls::{LVCF_FMT, LVCF_SUBITEM, LVCF_TEXT, LVCF_WIDTH, LVCFMT_LEFT, LVCOLUMNW, LVIF_TEXT, LVITEMW, LVM_INSERTCOLUMNW, LVM_INSERTITEMW, LVM_SETEXTENDEDLISTVIEWSTYLE, LVM_SETITEMW, LVS_EX_DOUBLEBUFFER, LVS_EX_FULLROWSELECT, LVS_NOSORTHEADER, LVS_REPORT, LVS_SHOWSELALWAYS, WC_LISTVIEWW}, WindowsAndMessaging::{CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, EnumChildWindows, GetDlgCtrlID, HMENU, IDC_ARROW, LoadCursorW, RegisterClassExW, SWP_NOMOVE, SWP_NOOWNERZORDER, SendMessageW, SetWindowPos, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CREATE, WM_DESTROY, WM_SIZE, WNDCLASSEXW, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_VISIBLE}}};
use windows_core::{BOOL, w};

use crate::{App, AppWeak, lib_common::{RcValueRef, WSTR, WndMsgHandler, wnd_proc}};

pub struct LogWnd {
    _app: AppWeak,
    handle: HWND,
}

pub type LogWndWeak = Weak<LogWnd>;
pub type LogWndRc = Rc<LogWnd>;

impl RcValueRef<LogWnd> for LogWndRc {}

impl WndMsgHandler for LogWnd {
    fn handle(&self) -> HWND { self.handle }

    fn set_handle(&mut self, hwnd: HWND) { self.handle = hwnd; }

    fn message_handler(&mut self, message: u32, _wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match message {
            WM_CREATE => return self.on_create(lparam),
            WM_SIZE => return self.on_size(lparam.0 as i32 & u16::MAX as i32, lparam.0 as i32 >> u16::BITS),
            WM_DESTROY => return self.on_destroy(),
            _ => {}
        }
        None
    }
}

impl LogWnd {
    pub fn init(app:  Weak<RefCell<App>>) -> LogWndWeak {
        let wnd = Rc::new(Self {
            _app: app,
            handle: Default::default(), // WM_NCCREATEの処理の中で設定される
        });

        let window_class = w!("applog");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpszClassName: window_class,
            lpfnWndProc: Some(wnd_proc::<LogWnd>),
            style: CS_HREDRAW | CS_VREDRAW,
            hbrBackground: unsafe { GetSysColorBrush(COLOR_3DFACE) },
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap(),
            ..Default::default()
        };
        unsafe { RegisterClassExW(&wc) };
        
        let _ = unsafe { CreateWindowExW(WINDOW_EX_STYLE::default(), window_class, w!("Application Monitoring"),  WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100, 100, CW_USEDEFAULT, CW_USEDEFAULT, None, None, None, Some(&wnd as *const _ as _)) };

        Rc::downgrade(&wnd)
    }

    fn on_create(&mut self, _lparam: LPARAM) -> Option<LRESULT> {
        let h = unsafe { CreateWindowExW(WINDOW_EX_STYLE::default(), WC_LISTVIEWW, None,
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(LVS_REPORT | LVS_SHOWSELALWAYS | LVS_NOSORTHEADER), 0, 0, 0, 0, Some(self.handle), Some(HMENU(1 as _)), None, None) };
        let h = if h.is_ok() { h.unwrap() } else { return None };

        unsafe { SendMessageW(h, LVM_SETEXTENDEDLISTVIEWSTYLE, Some(WPARAM(0)), Some(LPARAM((LVS_EX_DOUBLEBUFFER | LVS_EX_FULLROWSELECT) as _))) };
        
        let mut header = ["Time", "Message"].map(|i| WSTR::from(i));
        let header_size = [150, 600];

        let mut col = LVCOLUMNW::default();
        col.mask =  LVCF_FMT | LVCF_WIDTH | LVCF_TEXT | LVCF_SUBITEM;
        col.fmt = LVCFMT_LEFT;
        for i in 0 .. header.len() {
            col.pszText = header[i].PWSTR();
            col.cx = header_size[i];
            col.iSubItem = i as _;
            unsafe { SendMessageW(h, LVM_INSERTCOLUMNW,Some(WPARAM(i)), Some(LPARAM(&col as *const _ as _))) };    
        }
        Some(LRESULT::default())
    }

    fn on_size(&mut self, w: i32, h: i32) -> Option<LRESULT> {
        let hroot = get_ctrl(self.handle, 1);        
        let _ = unsafe { SetWindowPos(hroot, None, 0, 0, w, h, SWP_NOOWNERZORDER | SWP_NOMOVE) };
        Some(LRESULT::default())
    }

    fn on_destroy(&mut self) -> Option<LRESULT> {
        Some(LRESULT::default())
    }

    pub fn set_item(&self, v: &Vec<String>) {
        let h = get_ctrl(self.handle, 1);

        let mut item = LVITEMW::default();
        item.mask = LVIF_TEXT;
        for (idx, l) in v.iter().rev().enumerate() {
            let (time, msg) = l.split_once("\t").unwrap_or((Default::default(), l));

            item.iItem = idx as _;
            item.iSubItem = 0;
            let mut txt= WSTR::from(time);
            item.pszText = txt.PWSTR();
            unsafe { SendMessageW(h, LVM_INSERTITEMW,None, Some(LPARAM(&item as *const _ as _))) };    

            item.iSubItem = 1;
            let mut txt= WSTR::from(msg);
            item.pszText = txt.PWSTR();
            unsafe { SendMessageW(h, LVM_SETITEMW,None, Some(LPARAM(&item as *const _ as _))) };    
        }
    }
}

extern "system" fn enum_child(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let (id, hwndreturn) = unsafe { &mut *(lparam.0 as *mut (isize, &mut HWND)) };
    if unsafe { GetDlgCtrlID(hwnd) } != *id as i32 { return TRUE }
    **hwndreturn = hwnd;
    FALSE
}

pub fn get_ctrl(hwnd_root: HWND, id: isize) -> HWND {
    let mut hwnd = HWND::default();
    let _ = unsafe { EnumChildWindows(Some(hwnd_root), Some(enum_child), LPARAM(&mut (id, &mut hwnd) as *mut _ as _)) };
    hwnd
}