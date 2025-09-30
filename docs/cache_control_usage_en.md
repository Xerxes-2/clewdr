# cache_control Usage Documentation

## Overview

`cache_control` is a feature provided by the Anthropic Claude API for caching system prompts to improve performance and reduce latency for repeated requests. The ClewdR project implements support for this feature in its Claude Code integration.

## Location in the Project

### 1. Main Implementation: `src/middleware/claude/request.rs`

In the `ClaudeCodePreprocess` request handler, the code detects whether system messages contain a `cache_control` field:

```rust
// Lines 252-260
let cache_systems = body
    .system
    .as_ref()
    .expect("System messages should be present")
    .as_array()
    .expect("System messages should be an array")
    .iter()
    .filter(|s| s["cache_control"].as_object().is_some())
    .collect::<Vec<_>>();
```

### 2. System Prompt Hash Calculation

If system messages with `cache_control` are detected, a hash value is calculated for cache management:

```rust
// Lines 261-265
let system_prompt_hash = (!cache_systems.is_empty()).then(|| {
    let mut hasher = DefaultHasher::new();
    cache_systems.hash(&mut hasher);
    hasher.finish()
});
```

### 3. Context Passing: `src/middleware/claude/request.rs`

The calculated hash is stored in `ClaudeCodeContext`:

```rust
// Lines 180-190
#[derive(Debug, Clone)]
pub struct ClaudeCodeContext {
    pub(super) stream: bool,
    pub(super) api_format: ClaudeApiFormat,
    pub(super) system_prompt_hash: Option<u64>,  // Cache hash value
    pub(super) usage: Usage,
}
```

### 4. State Management: `src/claude_code_state/mod.rs`

The `ClaudeCodeState` struct maintains the system prompt hash:

```rust
// Lines 22-34
#[derive(Clone)]
pub struct ClaudeCodeState {
    pub cookie_actor_handle: CookieActorHandle,
    pub cookie: Option<CookieStatus>,
    pub cookie_header_value: HeaderValue,
    pub proxy: Option<wreq::Proxy>,
    pub endpoint: url::Url,
    pub client: wreq::Client,
    pub api_format: ClaudeApiFormat,
    pub stream: bool,
    pub system_prompt_hash: Option<u64>,  // Used for Cookie cache matching
    pub usage: Usage,
}
```

### 5. Cookie Request Optimization: `src/claude_code_state/mod.rs`

When requesting a new Cookie, the system prompt hash is passed along:

```rust
// Lines 85-89
pub async fn request_cookie(&mut self) -> Result<CookieStatus, ClewdrError> {
    let res = self
        .cookie_actor_handle
        .request(self.system_prompt_hash)  // Pass hash for matching
        .await?;
    // ...
}
```

### 6. Cookie Dispatch Logic: `src/services/cookie_actor.rs`

The Cookie Actor uses Moka cache to manage hash-based Cookie allocation:

```rust
// Lines 117-141
fn dispatch(
    &self,
    state: &mut CookieActorState,
    hash: Option<u64>,
) -> Result<CookieStatus, ClewdrError> {
    Self::reset(state, self.storage);
    if let Some(hash) = hash
        && let Some(cookie) = state.moka.get(&hash)
        && let Some(cookie) = state.valid.iter().find(|&c| c == &cookie)
    {
        // If there's a cache hash, try to return the same Cookie
        // This allows leveraging Claude API's prompt caching feature
        state.moka.insert(hash, cookie.clone());
        return Ok(cookie.clone());
    }
    // Otherwise, round-robin allocate a new Cookie
    let cookie = state
        .valid
        .pop_front()
        .ok_or(ClewdrError::NoCookieAvailable)?;
    state.valid.push_back(cookie.clone());
    if let Some(hash) = hash {
        state.moka.insert(hash, cookie.clone());
    }
    Ok(cookie)
}
```

### 7. Provider Integration: `src/providers/claude/mod.rs`

In the Claude Provider invocation, the system prompt hash is passed to the state object:

```rust
// Lines 163-167
async fn invoke(&self, request: Self::Request) -> Result<Self::Output, ClewdrError> {
    let mut state = ClaudeCodeState::new(self.shared.cookie_actor_handle.clone());
    state.api_format = request.context.api_format();
    state.stream = request.context.is_stream();
    state.system_prompt_hash = request.context.system_prompt_hash();
    // ...
}
```

## How It Works

### Cache Optimization Flow

1. **Detect cache_control**: When a client sends system prompts containing the `cache_control` field, the middleware detects these markers.

2. **Calculate Hash**: A hash value is calculated for system messages containing `cache_control`, used as a cache key.

3. **Cookie Reuse**: Requests with the same system prompt hash will preferably use the same Cookie, allowing the Claude API to reuse cached system prompts, reducing latency and costs.

4. **Moka Cache**: Uses Moka in-memory cache (TTL 1 hour) to maintain the hash-to-Cookie mapping.

### Benefits

- **Performance Improvement**: Reusing cached system prompts can significantly reduce initial response latency
- **Cost Optimization**: Cached tokens have lower billing costs
- **Smart Allocation**: Only requests containing `cache_control` trigger the Cookie matching logic

## Usage Example

In requests sent to ClewdR, system prompts can include the `cache_control` field:

```json
{
  "model": "claude-3-sonnet-20240229",
  "system": [
    {
      "type": "text",
      "text": "You are Claude Code, Anthropic's official CLI for Claude.",
      "cache_control": {"type": "ephemeral"}
    },
    {
      "type": "text",
      "text": "Additional context that should be cached..."
    }
  ],
  "messages": [
    {
      "role": "user",
      "content": "Hello!"
    }
  ]
}
```

## Related Configuration

- **Moka Cache Configuration** (`src/services/cookie_actor.rs` lines 52-54):
  - Max capacity: 1000 entries
  - Idle timeout: 60 minutes (3600 seconds)

## Summary

The `cache_control` feature is primarily used in the Claude Code API integration. Through an intelligent Cookie allocation strategy, it ensures that requests with the same system prompts can reuse the same Cookie, fully leveraging Anthropic's prompt caching feature to improve performance and reduce costs.
