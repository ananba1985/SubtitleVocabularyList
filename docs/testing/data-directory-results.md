# SubtitleVocabularyList 资料目录迁移与自动清理验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-DATA-DIR-001 |
| 文档版本 | 0.2 |
| 更新日期 | 2026-10-23 |
| 状态 | 历史迁移记录保持；自动清理最小正常验证通过，本轮未操作日常资料、打包或安装 |
| 对应版本 | 原迁移 0.6.1／schema 7；清理增量验证工作区 0.26.3／schema 10 |
| 对应需求 | NFR-02、NFR-06，TC-49 |
| 依据 | [目录方案](../development/data-directory.md)、[测试计划](test-plan.md) |

## 1 授权与范围

用户最后确认安装位置自行选择为 `D:\SubtitleVocabularyList`，要求测试用调试版和本地 exe 默认匹配该目录的 Data，并迁移已有资料和缓存。本轮执行这一正常迁移和本地启动，不构建安装器，不安装或替换系统中的发行程序。

## 2 资料迁移

主要源为工程 `.local/dev-data`，schema 6；旧安装版 AppData 词库 schema 1，entries、sources 和 examples 均为空。没有把 pot-acceptance、replacement-acceptance 等隔离测试数据混入日常资料。

客户端退出后，以 SQLite backup 保存包含 WAL 的一致性快照，将主要源目录整体移动到 `D:\SubtitleVocabularyList\Data`。修正 sources.corpus_path 24 条、sources.audio_path 24 条、tasks.snapshot_json 3 条的旧目录定位；对快照和目标的 24 张表逐行比较，除这些预期路径变化外记录相同，integrity_check 为 ok。

| 项目 | 实际结果 |
| --- | --- |
| 来源记录 | 25 条，包含 24 份已导入剧集；app_info 按 imported_at 统计，显示 sourceCount 24 |
| 例句／出现记录 | 10,308 条 examples、77,050 条 occurrences 保持 |
| 中文解释 | 迁移时 11,774 条 explanations 保持；正常启动的既有后台准备又保存 9 项，最终为 11,783 |
| 已收录词条 | entries 为 0，未用验收样本制造日常词条 |
| 路径文件 | 48 个来源字幕／音轨文件路径、6 个保存媒体文件全部存在 |
| 原数据库快照 | Data/backups/migration-6b9b8bd6-ce87-475a-8c49-2118ed9741b3/before.sqlite3 |
| 旧安装资料 | 空库及原文件保存到 Data/backups/legacy-installed-data |
| 缓存 | 最新开发缓存移入 Data/cache/webview；旧安装缓存保存到 Data/backups/legacy-installed-cache |

三个旧 AppData 目录和原 `.local/dev-data` 在启动、退出后均不存在，没有重新创建。跨盘旧缓存及空库迁移采用已核对绝对路径和链接的原生 robocopy /MOVE，三项均返回 1，表示正常移动，不把 robocopy 的非零成功码记成失败。

## 3 本地运行与检查

`cargo check --features desktop` 通过。`pnpm desktop:build` 实际完成前端类型／构建和 Windows debug 编译、导出，生成 `.local/build/SubtitleVocabularyList.exe` 与默认 D 盘资料路径的 run-local.ps1，命令返回 0。调试入口的 -CheckOnly 也显示同一资料目录；没有再次启动独立的调试 CLI 或进行兼容矩阵测试。

不设置 SVL_DATA_DIR，直接启动新导出的 exe，app_info 返回 version 0.6.1、dataDirectory 为指定 D 盘目录、schemaVersion 7、sourceCount 24。已实际完成 schema 6→7。真实 WebView2 进程的 user-data-dir 和 Crashpad 目录均在 Data/cache/webview/EBWebView。主界面已加载并显示 0.6.1；临时目录代码在 Tauri 创建线程前为本进程及子进程设置 Data/cache/temp，不修改系统环境。

检查后通过 app_quit 退出测试客户端，主程序进程结束。既有 WebView2 Chrome_WidgetWin_0 注销错误 1412 仍出现，没有把目录修复描述为已解决该退出诊断。

