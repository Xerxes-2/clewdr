//! Assembling a [`ClewdrConfig`] from its two sources: the TOML file and the
//! `CLEWDR_`-prefixed environment.
//!
//! Kept apart from the config struct itself because the merge has rules of its
//! own -- how an environment string becomes a typed value, and what happens
//! when part of the input is unusable -- and those rules are worth reading and
//! testing without the several hundred lines of accessors next to them.

use std::{borrow::Cow, collections::HashSet, hash::Hash};

use serde::de::DeserializeOwned;
use toml::{Table, Value};
use tracing::warn;

/// The environment variable prefix that marks a config override.
const ENV_PREFIX: &str = "CLEWDR_";

/// The cookie fields, which are parsed per-entry rather than with the rest of
/// the config so that one bad cookie cannot take the whole file down with it.
const COOKIE_KEYS: [&str; 2] = ["cookie_array", "wasted_cookie"];

/// The config file and environment, merged and ready to deserialize.
///
/// Produced by [`merge_sources`]; the cookie entries are held apart from
/// `settings` because they are parsed one at a time.
pub(super) struct MergedConfig {
    /// Everything except the cookie fields.
    pub settings: Table,
    /// The raw `cookie_array` entries, still to be parsed individually.
    pub cookie_array: Vec<Value>,
    /// The raw `wasted_cookie` entries, still to be parsed individually.
    pub wasted_cookie: Vec<Value>,
}

/// Merges `toml_text` with the `CLEWDR_` variables in `env`.
///
/// `template` supplies the type of each key -- see [`coerce`] for why an
/// environment string cannot be typed without it. Unusable input is dropped
/// with a warning rather than failing the load: a syntax error in the file
/// should not stop `CLEWDR_PASSWORD` from taking effect, since the environment
/// is often the only way in on a host where the file cannot be edited.
pub(super) fn merge_sources<K, V>(
    toml_text: &str,
    env: impl IntoIterator<Item = (K, V)>,
    template: &Table,
) -> MergedConfig
where
    K: AsRef<str>,
    V: Into<String>,
{
    let mut settings = match toml_text.parse::<Table>() {
        Ok(table) => table,
        Err(e) => {
            warn!("Ignoring unreadable config file: {e}");
            Table::new()
        }
    };

    let string_field = Value::String(String::new());
    for (key, value) in env {
        let Some(key) = strip_prefix(key.as_ref()) else {
            continue;
        };
        let raw = value.into();
        // The template holds no entry for a field whose default is `None`,
        // since TOML has no null to write. Every such field in the config is
        // string-shaped, and the config's own
        // `optional_fields_are_all_settable_from_the_environment` keeps it so.
        let target = template.get(&key).unwrap_or(&string_field);
        if let Some(value) = coerce(&raw, target) {
            settings.insert(key, value);
        } else {
            warn!(
                "Ignoring {ENV_PREFIX}{}: {raw:?} is not a valid {}",
                key.to_uppercase(),
                target.type_str()
            );
        }
    }

    let [cookie_array, wasted_cookie] = COOKIE_KEYS.map(|key| match settings.remove(key) {
        Some(Value::Array(items)) => items,
        Some(other) => {
            warn!(
                "Ignoring {key}: expected an array, found {}",
                other.type_str()
            );
            Vec::new()
        }
        None => Vec::new(),
    });

    MergedConfig {
        settings,
        cookie_array,
        wasted_cookie,
    }
}

/// Strips the `CLEWDR_` prefix and normalises the key, or returns `None` for a
/// variable that is not ours.
///
/// The comparison ignores case, matching the previous loader: shells and
/// container runtimes are not consistent about it.
fn strip_prefix(name: &str) -> Option<String> {
    name.len()
        .checked_sub(ENV_PREFIX.len())
        .filter(|_| name[..ENV_PREFIX.len()].eq_ignore_ascii_case(ENV_PREFIX))
        .map(|_| name[ENV_PREFIX.len()..].to_ascii_lowercase())
}

