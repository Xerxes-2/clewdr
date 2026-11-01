# Release Notes — v0.13.8

## Features

- Align Claude Code API requests with the CLI User-Agent, Stainless headers,
  API version, and beta query while keeping the 2.1.316 version metadata.
- Expand cookie-issued OAuth scopes and use the CLI's JSON token refresh flow.
  Accounts continue to be added through cookies.

## Fixes

- Compute the billing CCH checksum from the final request bytes and replace
  existing billing blocks instead of adding duplicates.
- Preserve the stored refresh token when a refresh response omits it, and save
  rotated tokens automatically. Re-authorize through the cookie when needed.
- Retry rejected scope expansion with the existing grant for older accounts.
- Decode the response compression formats advertised by the API client.

## 更新内容

- 保留 Claude Code 2.1.316 版本信息，对齐 CLI User-Agent、Stainless 请求头、
  API 版本和 beta 查询参数。
- 扩展 Cookie 换取 OAuth token 时的授权范围，使用 CLI 的 JSON 刷新流程；
  账号仍通过 Cookie 添加。
- 根据最终请求字节计算 billing CCH 校验值，替换已有 billing 块，避免重复注入。
- 刷新响应省略 refresh token 时保留旧值，自动保存轮换后的 token；必要时使用
  Cookie 重新授权。
- 扩展授权范围被拒绝时按原授权范围重试，兼容旧账号。
- 支持解码 API 客户端声明的响应压缩格式。
