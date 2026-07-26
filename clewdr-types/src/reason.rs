use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "display", derive(thiserror::Error))]
pub enum Reason {
    #[cfg_attr(feature = "display", error("Normal Pro account"))]
    NormalPro,
    #[cfg_attr(feature = "display", error("Free account"))]
    Free,
    #[cfg_attr(feature = "display", error("Organization Disabled"))]
    Disabled,
    #[cfg_attr(feature = "display", error("Banned"))]
    Banned,
    #[cfg_attr(feature = "display", error("Null"))]
    Null,
    #[cfg_attr(feature = "display", error("Restricted/Warning: until {}", format_timestamp(*.0)))]
    Restricted(i64),
    #[cfg_attr(feature = "display", error("429 Too many request: until {}", format_timestamp(*.0)))]
    TooManyRequest(i64),
}

#[cfg(feature = "display")]
fn format_timestamp(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0).map_or("Invalid date".to_string(), |t| {
        t.format("UTC %Y-%m-%d %H:%M:%S").to_string()
    })
}
