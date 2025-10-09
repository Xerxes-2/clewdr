# cache_control 使用说明

## 概述

`cache_control` 是 Anthropic Claude API 提供的一个功能，用于缓存系统提示（system prompts），以提高性能并减少重复请求的延迟。ClewdR 项目在 Claude Code 集成中实现了对此功能的支持。

## 在项目中的位置

### 1. 主要实现位置：`src/middleware/claude/request.rs`

在 `ClaudeCodePreprocess` 的请求处理中，代码会检测系统消息中是否包含 `cache_control` 字段：

```rust
// 第 252-260 行
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

### 2. 系统提示哈希计算

如果检测到带有 `cache_control` 的系统消息，会计算一个哈希值用于缓存管理：

```rust
// 第 261-265 行
let system_prompt_hash = (!cache_systems.is_empty()).then(|| {
    let mut hasher = DefaultHasher::new();
    cache_systems.hash(&mut hasher);
    hasher.finish()
});
```

### 3. 上下文传递：`src/middleware/claude/request.rs`

计算出的哈希值会被存储在 `ClaudeCodeContext` 中：

```rust
// 第 180-190 行
#[derive(Debug, Clone)]
pub struct ClaudeCodeContext {
    pub(super) stream: bool,
    pub(super) api_format: ClaudeApiFormat,
    pub(super) system_prompt_hash: Option<u64>,  // 缓存哈希值
    pub(super) usage: Usage,
}
```

### 4. 状态管理：`src/claude_code_state/mod.rs`

`ClaudeCodeState` 结构体维护系统提示哈希：

```rust
// 第 22-34 行
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
    pub system_prompt_hash: Option<u64>,  // 用于Cookie缓存匹配
    pub usage: Usage,
}
```

### 5. Cookie 请求优化：`src/claude_code_state/mod.rs`

当请求新的 Cookie 时，会传递系统提示哈希值：

```rust
// 第 85-89 行
pub async fn request_cookie(&mut self) -> Result<CookieStatus, ClewdrError> {
    let res = self
        .cookie_actor_handle
        .request(self.system_prompt_hash)  // 传递哈希值用于匹配
        .await?;
    // ...
}
```

### 6. Cookie 分发逻辑：`src/services/cookie_actor.rs`

Cookie Actor 使用 Moka 缓存来管理基于哈希的 Cookie 分配：

```rust
// 第 117-141 行
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
        // 如果有缓存哈希，尝试返回相同的 Cookie
        // 这样可以利用 Claude API 的提示缓存功能
        state.moka.insert(hash, cookie.clone());
        return Ok(cookie.clone());
    }
    // 否则轮询分配新的 Cookie
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

### 7. Provider 集成：`src/providers/claude/mod.rs`

在 Claude Provider 的调用中，系统提示哈希被传递到状态对象：

```rust
// 第 163-167 行
async fn invoke(&self, request: Self::Request) -> Result<Self::Output, ClewdrError> {
    let mut state = ClaudeCodeState::new(self.shared.cookie_actor_handle.clone());
    state.api_format = request.context.api_format();
    state.stream = request.context.is_stream();
    state.system_prompt_hash = request.context.system_prompt_hash();
    // ...
}
```

## 工作原理

### 缓存优化流程

1. **检测 cache_control**：当客户端发送包含 `cache_control` 字段的系统提示时，中间件会检测到这些标记。

2. **计算哈希值**：对包含 `cache_control` 的系统消息计算哈希值，用作缓存键。

3. **Cookie 复用**：具有相同系统提示哈希的请求会尽可能使用同一个 Cookie，这样 Claude API 可以复用缓存的系统提示，减少延迟和成本。

4. **Moka 缓存**：使用 Moka 内存缓存（TTL 1小时）来维护哈希到 Cookie 的映射关系。

### 优势

- **性能提升**：复用缓存的系统提示可以显著减少首次响应延迟
- **成本优化**：缓存的 token 计费更低
- **智能分配**：只有包含 `cache_control` 的请求才会触发 Cookie 匹配逻辑

## 使用示例

在发送给 ClewdR 的请求中，系统提示可以包含 `cache_control` 字段：

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

## 相关配置

- **Moka 缓存配置**（`src/services/cookie_actor.rs` 第 52-54 行）：
  - 最大容量：1000 个条目
  - 空闲超时：60 分钟（3600 秒）

## 总结

`cache_control` 功能主要用于 Claude Code API 集成，通过智能的 Cookie 分配策略，确保使用相同系统提示的请求能够复用同一个 Cookie，从而充分利用 Anthropic 的提示缓存功能，提升性能并降低成本。
