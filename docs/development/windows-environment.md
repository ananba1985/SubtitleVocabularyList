# SubtitleVocabularyList Windows 开发环境

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-DEV-001 |
| 文档版本 | 0.14 |
| 更新日期 | 2026-10-17 |
| 状态 | 项目开发中；0.8.2 默认入口真实发行打包通过，本轮未安装 |
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

常用入口 `本地调试运行.ps1`、`本地打包运行.ps1`、`生成安装包.ps1` 均位于项目根目录；环境、清理、媒体构建和源码导出等内部脚本保留在 `scripts/`。既有 pnpm 命令调用根目录入口。

`本地调试运行.ps1` 从任意当前目录定位工作区，为当前子进程提供本机 PATH、工作区媒体资源和默认 `D:\SubtitleVocabularyList\Data`，退出后恢复调用环境。开发配置采用与发行相同的应用标识以共用单实例机制，关闭发行资源打包；本地模型和 Windows 英语声音仍使用本机已有服务。

开发启动仍调用 `development-cleanup.ps1` 清理专用 `.local/tmp`、`.tools/tmp`，以及名称带有效 UUID 的 `.local/startup-dependency-cache-*`、`.local/startup-webview-*`、`node_modules/.vite-startup-validation-*`。这部分仅处理已定义的临时目录，媒体工具、正常 `node_modules/.vite` 和其他验证记录保留；与 `-DataDirectory` 相交的目录不删除。客户端仍运行、路径带重解析点或文件被占用时跳过/延后这部分清理。

2026-10-12 起，根目录三个入口在各自编译命令前统一执行 `cargo clean --manifest-path src-tauri/Cargo.toml`，清理 Cargo 构建目录中的全部旧产物，包括 debug／release、增量缓存、依赖编译产物、探针和打包中间文件。随后按原流程开发运行、生成本地 debug exe 或生成安装包。清理失败即停止该入口，不继续编译；发行入口仍先检查工作区干净和资源目录。项目外的用户资料、Cargo 下载缓存、`.tools` 中的工具／模型及根目录 `release/<版本>` 交付物保持。

0.8.9 修正运行中的调试 exe 阻止清理的问题：三个入口通过 `scripts/build-preflight.ps1`，先用 Cargo metadata 确定实际构建目录，再按可执行文件绝对路径检查该目录内仍在运行的进程。发现占用时列出 PID 和路径，提示保存输入、从托盘退出及结束调试终端，在清理／编译前停止；生成安装包入口在准备底库和编译离线工具之前检查，避免先处理资源再遇到锁定。检查不强制结束应用，不匹配其他目录的安装版。Windows 的“Access is denied”在本次场景来自运行中的 exe 文件锁，无需提升管理员权限。

本次定位到上一轮保留的隔离调试客户端占用了 `src-tauri/target/debug/subtitle-vocabulary-list.exe`，已正常退出并核对该进程结束。Windows PowerShell 5.1 下四个脚本语法检查、真实 Cargo 目录空闲时的预检及三个入口 `-CheckOnly` 通过，记录保留在 `.local/build-preflight-0.8.9`。没有为验证创建额外占用进程，没有实际清理 Cargo、重新打包或安装；本次不宣称完成 0.8.9 安装包验证。

三个入口的 `-CheckOnly` 会按执行顺序显示清理和编译计划，不执行清理、编译或启动。每次实际执行都会从清理后的 Cargo 构建目录重新编译，不能复用之前的编译产物，耗时将增加；同一次开发运行中的源码热更新和 Tauri 后续重编译保持原行为。`pnpm desktop:dev`、`pnpm desktop:build`、`pnpm desktop:package` 及调试 CMD 均继续调用这些根目录入口，无需另记手动清理步骤。

本轮最小检查：三个入口语法检查通过，使用 Windows PowerShell 5.1 执行各自 `-CheckOnly` 均返回 0，显示先清理再编译的计划；实际 `cargo clean --manifest-path src-tauri/Cargo.toml --dry-run` 返回 0，预览 59,494 个文件、约 63.5 GiB 的文件大小合计，没有删除文件。未执行实际清理、编译或安装包构建。此项为构建脚本维护，应用版本保持 0.6.1。

