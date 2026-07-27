use std::{fmt::Write, mem};

use anthropic_wire::{
    ContentBlock, CreateMessageParams, ImageSource, Message, MessageContent, Role,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use futures::{StreamExt, stream};
use serde_json::Value;
use tracing::warn;
use wreq::multipart::{Form, Part};

use crate::{
    claude_web_state::ClaudeWebState,
    config::CLEWDR_CONFIG,
    types::claude_web::request::{Attachment, Tool, WebRequestBody},
    utils::{TIME_ZONE, print_out_text},
};

/// The subset of the upload endpoint's response that matters here.
#[derive(serde::Deserialize)]
struct UploadResponse {
    file_uuid: String,
}

impl ClaudeWebState {
    pub fn transform_request(&self, mut value: CreateMessageParams) -> Option<WebRequestBody> {
        let system = value.system.take();
        let msgs = mem::take(&mut value.messages);
        let system = merge_system(system.unwrap_or_default());
        let merged = merge_messages(msgs, &system)?;

        let mut tools = vec![];
        if CLEWDR_CONFIG.load().web_search {
            tools.push(Tool::web_search());
        }
        Some(WebRequestBody {
            max_tokens_to_sample: value.max_tokens,
            attachments: vec![Attachment::new(merged.paste)],
            files: vec![],
            model: if self.is_pro() {
                Some(value.model)
            } else {
                None
            },
            rendering_mode: if value.stream.unwrap_or_default() {
                "messages".to_string()
            } else {
                "raw".to_string()
            },
            prompt: merged.prompt,
            timezone: TIME_ZONE.to_string(),
            images: merged.images,
            tools,
        })
    }

    /// Upload images to the Claude.ai
    ///
    /// Images that fail to upload are skipped, so the result may be shorter
    /// than `imgs`.
    ///
    /// # Panics
    /// If the configured endpoint cannot be joined with the upload path,
    /// which would mean the endpoint itself is malformed.
    pub async fn upload_images(&self, imgs: Vec<ImageSource>) -> Vec<String> {
        // upload images
        stream::iter(imgs)
            .filter_map(async |img| {
                let ImageSource::Base64 { media_type, data } = img else {
                    warn!("Image type is not base64");
                    return None;
                };
                // decode the image
                let bytes = BASE64_STANDARD
                    .decode(data)
                    .inspect_err(|e| {
                        warn!("Failed to decode image: {}", e);
                    })
                    .ok()?;
                // choose the file name based on the media type (extract main type before any params)
                let main_type = media_type.split(';').next().unwrap_or(&media_type);
                let file_name = match main_type.to_lowercase().as_str() {
                    "image/png" => "image.png",
                    "image/jpeg" | "image/jpg" => "image.jpg",
                    "image/gif" => "image.gif",
                    "image/webp" => "image.webp",
                    "application/pdf" => "document.pdf",
                    _ => "file",
                };
                // create the part and form
                let part = Part::bytes(bytes).file_name(file_name);
                let form = Form::new().part("file", part);
                let endpoint = self
                    .endpoint
                    .join(&format!("api/{}/upload", self.org_uuid.as_ref()?))
                    .expect("Url parse error");
                // send the request into future
                let res = self
                    .build_request(http::Method::POST, &endpoint)
                    .multipart(form)
                    .send()
                    .await
                    .inspect_err(|e| {
                        warn!("Failed to upload image: {}", e);
                    })
                    .ok()?;
                // get the response json
                let json = res
                    .json::<UploadResponse>()
                    .await
                    .inspect_err(|e| {
                        warn!("Failed to parse image response: {}", e);
                    })
                    .ok()?;
                // extract the file_uuid
                Some(json.file_uuid)
            })
            .collect::<Vec<_>>()
            .await
    }
}

/// Merged messages and images
#[derive(Default, Debug)]
struct Merged {
    pub paste: String,
    pub prompt: String,
    pub images: Vec<ImageSource>,
}

/// Merges multiple messages into a single text prompt, handling system instructions
/// and extracting any images from the messages
///
/// # Arguments
/// * `msgs` - Vector of messages to merge
/// * `system` - System instructions to prepend
///
/// # Returns
/// * `Option<Merged>` - Merged prompt text, images, and additional metadata, or None if merging fails
///
/// Folds runs of the same role into one message, joined with a newline, so the
/// transcript alternates speakers the way the web prompt format expects.
fn fold_runs_of_same_role(messages: impl Iterator<Item = (Role, String)>) -> Vec<(Role, String)> {
    let mut folded: Vec<(Role, String)> = Vec::new();
    for (role, text) in messages {
        match folded.last_mut() {
            Some((prev_role, acc)) if *prev_role == role => {
                acc.push('\n');
                acc.push_str(&text);
            }
            _ => folded.push((role, text)),
        }
    }
    folded
}

fn merge_messages(msgs: Vec<Message>, system: &str) -> Option<Merged> {
    if msgs.is_empty() {
        return None;
    }
    let h = CLEWDR_CONFIG
        .load()
        .custom_h
        .clone()
        .unwrap_or("Human".to_string());
    let a = CLEWDR_CONFIG
        .load()
        .custom_a
        .clone()
        .unwrap_or("Assistant".to_string());

    let user_real_roles = CLEWDR_CONFIG.load().use_real_roles;
    let line_breaks = if user_real_roles { "\n\n\x08" } else { "\n\n" };
    let system = system.trim();
    let mut w = String::new();

    let mut imgs: Vec<ImageSource> = vec![];

    let flattened = msgs.into_iter().filter_map(|m| match m.content {
        MessageContent::Blocks { content } => {
            // collect all text blocks, join them with new line
            let blocks = content
                .into_iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text, .. } => Some(text.trim().to_string()),
                    ContentBlock::Image { source, .. } => {
                        match source {
                            ImageSource::Base64 { .. } => {
                                // push image to the list
                                imgs.push(source);
                            }
                            ImageSource::Url { url } => {
                                if let Some(source) = ImageSource::from_data_url(&url) {
                                    imgs.push(source);
                                } else {
                                    warn!("Unsupported image url source");
                                }
                            }
                            ImageSource::File { .. } => {
                                warn!("Image file sources are not supported");
                            }
                        }
                        None
                    }
                    ContentBlock::ImageUrl { image_url } => {
                        // oai image
                        if let Some(source) = ImageSource::from_data_url(&image_url.url) {
                            imgs.push(source);
                        }
                        None
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if blocks.is_empty() {
                None
            } else {
                Some((m.role, blocks))
            }
        }
        MessageContent::Text { content } => {
            // plain text
            let content = content.trim().to_string();
            if content.is_empty() {
                None
            } else {
                Some((m.role, content))
            }
        }
    });
    // Collected eagerly rather than left lazy: the closure above borrows `imgs`
    // mutably, and draining it here releases that borrow before `imgs` is read.
    let mut msgs = fold_runs_of_same_role(flattened).into_iter();
    // first message does not need prefix
    if system.is_empty() {
        let first = msgs.next()?;
        w += first.1.as_str();
    } else {
        w += system;
    }
    for (role, text) in msgs {
        let prefix = match role {
            Role::System => {
                warn!("System message should be merged into the first message");
                continue;
            }
            Role::User => format!("{h}: "),
            Role::Assistant => format!("{a}: "),
        };
        write!(w, "{line_breaks}{prefix}{text}").ok()?;
    }
    print_out_text(w.clone(), "paste.txt");

    // prompt polyfill
    let p = CLEWDR_CONFIG.load().custom_prompt.clone();

    Some(Merged {
        paste: w,
        prompt: p,
        images: imgs,
    })
}

/// Merges system message content into a single string
/// Handles both string and array formats for system messages
///
/// # Arguments
/// * `sys` - System message content as a JSON Value
///
/// # Returns
/// Merged system message as a string
fn merge_system(sys: Value) -> String {
    match sys {
        Value::String(s) => s,
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v["text"].as_str())
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
