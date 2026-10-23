# SubtitleVocabularyList Android 开发指南

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-DEV-ANDROID-001 |
| 文档版本 | 0.2 |
| 更新日期 | 2026-10-22 |
| 状态 | 开发入口与 Android 工程已接入；工具链与首次 debug 构建通过，真机部署待手机允许 USB 安装；见 Android 验证记录 |
| 依据 | [Android PRD](../requirements/android-PRD.md)、[架构与开发设计](../design/android-architecture.md)、[开发计划](../planning/android-development-plan.md) |

## 1 开发原则

Windows 和 Android 在同一仓库维护。词库、合并、复习、同步及生成类型只有一套来源；Kotlin 只负责系统文件、Intent、媒体、语音、凭据和生命周期。手机页面可有不同布局，但不在页面重算掌握状态或维护第二份同步记录。

日常开发使用增量构建与一条对应功能的正常流程。不调用 Windows 打包脚本来构建 Android，不在每轮自动 cargo clean，不生成发行目录、发布包或安装材料。开发运行工具所需的 debug 编译／设备启动属于本地调试；手动导出 APK／发行交付另设入口，只有用户请求时执行。

## 2 本机环境盘点

以下信息来自本机目录、版本命令和首次实际构建；具体机器路径仅放忽略的 `.local/android-development.json`，示例配置见 `examples/android-development.example.json`。

| 项目 | 已核实状态 | 使用方式 |
| --- | --- | --- |
| Android Studio | 已有安装，产品构建号 `261.26222.65.2614.16379836` | 复用现有 IDE |
| 主 SDK | `E:\Android_SDK`；已补齐 Platform 36 与 Command-line Tools 23.0，Platform-Tools 37.0.1 | 开发脚本从本机配置选定，不取另一套 SDK 的工具 |
| NDK | `28.2.13676358`，已完成 arm64 Rust 原生库编译 | 版本由 `scripts/android-toolchain.json` 固定 |
| 第二套 SDK | Visual Studio 共享目录下另有 SDK | 保留，脚本只改变项目进程环境 |
| JDK | 项目会话使用现有 21.0.10；系统 JAVA_HOME 仍是 25 | `.local` 指定 javaHome，不更改全局默认 |
| Rust | Android 会话使用单独的 1.95.0 工具链与 `aarch64-linux-android` | 未更改其他项目默认工具链 |
| pnpm／Tauri | pnpm 10.6.5；CLI 2.12.1 | 使用现有锁定依赖 |
| Gradle／AGP／Kotlin | wrapper 9.6.1、AGP 9.3.1、Kotlin 2.2.10 | 已完成首次 debug 构建；模板迁移警告保留，不顺带升级 |
| Kotlin 增量缓存 | Cargo 插件源码在 C 盘，项目在 D 盘时触发跨盘根路径异常 | 本工程设置 `kotlin.incremental=false`；保留 Gradle 任务复用和 Rust 增量编译 |
| Windows 开发者模式 | 用户开启后符号链接成功 | 是 Windows 开发机设置，与手机正式运行无关 |
| 设备 | Mi 10 Pro，API 33，arm64 | ADB 已授权；安装被手机系统拒绝，等待用户允许 USB 安装 |

首次完整 debug 构建已经证明选定工具链可以协作；真机启动和功能验收仍须分别取得证据。本轮未生成发行包或发布材料。