/// Unwraps a value that is written as a quoted string, leaving anything else
/// untouched.
///
/// Compose files and `.env` files hand the quotes through as data, unlike a
/// shell, so `CLEWDR_PASSWORD="12345"` arrives here with them attached and
/// almost never means a password with quotes in it. The previous loader
/// absorbed this and people wrote it that way for years; #157 is what removing
/// that did.
///
/// Parsing is delegated to TOML rather than trimming the first and last byte,
/// so a value that merely begins and ends with a quote -- `"a" "b"` -- is not
/// mistaken for one string, and escapes inside a real one are honoured.
/// Single quotes are left alone: the old loader did not strip them, and
/// treating them as syntax now would break the reverse case.
fn unquote(raw: &str) -> Cow<'_, str> {
    let trimmed = raw.trim();
    if !(trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"')) {
        // Also what the old loader did. Trimming a secret is not obviously
        // right, but it is what 0.13.2 shipped: anyone whose value picked up
        // stray whitespace from a compose file has been using the trimmed
        // form, and changing that now locks them out the same way #157 did.
        // Whitespace that is meant to be there can be quoted.
        return Cow::Borrowed(trimmed);
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    // Where the quotes were the only decoration, the inside is the answer and
    // nothing needs allocating. An inner quote or a backslash means it may not
    // be one string at all, so TOML decides -- and if it is not, the value
    // stays exactly as the user wrote it.
    if inner.contains('"') || inner.contains('\\') {
        return match parse_toml_fragment(trimmed) {
            Some(Value::String(s)) => Cow::Owned(s),
            _ => Cow::Borrowed(raw),
        };
    }
    Cow::Borrowed(inner)
}

/// Reads `raw` as a TOML value, or `None` if it is not one.
fn parse_toml_fragment(raw: &str) -> Option<Value> {
    format!("v = {raw}").parse::<Table>().ok()?.remove("v")
}

/// Reads `raw` as the same type as `target`, or `None` if it does not fit.
///
/// The environment gives us nothing but strings, so the target's type is what
/// decides how one is read. Guessing from the text instead -- the approach this
/// replaced -- silently discarded `CLEWDR_PASSWORD=12345`, because a value that
/// parsed as a number could no longer be taken as the string the field wanted.
fn coerce(raw: &str, target: &Value) -> Option<Value> {
    // A quoted value is unwrapped first, so the type below sees what the user
    // meant rather than the quoting they wrote it with.
    let raw = unquote(raw);
    let raw = raw.as_ref();
    match target {
        Value::String(_) => Some(Value::String(raw.to_owned())),
        Value::Boolean(_) => parse_bool(raw).map(Value::Boolean),
        Value::Integer(_) => raw.trim().parse().ok().map(Value::Integer),
        Value::Float(_) => raw.trim().parse().ok().map(Value::Float),
        // Structured values arrive as TOML fragments, which is how a cookie
        // list has always been written into a single variable.
        Value::Array(_) | Value::Table(_) | Value::Datetime(_) => parse_toml_fragment(raw),
    }
}

