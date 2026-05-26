# Codex Helper

[English README](README.md)

Codex Helper 是一个面向 Codex desktop 的轻量本地增强启动器。

它聚焦少量本地和远端工作流补足，同时尽量保持 Codex desktop 原有的使用体验。

## 功能

- **项目级会话 Fork**：在本地和远端项目之间 Fork 会话，或 Fork 到同侧的另一个项目，不需要手动复制 session 文件。
- **Markdown 导出**：将会话导出为 Markdown，便于在 Codex 之外分享、评审或归档。
- **远端项目 Zed 打开**：通过接近原生的菜单动作，在 Zed 中打开远端 Codex 项目上下文。
- **远程端口转发**：检测并转发 Codex SSH 会话中的 web 端口，让远端 dev server 可以在本地打开。
- **近乎原生的设置界面**：在 Codex Settings 中配置 Helper 功能，界面和交互尽量贴合 Codex 原应用。

## 特点

- **动态注入**：Codex Helper 保持 Codex 应用文件不变，增强层可回退。
- **克制补足**：只补充 Codex 尚未覆盖的工作流，不重复原生能力，也不增加容易产生歧义的操作。
- **贴近 Codex 交互**：控制入口放在 Codex 既有界面中，并尽量沿用原应用的视觉和交互形式。

## 本地状态

Codex Helper 使用一个状态目录：

```text
~/.codex-helper/
  logs/
  scripts/
  config.json
  state.json
```

失败记录会写入 `~/.codex-helper/logs/codex-helper.jsonl`。

## 开发

```bash
bun install
bun run check
bun run launch
env RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml
bun run build:app
```

Tauri 后端当前默认目标是 macOS 的 `/Applications/Codex.app`。

本仓库使用 `Bun` 运行开发脚本。构建后的 `CodexHelper.app` 是 Rust/Tauri 应用，运行时不需要 Bun；注入脚本会在 Codex renderer 中执行。

`bun run build:app` 会运行 `scripts/build-macos-dmg.sh`，并写入 `dist/macos/CodexHelper-<version>-macos-<arch>.dmg`。

## 发布

推送 `v*` tag 会通过 GitHub Actions 构建已签名、已 notarize 的 DMG。secret 配置和本地签名说明见 [docs/release-guide.md](docs/release-guide.md)。

App 和菜单栏图标位于 `src-tauri/icons/`：

- `icon.png`：应用图标，已提交，用于 `.app`、DMG 和 Tauri bundle metadata。
- `tray.png`：菜单栏 template 源图，黑色透明背景；构建时生成 `tray-menu.png`。

如果本地 Rust 构建遇到 `sccache: error: Operation not permitted`，请按上面的示例使用 `RUSTC_WRAPPER=` 运行 Cargo。

## 鸣谢

这个项目参考了 BigPizzaV3/CodexPlusPlus 的动态 launcher 思路，以及 b-nnett/codex-plusplus 探索过的 tweak-oriented 用户体验。