## 4 未覆盖边界

未执行新安装包实装、跨发行版本升级、实际音频重听、站点同步、权限／故障注入或并发竞争测试。文件存在和数据比较证明本次迁移范围，不能代替这些场景验收。安装版“可执行文件目录/Data”的路径规则已经写入源码，发行分支实际运行留给用户手动生成新包后的安装验证；现存 0.1 包仍是旧代码。

## 5 成功启动后的自动清理最小验证

2026-10-23 按用户授权补齐 NFR-06 的本地回收规则；需求、架构与[目录规则](../development/data-directory.md#4-成功启动后的自动清理)同步。每次桌面启动只在主界面初始资料读取成功、宿主完整 setup 通过后安排一次后台维护；当前库的迁移、quick_check 和 foreign_key_check 为清理前置条件。非空旧 schema 升级前新增一致性自动快照，现有无新 schema 的启动不会重复备份完整数据库。

### 5.1 已执行的最小正常验证

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features desktop --lib data_maintenance::tests::successful_startup_retires_owned_artifacts_and_keeps_two_backups_and_learning_data --offline
cargo check --manifest-path src-tauri/Cargo.toml --features desktop --lib --offline
pnpm exec tsc --noEmit
pnpm version:check
```

选中的 **1 项正常测试通过**，其余 117 项未执行。Windows 桌面 lib 编译检查、前端 TypeScript 检查和统一版本检查通过。正常测试在合成临时 Data 中覆盖：

| 场景 | 实际结果 |
| --- | --- |
| 旧 schema 正常升级 | schema 9 升级为 10，先生成可读取的自动快照，旧词条内容保持 |
| 旧目录回收 | 旧安装缓存、空旧安装库及迁移包装目录移除；迁移数据库先转存为一致性自动快照 |
| 备份轮换 | 超过两份时，仅保留快照时间最新的两份，按实际文件名核对 |
| 手工内容保持 | `backups/manual-backup.sqlite3`、手工子目录及笔记文件逐字节保持 |
| 学习资料保持 | 例句和作答的完整同步导出清理前后相同，旧词条可读，原声音频代表文件字节保持，当前 WebView 缓存代表文件保持 |

另外用实际 `scripts/data-migration.py` 的 snapshot／retarget 正常入口处理一个隔离合成 SQLite：路径改写及全部表比较成功后，生成 `verified=true`、`integrity=ok` 且 targetRoot 匹配目标目录的 `migration-verified.json`；业务文字保持。Python 语法检查通过，正常脚本验证退出 0，合成临时目录已移除。

初次无桌面 feature 的 lib 测试编译还暴露出已有 assessment 测试引用桌面限定夹具的限制；本轮未修改该无关测试，改用本次实际涉及的 desktop feature 执行上述单个正常例。实现过程中发现的 SQLite API 名称和夹具字段编译问题已修正，不将失败的编译尝试计为通过。

### 5.2 保留边界

未运行新 GUI、生成或安装发行包，也未对 `D:\SubtitleVocabularyList\Data` 执行清理；目录里的现有文件将在用户运行包含本实现的新桌面版本且通过成功门槛后按规则处理。前端触发和宿主一次性调度已接入并经类型／编译检查，合成测试不代替真实版本升级和主窗启动验收。

手工备份和来源不明文件保留，包括未标记为自动快照的旧人工释义修复备份。非空旧库、额外未知文件、链接和占用等保护属于代码约束；本轮未追加负例、故障注入、真实进程竞争、兼容矩阵或完整回归，也未重新验证同步／播放。Android 不接入此 Windows 旧目录回收流程，不因本次改动新增其升级备份。

## 6 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-11 | 记录真实资料和缓存迁移、路径修正、24 表比较及本地 debug exe 的实际启动，保留安装边界 |
| 0.2 | 2026-10-23 | 记录首屏成功后的旧目录回收、两份自动快照、迁移回执及手工／完整学习资料保持的最小正常验证，区分真实 Data 和实装边界 |
