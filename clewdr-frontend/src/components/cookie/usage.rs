use leptos::prelude::*;

use crate::{
    i18n::use_i18n,
    types::{CookieStatus, UsageBreakdown},
    utils::format_iso,
};

// Leptos props are owned by the generated props struct; borrowing here would
// force a lifetime the component macro cannot express.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos component props must be owned"
)]
#[component]
pub fn UsageDetails(cookie: CookieStatus) -> impl IntoView {
    let i = use_i18n();

    let groups: Vec<(String, UsageBreakdown)> = [
        ("cookieStatus.quota.session", &cookie.session_usage),
        ("cookieStatus.quota.sevenDay", &cookie.weekly_usage),
        (
            "cookieStatus.quota.sevenDaySonnet",
            &cookie.weekly_sonnet_usage,
        ),
        ("cookieStatus.quota.sevenDayOpus", &cookie.weekly_opus_usage),
        ("cookieStatus.quota.total", &cookie.lifetime_usage),
    ]
    .into_iter()
    .filter(|(_, u)| u.any_nonzero())
    .map(|(key, u)| (i.t(key), u.clone()))
    .collect();

    let quotas: Vec<(String, f64, Option<String>)> = [
        (
            cookie.session_utilization,
            cookie.session_resets_at.as_deref(),
            "cookieStatus.quota.session",
        ),
        (
            cookie.seven_day_utilization,
            cookie.seven_day_resets_at.as_deref(),
            "cookieStatus.quota.sevenDay",
        ),
        (
            cookie.seven_day_sonnet_utilization,
            cookie.seven_day_sonnet_resets_at.as_deref(),
            "cookieStatus.quota.sevenDaySonnet",
        ),
    ]
    .into_iter()
    .filter_map(|(val, reset, key)| {
        val.map(|v| {
            (
                i.t(key),
                v,
                reset.map(|s| format!("{} {}", i.t("cookieStatus.quota.resetsAt"), format_iso(s))),
            )
        })
    })
    .collect();

    if groups.is_empty() && quotas.is_empty() {
        return None;
    }

    Some(view! {
        {groups.into_iter().map(|(title, u)| {
            let has_sonnet = u.sonnet_input_tokens > 0 || u.sonnet_output_tokens > 0;
            let has_opus = u.opus_input_tokens > 0 || u.opus_output_tokens > 0;
            view! {
                <div>
                    <span>{title}</span>
                    " · "
                    {i.t("cookieStatus.usage.totalInput")} ": " {u.total_input_tokens.to_string()}
                    " / "
                    {i.t("cookieStatus.usage.totalOutput")} ": " {u.total_output_tokens.to_string()}
                    {has_sonnet.then(|| view! {
                        <div class="text-xs text-mute" style="padding-left:0.5rem">
                            {i.t("cookieStatus.usage.sonnetInput")} ": " {u.sonnet_input_tokens.to_string()}
                            " / "
                            {i.t("cookieStatus.usage.sonnetOutput")} ": " {u.sonnet_output_tokens.to_string()}
                        </div>
                    })}
                    {has_opus.then(|| view! {
                        <div class="text-xs text-mute" style="padding-left:0.5rem">
                            {i.t("cookieStatus.usage.opusInput")} ": " {u.opus_input_tokens.to_string()}
                            " / "
                            {i.t("cookieStatus.usage.opusOutput")} ": " {u.opus_output_tokens.to_string()}
                        </div>
                    })}
                </div>
            }
        }).collect::<Vec<_>>()}
        {quotas.into_iter().map(|(label, pct, reset)| {
            let capped = pct.min(100.0);
            let level = if capped < 70.0 { "low" } else if capped < 90.0 { "mid" } else { "high" };
            view! {
                <div>
                    <div class="progress-label">
                        <span>{label}</span>
                        <span>
                            {format!("{pct:.0}%")}
                            {reset.map(|r| format!(" · {r}"))}
                        </span>
                    </div>
                    <div class="progress">
                        <div
                            class=format!("progress-bar {level}")
                            style=format!("width:{capped:.0}%")
                        />
                    </div>
                </div>
            }
        }).collect::<Vec<_>>()}
    })
}
