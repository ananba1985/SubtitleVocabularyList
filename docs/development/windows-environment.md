# SubtitleVocabularyList Windows 开发环境

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-DEV-001 |
| 文档版本 | 0.5 |
| 更新日期 | 2026-10-07 |
| 状态 | 项目开发中；手动入口与本机环境检查通过，未执行本轮发行打包 |
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

## 4 日常开发、本地 exe 与验证

功能开发只执行与变更相关的本地验证，不自动打包、覆盖安装或重跑整套发行验收。默认开发入口为：

```powershell
pnpm desktop:dev
```

`run-dev.ps1` 从任意当前目录定位工作区，为当前子进程提供本机 PATH、工作区媒体资源和 `.local/dev-data`，退出后恢复调用环境。开发配置使用独立应用标识，关闭发行资源打包；本地模型和 Windows 英语声音仍使用本机已有服务。

需要一个本地调试 exe 时手动执行：

```powershell
pnpm desktop:build
```

`build-local.ps1` 使用 debug 和 no-bundle，不要求 Git 工作区干净；输出 `.local/build/SubtitleVocabularyList.exe` 与 `run-local.ps1`。优先执行生成的 launcher，它会补齐本机 PATH 和指定的数据目录。此 exe 仍依赖当前工作区、本机媒体工具和 WebView2，不用来代替独立发行目录。调试代码默认数据目录为当前编译工作区的 `.local/dev-data`，可通过脚本 `-DataDirectory` 或进程 `SVL_DATA_DIR` 显式覆盖；发行程序仍使用正式应用数据目录。

三个脚本均支持 `-CheckOnly`；开发运行、调试构建和发行打包的执行计划可在不启动应用、不构建安装包的情况下检查。此次在工作区父目录使用 Windows PowerShell 5.1 执行三个 CheckOnly 均通过，识别到本机 FFmpeg/ffprobe/Tesseract、Whisper 与模型；PowerShell 语法和 Rust 全目标检查通过；以模拟 pnpm/cargo 验证了本地构建的 debug/no-bundle 参数、exe 导出、生成 launcher 的语法、带空格路径及环境恢复，没有调用真实编译器或启动程序。未据这些检查声称本轮启动了调试程序或生成了新安装包。

日常验证按实际变更选用：

```powershell
pnpm build
pnpm test:core
pnpm desktop:dev
```

构建通过后再验证实际桌面窗口、快捷键、截图、语音、真实视频与重启；后台 SSH 会话中的程序运行不等于交互桌面验收。使用独立测试词库，避免与用户实际资料混用。

新机器已确认 Git 2.56.0.2、Node.js 22.23.3、pnpm 10.6.5、Rust 1.95.0、Visual Studio 2022 Build Tools、FFmpeg 9.0.2、Tesseract 英语数据和校验通过的 Whisper 模型；系统包含 Microsoft Zira 英语声音。脚本实际使用 Windows 自带 PowerShell 5.1 执行，修正了绝对路径拼接与校验响应字节类型的兼容问题。29 项核心测试、调试构建与 S04E01 媒体处理已通过，界面实际结果与未完成项见[台式机记录](../testing/desktop-machine-results.md)。这些证据不代表整个 0.1 已交付。

完成 bug 修复或大功能后，按[版本管理规范](version-management.md)递增一次并执行 pnpm version:check。版本变化同步 package.json、Tauri、Cargo 与本项目锁文件，不自动生成安装包。发行脚本和源码导出在耗时工作前检查版本一致性；产物命名使用项目版本，不使用依赖版本。

## 5 手动版本发布与离线发行构建

三个 `build-offline-*.ps1` 入口从固定版本和摘要准备媒体、OCR 与转写资源，使用 Visual Studio 2022 的 MSVC 与 CMake，写入忽略目录 `.tools/release-resources`。FFmpeg 的 Windows 构建辅助程序和源归档保存在 `.tools/release-cache`，不作为最终用户依赖。

仅在需要打包或发布版本时，提交源码并保持当前分支工作区干净，然后手动执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-release.ps1
```

已有本次源码构建、校验过的资源时，可指定 `-SkipToolBuild`。入口仍收集许可、构建 NSIS、检查资源摘要并导出源码材料。交付目录按 tauri.conf.json 的版本生成 `release/<版本>`，包含安装器、`standalone/SubtitleVocabularyList.exe` 及其完整 `tools`、源码材料 ZIP 与 SHA256SUMS；此目录不提交 Git。源码未提交、依赖条款缺失或原始 Cargo 归档摘要不符时停止。独立 exe 使用随目录的发行资源，复制或移动时须保留整个 standalone 目录；用户词库仍留在应用数据目录。脚本不启动或安装程序。`pnpm desktop:package` 是复用已准备工具的手动快捷入口，资源未准备时执行上面的完整脚本。最终用户说明见[安装使用](../release/installation.md)，实际结果见[安装验证](../testing/offline-release-results.md)。

`temporary-offline-test.ps1` 为整体验收准备临时防火墙规则，需要管理员权限及用户确认 UAC。调用者提供隔离安装目录和 `.tools` 内的新信号目录；只阻断这个程序和四个工具的非回环出站流量，保留本机模型访问。正常执行在收到 `offline-done` 或最长 15 分钟租期结束后删除本次规则；若异常终止管理员进程，应根据信号中记录的规则名核对清理。脚本准备和语法检查不代表实际离线验收通过。

## 6 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-06 | 建立新机器开发与媒体资源准备说明，明确当前实际验证边界 |
| 0.2 | 2026-10-07 | 关联台式机核心、构建、媒体和界面的真实验证边界 |
| 0.3 | 2026-10-07 | 区分开发环境与随包运行资源，增加源码匹配 NSIS、许可收集和源材料构建入口 |
| 0.4 | 2026-10-07 | 明确开发阶段只做本地验证，提供热更新运行、调试 exe 与手动安装/独立运行目录入口及本轮验证范围 |
| 0.5 | 2026-10-07 | 接入统一版本检查、交付递增与手动发布命名，修正源码导出项目/依赖版本变量复用 |
