# Codex Helper

[English README](README.md)

Codex Helper 是一个面向 Codex desktop 的轻量本地增强启动器。

它聚焦少量本地和远端工作流补足，同时尽量保持 Codex desktop 原有的使用体验。

## 功能

- **远程端口转发**：检测并转发 Codex SSH 会话中的 web 端口，让远端 dev server 可以在本地打开。
- **Helper Settings**：从菜单栏打开独立设置窗口配置 Helper，界面尽量贴近 Codex。
- **用量限制浮层隐藏**：可选隐藏 *You're out of Codex and Work usage* 卡片。这只影响界面显示，不会重置或绕过账号额度。
- **Provider 管理**：在 Official ChatGPT 登录、API Key（DeepSeek、Kimi、MiniMax、DashScope 或自定义）、GitHub Copilot 和 xAI Grok 之间切换。Helper 会写入 `~/.codex/config.toml` 以及接近原生的模型 Catalog。Copilot 与 Grok 使用设备码 OAuth，令牌保存在 `~/.codex-helper/oauth/`。
- **本机 Provider 代理**：非 Official 流量走 `127.0.0.1:3721`（`/v1/responses` 与 `/v1/chat/completions`）。Helper 负责注入上游密钥、清洗 ChatGPT 桌面端工具载荷，并可为其他 Agent 发放本地 Endpoint Key。

## 特点

- **动态注入**：Codex Helper 保持 Codex 应用文件不变，增强层可回退。
- **克制补足**：只补充 Codex 尚未覆盖的工作流，不重复原生能力，也不增加容易产生歧义的操作。
- **贴近 Codex 交互**：交互尽量沿用 Codex 视觉语言。Helper Settings 使用独立窗口，不改动 Codex Settings。

## 本地状态

Codex Helper 使用一个状态目录：

```text
~/.codex-helper/
  logs/
  scripts/
  config.json
  providers.json
  endpoint.json
  oauth/
  state.json
```

诊断记录按本机日期写入 `~/.codex-helper/logs/codex-helper-YYYY-MM-DD.jsonl`。Helper Settings 的 Logs 页支持分页浏览、内容搜索，以及打开一条记录查看已存储的 detail。General 页的 Diagnostics 开关默认关闭，打开后会把经过本地 Helper 代理的 API Provider LLM 请求元数据写入 logs（密钥 header 会丢弃，并保留短 userPreview 供搜索），不存储请求/响应 body；若开关打开，对可疑响应或被改写的 function_call 只保留有界 compat 摘要。Official ChatGPT 登录流量不可见。

## 开发

```bash
bun install
bun run check
bun run launch
env RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml
bun run build:app
```

Tauri 后端当前默认目标是 macOS 的 `/Applications/ChatGPT.app`。

本仓库使用 `Bun` 运行开发脚本。构建后的 `CodexHelper.app` 是 Rust/Tauri 应用，运行时不需要 Bun；注入脚本会在 Codex renderer 中执行。

`bun run build:app` 会运行 `scripts/build-macos-dmg.sh`，并写入 `dist/macos/CodexHelper-<version>-macos-<arch>.dmg`。

## 发布

推送 `v*` tag 会通过 GitHub Actions 构建已签名、已 notarize 的 DMG 和 updater 归档。secret 配置和本地签名说明见 [docs/release-guide.md](docs/release-guide.md)。
Helper Settings → About 会检查 GitHub Releases 上的 `latest.json` 并安装更新。

App 和菜单栏图标位于 `src-tauri/icons/`：

- `icon.png`：应用图标，已提交，用于 `.app`、DMG 和 Tauri bundle metadata。
- `tray.png`：菜单栏 template 源图，黑色透明背景；构建时生成 `tray-menu.png`。

如果本地 Rust 构建遇到 `sccache: error: Operation not permitted`，请按上面的示例使用 `RUSTC_WRAPPER=` 运行 Cargo。

## 鸣谢

这个项目参考了 BigPizzaV3/CodexPlusPlus 的动态 launcher 思路，以及 b-nnett/codex-plusplus 探索过的 tweak-oriented 用户体验。
