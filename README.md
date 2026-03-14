# Voices Summary

`Voices Summary` 是一个面向 Windows 的录音笔同步与会话整理桌面应用。程序常驻系统托盘，自动识别指定 USB 录音笔，将新音频同步到本地数据目录，并调用外部 HTTP 服务完成带说话人标记的转写和摘要生成。

## 主要功能

- 自动识别指定录音笔设备，按 `VID/PID`、卷标和目录特征进行匹配
- 扫描录音笔映射盘符，发现未同步音频并复制到本地数据目录
- 使用 SQLite 持久化设备、文件、任务状态、日志、转写和摘要信息
- 调用自定义 HTTP 转写服务，要求返回带 `speakerLabel` 的分段转写结果
- 调用自定义 HTTP 摘要服务，生成要点列表和完整摘要
- 提供桌面控制台，查看设备状态、任务列表、完整转写、摘要和失败日志
- 支持任务重试、打开原始音频/转写稿/摘要文件、后台托盘运行

## 技术栈

- 桌面端: Rust + Tauri 2
- 前端: Vue 3 + TypeScript + Vite
- 包管理: Bun
- 本地存储: SQLite

## 目录结构

```text
.
├─ docs/                 需求文档
├─ src/                  Vue 前端
│  ├─ App.vue            主界面
│  ├─ lib/api.ts         Tauri 命令调用与浏览器 mock
│  └─ types/models.ts    前端类型定义
├─ src-tauri/            Rust / Tauri 后端
│  ├─ src/main.rs        Tauri 启动、托盘、窗口生命周期
│  ├─ src/commands.rs    Tauri 命令与共享状态
│  ├─ src/db.rs          SQLite 数据访问层
│  ├─ src/device.rs      Windows 设备扫描
│  ├─ src/providers.rs   转写/摘要 HTTP 适配层
│  └─ src/services/      扫描导入与任务处理
└─ package.json          前端与 Tauri 脚本
```

## 环境要求

- Windows 10 或更高版本
- Node.js 24+
- Bun 1.3+
- Rust stable toolchain
- WebView2 Runtime

如果需要生成安装包，还需要可访问 WiX 下载源，或提前在系统中安装 WiX Toolset。

## 配置说明

应用首次启动时会在用户配置目录生成 `settings.json`。配置项包括：

- 数据目录与输出目录
- 录音笔匹配规则
- 扫描目录与允许扩展名
- 转写服务 URL、鉴权 Header、API Key
- 摘要服务 URL、鉴权 Header、API Key
- 扫描间隔、请求超时、最大重试次数、并发任务数

转写服务需返回包含说话人标记的分段结果，示例响应结构：

```json
{
  "segments": [
    {
      "speakerLabel": "Speaker 1",
      "startMs": 0,
      "endMs": 3200,
      "text": "今天先确认录音同步情况。"
    }
  ]
}
```

摘要服务示例响应结构：

```json
{
  "title": "会议摘要",
  "bullets": ["确认同步状态", "安排后续处理"],
  "fullText": "本次会话确认了录音同步情况，并安排了后续处理动作。"
}
```

## 开发运行

安装依赖：

```powershell
bun install
```

启动前端开发服务器：

```powershell
bun run dev
```

启动 Tauri 桌面应用：

```powershell
bun run tauri:dev
```

如果本机未安装 Bun，也可以临时使用 npm 安装前端依赖，但仓库默认以 Bun 为准：

```powershell
npm install
```

## 构建

仅构建前端资源：

```powershell
bun run build
```

检查 Rust 后端：

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
```

构建桌面应用调试版：

```powershell
.\node_modules\.bin\tauri.cmd build --debug
```

构建正式版：

```powershell
bun run tauri:build
```

调试版可执行文件默认输出到：

```text
src-tauri/target/debug/voices-summary.exe
```

## 当前实现边界

- 当前仅实现 Windows 平台
- 录音笔检测基于 USB 磁盘与 PowerShell / WMI 查询
- 外部转写与摘要服务采用自定义 HTTP 协议，不绑定具体供应商
- MSI 安装包构建依赖 WiX；若网络受限，可能只能先生成 `.exe`