Windows PowerShell 5.1 的独立夹具实际验证了临时目录删除、CheckOnly 无副作用、普通/自定义数据目录和正常缓存保留、junction 目标保留、锁定文件不阻止启动及释放后重试；实际启动脚本在模拟 pnpm 下确认清理先发生、执行继续且环境恢复。结果见[验证记录](../testing/performance-results.md#7-开发启动自动清理临时目录)。本轮为开发脚本维护，不改变应用版本或生成安装包。

可双击根目录 `本地调试运行.cmd` 启动同一开发入口，脚本结束后不执行 pause；专用控制台可随启动命令结束关闭，Windows Terminal 的标签关闭也受其配置影响。在用户已打开的终端执行 `pnpm desktop:dev`，退出后应返回提示符，不强制关闭整个终端。主窗口默认完全退出；选择并保存“常驻托盘”时，开发进程和终端继续等待，需从托盘“退出”。

调试日志以 `[startup]` 记录原生 setup、窗口尺寸、数据库、服务就绪和页面加载的累计耗时；前端控制台记录模块及首次视图耗时。应将 pnpm/Vite/Rust 编译与运行期分开测量，冷启动和再次启动分别记录。Windows 的 webview Started 对应内容加载，Finished 对应页面导航完成，都不等于完整 React 界面已可用；还需核对 `[startup] first view` 和实际界面。早期约 27/41 秒及 498 ms 的采样保留在[桌面记录](../testing/desktop-import-results.md#8-窗口启动与关闭行为修复)，2026-10-08 的请求跟踪已将本机长等待定位到 Vite 对大量生成目录的扫描，最新结论见[启动性能修复](../testing/performance-results.md#6-开发启动扫描瓶颈与修复)。

`vite.config.ts` 排除 `src-tauri`、私有工具、运行数据及生成目录的前端监听；Rust 仍由 Tauri CLI 独立监听。依赖扫描只以 `index.html` 为入口，避免遍历工具源码里的 HTML。前端源文件和配置热更新保持；新依赖缓存和 WebView 用户目录的样本中，页面完成约 618 ms，完整首个 React 视图约 1.3 秒。测量启动慢时可临时为开发子进程设置 `DEBUG=vite:time,vite:deps`，记录 HTTP 请求和依赖扫描耗时，定位后撤销调试环境变量。不要只根据 Vite ready、Cargo finished 或窗口显示判定全部界面就绪。

0.1.4 最终开发入口重复启动实测约 6.2 秒，包括 PowerShell、pnpm、Tauri CLI、Vite 准备及 0.57 秒 Cargo 增量检查；新版本第一次重新编译另需约 27.3 秒。这些历史数据来自保留 Cargo 缓存的旧入口，不代表 2026-10-12 编译前完整清理后的启动等待。前端日常修改仍使用热更新；已经编译的应用运行期约 1.3 秒。编译准备与应用运行期继续分开记录。

Tauri CLI 在应用正常退出时[停止 beforeDevCommand 的子进程树](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.1/crates/tauri-cli/src/dev.rs#L311-L336)。Windows 终止前端 pnpm 子进程可能打印 ELIFECYCLE；应检查主开发入口退出状态、应用和监听端口是否仍存在，而不能把子进程停止日志直接当成应用仍常驻。本轮开发入口返回 0，应用和 1420 监听均结束，WebView2 清理另打印 1412 诊断，未隐藏这些原始日志。

需要一个本地调试 exe 时手动执行：

```powershell
pnpm desktop:build
```

`本地打包运行.ps1` 使用 debug 和 no-bundle，不要求 Git 工作区干净；输出 `.local/build/SubtitleVocabularyList.exe` 与 `run-local.ps1`。优先执行生成的 launcher，它会补齐本机 PATH 和指定的数据目录。此 exe 仍依赖当前工作区、本机媒体工具和 WebView2，不用来代替独立发行目录。调试代码和本地 exe 默认使用 `D:\SubtitleVocabularyList\Data`，可通过脚本 `-DataDirectory` 或进程 `SVL_DATA_DIR` 显式覆盖；发行程序默认使用可执行文件目录下的 Data。主窗、小窗缓存及进程临时目录位于资料目录内，迁移和实际验证见[目录说明](data-directory.md)。

三个脚本均支持 `-CheckOnly`；开发运行、调试构建和发行打包的执行计划可在不启动应用、不构建安装包的情况下检查。此次在工作区父目录使用 Windows PowerShell 5.1 执行三个 CheckOnly 均通过，识别到本机 FFmpeg/ffprobe/Tesseract、Whisper 与模型；PowerShell 语法和 Rust 全目标检查通过；以模拟 pnpm/cargo 验证了本地构建的 debug/no-bundle 参数、exe 导出、生成 launcher 的语法、带空格路径及环境恢复，没有调用真实编译器或启动程序。未据这些检查声称本轮启动了调试程序或生成了新安装包。

2026-10-11 根目录入口迁移后，在工作区父目录用 Windows PowerShell 5.1 执行三个脚本的 `-CheckOnly`，均返回 0，工作区、数据目录和输出目录正确；三个入口及源码导出脚本的语法检查通过。未启动客户端、生成 exe 或构建安装包；此项维护保持应用版本 0.6.0。

同日中文命名后，三个 pnpm 入口的 `-CheckOnly` 和从工作区父目录调用的 `本地调试运行.cmd -CheckOnly` 均返回 0。CMD 入口使用 UTF-8 和 CRLF，调用结束恢复原代码页；语法检查通过，未执行实际编译或打包。

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

0.8.2 的离线构建入口统一使用 `native-build-command.ps1` 执行需要重定向日志的 CMake／make 命令。Windows PowerShell 5.1 会把重定向的原生 stderr 包装为错误记录，`ErrorActionPreference=Stop` 因而会被普通弃用警告提前触发；辅助函数只在这次原生命令调用中允许继续记录输出，结束后恢复原错误策略，并根据实际退出码判定构建结果。警告保留在 configure／build／install 日志，真正的非零退出码仍中断并给出日志位置。不通过关闭全脚本错误处理或默认跳过离线工具来绕过问题。

2026-10-15 已用 Windows PowerShell 5.1 直接运行根目录默认入口，未传 `-SkipToolBuild`：OCR 依赖、Whisper 和 FFmpeg 正常重建，应用 release、NSIS、独立运行目录、源码材料及摘要均生成，脚本退出 0。实际版本、源码提交和产物摘要见[发行验证记录](../testing/offline-release-results.md)。此前只使用 `-SkipToolBuild` 生成 0.8.1 的结果不作为默认工具重建流程通过的证据。

仅在需要打包或发布版本时，提交源码并保持当前分支工作区干净，然后手动执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File 生成安装包.ps1
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
| 0.6 | 2026-10-07 | 补充可双击开发入口、可选托盘和终端生命周期，记录启动分阶段诊断及实测边界 |
| 0.7 | 2026-10-08 | 定位并修复 Vite 对原生/私有生成目录的扫描，明确页面与 React 首次视图耗时和热更新边界 |
| 0.8 | 2026-10-08 | 接入限定范围的开发启动自动清理、词库/缓存保护、CheckOnly 和失败延后，记录实际脚本验证 |
| 0.9 | 2026-10-11 | 将三个常用 PowerShell 入口和调试 CMD 入口移至根目录，更新辅助脚本定位及 pnpm／文档引用 |
| 0.10 | 2026-10-11 | 按用户指定改为中文文件名，同步 CMD、pnpm、使用文档及源码材料说明 |
| 0.11 | 2026-10-11 | 对齐 0.6.1 共享资料默认值、单实例标识、缓存位置及真实迁移与本地 exe 验证 |
| 0.12 | 2026-10-12 | 三个根目录入口编译前完整清理 Cargo 旧构建产物，CheckOnly 保留只读计划；明确重新编译成本和检查边界 |
| 0.13 | 2026-10-15 | 修正 Windows PowerShell 5.1 原生警告重定向误中断，记录无跳过参数的真实完整打包结果 |
| 0.14 | 2026-10-17 | 增加三个构建入口的运行产物检查，提前提示 exe 占用及退出方法，保留完整 Cargo 清理规则 |
