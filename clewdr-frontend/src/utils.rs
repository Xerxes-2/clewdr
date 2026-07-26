pub fn mask_str(s: &str, visible: usize) -> String {
    if s.len() > visible * 2 {
        format!("{}...{}", &s[..visible], &s[s.len() - visible..])
    } else {
        s.to_string()
    }
}

/// Render a Unix timestamp using the browser's locale.
///
/// Milliseconds are computed in `f64`, matching the JavaScript `Date` API. The
/// precision loss only bites past year 285616, so it cannot affect a cookie
/// reset time.
#[expect(
    clippy::cast_precision_loss,
    reason = "JS Date takes an f64; timestamps are far inside its exact range"
)]
pub fn format_timestamp(ts: i64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64((ts * 1000) as f64));
    to_locale_string(&date)
}

pub fn format_iso(iso: &str) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(iso));
    to_locale_string(&date)
}

fn to_locale_string(date: &js_sys::Date) -> String {
    date.to_locale_string("default", &wasm_bindgen::JsValue::UNDEFINED)
        .as_string()
        .unwrap_or_else(|| "N/A".into())
}

pub fn copy_to_clipboard(text: String) {
    wasm_bindgen_futures::spawn_local(async move {
        let window = web_sys::window().unwrap();
        let clipboard = window.navigator().clipboard();
        let _ = wasm_bindgen_futures::JsFuture::from(clipboard.write_text(&text)).await;
    });
}
