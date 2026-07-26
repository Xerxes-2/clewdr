# ClewdR

<p align="center">
  <img src="./assets/clewdr-logo.svg" alt="ClewdR" height="60">
</p>

ClewdR 是面向 Claude（Claude.ai 与 Claude Code）的 Rust 代理，用单个二进制同时提供原生 Claude 协议和 OpenAI 兼容接口。

它以单个静态可执行文件运行在 Linux、macOS、Windows 和 Android 上，另有 Docker 镜像；典型占用 `<10 MB` 内存、`<1 秒` 启动、`~15 MB` 体积。

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

## 添加 Cookie

ClewdR 需要至少一个 Claude.ai Cookie 才能转发请求。

1. 在浏览器开发者工具中导出 Claude.ai Cookie。
2. 以 `cookie: value` 的形式粘贴到 `Claude` 页签并保存，ClewdR 会自动检测有效性。
3. 如需自定义网络出口，可设置上游代理或指纹选项。

其余页签负责其他配置。`Dashboard` 查看健康状态、连接数和限流命中；`Settings` 修改管理员密码、上游代理，并支持不重启热重载配置。

如忘记密码，删除 `clewdr.toml` 再启动即可。Docker 建议挂载该文件所在目录以持久化。

## 接入客户端

以下路径均相对于 `http://127.0.0.1:8484`。

| | Claude.ai | Claude Code |
|---|---|---|
| Claude 原生 | `/v1/messages` | `/code/v1/messages` |
| OpenAI 兼容 | `/v1/chat/completions` | `/code/v1/chat/completions` |
| 模型列表 | `/v1/models` | `/code/v1/models` |
| Token 计数 | — | `/code/v1/messages/count_tokens` |

所有端点均支持流式返回。API 密码在启动时打印到控制台，与管理员密码是分开的两个。

SillyTavern：

```json
{
  "api_url": "http://127.0.0.1:8484/v1/chat/completions",
  "api_key": "控制台显示的密码",
  "model": "claude-3-sonnet-20240229"
}
```

其他 OpenAI 兼容客户端（Continue、Cursor 等）配置方式相同：把 API base 指向 `http://127.0.0.1:8484/v1/`，密钥填 API 密码即可。

## 从源码构建

前端会编译成 WebAssembly 输出到 `static/`，再由后端提供服务。该目录在 `.gitignore` 中，因此必须先构建前端，否则 `cargo run` 起来的服务是没有界面的。`cargo xtask` 负责处理这个顺序依赖：

```bash
cargo xtask check     # 检查所需的工具链组件
cargo xtask build     # release 构建前端和后端
cargo xtask dev       # 同时启动，前端热重载，监听 :3000
cargo xtask lint      # 对所有有效的 feature 组合跑 clippy
cargo xtask fmt       # 格式化（始终使用 nightly）
cargo xtask ci        # CI 跑的全部检查
```

构建前端需要 `rustup target add wasm32-unknown-unknown` 和 `cargo binstall trunk`。运行 `cargo xtask` 本身不需要装任何东西。

如果绕过 xtask 手动构建，有两点需要注意。格式化必须走 **nightly**，因为 `.rustfmt.toml` 里用了 nightly 专属选项，stable 会静默跳过。

`--all-features` 也用不了：`embed-resource`/`external-resource` 和 `portable`/`xdg` 是两组互斥 feature，由 `build.rs` 强制校验。

## 资源

- Wiki：<https://github.com/Xerxes-2/clewdr/wiki>

## 致谢

- [wreq](https://github.com/0x676e67/wreq) 提供指纹识别能力。
- [Clewd](https://github.com/teralomaniac/clewd) 提供参考实现。
- [Clove](https://github.com/mirrorange/clove) 提供 Claude Code 相关逻辑。
