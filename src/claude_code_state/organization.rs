use colored::Colorize;
use http::Method;
use serde_json::Value;
use snafu::ResultExt;

use super::ClaudeCodeState;
use crate::{
    config::Reason,
    error::{CheckClaudeErr, ClewdrError, WreqSnafu},
    utils::print_out_json,
};

impl ClaudeCodeState {
    pub async fn get_organization(&self) -> Result<String, ClewdrError> {
        let end_point = self
            .endpoint
            .join("api/bootstrap")
            .expect("Url parse error");
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
        let memberships = bootstrap["account"]["memberships"]
            .as_array()
            .ok_or(Reason::Null)?;
        let boot_acc_info = memberships
            .iter()
            .find(|m| {
                m["organization"]["capabilities"]
                    .as_array()
                    .is_some_and(|c| c.iter().any(|c| c.as_str() == Some("chat")))
            })
            .and_then(|m| m["organization"].as_object())
            .ok_or(Reason::Null)?;
        let capabilities = boot_acc_info["capabilities"]
            .as_array()
            .map(|a| a.iter().filter_map(|c| c.as_str()).collect::<Vec<_>>())
            .unwrap_or_default();
        if !capabilities.iter().any(|c| {
            c.contains("pro")
                || c.contains("enterprise")
                || c.contains("raven")
                || c.contains("max")
        }) {
            return Err(Reason::Free.into());
        }
        let email = bootstrap["account"]["email_address"]
            .as_str()
            .unwrap_or_default();
        let uuid = boot_acc_info["uuid"]
            .as_str()
            .ok_or(ClewdrError::UnexpectedNone {
                msg: "Failed to get organization UUID",
            })?
            .to_string();

        println!(
            "[{}]\nemail: {}\ncapabilities: {}",
            self.cookie.as_ref().unwrap().cookie.mask().green(),
            email.blue(),
            capabilities.join(", ").blue()
        );
        Ok(uuid)
    }
}
