

use time::{UtcDateTime, format_description};
use windows::{
    core::{Error, HSTRING},
    Win32::{
        Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS},
        System::EventLog::{
            EvtClose, EvtNext, EvtQuery, EvtRender, EVT_HANDLE,
            EvtQueryChannelPath, EvtRenderEventXml,
        },
    },
};
use windows_core::HRESULT;

fn render_event_xml(h_event: EVT_HANDLE) -> Result<String, Error> {
    let mut used: u32 = 0;
    let mut count: u32 = 0;
    unsafe {
        let res = EvtRender(
            None,
            h_event,
            EvtRenderEventXml.0,
            0,
            None,
            &mut used,
            &mut count,
        );
        if res.is_ok() {
            // 必要サイズ 0 で成功することはないはず
        } else if res.as_ref().unwrap_err().code().ne(&HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0)) {
            return Err(res.unwrap_err());
        }
    }

    // UTF-16 バッファを割り当て（used はバイト単位なので /2）
    let u16_len = (used as usize + 1) / 2;
    let mut buf: Vec<u16> = vec![0u16; u16_len];
    unsafe {
        EvtRender(
            None,
            h_event,
            EvtRenderEventXml.0,
            used,
            Some(buf.as_mut_ptr() as *mut _),
            &mut used,
            &mut count,
        )?;
    }
    // 末尾の 0 を除いて UTF-16 → String へ
    let nul = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Ok(String::from_utf16_lossy(&buf[..nul]))
}

fn ev_query(path: &str, query: &str) -> windows::core::Result<Vec<String>> {
    let path = HSTRING::from(path);
    let query = HSTRING::from(query);

    let flags = EvtQueryChannelPath.0; // | EvtQueryReverseDirection.0;
    let h_results = unsafe { EvtQuery(None, &path, &query, flags)? };

    // 一度に取得するイベント数
    const BATCH: u32 = 16;
    let mut events = [0isize; BATCH as usize];
    let mut returned: u32 = 0;
    let mut v = Vec::new();
    let mut err = Option::default();

    loop {
        // タイムアウトを短めに設定（例：1000ms）
        let next = unsafe { EvtNext(h_results, &mut events, 5000, 0, &mut returned) };
        match next {
            Ok(()) => {
                // 取得できた分だけ処理
                for i in 0..returned as usize {
                    let h_event = events[i];
                    let xml = render_event_xml(EVT_HANDLE(h_event))?;
                    v.push(xml);
                    unsafe { EvtClose(EVT_HANDLE(h_event))? }; // イベントハンドルは都度閉じる
                    events[i] = 0;
                }
            }
            Err(e) => {
                if e.code().ne(&HRESULT::from_win32(ERROR_NO_MORE_ITEMS.0)) {
                    err = Some(e);
                }
                break
            }
        }
    }

    unsafe { EvtClose(h_results)? };
    if err.is_none() { Ok(v) } else { Err(err.unwrap()) }
}

pub fn evl_query_evid_time(event_id: usize, utctime_from: UtcDateTime) -> Result<Vec<UtcDateTime>, String> {
    // Event[System[(EventID=6006 or EventID=6008) and TimeCreated[@SystemTime>='2025-11-14T00:00:00.000Z']]]
    let query = format!(
        "Event[System[EventID={} and TimeCreated[@SystemTime>'{}']]]",
        event_id,
        utctime_from.format(&format_description::parse("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z").unwrap()).unwrap());
    let mut r = ev_query("System", &query).map_err(|e| e.to_string())?;

    let fmt = format_description::parse("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:7]Z").unwrap();
    let mut v = Vec::new();
    for l in r.iter_mut() {
        const PAT: &str = "<TimeCreated SystemTime='";
        const TIME_FORM: &str = "YYYY-MM-DDThh:mm:ss.xxxxxxxZ";
        let i = l.find(PAT).ok_or_else(|| format!("xml parse error:{}", l))? + PAT.len();
        let s = &l[i .. i + TIME_FORM.len()];
        let time = UtcDateTime::parse(&s, &fmt).map_err(|e| format!("xml time parse error:{e:}:{s:}"))?;
        v.push(time);
    }
    Ok(v)
}

pub fn evl_shutdown_normal(utctime_from: UtcDateTime) -> Result<Vec<UtcDateTime>, String> {
    evl_query_evid_time(6006, utctime_from)
}

pub fn evl_shutdown_abnormal(utctime_from: UtcDateTime) -> Result<Vec<UtcDateTime>, String> {
    evl_query_evid_time(6008, utctime_from)
}