use std::collections::HashMap;

use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    Zh,
}

impl Locale {
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Zh => "zh",
        }
    }
}

type FlatMap = HashMap<String, String>;

fn flatten(value: &serde_json::Value, prefix: &str, out: &mut FlatMap) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(v, &key, out);
            }
        }
        serde_json::Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
        }
        _ => {}
    }
}

fn load_locale(json: &str) -> FlatMap {
    let value: serde_json::Value = serde_json::from_str(json).unwrap();
    let mut map = FlatMap::new();
    flatten(&value, "", &mut map);
    map
}

#[derive(Clone, Copy)]
pub struct I18n {
    locale: RwSignal<Locale>,
    en: StoredValue<FlatMap>,
    zh: StoredValue<FlatMap>,
}

impl I18n {
    fn new() -> Self {
        let lang = crate::storage::get("lang").unwrap_or_default();
        let locale = if lang == "zh" { Locale::Zh } else { Locale::En };
        Self {
            locale: RwSignal::new(locale),
            en: StoredValue::new(load_locale(include_str!("../locales/en.json"))),
            zh: StoredValue::new(load_locale(include_str!("../locales/zh.json"))),
        }
    }

    pub fn t(&self, key: &str) -> String {
        let raw = match self.locale.get() {
            Locale::En => self.en,
            Locale::Zh => self.zh,
        };
        raw.read_value()
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut s = self.t(key);
        for (k, v) in args {
            s = s.replace(&format!("{{{{{k}}}}}"), v);
        }
        s
    }

    pub fn locale(&self) -> Locale {
        self.locale.get()
    }

    pub fn set_locale(&self, locale: Locale) {
        self.locale.set(locale);
        crate::storage::set("lang", locale.code());
    }
}

pub fn provide_i18n() {
    provide_context(I18n::new());
}

pub fn use_i18n() -> I18n {
    expect_context::<I18n>()
}
