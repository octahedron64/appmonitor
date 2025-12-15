use std::rc::{Rc, Weak};
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        UI::{WindowsAndMessaging::*},
        System::Registry::*, Graphics::Gdi::{HFONT, DeleteObject},
    },
};

pub trait RcValueRef<T> {
    fn get_ref(&self) -> &T {
        unsafe { &*Rc::<T>::as_ptr(&*(self as *const _ as *const Rc<T>)) }
    }
    fn get_mut(&mut self) -> &mut T {
        unsafe { &mut*(Rc::<T>::as_ptr(&*(self as *const _ as *const Rc<T>)) as *mut _) }
    }
}

pub trait WndMsgHandler {
    fn handle(&self) -> HWND;
    fn set_handle(&mut self, hwnd: HWND);
    fn message_handler(&mut self, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT>;
}

pub extern "system" fn wnd_proc<T: WndMsgHandler>(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let mut w = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut T;
        if w.is_null() {
            if message == WM_NCCREATE {
                let rc = (*(lparam.0 as *const CREATESTRUCTW)).lpCreateParams as *const Rc<T>;
                let raw = Rc::into_raw((*rc).clone()); // SetWindowLongPtrW内に生as *mut Tポインタを格納しDrop回避
                SetWindowLongPtrW(window, GWLP_USERDATA, raw as _);
                w = raw as *mut T;
                (*w).set_handle(window);
            } else {
                return DefWindowProcW(window, message, wparam, lparam)
            }
        }

        let r = (*w).message_handler(message, wparam, lparam);
        if message == WM_NCDESTROY { Rc::from_raw(w); } // SetWindowLongPtrW内の生ポインタからRcを復帰してDrop
        r.unwrap_or_else(|| DefWindowProcW(window, message, wparam, lparam))
    }
}

pub fn wnd_instance<T>(h: HWND) -> Weak<T> {
    let w = unsafe { GetWindowLongPtrW(h, GWLP_USERDATA) } as *mut T;
    if !w.is_null() {
        let rc: Rc<T> = unsafe { Rc::from_raw(w) };
        let r: Weak<T> = Rc::downgrade(&rc);
        let _ = Rc::into_raw(rc);
        r
    } else {
        Weak::default()
    }
}

pub struct Handle (pub HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_invalid() { let _ = unsafe { CloseHandle(self.0) }; }
    }    
}

pub struct Menu (pub HMENU);
impl Drop for Menu {
    fn drop(&mut self) {
        if !self.0.is_invalid() { let _ = unsafe { DestroyMenu(self.0) }; }
    }    
}

pub struct Font(pub HFONT);
impl Drop for Font {
    fn drop(&mut self) {
        if !self.0.is_invalid() { let _ = unsafe { DeleteObject(self.0.into()) }; }
    }
}

pub struct Icon(pub HICON);
impl Drop for Icon {
    fn drop(&mut self) {
        if !self.0.is_invalid() { let _ = unsafe { DestroyIcon(self.0) }; }
    }
}

pub struct RegKey (pub HKEY);
impl Drop for RegKey {
    fn drop(&mut self) {
        if !self.0.is_invalid() { let _ = unsafe { RegCloseKey(self.0) }; }
    }    
}

pub fn delete_reg(key: &str, vname: &str) -> Result<()> {
    let mut h = RegKey(HKEY::default()); // auto drop resouce
    unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, WSTR::from(key).PCWSTR(), None,
        None, REG_OPTION_NON_VOLATILE, KEY_READ | KEY_WRITE, None, &mut h.0, None) }.ok()?;

    let val_name = WSTR::from(vname);
    unsafe { RegDeleteKeyValueW(h.0, None, val_name.PCWSTR()).ok() }
}

pub fn load_reg_sz(key: &str, vname: &str) -> String {
    let mut h = RegKey(HKEY::default()); // auto drop resouce
    let r = unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, WSTR::from(key).PCWSTR(), None,
        None, REG_OPTION_NON_VOLATILE, KEY_READ, None, &mut h.0, None) };
    if r.is_err() {
        return String::default()
    }

    let mut bufsize: u32 = 0;
    let val_name = WSTR::from(vname);
    let r = unsafe { RegQueryValueExW(h.0, val_name.PCWSTR(), None, None, None, Some(&mut bufsize)) };
    if r.is_err() {
        return String::default()
    }

    let mut buf = WSTR::new((bufsize / 2) as usize); // bufサイズはバイト数で返ってくるが、utf-16文字数としては半分になる
    let r = unsafe { RegQueryValueExW(h.0, val_name.PCWSTR(), None, None, Some(buf.byte_ptr()), Some(&mut bufsize)) };
    if r.is_err() {
        return  String::default()
    }
    
    buf.to_string((bufsize / 2 - 1) as usize) // -1：終端NULL処理不要
}

pub fn store_reg_sz(key: &str, vname: &str, val: &str) -> Result<()> {
    let mut h = RegKey(HKEY::default()); // auto drop resouce
    unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, WSTR::from(key).PCWSTR(), None,
        None, REG_OPTION_NON_VOLATILE, KEY_READ | KEY_WRITE, None, &mut h.0, None) }.ok()?;

    let val_name = WSTR::from(vname);
    let buf = WSTR::from(val);
    unsafe { RegSetValueExW(h.0, val_name.PCWSTR(), None, REG_SZ, Some(&buf.to_vec_byte())).ok() }
}

