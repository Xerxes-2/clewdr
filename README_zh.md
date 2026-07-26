# ClewdR

<p align="center">
  <img src="./assets/clewdr-logo.svg" alt="ClewdR" height="60">
</p>

ClewdR 是面向 Claude（Claude.ai、Claude Code）的 Rust 代理。  
它提供低资源占用的多端点转发，并附带一个 Leptos/WASM 管理界面用于管理 Cookie 和配置。

---

## 核心特点

- 对接 Claude Web、Claude Code。
- 单个静态二进制可运行在 Linux、macOS、Windows、Android，另有 Docker 镜像。
- 网页控制台可查看状态、编辑 Cookie，并支持热加载配置。
- 同时支持 OpenAI 兼容接口和原生 Claude 协议，流式响应可用。
- 典型占用：`<10 MB` 内存、`<1 秒` 启动、`~15 MB` 二进制。

## 支持的端点

| 服务 | 地址 |
|------|------|
| Claude 原生 | `http://127.0.0.1:8484/v1/messages` |
| Claude OpenAI 兼容 | `http://127.0.0.1:8484/v1/chat/completions` |
| Claude Code | `http://127.0.0.1:8484/code/v1/messages` |

所有端点均支持流式返回。

## 快速开始

1. 从 GitHub Releases 下载对应平台的最新版。  
   Linux/macOS 示例：
   ```bash
   curl -L -o clewdr.tar.gz https://github.com/Xerxes-2/clewdr/releases/latest/download/clewdr-linux-x64.tar.gz
   tar -xzf clewdr.tar.gz && cd clewdr-linux-x64
   chmod +x clewdr
   ```
2. 运行二进制：
   ```bash
   ./clewdr
   ```
3. 打开 `http://127.0.0.1:8484`，使用控制台（或 Docker 容器日志）显示的管理员密码登录。

## Web 管理界面

- `Dashboard`：查看健康状态、限流命中、连接数。
- `Claude`：粘贴浏览器导出的 Cookie，ClewdR 自动检测有效性。
- `Settings`：修改管理员密码、上游代理、指纹配置，支持热重载。

如忘记密码，删除 `clewdr.toml` 再启动即可。Docker 建议挂载该文件所在目录以持久化。

## 配置上游

### Claude

1. 在浏览器开发者工具导出 Claude.ai Cookie。  
2. 粘贴至 Claude 页签并保存，ClewdR 会实时标记状态。  
3. 如需自定义网络出口，可设置上游代理或指纹选项。

## 客户端示例

SillyTavern：

```json
{
  "api_url": "http://127.0.0.1:8484/v1/chat/completions",
  "api_key": "控制台显示的密码",
  "model": "claude-3-sonnet-20240229"
}
```

Continue（VS Code）：

```json
{
  "models": [
    {
      "title": "Claude via ClewdR",
      "provider": "openai",
      "model": "claude-3-sonnet-20240229",
      "apiBase": "http://127.0.0.1:8484/v1/",
      "apiKey": "控制台显示的密码"
    }
  ]
}
```

Cursor：

```json
{
  "openaiApiBase": "http://127.0.0.1:8484/v1/",
  "openaiApiKey": "控制台显示的密码"
}
```

## 从源码构建

前端会编译成 WebAssembly，由 Trunk 输出到 `static/`，再由后端提供服务。该目录
在 `.gitignore` 中，因此必须先构建前端 —— 否则新克隆的仓库直接 `cargo run` 起来
的服务是没有界面的。`cargo xtask` 负责处理这个顺序依赖：

```bash
cargo xtask check     # 检查所需的工具链组件
cargo xtask build     # release 构建前端和后端
cargo xtask dev       # 同时启动，前端热重载，监听 :3000
cargo xtask lint      # 对所有有效的 feature 组合跑 clippy
cargo xtask fmt       # 格式化（始终使用 nightly）
cargo xtask ci        # CI 跑的全部检查
```

运行 `cargo xtask` 本身不需要安装任何额外工具。构建前端则还需要：

```bash
rustup target add wasm32-unknown-unknown
cargo binstall trunk
```

`cargo xtask dev` 在 <http://127.0.0.1:3000> 提供服务，并把 `/api` 代理到 `:8484`
上的后端。改前端代码会自动重建并刷新；改后端代码需要重启。

如果你手动构建，有两点需要注意：

- 格式化必须走 **nightly**（`cargo +nightly fmt`）。`.rustfmt.toml` 里用了
  nightly 专属选项，stable 会静默跳过。
- `--all-features` 用不了。`embed-resource`/`external-resource` 和
  `portable`/`xdg` 是两组互斥 feature，由 `build.rs` 强制校验，全开会直接编译失败。

## 资源

- Wiki：<https://github.com/Xerxes-2/clewdr/wiki>  

## 致谢

- [wreq](https://github.com/0x676e67/wreq) 提供指纹识别能力。  
- [Clewd](https://github.com/teralomaniac/clewd) 提供参考实现。  
- [Clove](https://github.com/mirrorange/clove) 提供 Claude Code 相关逻辑。
