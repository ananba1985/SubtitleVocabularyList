# SubtitleVocabularyList Android 开发验证记录

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-ANDROID-001 |
| 文档版本 | 0.3 |
| 更新日期 | 2026-10-22 |
| 状态 | 开发中；Android 已实装，离线查词、三类收录、合并及编辑通过；0.17.1 正在修复重开时暴露的初始化竞态，完整 P0 尚未完成 |
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
| debug APK 资源检查 | 包内存在 svl/manifest.json、base.sqlite3（67,145,728 字节）、ecdict.sqlite3（71,118,848 字节）及 ECDICT 许可 | 证明资源已入包；首次安装与离线查词的真机结果见 2.1 |

开发目标设备为 Mi 10 Pro、API 33、arm64。用户解决手机 USB 安装限制后，0.17.0 已通过 ADB 成功安装并冷启动。操作对象为该设备上的真实 Tauri WebView，通过开发调试通道执行界面输入／点击；没有使用浏览器假后端或直接调用数据库替代页面流程。

### 2.1 手机单词本正常流程（AND-M-01／02）

| 操作 | 实际结果 | 需求对应 |
| --- | --- | --- |
| 冷启动、应用信息／能力／任务读取 | 页面展示本机词条数、版本及真实任务；六个通用词库／许可文件已首次安装，词典警告为空 | AND-M-01、AND-FR-01、16 |
| 输入 reluctant，点击离线查词 | 返回 ECDICT 中文释义，采用后保存原文和例句 | AND-FR-02、05；AND-AC-02 本地查询 |
| 再次收录 reluctant 和另一例句 | 匹配页默认选择合并；确认后仍为一个词条、保留两条例句，主动收录次数由 1 变 2 | AND-FR-03；AND-AC-01 合并部分 |
| 收录短语 look forward to、整句 Keep moving forward. | 类型及中文释义分别保存，单词本总计三个词条 | AND-FR-01、02；AND-AC-01 本地录入部分 |
| 搜索 reluctant 并打开详情 | 正确筛选，显示两条例句及各自语境释义、两次主动收录 | AND-FR-01、03 |
| 修改第二条释义并保存 | 界面取得更新结果；两条例句和收录次数保持 | AND-FR-01、03 |

测试资料均为合成词句，保留在本机应用私有库，供后续同步正常流程使用。具名调试材料放在忽略的 `.local/android-acceptance`；Windows 隔离资料仍为 `.local/android-host-acceptance/Data`。不读取或修改桌面正式 Data。

0.17.1 升级重开时观察到 app_info 返回 state not managed，推断页面请求早于应用状态注册；尚未取得该次原生日志确认完整时序。现增加共享 HostStartup 就绪通知，手机刷新先等待 app_ready；修正 Android 原生系统栏／键盘区域与搜索布局。包含修复的完整 debug 构建通过。重开保持及修复后的完整布局尚须在手机连接恢复后继续同一正常路径，不能据此前的收录结果标记全部 AND-AC-01 通过。

## 3 工具问题与实际处置

- Windows 最初拒绝创建 jniLibs 符号链接。用户开启开发者模式后，链接成功并完成构建；未绕过系统权限或重写工具链。
- 初次 Rust 目标添加遇到镜像组件 URL 404；从官方源安装单独的 1.95.0 工具链及 arm64 标准库成功，没有更改默认工具链。
- 首次 Gradle 构建发现 Cargo 插件源码与项目分处 C、D 盘，Kotlin 增量缓存报告 different roots。编译器回退后首次构建成功；本工程固定 `kotlin.incremental=false`，随后含本仓库原生插件的构建通过。Gradle 任务复用与 Rust 增量编译仍保留。
- SDK 与 JDK 仅在项目开发进程设置；生成的本机路径、缓存和原生库均忽略，不提交设备序列号或凭据。

开发热更新使用 127.0.0.1 与 USB 调试转发。完整 debug APK 已内置界面、Rust 库及词典，正式运行架构不依赖开发服务器、Windows 客户端或同一网络。实际手机断开电脑后的运行证据仍须在设备可用后补齐。

## 4 后续验证

当前只补 0.17.1 正常重开、已存三类资料及编辑保持，并确认修复后的手机布局；已通过的首次资源安装、查询、三类录入、合并与编辑不重跑。之后按计划接入真实账号／完整同步、复习、媒体和 OCR。只补尚缺证据，不重跑已通过场景或扩大测试范围。

## 5 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-22 | 初步工具链、双目标原生编译和类型检查；记录符号链接阻塞 |
| 0.2 | 2026-10-22 | 记录权限解决、完整 debug 构建、共用离线资源入包、Windows 隔离入口启动及手机安装／断连边界 |
| 0.3 | 2026-10-22 | 记录实际手机安装与单词本界面流程、真实 ECDICT 查询、同词合并和编辑；登记正常重开暴露的初始化竞态及待复核项 |
