use fxhash::FxHashSet;
use time::{Duration, UtcDateTime};
use windows::Win32::{Foundation::{LPARAM, LRESULT, WPARAM}, System::RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification}, UI::{Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFY_ICON_MESSAGE, NOTIFYICONDATAW, Shell_NotifyIconW},
    WindowsAndMessaging::{CreateWindowExW, EnumWindows, HICON, IDI_APPLICATION, KillTimer, LoadIconW, MB_OK, MessageBoxW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SetTimer, WINDOW_EX_STYLE, WM_CREATE, WM_DESTROY, WM_LBUTTONUP, WM_TIMER, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WS_POPUP, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK}}};
use windows_core::{BOOL, HRESULT, PCWSTR, w};

use super::*;
use crate::{lib_common::{Icon, RcValueRef, WSTR, WndMsgHandler, wnd_proc}, lib_ev::{evl_shutdown_abnormal, evl_shutdown_normal}, lib_log::Log, lib_window::WindowInfo, wnd_log::LogWnd};

const TASKTRAY_ICON_TEXT: PCWSTR = if cfg!(debug_assertions) {
    w!("applog_D")
} else {
    w!("applog")
};

const APP_ERROR_CAP: PCWSTR = w!("Application Monitor ERROR");

pub struct MainWnd {
    app: Weak<RefCell<App>>,
    handle: HWND,
    msg_taskbar_restart: u32,

    log: Log,
    last_shutdown_log_time: UtcDateTime,
    heartbeat_time: UtcDateTime,
    heart_beat_count: usize,

    hash_wnd_old: FxHashSet<usize>,
    hash_wnd_now: FxHashSet<usize>,
}

pub type MainWndWeak = Weak<MainWnd>;
pub type MainWndRc = Rc<MainWnd>;

impl RcValueRef<MainWnd> for MainWndRc {}

const MAIN_WINDOW_CLASS: PCWSTR = if cfg!(debug_assertions) {
    w!("applog_main_window_class_D")
} else { 
    w!("applog_main_window_class")
};

impl MainWnd {
    pub fn init(app:  Weak<RefCell<App>>) -> MainWndWeak {
        let wnd = Rc::new(Self {
            app: app,
            handle: Default::default(), // WM_NCCREATEの処理の中で設定される
            msg_taskbar_restart: 0,
            log: Default::default(),
            last_shutdown_log_time: UtcDateTime::now(),
            heartbeat_time: UtcDateTime::now(),
            heart_beat_count: 0,
            hash_wnd_old: Default::default(),
            hash_wnd_now: Default::default(),
        });

        let window_class = MAIN_WINDOW_CLASS;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpszClassName: window_class,
            lpfnWndProc: Some(wnd_proc::<MainWnd>),
            ..Default::default()
        };
        unsafe { RegisterClassExW(&wc) };
        
        let _ = unsafe { CreateWindowExW(WINDOW_EX_STYLE::default(), window_class, w!("MAIN-WINDOW"),  WS_POPUP,
            0, 0, 0, 0, None, None, None, Some(&wnd as *const _ as _)) };

        Rc::downgrade(&wnd)
    }

    pub fn check_instance() -> Result<HWND> {
        let mut ret: Result<HWND> = Err(Error::empty());
        let _ = unsafe { EnumWindows(Some(WindowInfo::enum_window_mine), LPARAM(&mut (&mut ret, &MAIN_WINDOW_CLASS) as *mut _ as _))};
        ret
    }

    pub fn notify_icon(&mut self, hwnd: HWND, nim: NOTIFY_ICON_MESSAGE) -> BOOL {
        let h = Icon(if nim == NIM_ADD {
            unsafe { LoadIconW(None, IDI_APPLICATION).unwrap() }
        } else { HICON::default() });
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hIcon: h.0,
            hWnd: hwnd,
            uCallbackMessage: WMU_TASKTRAY,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uID: ID_TASKTRAY,
            ..Default::default()
        };
        unsafe { std::ptr::copy(TASKTRAY_ICON_TEXT.as_ptr(),  &mut nid.szTip as _, TASKTRAY_ICON_TEXT.as_wide().len()) };
        unsafe { Shell_NotifyIconW(nim, &nid) }
    }

    fn log_purge_filter(v: &[String]) -> usize {
        let now = UtcDateTime::now();
        for (idx, l) in v.iter().enumerate() {
            let t = Log::parse_record_time(l).unwrap_or_else(|_| UtcDateTime::now());
            if now - t < Duration::days(90) { return idx }
        }
        0
    }

    fn log_init(&mut self) -> Result<()> {
        self.log = Log::new(MainWnd::log_purge_filter).unwrap();
        
        let r = self.log.load_tickfile();
        if let Err(e) = r {
            if e.kind().ne(&std::io::ErrorKind::NotFound) {
                return Err(Error::new(HRESULT::from_win32(e.raw_os_error().unwrap_or_default() as _), e.to_string()))
            }
        } else {
            let (last_shutdown_time, last_uptime) = r.unwrap();

            self.log.log_write(Some(last_uptime), "アプリモニタ：前回最終稼働").unwrap();

            let normal = evl_shutdown_normal(last_shutdown_time).unwrap();
            for t in normal.iter() {
                self.log.log_write(Some(*t), "前回PCシャットダウン").unwrap();
                self.last_shutdown_log_time = *t + Duration::seconds(10);
            }
            let abnormal = evl_shutdown_abnormal(last_shutdown_time).unwrap();
            for t in abnormal.iter() {
                self.log.log_write(Some(*t), "前回PCシャットダウン(異常) 日時は今回の起動時点").unwrap();
                self.last_shutdown_log_time = *t + Duration::seconds(10);
            }

            let num_shutdown = normal.len() + abnormal.len();
            if num_shutdown == 0 {
                self.log.log_write(None, "前回PCシャットダウンが検知不可").unwrap();
            } else if num_shutdown > 1 {
                self.log.log_write(None, "複数回のPCシャットダウンあり").unwrap();
            }
        }

        self.heartbeat_time = UtcDateTime::now();
        self.log.log_write(Some(self.heartbeat_time), "アプリモニタ：起動")?;
        self.log.store_tickfile(self.last_shutdown_log_time, self.heartbeat_time)?;
        Ok(())
    }
}

