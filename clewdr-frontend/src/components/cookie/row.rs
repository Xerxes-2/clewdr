use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::usage::UsageDetails;
use crate::{
    api,
    i18n::use_i18n,
    types::{CookieStatus, Reason, UselessCookie},
    utils::{self, format_iso, format_timestamp},
};

fn confirm_and_delete(cookie: String, deleting: RwSignal<bool>) {
    let i = use_i18n();
    let window = web_sys::window().unwrap();
    if !window
        .confirm_with_message(&i.t("cookieStatus.deleteConfirm"))
        .unwrap_or(false)
    {
        return;
    }
    deleting.set(true);
    let refresh = expect_context::<RwSignal<u32>>();
    spawn_local(async move {
        let _ = api::delete_cookie(&cookie).await;
        deleting.set(false);
        refresh.update(|v| *v += 1);
    });
}

#[component]
fn DeleteBtn(cookie: String) -> impl IntoView {
    let deleting = RwSignal::new(false);
    let c = cookie.clone();
    view! {
        <button
            class="icon-del"
            disabled=move || deleting.get()
            on:click=move |_| confirm_and_delete(c.clone(), deleting)
        >
            {move || if deleting.get() { "..." } else { "✕" }}
        </button>
    }
}

#[component]
pub fn ValidRow(cookie: CookieStatus) -> impl IntoView {
    let i18n = use_i18n();
    let cookie_str = StoredValue::new(cookie.cookie.clone());
    let masked = utils::mask_str(&cookie.cookie, 6);
    let expanded = RwSignal::new(false);

    let details_cookie = cookie.clone();

    view! {
        <div class="cookie-row">
            <div class="flex-1">
                <div class="row-sm">
                    <span
                        class="text-mono text-xs"
                        style="color:#4ade80; cursor:pointer"
                        on:click=move |_| expanded.update(|e| *e = !*e)
                    >
                        {move || if expanded.get() { cookie_str.get_value() } else { masked.clone() }}
                    </span>
                    <button
                        class="icon-copy"
                        on:click=move |_| utils::copy_to_clipboard(cookie_str.get_value())
                    >"📋"</button>
                </div>

                <details style="margin-top:0.25rem">
                    <summary>{i18n.t("cookieStatus.meta.summary")}</summary>
                    <div class="stack-sm" style="margin-top:0.5rem">
                        <UsageDetails cookie=details_cookie />
                    </div>
                </details>
            </div>
            <div class="row-sm">
                <span class="text-xs text-dim">{move || use_i18n().t("cookieStatus.status.available")}</span>
                <DeleteBtn cookie=cookie.cookie />
            </div>
        </div>
    }
}

#[component]
pub fn ExhaustedRow(cookie: CookieStatus) -> impl IntoView {
    let i18n = use_i18n();
    let masked = utils::mask_str(&cookie.cookie, 6);

    let cooldown = if let Some(ts) = cookie.reset_time {
        format!(
            "{}: {}",
            i18n.t("cookieStatus.status.cooldownFull"),
            format_timestamp(ts)
        )
    } else if let Some(ref s) = cookie.seven_day_sonnet_resets_at {
        format!(
            "{}: {}",
            i18n.t("cookieStatus.status.cooldownSonnet"),
            format_iso(s)
        )
    } else if let Some(ref s) = cookie.seven_day_resets_at {
        format!(
            "{}: {}",
            i18n.t("cookieStatus.status.cooldownFull"),
            format_iso(s)
        )
    } else {
        i18n.t("cookieStatus.status.unknownReset")
    };

    view! {
        <div class="cookie-row">
            <span class="text-mono text-xs truncate flex-1" style="color:#facc15">{masked}</span>
            <div class="row-sm">
                <span class="text-xs text-dim">{cooldown}</span>
                <DeleteBtn cookie=cookie.cookie />
            </div>
        </div>
    }
}

#[component]
pub fn InvalidRow(cookie: UselessCookie) -> impl IntoView {
    let masked = utils::mask_str(&cookie.cookie, 6);
    let reason = get_reason_text(cookie.reason.as_ref());

    view! {
        <div class="cookie-row">
            <span class="text-mono text-xs truncate flex-1" style="color:#f87171">{masked}</span>
            <div class="row-sm">
                <span class="text-xs text-dim">{reason}</span>
                <DeleteBtn cookie=cookie.cookie />
            </div>
        </div>
    }
}

fn get_reason_text(reason: Option<&Reason>) -> String {
    let i = use_i18n();
    let Some(r) = reason else {
        return i.t("cookieStatus.status.reasons.unknown");
    };
    match r {
        Reason::Free => i.t("cookieStatus.status.reasons.freAccount"),
        Reason::Disabled => i.t("cookieStatus.status.reasons.disabled"),
        Reason::Banned => i.t("cookieStatus.status.reasons.banned"),
        Reason::Null => i.t("cookieStatus.status.reasons.invalid"),
        Reason::NormalPro => "Normal Pro".into(),
        Reason::Restricted(ts) => {
            format!(
                "{} {}",
                i.t("cookieStatus.status.reasons.restricted"),
                format_timestamp(*ts)
            )
        }
        Reason::TooManyRequest(ts) => {
            format!(
                "{} {}",
                i.t("cookieStatus.status.reasons.rateLimited"),
                format_timestamp(*ts)
            )
        }
    }
}