Tauri 的 Android 开发需要 SDK、NDK 与相应 Rust 目标，安装 Android Studio 本身不是全部条件。[Tauri 环境要求](https://v2.tauri.app/start/prerequisites/)

## 3 环境准备顺序（AND-M-00）

1. 在现有 Android Studio 的 SDK Manager 中确认 SDK 路径为 `E:\Android_SDK`，补齐 Command-line Tools、所需 NDK、必要 CMake 及生成工程实际要求的 Platform。记录确切版本，不能按目录名称排序自动取“最新 NDK”。
2. 初始优先选兼容 Tauri 生成工程的 NDK r28 或更新稳定版，记录完整 revision；Native 库同步采用 16 KiB 页构建设置。不要因某个本机目录已有其他版本就忽略生成工程要求。[Android Native 页大小要求](https://developer.android.com/guide/practices/page-sizes)
3. 为本项目选择 JDK 21 与相容的 Gradle／AGP，保留生成 wrapper；仅当有具体不兼容证据时调整版本。Gradle 从 8.5 可使用 JDK 21 运行，JDK 25 则需要 9.1.0 或更高，不能直接混配。[Gradle Java 兼容表](https://docs.gradle.org/current/userguide/compatibility.html)
4. 安装 `aarch64-linux-android` Rust 目标。需要模拟器时再增加所用 ABI，用户未要求时不扩大目标矩阵。
5. 在 AND-M-01 完成宿主特性及平台配置后初始化 Android 工程，确认 IDE 与命令行使用同一 SDK／JDK／NDK，再进行第一次 debug 开发运行。

开发入口已在当前进程设置并恢复以下环境；NDK_HOME 读取仓库工具链清单中锁定的 28.2.13676358，不修改系统全局配置。

```powershell
$env:JAVA_HOME = 'D:\.gradle\jdks\jetbrains_s_r_o_-21-amd64-windows.2'
$env:ANDROID_HOME = 'E:\Android_SDK'
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
$env:PATH = "$env:JAVA_HOME\bin;$env:ANDROID_HOME\platform-tools;$env:PATH"
```

已接入 `scripts/android-environment.ps1` 只负责加载本机配置和检查路径、版本、目标，不安装工具、不修改系统环境、不终止其他项目的 ADB。用户特定路径保存在忽略的本机配置，仓库内仅保留示例字段。NDK 完整路径与 Gradle JDK 明确显示在检查结果中，缺项一次列清。

## 4 工程初始化与开发入口

### 4.1 当前可用与未来入口

`android:check`、`android:init`、`android:dev`、`android:studio`、`android:build` 已接入，环境检查与初始化通过。dev 已完成 arm64 Rust 编译，Windows 符号链接权限仍阻止后续部署；studio／手动 build 未实际执行，不宣称已可安装。`android:package` 仍为按请求发布的后续入口。

| 拟增入口 | 用途 | 行为边界 |
| --- | --- | --- |
| `pnpm android:check` | 只读开发环境检查 | 确认工具路径／固定版本／目标和工程状态，不触发下载或编译 |
| `pnpm android:init` | 一次性初始化 Android 工程 | 环境和平台配置准备后使用；不覆盖已维护工程，不自动装全 ABI |
| `pnpm android:dev` | 日常手机增量调试 | 使用开发配置、mobile 特性和选定设备；不调用 NSIS 或发行入口 |
| `pnpm android:studio` | 在 Android Studio 打开同一工程 | 交给 Tauri Android dev 的 `--open`，保持其所需开发上下文 |
| `pnpm android:build` | 用户明确请求时导出本地 debug APK | 首期 arm64，留在指定持久产物目录；不自动升级、签名发布或安装 |
| `pnpm android:package` | 用户明确请求时发布 | 正式签名、离线资源、条款／源码材料与摘要；与日常开发分开 |

完成宿主与配置改造后，拟采用以下 CLI 路径封装入口：

```powershell
# Rust 目标准备好之后，一次性初始化。
pnpm tauri android init --skip-targets-install

# 调试配置由脚本传入；DEVICE 为实际选择的设备，不写死个人序列号。
pnpm tauri android dev --features mobile
pnpm tauri android dev --features mobile --open

# 仅用户请求导出本地 debug APK 时执行。
pnpm tauri android build --debug --target aarch64 --apk --features mobile
```

这些命令必须在架构中的 app-host／desktop／mobile 特性拆分和 Android 配置生效后使用。CLI 没有 Android `--no-default-features` 选项；不要把 cargo 参数拼成不存在的 Tauri 参数。已有 `desktop:*` 脚本继续保持入口，特性调整时更新内部调用。

### 4.2 前端和设备连接

Android 采用独立开发配置处理 Vite host／devUrl。当前 `pnpm dev` 固定绑定 127.0.0.1，不能假定物理手机可直接访问。初期优先 USB 与明确配置的本地调试转发；需要局域网调试时使用 Tauri 的 host／TAURI_DEV_HOST 机制并相应配置 Vite。只在开发配置开放所需地址，不改动正式 CSP 或把所有网络请求搬到 WebView。[Tauri Vite 配置](https://v2.tauri.app/start/frontend/vite/)

设备运行时确认实际使用主 SDK 的 adb。只对本项目选定设备执行操作，不自动重启全局 ADB 服务或清除设备数据。开发专用合成词库与个人服务器资料区分；不能把桌面正式 Data 目录复制进 APK 或用空库覆盖它。

## 5 日常改动与最低验证

| 改动类型 | 应修改的唯一来源 | 最少验证方向 |
| --- | --- | --- |
| 收录／合并／学习规则 | 现有 Rust 领域模块 | 一条对应核心正常流程；若改到共用路径，再确认 Windows 对应正常入口可用 |
| IPC 字段／命令 | Rust DTO 和统一注册／导出 | 生成类型并核对，前端类型检查；Kotlin 只调整相关平台 DTO |
| 手机列表／面板布局 | 手机页面与可复用组件 | 一次开发界面操作；无需生成发行包 |
| 文件／媒体／OCR／TTS | 本仓库 Android 插件或 Tesseract 库封装 | 实际设备的一条对应流程，记录素材及输出，不用模拟接口代替声音或视频 |
| 同步／授权 | 同一 Rust 连接与传输实现 | 对应真实授权／双向资料正常路径，保留远端账号原资料，不凭 ack 判定成功 |
| 文档 | 相应职责文档及索引 | 链接、编号、范围一致性、git diff --check 与 version:check |

运行检查遵守 AGENTS.md 的最低必要范围，达到当次正常路径完成条件后停止。额外负面、故障注入、压力、长期和扩大回归检查须先获得用户同意；计划中的发布活动也不自动授权安装或上架。

## 6 维护与交付约束

- 原声、词库、模型、凭据、签名密钥、local.properties 和机器路径不提交公开仓库。离线资源由固定来源与摘要准备，开发和发布复用同一资源清单。
- Tesseract NDK 构建只在源码、工具链或目标变化时更新缓存；Windows 与 Android 的构建输出分开，不复制 EXE／DLL 当手机资源，不引入 Android FFmpeg 大工具包来替代已选媒体接口。
- Android 工程维护源文件与构建结果区分，变更生成模板时审查差异，不反复删除重建工程。保留本机具名调试数据供检查；只清理真正临时中间文件。
- 应用版本继续由 package.json 驱动，Tauri／Cargo／Android 同步更新。文档交付不递增应用版本；首个实际大功能交付按 minor 规则，不预先写死发布版本、工期或标签。
- 提交只包含本轮文件，验证后提交推送当前分支，不新建分支或 PR；报告实际设备、构建、同步和交付证据，未执行项如实保留。

每次开发启动／debug 构建先由 `scripts/prepare-android-assets.mjs` 从共用 `.tools/release-dictionaries` 清单校验并暂存通用词库及许可，不读取个人 Data。Gradle 将 `.local/android-assets` 编入 APK；Android 在后台线程校验并原子安装资源到私有 files 目录，准备后再提供离线查询。实际设备结果以 [Android 验证记录](../testing/android-results.md) 为准。

## 7 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-22 | 记录现有 Android Studio／双 SDK／JDK／Rust 状态、准备顺序、拟增入口、增量开发与单一业务核心维护规则 |
| 0.2 | 2026-10-22 | 接入受控 Android 工程与会话工具链，记录首次 debug 构建、跨盘 Kotlin 缓存处理及共用离线资源准备；真机验收单独记录 |
