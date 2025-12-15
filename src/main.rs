#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[allow(dead_code)]
mod lib_common;
mod lib_ev;
mod lib_log;
mod lib_window;
mod wnd_main;
mod wnd_log;

use std::{cell::RefCell, rc::{Rc, Weak}};

use windows::Win32::{
    Foundation::HWND, UI::{Controls::{ICC_BAR_CLASSES, ICC_LISTVIEW_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx}, WindowsAndMessaging::{DispatchMessageW, GetMessageW, IsDialogMessageW, MSG, SendMessageW, TranslateMessage, WM_HOTKEY, WM_USER}}
};
use windows_core::{Error, Result};

use crate::{lib_common::WndMsgHandler, wnd_main::{MainWnd, MainWndRc, MainWndWeak}};

const WMU_TASKTRAY: u32 = WM_USER + 101;
const ID_TASKTRAY: u32 = 0;

fn main() -> Result<()> {
    AppRc::init().run()
}

pub struct App {
    main_wnd: MainWndWeak,
    dlg_wnd: Option<HWND>,
}

pub type AppRef = RefCell<App>;
pub type AppWeak = Weak<AppRef>;
pub type AppRc = Rc<AppRef>;

pub trait AppBehavior {
    fn init() -> AppRc;
    fn main_wnd(&self) -> MainWndRc;
    // fn check_previous_instance() -> Result<()>;
    fn run(&mut self) -> Result<()>;
}

impl AppBehavior for AppRc {
    fn init() -> AppRc {
        Rc::new(RefCell::new(App {
            main_wnd: MainWndWeak::default(),
            dlg_wnd: None,
        }))
    }

    fn main_wnd(&self) -> MainWndRc {
        self.borrow().main_wnd.upgrade().unwrap()
    }

    fn run(&mut self) -> Result<()> {
        if MainWnd::check_instance().is_ok() { return Ok(()) }

        unsafe {
            let mut icc = INITCOMMONCONTROLSEX::default();
            icc.dwSize = std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32;
            icc.dwICC = ICC_BAR_CLASSES | ICC_LISTVIEW_CLASSES;
            let _ = InitCommonControlsEx(&icc);
        }
        
        self.borrow_mut().main_wnd = MainWnd::init(Rc::downgrade(&self));

        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.into() {
            if message.message == WM_HOTKEY {
                let handle = self.main_wnd().handle();
                unsafe { SendMessageW(handle, message.message, Some(message.wParam), Some(message.lParam)); }
            }
            if self.borrow().dlg_wnd.is_some() {
                let hdlg = self.borrow().dlg_wnd.unwrap();
                if unsafe {IsDialogMessageW(hdlg, &message)}.into() { continue; }
            }
            let _ = unsafe { TranslateMessage(&message) };
            unsafe { DispatchMessageW(&message); }
        }

        Ok(())
    }
}
