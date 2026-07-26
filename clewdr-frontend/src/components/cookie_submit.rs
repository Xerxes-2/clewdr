use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{api, i18n::use_i18n, utils};

#[derive(Clone)]
struct CookieResult {
    cookie: String,
    success: bool,
    message: String,
}

// Roughly half of this is a single `view!` template, which reads better whole
// than split across helpers that each take a dozen signals.
#[expect(
    clippy::too_many_lines,
    reason = "mostly one contiguous view! template"
)]
#[component]
pub fn CookieSubmitForm() -> impl IntoView {
    let i18n = use_i18n();
    let cookies_input = RwSignal::new(String::new());
    let is_submitting = RwSignal::new(false);
    let results = RwSignal::new(Vec::<CookieResult>::new());
    let status_msg = RwSignal::new(Option::<(String, bool)>::None);

    let on_submit = {
        let i = use_i18n();
        move |ev: ev::SubmitEvent| {
            ev.prevent_default();
            let input = cookies_input.get_untracked();
            let lines: Vec<String> = input
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();

            if lines.is_empty() {
                status_msg.set(Some((i.t("cookieSubmit.error.empty"), false)));
                return;
            }

            is_submitting.set(true);
            status_msg.set(None);
            results.set(vec![]);

            spawn_local(async move {
                let mut new_results = Vec::new();
                let mut ok_count = 0u32;
                let mut err_count = 0u32;

                for line in &lines {
                    match api::post_cookie(line).await {
                        Ok(()) => {
                            ok_count += 1;
                            new_results.push(CookieResult {
                                cookie: line.clone(),
                                success: true,
                                message: i.t("cookieSubmit.success"),
                            });
                        }
                        Err(e) => {
                            err_count += 1;
                            let msg = if e.contains("Invalid cookie") {
                                i.t("cookieSubmit.error.format")
                            } else if e.contains("Authentication") {
                                i.t("cookieSubmit.error.auth")
                            } else {
                                e
                            };
                            new_results.push(CookieResult {
                                cookie: line.clone(),
                                success: false,
                                message: msg,
                            });
                        }
                    }
                }

                results.set(new_results);

                if err_count == 0 {
                    status_msg.set(Some((
                        i.tf(
                            "cookieSubmit.allSuccess",
                            &[("count", &ok_count.to_string())],
                        ),
                        true,
                    )));
                    cookies_input.set(String::new());
                } else if ok_count == 0 {
                    status_msg.set(Some((
                        i.tf(
                            "cookieSubmit.allFailed",
                            &[("count", &err_count.to_string())],
                        ),
                        false,
                    )));
                } else {
                    let total = ok_count + err_count;
                    status_msg.set(Some((
                        i.tf(
                            "cookieSubmit.partialSuccess",
                            &[
                                ("successCount", &ok_count.to_string()),
                                ("total", &total.to_string()),
                                ("errorCount", &err_count.to_string()),
                            ],
                        ),
                        false,
                    )));
                }

                is_submitting.set(false);
            });
        }
    };

    view! {
        <form on:submit=on_submit class="stack">
            <div>
                <label class="label">
                    {move || i18n.t("cookieSubmit.value")}
                </label>
                <textarea
                    class="textarea"
                    rows="5"
                    placeholder=move || i18n.t("cookieSubmit.placeholderMulti")
                    disabled=move || is_submitting.get()
                    prop:value=move || cookies_input.get()
                    on:input=move |ev| {
                        cookies_input.set(event_target_value(&ev));
                    }
                />
                <p class="text-xs text-mute" style="margin-top:0.25rem">
                    {move || i18n.t("cookieSubmit.descriptionMulti")}
                </p>
            </div>

            <Show when=move || status_msg.get().is_some()>
                {move || {
                    let (msg, ok) = status_msg.get().unwrap();
                    let cls = if ok { "alert alert-success" } else { "alert alert-error" };
                    view! { <div class=cls>{msg}</div> }
                }}
            </Show>

            <Show when=move || !results.get().is_empty()>
                <div style="background:var(--card); border-radius:var(--radius); padding:0.75rem; max-height:15rem; overflow-y:auto" class="stack-sm">
                    <h4 class="h4">{move || i18n.t("cookieSubmit.resultDetails")}</h4>
                    <For
                        each=move || results.get()
                        key=|r| r.cookie.clone()
                        let:result
                    >
                        {
                            let border = if result.success {
                                "border-left: 3px solid var(--success)"
                            } else {
                                "border-left: 3px solid var(--danger)"
                            };
                            let bg = if result.success {
                                "background:rgba(34,197,94,0.05)"
                            } else {
                                "background:rgba(239,68,68,0.05)"
                            };
                            let color = if result.success { "color:#4ade80" } else { "color:#f87171" };
                            let icon = if result.success { "✓" } else { "✗" };
                            let short = utils::mask_str(&result.cookie, 15);
                            view! {
                                <div class="text-xs" style=format!("padding:0.5rem; border-radius:var(--radius-sm); {border}; {bg}")>
                                    <div class="row-start">
                                        <span style=color>{icon}</span>
                                        <div class="flex-1">
                                            <div class="text-mono truncate text-xs text-dim">
                                                {short}
                                            </div>
                                            <div style=format!("margin-top:0.25rem; {color}")>
                                                {result.message.clone()}
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            }
                        }
                    </For>
                </div>
            </Show>

            <button
                type="submit"
                class="btn btn-primary btn-block"
                disabled=move || is_submitting.get()
            >
                {move || {
                    if is_submitting.get() {
                        i18n.t("cookieSubmit.submitting")
                    } else {
                        i18n.t("cookieSubmit.submitButton")
                    }
                }}
            </button>
        </form>
    }
}