impl WndMsgHandler for MainWnd {
    fn handle(&self) -> HWND {
        self.handle
    }

    fn set_handle(&mut self, hwnd: HWND) {
        self.handle = hwnd;
    }

    fn message_handler(&mut self, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match message {
            WM_CREATE => {
                if let Err(e) = self.log_init() {
                    let emsg = WSTR::from(&e.to_string()).PCWSTR();
                    unsafe { MessageBoxW(None, emsg, APP_ERROR_CAP, MB_OK); }
                    return Some(LRESULT(-1))
                }
                let _ = unsafe { WTSRegisterSessionNotification(self.handle, NOTIFY_FOR_THIS_SESSION) };
                unsafe { SetTimer(Some(self.handle), 0, 6000, None); }
                self.msg_taskbar_restart = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
                let _ = self.notify_icon(self.handle, NIM_ADD);
            }
            WM_WTSSESSION_CHANGE => {
                if wparam.0 == WTS_SESSION_LOCK as usize {
                        let _ = self.log.log_write(None, "画面ロック");
                } else if wparam.0 == WTS_SESSION_UNLOCK as usize {
                        let _ = self.log.log_write(None, "画面ロック解除");
                }
            }
            WM_TIMER => {
                self.heart_beat_count += 1;
                if self.heart_beat_count > 9 {
                    self.heart_beat_count = 0;
                    let _ = self.log.store_tickfile(self.last_shutdown_log_time, UtcDateTime::now());
                }

                std::mem::swap(&mut self.hash_wnd_now, &mut self.hash_wnd_old);
                self.hash_wnd_now.clear();
                let _ = unsafe { EnumWindows(Some(WindowInfo::enum_window), LPARAM(&mut self.hash_wnd_now as *mut _ as _)) };
                for hwnd in self.hash_wnd_now.iter() {
                    if !self.hash_wnd_old.contains(&hwnd) {
                        let _ = self.log.log_write(None, &WindowInfo::window_process_name(HWND(*hwnd as _)));
                    }
                }
            }
            WMU_TASKTRAY => {
                if wparam.0 as u32 == ID_TASKTRAY && lparam.0 as u32 == WM_LBUTTONUP {
                    match self.log.log_load_all() {
                        Ok(data) => { 
                            let w = LogWnd::init(self.app.clone());
                            if let Some(rc) = w.upgrade().as_ref() { rc.set_item(&data); }
                        }
                        Err(e) => {
                            let emsg = WSTR::from(&e.to_string()).PCWSTR();
                            unsafe { MessageBoxW(None, emsg, APP_ERROR_CAP, MB_OK); }
                        }
                    }
                }
            }
            WM_DESTROY => {
                let _ = unsafe { KillTimer(Some(self.handle), 0) };
                let _ = unsafe { WTSUnRegisterSessionNotification(self.handle) };
                let _ = self.notify_icon(self.handle, NIM_DELETE);
                unsafe { PostQuitMessage(0); }
            }
            _ => {
                if message == self.msg_taskbar_restart {
                    let _ = self.notify_icon(self.handle, NIM_DELETE);
                    let _ = self.notify_icon(self.handle, NIM_ADD);
                }
            }
        }
        None
    }
}
