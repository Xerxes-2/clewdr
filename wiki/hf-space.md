# ClewdR 部署到 huggingface space

**ClewdR** 是一个高性能、功能丰富的 Claude 逆向代理实现，使用 Rust 语言完全重写，旨在克服原版 Clewd 修改版 的局限性。ClewdR 设计注重速度、可靠性和易用性，为用户提供与 Claude AI 模型交互的无缝体验，同时显著改善用户体验。

## 开始

### 创建空间

1. 前往 [HF Space](https://hf.space) 点击右侧的 **New space** 创建一个空间

![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-14-31_651.avif)

2. 填写详情页

![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-14-35_000.avif)

![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-14-37_973.avif)

### 上传 Dockerfile

![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-14-41_795.avif)

![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-14-44_914.avif)

### [下载 Dockerfile.huggingface](https://github.com/Xerxes-2/clewdr/blob/master/Dockerfile.huggingface)

手动重命名为`Dockerfile`并上传
![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-14-48_851.avif)

状态为 Running 后打开日志
![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-14-52_190.avif)
![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-14-56_188.avif)

`LLM API Password` 为请求 API 的密码
`Web Admin Password` 为前端管理密码

### API 地址

![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-14-58_660.avif)
![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-15-02_846.avif)

复制后加上`/v1`后缀即可在酒馆使用,为 claude 加反向代理

举例 API 地址

```
https://3v4pyve7-clewdr.hf.space/v1
```

认证令牌为上文的 `LLM API Password`

如果你不想每次重启都要手动加 cookie,请继续看下面的**自定义变量**

### 自定义变量

![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-15-05_524.avif)

![](https://raw.githubusercontent.com/Goojoe/PicList/master/images/2025-04-26_15-15-08_059.avif)

配置如下

> 注意!!! 隐私信息比如 密码 Cookie 请填到 Secrets 里

简单配置

```env
CLEWDR_COOKIE_ARRAY=[{cookie="sk-ant-sid01-xxx"},{cookie="sk-ant-sid01-yyy"}]
CLEWDR_PASSWORD=your_secure_password
CLEWDR_ADMIN_PASSWORD=your_admin_password
```

> `CLEWDR_COOKIE_ARRAY` 是一段 TOML 数组，每个元素写成 `{cookie="..."}`。
> 写错格式的单条 cookie 会被跳过并在日志里告警，不影响其他配置。

由于此cookie多个添加方法有些不同，可以使用js来批量处理(浏览器F12控制台运行)，可处理多行和`,`分割的格式

```
(async function() {
  const raw = `
sk-ant-sidXX-CV27
sk-ant-sidXX-XYZ9
`.trim(); // ← 替换为你的 cookie 原始输入

  const cookies = raw
    .split('\n')
    .flatMap(line => line.split(','))
    .map(c => c.trim())
    .filter(Boolean)
    .map(c => [c]);

  const json = JSON.stringify(cookies);
  await navigator.clipboard.writeText(json);
  console.log("已处理并复制到剪贴板：", json);
})();

```

完整默认配置文件

```env
clewdr_cookie_array = [[]]
clewdr_wasted_cookie = [] #这个可以不用管
clewdr_ip = "127.0.0.1" #此项禁止加入变量
clewdr_port = 8484  #此项禁止加入变量
clewdr_check_update = true
clewdr_auto_update = false
clewdr_password = "password"
clewdr_admin_password = "password"
clewdr_max_retries = 5
clewdr_preserve_chats = false
clewdr_skip_first_warning = false
clewdr_skip_second_warning = false
clewdr_skip_restricted = false
clewdr_skip_non_pro = false
clewdr_use_real_roles = true
clewdr_custom_prompt = ""
```

### 更新

前往 Settings -> Factory rebuild 点击按钮即可
