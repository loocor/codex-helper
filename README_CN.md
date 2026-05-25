# Codex Helper

[English README](README.md)

Codex Helper 是一个面向 Codex desktop 的轻量本地启动器。它在不修改已安装 Codex 应用的前提下，为 Codex 增加本地设置、会话工具、诊断日志和端口转发控制。

Codex Helper 通过 `CodexHelper.app` 启动 Codex，经由浏览器调试接口注入一段小型运行时，并把所有 Helper 自有状态保存在 `~/.codex-helper` 下。

## 功能

- 通过 `CodexHelper.app` 启动带有 Helper 增强能力的 Codex。
- 启动后作为 macOS 菜单栏 helper 静默运行。
- 在 Codex Settings 中加入 Helper 设置区。
- 在 Codex Settings 中查看后端状态、运行时日志和 bridge 诊断信息。
- 为远程上下文恢复 Codex 原生的 Zed 打开选项。
- 提供可选的会话操作：导出 Markdown、删除会话、在工作区之间移动会话。
- 在 Codex Settings 中查看和恢复已删除会话的本地备份。
- 检测并转发远程 Codex 会话中的端口。
- 打开 DevTools 以检查注入运行时的行为。

## 设计

Codex Helper 使用外部运行时注入，而不是修改 Codex 应用文件。Tauri 应用会启动带本地调试端点的 Codex，选择 Codex renderer target，安装一个小型 bridge，然后加载本地增强脚本。

默认启动过程是静默的。Codex Helper 在正常使用时不会显示单独的管理窗口。注入成功后，配置入口会出现在 Codex Settings 中，作为 Helper 设置区提供 General、Deleted Sessions、Logs 和 About 页面。

这让已安装的 Codex 应用 bundle 保持不变。移除 Codex Helper 只会移除 launcher、`~/.codex-helper` 和本地增强文件。

## 安全模型

Codex Helper 将自身运行时和状态保留在 Codex 应用 bundle 之外。

- 不 patch `app.asar`。
- Helper 自有日志、配置、备份和状态都保存在 `~/.codex-helper` 下。
- 删除会话前，会先在 `~/.codex-helper/backups` 下创建本地备份。
- 功能开关默认由用户选择启用，并保存在 Helper 配置中。

## 本地状态

Codex Helper 使用一个状态目录：

```text
~/.codex-helper/
  logs/
  backups/
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

本仓库使用 Bun 运行开发脚本。构建后的 `CodexHelper.app` 是 Rust/Tauri 应用，运行时不需要 Bun；注入脚本会在 Codex renderer 中执行。

`bun run build:app` 会运行 `scripts/build-macos-dmg.sh`，并写入 `dist/macos/CodexHelper-<version>-macos-<arch>.dmg`。

## 发布

推送 `v*` tag 会通过 GitHub Actions 构建已签名、已 notarize 的 DMG。secret 配置和本地签名说明见 [docs/release-guide.md](docs/release-guide.md)。

## 鸣谢

这个项目受到两个 Codex desktop 增强项目的启发：

- [BigPizzaV3/CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus) 在动态注入方向上提供了启发，Codex Helper 参考了其中部分实现思路，同时继续保持不修改已安装 Codex 应用的边界。
- [b-nnett/codex-plusplus](https://github.com/b-nnett/codex-plusplus) 在 UI 结合方式和 User Script（当前尚未展开开发）方向上提供了启发。该项目采用直接 patch Codex 应用的方式，而 Codex Helper 当前采用动态注入，所以这部分暂时保持为独立方向。
