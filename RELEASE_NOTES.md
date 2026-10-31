# Release Notes

## Features

- Update the Claude Code version metadata and User-Agent to 2.1.316.

## Fixes

- Keep workspace versions and Cargo.lock consistent when preparing a release,
  preserving the dependency versions used by the pinned Nix build tools.
- Run the full CI checks before tagging a release.

## 更新内容

- 将 Claude Code 版本信息和 User-Agent 更新至 2.1.316。
- 修复发布时 workspace 版本与 Cargo.lock 不一致导致的构建失败，保留与
  Nix 构建工具匹配的依赖版本。
- 打标签前运行完整检查。