pub fn load_reg_multi_sz(key: &str, vname: &str, vec: &mut Vec<String>) {
    let mut h = RegKey(HKEY::default()); // auto drop resouce
    let r = unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, WSTR::from(key).PCWSTR(), None,
        None, REG_OPTION_NON_VOLATILE, KEY_READ, None, &mut h.0, None) };
    if r.is_err() { return }

    let mut bufsize: u32 = 0;
    let val_name = WSTR::from(vname);
    let r = unsafe { RegQueryValueExW(h.0, val_name.PCWSTR(), None, None, None, Some(&mut bufsize)) };
    if r.is_err() || bufsize <= 3 { return }

    let mut buf = WSTR::new((bufsize / 2) as usize); // bufサイズはバイト数で返ってくるが、utf-16文字数としては半分になる
    let r = unsafe { RegQueryValueExW(h.0, val_name.PCWSTR(), None, None, Some(buf.byte_ptr()), Some(&mut bufsize)) };
    if r.is_err() { return }

    let l = buf.to_string(((bufsize - 4) / 2) as usize); // -2：終端NULL処理不要
    for i  in l.split('\0') {
        vec.push(i.to_string());
    }
}

pub fn store_reg_multi_sz(key: &str, vname: &str, vec: &Vec<String>) -> Result<()> {
    let mut h = RegKey(HKEY::default()); // auto drop resouce
    unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, WSTR::from(key).PCWSTR(), None,
        None, REG_OPTION_NON_VOLATILE, KEY_READ | KEY_WRITE, None, &mut h.0, None) }.ok()?;

    let mut buf = WSTR::new(1); // null終端のみ
    for i in vec {
        buf.append(i);
        buf.append(&String::from_utf16_lossy(&[0u16]));
    }

    let val_name = WSTR::from(vname);
    unsafe { RegSetValueExW(h.0, val_name.PCWSTR(), None, REG_MULTI_SZ, Some(&buf.to_vec_byte())).ok() }
}

pub struct WSTR (pub Vec<u16>);

impl WSTR {
    pub fn new(charnum: usize) -> Self { // 指定文字数分のメモリを確保し、先頭にNULL終端を付加
        debug_assert!(charnum > 0);
        let mut b = Vec::with_capacity(charnum);
        unsafe {b.set_len(charnum);}
        b[0] = 0u16;
        Self(b)
    }

    pub fn from(s: &str) -> Self {
        Self(s.encode_utf16().chain(Some(0u16)).collect()) // NULL終端を追加
    }

    pub fn from_ptr_null_search(p: *const u16) -> Self {
        let mut buf = Vec::<u16>::default();
        for i in 0 .. { unsafe {
            buf.push(*p.add(i));
            if *p.add(i) == 0 { break }
        }}
        Self(buf)
    }

    pub fn from_slice_null_search(v: &[u16]) -> Self {
        let mut buf = Vec::<u16>::with_capacity(v.len());
        if v.len() == 0 {
            buf.push(0u16);
        } else { 'block: {
            for i in 0..v.len() {            
                buf.push(v[i]);
                if v[i] == 0u16 { break 'block; }
            }
            debug_assert!(false);
        }}
        Self(buf)
    }

    pub fn from_slice_to_string(buf: &[u16], charnum: usize) -> String {
        String::from_utf16_lossy(&buf[..charnum])
    }

    pub fn from_slice_to_string_null_search(buf: &[u16]) -> String {
        for i in 0..buf.len() {
            if buf[i] == 0u16 { return String::from_utf16_lossy(&buf[..i]) }
        }
        debug_assert!(false);
        String::default()
    }

    pub fn to_string(&self, charnum: usize) -> String {
        Self::from_slice_to_string(&self.0, charnum)
    }

    pub fn to_string_null_search(&self) -> String {
        Self::from_slice_to_string_null_search(&self.0)
    }

    pub fn append(&mut self, s: &str) {
        let mut a: Vec<u16> = s.encode_utf16().chain(Some(0u16)).collect(); // NULL終端を追加
        self.0.pop(); // NULL終端を取り除く
        self.0.append(&mut a);
    }

    pub fn find_char(&self, c: char) -> Option<usize> {
        let mut buf = [0u16; 2];
        let ch = c.encode_utf16(&mut buf)[0];
        for i in 0..self.0.len() {
            if ch.eq(&self.0[i]) { return Some(i) }
        }
        None
    }

    pub fn rfind_char(&self, c: char) -> Option<usize> {
        let mut buf = [0u16; 2];
        let ch = c.encode_utf16(&mut buf)[0];
        for i in (0..self.0.len()).rev() {
            if ch.eq(&self.0[i]) { return Some(i) }
        }
        None
    }

    pub fn byte_ptr(&mut self) -> *mut u8 {
        &mut self.0[0] as *mut _ as _
    }

    pub fn to_vec_byte(&self) -> Vec<u8> {
        let mut v = Vec::<u8>::with_capacity(self.0.len() * 2);
        self.0.iter().for_each(|i| 
            i.to_le_bytes().iter().for_each(|b| v.push(*b)));
        v
    }

    #[allow(non_snake_case)]
    pub fn PCWSTR(&self) -> PCWSTR {
        PCWSTR::from_raw(&self.0[0])
    }

    #[allow(non_snake_case)]
    pub fn PWSTR(&mut self) -> PWSTR {
        PWSTR::from_raw(&mut self.0[0])
    }
}