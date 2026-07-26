use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;

use crate::{api, i18n::use_i18n, storage, types::ConfigData};

#[component]
// Roughly half of this is a single `view!` template, which reads better whole
// than split across helpers that each take a dozen signals.
#[expect(
    clippy::too_many_lines,
    reason = "mostly one contiguous view! template"
)]
pub fn ConfigTab() -> impl IntoView {
    let i18n = use_i18n();
    let config = RwSignal::new(Option::<ConfigData>::None);
    let original_password = RwSignal::new(String::new());
    let original_admin_password = RwSignal::new(String::new());
    let loading = RwSignal::new(true);
    let saving = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let toast = expect_context::<RwSignal<Option<(String, bool)>>>();

    let fetch_config = move || {
        loading.set(true);
        error.set(None);
        spawn_local(async move {
            match api::get_config().await {
                Ok(data) => {
                    original_password.set(data.password.clone());
                    original_admin_password.set(data.admin_password.clone());
                    config.set(Some(data));
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    fetch_config();

    let on_save = {
        let i = use_i18n();
        move |_| {
            let Some(cfg) = config.get_untracked() else {
                return;
            };
            saving.set(true);
            error.set(None);
            let orig_pwd = original_password.get_untracked();
            let orig_admin = original_admin_password.get_untracked();
            spawn_local(async move {
                match api::save_config(&cfg).await {
                    Ok(()) => {
                        if cfg.admin_password != orig_admin {
                            toast.set(Some((i.t("config.adminPasswordChanged"), true)));
                            gloo_timers::future::TimeoutFuture::new(3000).await;
                            storage::remove("authToken");
                            let window = web_sys::window().unwrap();
                            let _ = window.location().set_href("/?passwordChanged=true");
                        } else if cfg.password != orig_pwd {
                            toast.set(Some((i.t("config.passwordChanged"), true)));
                        } else {
                            toast.set(Some((i.t("config.success"), true)));
                        }
                    }
                    Err(e) => {
                        error.set(Some(e));
                        toast.set(Some((i.t("config.error"), false)));
                    }
                }
                saving.set(false);
            });
        }
    };

    let set_text = move |name: String, value: String| {
        config.update(|c| {
            let Some(c) = c.as_mut() else { return };
            match name.as_str() {
                "ip" => c.ip = value,
                "port" => c.port = value.parse().unwrap_or(c.port),
                "password" => c.password = value,
                "admin_password" => c.admin_password = value,
                "proxy" => c.proxy = if value.is_empty() { None } else { Some(value) },
                "rproxy" => c.rproxy = if value.is_empty() { None } else { Some(value) },
                "max_retries" => c.max_retries = value.parse().unwrap_or(c.max_retries),
                "custom_h" => c.custom_h = if value.is_empty() { None } else { Some(value) },
                "custom_a" => c.custom_a = if value.is_empty() { None } else { Some(value) },
                "custom_prompt" => c.custom_prompt = value,
                "custom_system" => {
                    c.custom_system = if value.is_empty() { None } else { Some(value) }
                }
                _ => {}
            }
        });
    };

    let set_bool = move |name: String, checked: bool| {
        config.update(|c| {
            let Some(c) = c.as_mut() else { return };
            match name.as_str() {
                "check_update" => c.check_update = checked,
                "auto_update" => c.auto_update = checked,
                "preserve_chats" => c.preserve_chats = checked,
                "web_search" => c.web_search = checked,
                "enable_web_count_tokens" => c.enable_web_count_tokens = checked,
                "sanitize_messages" => c.sanitize_messages = checked,
                "skip_first_warning" => c.skip_first_warning = checked,
                "skip_second_warning" => c.skip_second_warning = checked,
                "skip_restricted" => c.skip_restricted = checked,
                "skip_non_pro" => c.skip_non_pro = checked,
                "skip_rate_limit" => c.skip_rate_limit = checked,
                "skip_normal_pro" => c.skip_normal_pro = checked,
                "use_real_roles" => c.use_real_roles = checked,
                _ => {}
            }
        });
    };

    let on_input = move |ev: ev::Event| {
        let target = event_target::<HtmlInputElement>(&ev);
        set_text(target.name(), target.value());
    };

    let on_checkbox = move |ev: ev::Event| {
        let target = event_target::<HtmlInputElement>(&ev);
        set_bool(target.name(), target.checked());
    };

    let on_textarea = move |ev: ev::Event| {
        let target = event_target::<web_sys::HtmlTextAreaElement>(&ev);
        set_text(target.name(), target.value());
    };

    view! {
        <div class="stack">
            <Show when=move || loading.get()>
                <p class="loading">{move || i18n.t("common.loading")}</p>
            </Show>

            <Show when=move || error.get().is_some()>
                <div class="alert alert-error">
                    {move || error.get().unwrap_or_default()}
                    <button
                        class="link"
                        style="margin-left:0.5rem"
                        on:click=move |_| fetch_config()
                    >
                        {move || i18n.t("config.retry")}
                    </button>
                </div>
            </Show>

            <Show when=move || config.get().is_some()>
                {move || {
                    let cfg = config.get().unwrap();
                    view! {
                        <div class="stack-lg">
                            <div class="row-btw">
                                <h3>{i18n.t("config.title")}</h3>
                                <button
                                    class="btn btn-primary btn-sm"
                                    disabled=move || saving.get()
                                    on:click=on_save
                                >
                                    {move || {
                                        if saving.get() {
                                            i18n.t("config.saving")
                                        } else {
                                            i18n.t("config.saveButton")
                                        }
                                    }}
                                </button>
                            </div>

                            <div class="stack">
                                // Server
                                <ConfigSection
                                    title=i18n.t("config.sections.server.title")
                                    description=i18n.t("config.sections.server.description")
                                >
                                    <div class="grid-2">
                                        <TextInput name="ip" label=i18n.t("config.sections.server.ip") value=cfg.ip.clone() on_input=on_input />
                                        <TextInput name="port" label=i18n.t("config.sections.server.port") value=cfg.port.to_string() input_type="number" on_input=on_input />
                                    </div>
                                </ConfigSection>

                                // App
                                <ConfigSection title=i18n.t("config.sections.app.title")>
                                    <div class="row-lg">
                                        <Checkbox name="check_update" label=i18n.t("config.sections.app.checkUpdate") checked=cfg.check_update on_input=on_checkbox />
                                        <Checkbox name="auto_update" label=i18n.t("config.sections.app.autoUpdate") checked=cfg.auto_update on_input=on_checkbox />
                                    </div>
                                </ConfigSection>

                                // Network
                                <ConfigSection title=i18n.t("config.sections.network.title")>
                                    <TextInput name="password" label=i18n.t("config.sections.network.password") value=cfg.password.clone() input_type="password" on_input=on_input />
                                    <TextInput name="admin_password" label=i18n.t("config.sections.network.adminPassword") value=cfg.admin_password.clone() input_type="password" on_input=on_input />
                                    <TextInput name="proxy" label=i18n.t("config.sections.network.proxy") value=cfg.proxy.clone().unwrap_or_default() on_input=on_input />
                                    <TextInput name="rproxy" label=i18n.t("config.sections.network.rproxy") value=cfg.rproxy.clone().unwrap_or_default() on_input=on_input />
                                </ConfigSection>

                                // API
                                <ConfigSection title=i18n.t("config.sections.api.title")>
                                    <TextInput name="max_retries" label=i18n.t("config.sections.api.maxRetries") value=cfg.max_retries.to_string() input_type="number" on_input=on_input />
                                    <div class="grid-2" style="margin-top:0.5rem">
                                        <Checkbox name="preserve_chats" label=i18n.t("config.sections.api.preserveChats") checked=cfg.preserve_chats on_input=on_checkbox />
                                        <Checkbox name="web_search" label=i18n.t("config.sections.api.webSearch") checked=cfg.web_search on_input=on_checkbox />
                                        <Checkbox name="enable_web_count_tokens" label=i18n.t("config.sections.api.webCountTokens") checked=cfg.enable_web_count_tokens on_input=on_checkbox />
                                        <Checkbox name="sanitize_messages" label=i18n.t("config.sections.api.sanitizeMessages") checked=cfg.sanitize_messages on_input=on_checkbox />
                                    </div>
                                </ConfigSection>

                                // Cookie
                                <ConfigSection title=i18n.t("config.sections.cookie.title")>
                                    <div class="stack-sm">
                                        <Checkbox name="skip_non_pro" label=i18n.t("config.sections.cookie.skipFree") checked=cfg.skip_non_pro on_input=on_checkbox />
                                        <Checkbox name="skip_restricted" label=i18n.t("config.sections.cookie.skipRestricted") checked=cfg.skip_restricted on_input=on_checkbox />
                                        <Checkbox name="skip_second_warning" label=i18n.t("config.sections.cookie.skipSecondWarning") checked=cfg.skip_second_warning on_input=on_checkbox />
                                        <Checkbox name="skip_first_warning" label=i18n.t("config.sections.cookie.skipFirstWarning") checked=cfg.skip_first_warning on_input=on_checkbox />
                                        <Checkbox name="skip_normal_pro" label=i18n.t("config.sections.cookie.skipNormalPro") checked=cfg.skip_normal_pro on_input=on_checkbox />
                                        <Checkbox name="skip_rate_limit" label=i18n.t("config.sections.cookie.skipRateLimit") checked=cfg.skip_rate_limit on_input=on_checkbox />
                                    </div>
                                </ConfigSection>

                                // Prompt
                                <ConfigSection title=i18n.t("config.sections.prompt.title")>
                                    <Checkbox name="use_real_roles" label=i18n.t("config.sections.prompt.realRoles") checked=cfg.use_real_roles on_input=on_checkbox />
                                    <TextInput name="custom_h" label=i18n.t("config.sections.prompt.customH") value=cfg.custom_h.clone().unwrap_or_default() on_input=on_input />
                                    <TextInput name="custom_a" label=i18n.t("config.sections.prompt.customA") value=cfg.custom_a.clone().unwrap_or_default() on_input=on_input />
                                    <TextArea name="custom_prompt" label=i18n.t("config.sections.prompt.customPrompt") value=cfg.custom_prompt.clone() on_input=on_textarea />
                                    <TextArea name="custom_system" label=i18n.t("config.sections.prompt.customSystem") value=cfg.custom_system.clone().unwrap_or_default() on_input=on_textarea />
                                </ConfigSection>
                            </div>
                        </div>
                    }
                }}
            </Show>
        </div>
    }
}

#[component]
fn ConfigSection(
    title: String,
    #[prop(optional)] description: Option<String>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="config-section">
            <div class="config-section-title">{title}</div>
            {description.map(|d| {
                view! { <p class="config-section-desc">{d}</p> }
            })}
            {children()}
        </div>
    }
}

#[component]
fn TextInput(
    name: &'static str,
    label: String,
    value: String,
    #[prop(default = "text")] input_type: &'static str,
    on_input: impl Fn(ev::Event) + Copy + 'static,
) -> impl IntoView {
    view! {
        <div class="stack-sm">
            <label class="label-sm">{label}</label>
            <input
                type=input_type
                name=name
                value=value
                on:input=on_input
                class="input input-sm"
            />
        </div>
    }
}

#[component]
fn TextArea(
    name: &'static str,
    label: String,
    value: String,
    on_input: impl Fn(ev::Event) + Copy + 'static,
) -> impl IntoView {
    view! {
        <div class="stack-sm">
            <label class="label-sm">{label}</label>
            <textarea
                name=name
                rows="3"
                on:input=on_input
                class="textarea"
            >
                {value}
            </textarea>
        </div>
    }
}

#[component]
fn Checkbox(
    name: &'static str,
    label: String,
    checked: bool,
    on_input: impl Fn(ev::Event) + Copy + 'static,
) -> impl IntoView {
    view! {
        <label class="checkbox-row">
            <input
                type="checkbox"
                name=name
                checked=checked
                on:change=on_input
            />
            {label}
        </label>
    }
}
