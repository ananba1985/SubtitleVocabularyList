# SubtitleVocabularyList Windows 开发环境

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-DEV-001 |
| 文档版本 | 0.3 |
| 更新日期 | 2026-10-07 |
| 状态 | 环境与调试构建已验证；完整应用交付待完成 |
| 依据 | [架构设计](../design/architecture.md)、M-02、M-08 |

## 1 用途与安装位置

这是开发和测试环境准备说明，最终用户的安装与离线资源交付属于 M-08。源代码默认位于 `C:\Projects\codex\SubtitleVocabularyList`；脚本允许显式指定其他工作目录。私有工具、模型、测试输出与词库保存在忽略目录，不提交公开仓库。

依赖依据为 [Tauri Windows 前置条件](https://v2.tauri.app/start/prerequisites/)。Windows 需要 Microsoft C++ Build Tools、Windows SDK、Rust 的 MSVC 工具链和 WebView2；React 前端使用 Node.js 与项目固定版本的 pnpm。

## 2 开发依赖准备

在管理员 PowerShell 中执行已检出的 `scripts\setup-development.ps1`，或先从本仓库取得该脚本：

```powershell
.\scripts\setup-development.ps1 -WorkspaceDirectory 'C:\Projects\codex\SubtitleVocabularyList'
```

脚本在工具缺失时安装 Git for Windows 2.56.0.2、Node.js 22.23.3 和 Visual Studio 2022 C++ Build Tools，安装或选择 Rust 1.95.0 的 MSVC 工具链及 Clippy、rustfmt，安装 pnpm 10.6.5，并按锁文件准备前端依赖。目标目录不存在时克隆公开仓库；存在非 Git 目录时停止，不覆盖已有目录。已有检出不自动重置、切换分支或删除本地修改。

安装缓存位于当前账号 LocalAppData 下的 `SubtitleVocabularyList\development-setup`。Git 和 Node.js 下载核对 SHA256，Microsoft 安装器核对有效签名与发布者，Rustup 下载核对上游校验文件。脚本不自动重启 Windows；安装器要求重启时明确输出。

当前脚本不安装或管理 Codex、远程连接账号和模型服务。WebView2 已存在的机器直接使用现有运行时；缺少时按 Tauri 前置说明补充，不能据脚本结束就认定桌面界面可用。

## 3 媒体资源准备

在工程根目录的管理员 PowerShell 中运行：

```powershell
.\scripts\setup-media-development.ps1
```

该脚本校验并解压 Gyan 的 FFmpeg 9.0.2 essentials Windows 构建，安装缺失的英语 Tesseract 5.5.3，调用 `setup-tools.ps1` 准备 Whisper 1.8.3 和 ggml-base.en，并核对模型 SHA256。FFmpeg 构建来源由 [FFmpeg 下载页面](https://ffmpeg.org/download.html)列出；Tesseract 安装文件来自[上游发行版](https://github.com/tesseract-ocr/tesseract/releases/tag/5.5.3)。工具保留各自许可，开发安装不代表最终发行包许可核对已经完成。

脚本将媒体路径加入当前 PowerShell 的 Path，后续开发进程需要继承相同环境；不能假定已运行的桌面进程自动继承这些变化。Whisper 开发资源使用 `.tools\runtime`。工具位置由宿主提供，用户设置不再保存工具路径。发行程序使用自身资源目录，独立于开发工具。模型服务仍按已配置地址访问，服务不可用需要保留手动收录与其他离线功能。

## 4 验证顺序与当前边界

```powershell
pnpm build
pnpm test:core
pnpm tauri build --debug --no-bundle
```

构建通过后再验证实际桌面窗口、快捷键、截图、语音、真实视频与重启；后台 SSH 会话中的程序运行不等于交互桌面验收。使用独立测试词库，避免与用户实际资料混用。

新机器已确认 Git 2.56.0.2、Node.js 22.23.3、pnpm 10.6.5、Rust 1.95.0、Visual Studio 2022 Build Tools、FFmpeg 9.0.2、Tesseract 英语数据和校验通过的 Whisper 模型；系统包含 Microsoft Zira 英语声音。脚本实际使用 Windows 自带 PowerShell 5.1 执行，修正了绝对路径拼接与校验响应字节类型的兼容问题。29 项核心测试、调试构建与 S04E01 媒体处理已通过，界面实际结果与未完成项见[台式机记录](../testing/desktop-machine-results.md)。这些证据不代表整个 0.1 已交付。

## 5 离线发行构建

三个 `build-offline-*.ps1` 入口从固定版本和摘要准备媒体、OCR 与转写资源，使用 Visual Studio 2022 的 MSVC 与 CMake，写入忽略目录 `.tools/release-resources`。FFmpeg 的 Windows 构建辅助程序和源归档保存在 `.tools/release-cache`，不作为最终用户依赖。

提交本次源码后，在干净的主分支执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-release.ps1
```

已有本次源码构建、校验过的资源时，可指定 `-SkipToolBuild`。入口仍收集许可、构建 NSIS、检查资源摘要并导出源码材料。交付目录为 `release/0.1.0`，包含安装器、源码材料 ZIP 与 SHA256SUMS；此目录不提交 Git。源码未提交、依赖条款缺失或原始 Cargo 归档摘要不符时停止。最终用户说明见[安装使用](../release/installation.md)，实际结果见[安装验证](../testing/offline-release-results.md)。

`temporary-offline-test.ps1` 为整体验收准备临时防火墙规则，需要管理员权限及用户确认 UAC。调用者提供隔离安装目录和 `.tools` 内的新信号目录；只阻断这个程序和四个工具的非回环出站流量，保留本机模型访问。正常执行在收到 `offline-done` 或最长 15 分钟租期结束后删除本次规则；若异常终止管理员进程，应根据信号中记录的规则名核对清理。脚本准备和语法检查不代表实际离线验收通过。

## 6 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-06 | 建立新机器开发与媒体资源准备说明，明确当前实际验证边界 |
| 0.2 | 2026-10-07 | 关联台式机核心、构建、媒体和界面的真实验证边界 |
| 0.3 | 2026-10-07 | 区分开发环境与随包运行资源，增加源码匹配 NSIS、许可收集和源材料构建入口 |
