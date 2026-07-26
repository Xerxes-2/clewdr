/// Show the first and last `visible` characters of `s`, eliding the middle.
///
/// Counts characters rather than bytes. Slicing by byte offset panics when the
/// offset lands inside a multi-byte character, and one of the callers masks the
/// admin password, which is whatever the user typed. A panic there happens
/// while the component is being set up, so it takes the whole page down and
/// survives a reload, since the token is read back from local storage.
///
/// Characters here are scalar values, so a combining sequence can still be
/// split across the elision. That only affects how the mask looks.
pub fn mask_str(s: &str, visible: usize) -> String {
    let count = s.chars().count();
    if count > visible * 2 {
        let head: String = s.chars().take(visible).collect();
        let tail: String = s.chars().skip(count - visible).collect();
        format!("{head}...{tail}")
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

#[cfg(test)]
mod tests {
    use super::mask_str;

    /// A CJK character is one character but three bytes, so a password of
    /// these looked long enough to mask while byte 4 sat inside the second
    /// character. This panicked before.
    const CJK: &str = "\u{5bc6}\u{7801}\u{5bc6}\u{7801}";

    #[test]
    fn masks_long_ascii() {
        assert_eq!(mask_str("abcdefghijkl", 4), "abcd...ijkl");
    }

    #[test]
    fn leaves_short_input_alone() {
        assert_eq!(mask_str("abcd", 4), "abcd");
        assert_eq!(mask_str("", 4), "");
    }

    #[test]
    fn does_not_panic_on_multibyte_input() {
        assert_eq!(mask_str(CJK, 4), CJK);
    }

    #[test]
    fn masks_by_character_not_byte() {
        let s = "\u{5bc6}".repeat(10);
        assert_eq!(mask_str(&s, 2), "\u{5bc6}\u{5bc6}...\u{5bc6}\u{5bc6}");
    }

    /// Nothing is elided at the threshold itself, only past it.
    #[test]
    fn threshold_is_exclusive() {
        assert_eq!(mask_str("abcdefgh", 4), "abcdefgh");
        assert_eq!(mask_str("abcdefghi", 4), "abcd...fghi");
    }
}
