# SubtitleVocabularyList Android 开发验证记录

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-ANDROID-001 |
| 文档版本 | 0.2 |
| 更新日期 | 2026-10-22 |
| 状态 | 开发中；完整 Android debug 构建、离线资源入包和 Windows 共用入口启动通过，手机安装／功能验收尚未完成 |
| 依据 | [Android PRD](../requirements/android-PRD.md)、[开发计划](../planning/android-development-plan.md)、[开发指南](../development/android-development.md) |

## 1 当前实际范围

0.17.0 接入 Android 工程、会话工具链脚本、共享宿主命令目录及手机单词本界面。收录、合并、搜索、编辑与任务仍由原 Application／Store 执行。手机使用独立私有目录，不调用桌面进程或模型；正式运行只通过服务器交换个人资料。

已接入共用离线词库暂存、资源清单校验、随包 assets 和 Kotlin 后台原子安装；安装后由 Rust 重新载入同一词库。个人 SQLite 不参与资源复制。当前同步、播放、语音与 OCR 仍待接入；能力快照如实标注，不以共享核心已编译宣称 P0 完成。

## 2 已执行的最少正常检查

| 检查 | 实际结果 | 证明边界 |
| --- | --- | --- |
| `pnpm android:check` | 通过；本机配置选 E 盘 SDK，JDK 21.0.10，NDK 28.2.13676358，Rust 1.95.0 arm64 目标可用 | 只读检查不代替构建；不改全局环境 |
| `pnpm android:init` | 生成受控 Android Studio 工程；SDK／target 36，minSdk 29 | 初始化不等于运行 |
| Windows `cargo check --features desktop` | 共享宿主抽取后通过，1 分 08 秒 | 编译边界；后续实际启动另见下行 |
| `pnpm desktop:dev -DataDirectory .local/android-host-acceptance/Data -Incremental` | 当前共享核心已成功启动；database／services／webview Finished 均完成，页面约 781 ms 完成加载，随后结束调试进程 | 只核对共用宿主正常入口；使用具名隔离目录，未操作正式 Data，不扩展桌面回归 |
| `pnpm commands:generate` 与构建中的 commands:check | Rust 唯一命令表生成／检查通过 | 导出工具仍经 Windows ipc-types 特性执行；不复制一套手机类型 |
| 构建中的 `tsc --noEmit`、Vite | 手机布局和共享组件检查、前端编译通过 | 不代替手机上的布局和输入验收 |
| `node scripts/prepare-android-assets.mjs` | 6 个词库、许可及源清单文件按共用 manifest 校验并暂存 | 不读取个人 Data；构建资源不提交 Git |
| `pnpm android:build` | arm64 Rust、Kotlin、Gradle 及完整前端构建通过，生成本地 debug APK | 仅开发调试构建，未签发行包、制作发布材料或发布 |
| debug APK 资源检查 | 包内存在 svl/manifest.json、base.sqlite3（67,145,728 字节）、ecdict.sqlite3（71,118,848 字节）及 ECDICT 许可 | 证明资源已入包；手机安装、首次提取及实际离线查询仍待验证 |

开发目标设备为 Mi 10 Pro、API 33、arm64，ADB 曾正常授权。首次安装返回 `INSTALL_FAILED_USER_RESTRICTED`；用户按提示调整选项后再次尝试时 USB 设备连接断开，后续 ADB 查询明确返回 device not found；重启电脑侧 ADB 服务后曾显示 offline，尚未恢复可执行 shell 的连接。用户随后补充无备用 SIM，正在核查本地 APK 安装与既有 Root 状态；未执行手机解锁、刷机或修改安全中心配置。本轮尚未成功安装本项目，不能以构建成功或设备枚举代替运行证据；未传入真实账号凭据。

## 3 工具问题与实际处置

- Windows 最初拒绝创建 jniLibs 符号链接。用户开启开发者模式后，链接成功并完成构建；未绕过系统权限或重写工具链。
- 初次 Rust 目标添加遇到镜像组件 URL 404；从官方源安装单独的 1.95.0 工具链及 arm64 标准库成功，没有更改默认工具链。
- 首次 Gradle 构建发现 Cargo 插件源码与项目分处 C、D 盘，Kotlin 增量缓存报告 different roots。编译器回退后首次构建成功；本工程固定 `kotlin.incremental=false`，随后含本仓库原生插件的构建通过。Gradle 任务复用与 Rust 增量编译仍保留。
- SDK 与 JDK 仅在项目开发进程设置；生成的本机路径、缓存和原生库均忽略，不提交设备序列号或凭据。

开发热更新使用 127.0.0.1 与 USB 调试转发。完整 debug APK 已内置界面、Rust 库及词典，正式运行架构不依赖开发服务器、Windows 客户端或同一网络。实际手机断开电脑后的运行证据仍须在设备可用后补齐。

## 4 后续验证

手机稳定连接并允许安装后，继续同一正常路径：启动并调用共享 Rust、首次资源安装／离线查询、三类条目录入、同词追加例句、编辑与重开保持。之后按计划接入真实账号／完整同步、复习、媒体和 OCR。只补尚缺证据，不重跑已通过场景或扩大测试范围。

## 5 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-22 | 初步工具链、双目标原生编译和类型检查；记录符号链接阻塞 |
| 0.2 | 2026-10-22 | 记录权限解决、完整 debug 构建、共用离线资源入包、Windows 隔离入口启动及手机安装／断连边界 |