/// The spellings accepted for a boolean.
///
/// Wider than TOML's own `true`/`false` because the shipped Dockerfiles say
/// `CLEWDR_CHECK_UPDATE=FALSE`, and because `yes`/`no` and `on`/`off` were
/// accepted before and are the obvious things to reach for.
fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// Parses `entries` one at a time, dropping those that fail.
///
/// A cookie is rejected for being expired or malformed, which is a routine
/// thing for a file that accumulates them over months. Taking the whole config
/// down for one bad entry -- as deserializing the set in one go does -- loses
/// the user's port and password too, and hands them a freshly generated
/// password on the next start.
pub(super) fn parse_each<T>(entries: Vec<Value>, field: &str) -> HashSet<T>
where
    T: DeserializeOwned + Eq + Hash,
{
    entries
        .into_iter()
        .filter_map(|entry| {
            T::deserialize(entry)
                .inspect_err(|e| warn!("Skipping unusable entry in {field}: {e}"))
                .ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The type template the real loader passes in, kept small so the
    /// expectations below are readable.
    fn template() -> Table {
        let mut t = Table::new();
        t.insert("port".into(), Value::Integer(8484));
        t.insert("password".into(), Value::String(String::new()));
        t.insert("check_update".into(), Value::Boolean(true));
        t.insert("cookie_array".into(), Value::Array(vec![]));
        t
    }

    fn merge(toml_text: &str, env: &[(&str, &str)]) -> MergedConfig {
        merge_sources(toml_text, env.iter().copied(), &template())
    }

    #[test]
    fn a_setting_comes_from_the_file() {
        let merged = merge("port = 7777", &[]);
        assert_eq!(merged.settings["port"], Value::Integer(7777));
    }

    #[test]
    fn the_environment_overrides_the_file() {
        let merged = merge("port = 7777", &[("CLEWDR_PORT", "6666")]);
        assert_eq!(merged.settings["port"], Value::Integer(6666));
    }

    /// The bug this loader was written to fix. A numeric-looking password is a
    /// password, not a number: the previous loader parsed it as an integer,
    /// failed to turn that back into a string, and dropped the field, leaving
    /// the user with a generated password they had never seen.
    #[test]
    fn a_numeric_password_stays_a_string() {
        let merged = merge("", &[("CLEWDR_PASSWORD", "12345")]);
        assert_eq!(merged.settings["password"], Value::String("12345".into()));
    }

    /// Reported as #157: `CLEWDR_ADMIN_PASSWORD="12345"` locked an admin out
    /// after 0.13.3, because the quotes became part of the password.
    ///
    /// Compose files and `.env` files do not strip quotes the way a shell
    /// does, so writing them is an easy habit and the old loader silently
    /// absorbed it. That absorption was independent of the numeric-password
    /// bug below, which is where the previous version of this test went
    /// wrong: it assumed anyone using quotes was working around that bug, so
    /// fixing the bug made the quotes unnecessary. They are two rules, and
    /// only one of them was worth changing.
    #[test]
    fn a_quoted_value_is_unwrapped() {
        for (raw, want) in [
            ("\"12345\"", "12345"),
            ("\"hello world\"", "hello world"),
            ("\"true\"", "true"),
            ("\"a,b\"", "a,b"),
            ("\"\"", ""),
            // Escapes inside a quoted value are TOML's, as they were before.
            ("\"a\\\"b\"", "a\"b"),
        ] {
            let merged = merge("", &[("CLEWDR_PASSWORD", raw)]);
            assert_eq!(
                merged.settings["password"],
                Value::String(want.into()),
                "{raw:?} should unwrap to {want:?}"
            );
        }
    }

    /// Only a value that is *entirely* one quoted string is unwrapped. A
    /// password that merely contains quotes keeps every character, which is
    /// also what the old loader did.
    #[test]
    fn a_value_that_is_not_one_quoted_string_is_literal() {
        for raw in [
            "say \"hi\"",  // quotes inside
            "\"a\" \"b\"", // two of them
            "\"12345",     // unbalanced
            "\"\"\"",      // not a string literal
            "'12345'",     // single quotes were never stripped
            "\"",
        ] {
            let merged = merge("", &[("CLEWDR_PASSWORD", raw)]);
            assert_eq!(
                merged.settings["password"],
                Value::String(raw.into()),
                "{raw:?} should have been left alone"
            );
        }
    }

    /// Unquoting happens before the value is typed, so it reaches the other
    /// field types too -- as it did when figment parsed the environment.
    #[test]
    fn a_quoted_value_still_types_correctly() {
        assert_eq!(
            merge("", &[("CLEWDR_PORT", "\"6666\"")]).settings["port"],
            Value::Integer(6666)
        );
        assert_eq!(
            merge("", &[("CLEWDR_CHECK_UPDATE", "\"false\"")]).settings["check_update"],
            Value::Boolean(false)
        );
    }

    /// Surrounding whitespace is dropped, as it was before; quote the value to
    /// keep it. Everything else about an unquoted value survives, including
    /// text that happens to look like another type.
    #[test]
    fn an_unquoted_value_keeps_everything_but_its_padding() {
        let merged = merge("", &[("CLEWDR_PASSWORD", "  spaced  ")]);
        assert_eq!(merged.settings["password"], Value::String("spaced".into()));

        let merged = merge("", &[("CLEWDR_PASSWORD", "\"  padded  \"")]);
        assert_eq!(
            merged.settings["password"],
            Value::String("  padded  ".into()),
            "quoting is how padding is kept"
        );

        for raw in ["true", "1.5", "-5", "007", "[a,b]", "0x1f", ""] {
            let merged = merge("", &[("CLEWDR_PASSWORD", raw)]);
            assert_eq!(
                merged.settings["password"],
                Value::String(raw.into()),
                "password {raw:?} was mangled"
            );
        }
    }

    /// `CLEWDR_CHECK_UPDATE=FALSE` ships in the repo's own Dockerfile, so the
    /// uppercase spelling has to keep working.
    #[test]
    fn booleans_accept_the_spellings_that_were_accepted_before() {
        for raw in ["false", "FALSE", "False", "no", "NO", "off", "OFF", "0"] {
            let merged = merge("", &[("CLEWDR_CHECK_UPDATE", raw)]);
            assert_eq!(
                merged.settings["check_update"],
                Value::Boolean(false),
                "{raw:?} should be false"
            );
        }
        for raw in ["true", "TRUE", "True", "yes", "YES", "on", "ON", "1"] {
            let merged = merge("", &[("CLEWDR_CHECK_UPDATE", raw)]);
            assert_eq!(
                merged.settings["check_update"],
                Value::Boolean(true),
                "{raw:?} should be true"
            );
        }
    }

    /// A value that is not a boolean at all leaves the field alone, so the
    /// file's setting or the default stands.
    #[test]
    fn an_unparseable_boolean_is_ignored() {
        for raw in ["maybe", "", "2", "enabled"] {
            let merged = merge("check_update = false", &[("CLEWDR_CHECK_UPDATE", raw)]);
            assert_eq!(
                merged.settings["check_update"],
                Value::Boolean(false),
                "{raw:?} should not have overridden the file"
            );
        }
    }

    #[test]
    fn an_unparseable_number_is_ignored() {
        for raw in ["notanumber", "1.5", ""] {
            let merged = merge("port = 7777", &[("CLEWDR_PORT", raw)]);
            assert_eq!(
                merged.settings["port"],
                Value::Integer(7777),
                "{raw:?} should not have overridden the file"
            );
        }
    }

    /// The prefix match ignored case before, and container runtimes are not
    /// consistent about it.
    #[test]
    fn the_prefix_is_matched_regardless_of_case() {
        for name in ["CLEWDR_PORT", "clewdr_port", "Clewdr_Port"] {
            let merged = merge("", &[(name, "6666")]);
            assert_eq!(merged.settings["port"], Value::Integer(6666), "{name}");
        }
    }

    #[test]
    fn variables_without_the_prefix_are_left_alone() {
        let merged = merge(
            "",
            &[("PORT", "6666"), ("PATH", "/usr/bin"), ("CLEWDR", "x")],
        );
        assert!(!merged.settings.contains_key("port"));
        assert!(merged.settings.is_empty());
    }

    /// An unknown key is carried through and ignored later by serde, which is
    /// what happened before; it must not be treated as an error.
    #[test]
    fn an_unknown_key_is_harmless() {
        let merged = merge("", &[("CLEWDR_NOT_A_REAL_KEY", "x")]);
        assert_eq!(merged.settings["not_a_real_key"], Value::String("x".into()));
    }

    /// A field whose default is `None` is absent from the template. All of
    /// them are string-shaped, so the fallback must be a string and not, say,
    /// a failed lookup that drops the variable.
    #[test]
    fn a_key_absent_from_the_template_is_taken_as_a_string() {
        let merged = merge("", &[("CLEWDR_PROXY", "http://a:1")]);
        assert_eq!(merged.settings["proxy"], Value::String("http://a:1".into()));
    }

    /// Cookies have always been settable as a TOML fragment in one variable.
    #[test]
    fn a_structured_value_is_parsed_as_a_toml_fragment() {
        let merged = merge("", &[("CLEWDR_COOKIE_ARRAY", r#"[{cookie="abc"}]"#)]);
        assert_eq!(merged.cookie_array.len(), 1);
    }

    #[test]
    fn a_malformed_fragment_is_ignored() {
        let merged = merge("", &[("CLEWDR_COOKIE_ARRAY", "[[a],[b")]);
        assert!(merged.cookie_array.is_empty());
    }

    /// A syntax error in the file used to discard everything, including the
    /// environment. The environment is often the only way to configure a host
    /// whose file cannot be edited, so it has to survive.
    #[test]
    fn the_environment_survives_an_unreadable_file() {
        let merged = merge("this is not [[[ valid toml", &[("CLEWDR_PORT", "6666")]);
        assert_eq!(merged.settings["port"], Value::Integer(6666));
    }

    #[test]
    fn cookie_fields_are_held_apart_from_the_settings() {
        let merged = merge(
            r#"
            port = 7777
            [[cookie_array]]
            cookie = "a"
            [[wasted_cookie]]
            cookie = "b"
            "#,
            &[],
        );

        assert_eq!(merged.cookie_array.len(), 1);
        assert_eq!(merged.wasted_cookie.len(), 1);
        assert!(!merged.settings.contains_key("cookie_array"));
        assert!(!merged.settings.contains_key("wasted_cookie"));
    }

    /// The point of parsing entries one at a time.
    #[test]
    fn one_unusable_entry_does_not_take_the_others_with_it() {
        let entries = vec![
            Value::String("keep me".into()),
            Value::Integer(7),
            Value::String("me too".into()),
        ];

        let parsed: HashSet<String> = parse_each(entries, "cookie_array");

        assert_eq!(
            parsed,
            HashSet::from(["keep me".to_string(), "me too".to_string()])
        );
    }
}

#[cfg(test)]
mod figment_parity {
    use super::*;

    /// Every string case measured against figment 0.10 before this loader
    /// replaced it, so the comparison is recorded rather than remembered.
    /// `None` marks the inputs figment dropped -- it typed them as a number,
    /// bool or array, which then would not deserialize into a String, and the
    /// field silently kept its default. Those are the bug this loader fixed;
    /// everything else is behaviour it has to keep.
    #[test]
    fn matches_figment_except_where_figment_dropped_the_value() {
        let cases: &[(&str, Option<&str>)] = &[
            (r#""12345""#, Some("12345")),
            (r#""hello""#, Some("hello")),
            ("hello", Some("hello")),
            ("'12345'", Some("'12345'")),
            (r#""hello world""#, Some("hello world")),
            (r#""true""#, Some("true")),
            (r#""a,b""#, Some("a,b")),
            (r#""""#, Some("")),
            ("", Some("")),
            (r#""12345"#, Some(r#""12345"#)),
            (r#"say "hi""#, Some(r#"say "hi""#)),
            (r#""a" "b""#, Some(r#""a" "b""#)),
            (r#"""""#, Some(r#"""""#)),
            (r#"" ""#, Some(" ")),
            (r#""a\"b""#, Some(r#"a"b"#)),
            ("  spaced  ", Some("spaced")),
            (r#"  "  padded  "  "#, Some("  padded  ")),
            // figment dropped these; we keep them.
            ("12345", None),
            ("true", None),
            ("[a,b]", None),
        ];

        let mut t = Table::new();
        t.insert("password".into(), Value::String(String::new()));

        for (raw, figment) in cases {
            let merged = merge_sources("", [("CLEWDR_PASSWORD", *raw)], &t);
            let ours = match &merged.settings["password"] {
                Value::String(s) => s.clone(),
                other => panic!("{raw:?} became {other:?}"),
            };
            match figment {
                Some(want) => assert_eq!(&ours, want, "{raw:?} diverges from figment"),
                None => assert_eq!(&ours, raw, "{raw:?} should now survive verbatim"),
            }
        }
    }
}
