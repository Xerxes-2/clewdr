use std::fmt::Write;

use colored::Colorize;
use serde_json::Value;
use snafu::ResultExt;
use wreq::Method;

use crate::{
    claude_web_state::ClaudeWebState,
    config::{CLEWDR_CONFIG, Reason},
    error::{CheckClaudeErr, ClewdrError, WreqSnafu},
    utils::print_out_json,
};

impl ClaudeWebState {
    /// Bootstraps the application state by initializing connections to Claude.ai
    ///
    /// This function performs the following operations:
    /// 1. Sends a request to get the bootstrap data from Claude.ai
    /// 2. Validates the cookie and account information
    /// 3. Collects capabilities and checks if the account is pro
    /// 4. Retrieves organization information
    /// 5. Checks for account flags (restrictions, warnings, bans)
    ///
    /// # Returns
    /// * `Result<(), ClewdrError>` - Success or an error with details about cookie validity
    pub async fn bootstrap(&mut self) -> Result<(), ClewdrError> {
        let end_point = self
            .endpoint
            .join("api/bootstrap")
            .map_err(|e| ClewdrError::Whatever {
                message: format!("Parse URL error: {e}"),
                source: Some(Box::new(e)),
            })?;
        let res = self
            .build_request(Method::GET, end_point)
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to bootstrap",
            })?
            .check_claude()
            .await?;
        let bootstrap = res.json::<Value>().await.context(WreqSnafu {
            msg: "Failed to parse bootstrap response",
        })?;
        print_out_json(&bootstrap, "bootstrap_res.json");
        if bootstrap["account"].is_null() {
            return Err(Reason::Null.into());
        }
        let memberships =
            bootstrap["account"]["memberships"]
                .as_array()
                .ok_or(ClewdrError::UnexpectedNone {
                    msg: "Failed to get memberships from bootstrap",
                })?;
        let boot_acc_info = memberships
            .iter()
            .find(|m| {
                m["organization"]["capabilities"]
                    .as_array()
                    .is_some_and(|c| c.iter().any(|c| c.as_str() == Some("chat")))
            })
            .and_then(|m| m["organization"].as_object())
            .ok_or(ClewdrError::UnexpectedNone {
                msg: "Failed to find a valid organization in bootstrap",
            })?;
        let email = bootstrap["account"]["email_address"]
            .as_str()
            .unwrap_or_default();
        self.capabilities = boot_acc_info["capabilities"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| c.as_str())
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !self.is_pro() && CLEWDR_CONFIG.load().skip_non_pro {
            return Err(Reason::Free.into());
        }
        let mut w = String::new();
        writeln!(
            w,
            "[{}]\nemail: {}\ncapabilities: {}",
            self.cookie.as_ref().unwrap().cookie.mask().green(),
            email.blue(),
            self.capabilities.join(", ").blue()
        )?;

        // Bootstrap complete
        let end_point = self
            .endpoint
            .join("api/organizations")
            .expect("Url parse error");
        let res = self
            .build_request(Method::GET, end_point)
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to get organizations",
            })?
            .check_claude()
            .await?;
        let ret_json = res.json::<Value>().await.context(WreqSnafu {
            msg: "Failed to parse organizations response",
        })?;
        print_out_json(&ret_json, "org.json");
        let acc_info = ret_json
            .as_array()
            .and_then(|a| {
                a.iter()
                    .filter(|v| {
                        v.get("capabilities")
                            .and_then(|c| c.as_array())
                            .is_some_and(|c| c.iter().any(|c| c.as_str() == Some("chat")))
                    })
                    .max_by_key(|v| {
                        v.get("capabilities")
                            .and_then(|c| c.as_array())
                            .map(|c| c.len())
                            .unwrap_or_default()
                    })
            })
            .ok_or(ClewdrError::UnexpectedNone {
                msg: "Failed to find a valid organization in response",
            })?;

        self.check_flags(acc_info, w)?;

        let u =
            acc_info
                .get("uuid")
                .and_then(|u| u.as_str())
                .ok_or(ClewdrError::UnexpectedNone {
                    msg: "Failed to find UUID in organization response",
                })?;
        self.org_uuid = Some(u.to_string());
        Ok(())
    }

    /// Checks if the account has any restrictions, warnings or bans
    ///
    /// Examines the account flags to determine if the account can be used:
    /// - For banned accounts, returns an error immediately
    /// - For restricted accounts, checks expiration time and may return error based on config
    /// - For warned accounts, may skip based on configuration settings
    ///
    /// # Arguments
    /// * `acc_info` - Account information JSON containing active flags
    ///
    /// # Returns
    /// * `Result<(), ClewdrError>` - Ok if the account can be used, or error with reason
    fn check_flags(&self, acc_info: &Value, mut w: String) -> Result<(), ClewdrError> {
        let Some(active_flags) = acc_info.get("active_flags").and_then(|a| a.as_array()) else {
            return Ok(());
        };
        let now = chrono::Utc::now();
        let flag_time = active_flags
            .iter()
            .filter_map(|f| {
                let r#type = f["type"].as_str()?;
                let expire = f["expires_at"].as_str()?;
                let expire = chrono::DateTime::parse_from_rfc3339(expire).ok()?;
                if now > expire {
                    return None;
                }
                Some((r#type, expire))
            })
            .collect::<Vec<_>>();

        let banned = flag_time.iter().any(|(f, _)| f.contains("banned"));
        let find_flag = |flag: &str| {
            flag_time
                .iter()
                .filter(|(f, _)| f.contains(flag))
                .max_by_key(|(_, expire)| expire.timestamp())
                .cloned()
        };
        let restricted = find_flag("restricted");
        let second = find_flag("second_warning");
        let first = find_flag("first_warning");

        for (f, t) in flag_time {
            let hours = t.to_utc() - now;
            writeln!(w, "{}: expire in {} hours", f.red(), hours.num_hours())?;
        }
        if banned {
            writeln!(
                w,
                "{}",
                "Your account is banned, please use another account.".red()
            )?;
            print!("{w}");
            return Err(Reason::Banned.into());
        }
        if !w.is_empty() {
            print!("{w}");
        }
        if let Some((_, expire)) = restricted {
            if CLEWDR_CONFIG.load().skip_restricted {
                return Err(Reason::Restricted(expire.timestamp()).into());
            }
        } else if let Some((_, expire)) = second {
            if CLEWDR_CONFIG.load().skip_second_warning {
                return Err(Reason::Restricted(expire.timestamp()).into());
            }
        } else if let Some((_, expire)) = first {
            if CLEWDR_CONFIG.load().skip_first_warning {
                return Err(Reason::Restricted(expire.timestamp()).into());
            }
        } else if CLEWDR_CONFIG.load().skip_normal_pro {
            return Err(Reason::NormalPro.into());
        }
        Ok(())
    }
}
