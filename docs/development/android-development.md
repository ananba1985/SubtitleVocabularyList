# SubtitleVocabularyList Android 开发指南

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-DEV-ANDROID-001 |
| 文档版本 | 0.1 |
| 更新日期 | 2026-10-22 |
| 状态 | 初版待评审；已做本机只读环境盘点，Android 工程与下述新脚本尚未创建 |
| 依据 | [Android PRD](../requirements/android-PRD.md)、[架构与开发设计](../design/android-architecture.md)、[开发计划](../planning/android-development-plan.md) |

## 1 开发原则

Windows 和 Android 在同一仓库维护。词库、合并、复习、同步及生成类型只有一套来源；Kotlin 只负责系统文件、Intent、媒体、语音、凭据和生命周期。手机页面可有不同布局，但不在页面重算掌握状态或维护第二份同步记录。

日常开发使用增量构建与一条对应功能的正常流程。不调用 Windows 打包脚本来构建 Android，不在每轮自动 cargo clean，不生成发行目录、发布包或安装材料。开发运行工具所需的 debug 编译／设备启动属于本地调试；手动导出 APK／发行交付另设入口，只有用户请求时执行。

## 2 本机环境盘点

以下信息来自本轮目录、SDK 清单、配置文件和版本命令，路径仅作为当前电脑的开发说明，不写死到项目源码或提交 local.properties。

| 项目 | 已核实状态 | 后续动作 |
| --- | --- | --- |
| Android Studio | 安装于 `D:\Program Files\Android\Android Studio`，产品构建号 `261.26222.65.2614.16379836`；Studio 2026.1.4 配置指向 E 盘 SDK | 复用现有安装，不重新安装 IDE |
| 主 SDK | `E:\Android_SDK`；Platform 34、35、36.1；Build-Tools 34.0.0、35.0.0、36.0.0、36.1.0；Platform-Tools 37.0.1 | 初始化时按生成工程锁定所需 SDK；android-36.1 不当作 android-36 已安装的证明 |
| 主 SDK 的 NDK／CMake／Command-line Tools | 本轮未发现对应目录 | 在 SDK Manager 补齐需要的组件；CMake 若复用其他已安装位置，先核对并记录版本 |
| 第二套 SDK | Visual Studio 共享目录下另有 Android SDK，API 34／35、Build-Tools 35.0.0、Command-line Tools 12.0 | 不删除；本项目选择主 SDK，不混用其工具 |
| 当前 PATH 中 adb | 来自 Visual Studio 共享 SDK | Android 开发会话把主 SDK 的 platform-tools 放到前面 |
| 当前 ANDROID_HOME／ANDROID_SDK_ROOT／NDK_HOME | 本轮读取的三个进程环境变量均为空；主 SDK 尚无 NDK | 项目开发入口按本机配置设置当前进程环境，不修改系统全局默认值 |
| JAVA_HOME | 当前是 `C:\Program Files\BellSoft\LibericaJDK-25`，Studio 自带 JBR 也是 25.0.3 | 不直接假设生成工程的 Gradle 支持 JDK 25 |
| 已有 JDK 21 | `D:\.gradle\jdks\jetbrains_s_r_o_-21-amd64-windows.2`，21.0.10 | 优先供项目 Gradle 会话使用，锁定前与 wrapper／AGP 对照 |
| Rust | rustc 1.95.0；已安装目标只有 `x86_64-pc-windows-msvc` | 增加 `aarch64-linux-android`；不默认安装全部 ABI |
| pnpm／Tauri | pnpm 10.6.5；项目 Tauri CLI 2.12.1 | 复用锁文件中的版本，Android 支持不作为顺带升级整套依赖的理由 |
| 手机／模拟器 | 本轮未枚举、未启动、未安装 | 实施时选择实际设备并记录型号、系统和 ABI |

只读盘点不证明 JDK、Gradle、NDK 已能共同完成构建。本轮没有下载组件、接受 SDK 许可、初始化工程或改变全局配置。

Tauri 的 Android 开发需要 SDK、NDK 与相应 Rust 目标，安装 Android Studio 本身不是全部条件。[Tauri 环境要求](https://v2.tauri.app/start/prerequisites/)

## 3 环境准备顺序（AND-M-00）

1. 在现有 Android Studio 的 SDK Manager 中确认 SDK 路径为 `E:\Android_SDK`，补齐 Command-line Tools、所需 NDK、必要 CMake 及生成工程实际要求的 Platform。记录确切版本，不能按目录名称排序自动取“最新 NDK”。
2. 初始优先选兼容 Tauri 生成工程的 NDK r28 或更新稳定版，记录完整 revision；Native 库同步采用 16 KiB 页构建设置。不要因某个本机目录已有其他版本就忽略生成工程要求。[Android Native 页大小要求](https://developer.android.com/guide/practices/page-sizes)
3. 为本项目选择 JDK 21 与相容的 Gradle／AGP，保留生成 wrapper；仅当有具体不兼容证据时调整版本。Gradle 从 8.5 可使用 JDK 21 运行，JDK 25 则需要 9.1.0 或更高，不能直接混配。[Gradle Java 兼容表](https://docs.gradle.org/current/userguide/compatibility.html)
4. 安装 `aarch64-linux-android` Rust 目标。需要模拟器时再增加所用 ABI，用户未要求时不扩大目标矩阵。
5. 在 AND-M-01 完成宿主特性及平台配置后初始化 Android 工程，确认 IDE 与命令行使用同一 SDK／JDK／NDK，再进行第一次 debug 开发运行。

下面只展示**组件准备后**可采用的会话环境；NDK_HOME 由开发入口读取已锁定的完整版本路径。本次未执行这些设置。

```powershell
$env:JAVA_HOME = 'D:\.gradle\jdks\jetbrains_s_r_o_-21-amd64-windows.2'
$env:ANDROID_HOME = 'E:\Android_SDK'
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
$env:PATH = "$env:JAVA_HOME\bin;$env:ANDROID_HOME\platform-tools;$env:PATH"
```

拟增 `scripts/android-environment.ps1` 只负责加载本机配置和检查路径、版本、目标，不安装工具、不修改系统环境、不终止其他项目的 ADB。用户特定路径保存在忽略的本机配置，仓库内仅保留示例字段。NDK 完整路径与 Gradle JDK 明确显示在检查结果中，缺项一次列清。

## 4 工程初始化与开发入口

### 4.1 当前可用与未来入口

当前已有的命令为 `pnpm tauri android init/dev/build`，本轮只执行了三者的 `--help`，确认 CLI 参数。Android 工程尚未初始化，下面的项目级 `android:*` 脚本均是开发计划中的目标，当前不能宣称已经可运行。

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

## 7 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-22 | 记录现有 Android Studio／双 SDK／JDK／Rust 状态、准备顺序、拟增入口、增量开发与单一业务核心维护规则 |
