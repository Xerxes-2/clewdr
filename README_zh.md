# ClewdR

<p align="center">
  <img src="./assets/clewdr-logo.svg" alt="ClewdR" height="60">
</p>

ClewdR 是面向 Claude（Claude.ai 与 Claude Code）的 Rust 代理，用单个二进制同时提供原生 Claude 协议和 OpenAI 兼容接口。

它以单个静态可执行文件运行在 Linux、macOS、Windows 和 Android 上，另有 Docker 镜像；典型占用 `<10 MB` 内存、`<1 秒` 启动、`~15 MB` 体积。

## 快速开始

1. 从 GitHub Releases 下载对应平台的最新版。文件名格式为
   `clewdr-<os>-<arch>.zip`，`<os>` 可为 `linux`、`musllinux`、`macos`、
   `windows`、`android`，`<arch>` 可为 `x86_64` 或 `aarch64`。
   ```bash
   curl -L -O https://github.com/Xerxes-2/clewdr/releases/latest/download/clewdr-linux-x86_64.zip
   unzip clewdr-linux-x86_64.zip
   chmod +x clewdr
   ```
2. 运行二进制：
   ```bash
   ./clewdr
   ```
3. 打开 `http://127.0.0.1:8484`，使用控制台显示的管理员密码登录。

### 使用 Docker

```bash
docker run -d --name clewdr \
  -p 8484:8484 \
  -v clewdr-data:/etc/clewdr \
  ghcr.io/xerxes-2/clewdr:latest
```

镜像提供 `linux/amd64` 与 `linux/arm64` 两种架构。可用 `:latest`，或用 `:v0.13.1` 锁定版本。

务必挂载 `/etc/clewdr`，否则每次重建容器都会丢失已生成的密码。配置文件为该目录下的 `clewdr.toml`，日志在 `log/`。

从容器日志读取密码：

```bash
docker logs clewdr | grep Password
```

也可以自行指定，更便于自动化。任意配置项都可以通过环境变量设置：键名大写并加 `CLEWDR_` 前缀，嵌套键用双下划线分隔。

```bash
docker run -d --name clewdr \
  -p 8484:8484 \
  -v clewdr-data:/etc/clewdr \
  -e CLEWDR_PASSWORD=your-api-password \
  -e CLEWDR_ADMIN_PASSWORD=your-admin-password \
  ghcr.io/xerxes-2/clewdr:latest
```

镜像已预设 `CLEWDR_IP=0.0.0.0` 以保证端口可访问，并关闭了更新检查——镜像无法替换自身的二进制文件。

## 添加 Cookie

ClewdR 需要至少一个 Claude.ai Cookie 才能转发请求。

1. 在浏览器开发者工具中导出 Claude.ai Cookie。
2. 在 `Claude` → `提交Cookie` 中粘贴，一行一个，然后提交。
3. `Claude` → `Cookie状态` 可查看每个 Cookie 的状态和剩余额度。

`配置` 页签负责其余设置：API 密码与管理员密码、出站代理、重试次数、Cookie 跳过规则等。其中 IP 和端口属于服务器设置，需重启生效，其余保存后立即生效。

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
  "model": "claude-sonnet-4-6"
}
```

上面的模型列表端点会返回 ClewdR 当前接受的所有模型 ID，每个模型还带一个 `-thinking` 变体。

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
