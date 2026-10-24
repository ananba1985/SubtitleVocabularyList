# SubtitleVocabularyList 资料目录迁移与自动清理验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-DATA-DIR-001 |
| 文档版本 | 0.4 |
| 更新日期 | 2026-10-23 |
| 状态 | 历史迁移记录保持；自动清理最小正常验证通过，本轮未操作日常资料、打包或安装 |
| 对应版本 | 原迁移 0.6.1／schema 7；历史两份策略 0.26.3、临时恢复策略 0.26.4、jobs 分层 0.26.6／schema 10 |
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

本节保留上一轮“两份自动备份”策略的历史实现和验证。**0.26.4 起已由第 6 节的临时恢复策略替代**，以下旧命令及两份保留结果不作为当前策略的说明。

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

## 6 0.26.4 临时恢复备份最小验证

按用户新要求，自动数据库备份仅作为升级期间的临时恢复材料，不再固定保留两份。非空旧 schema 升级前保存一致性快照；升级、迁移校验和主界面首次正常读取初始资料均通过后，沿用原后台维护入口清除全部程序拥有的自动快照、其 sidecar 和已验证的迁移临时目录。自动快照区为空时删除空目录；有未知名称文件时保留该文件和目录。

迁移临时快照在成功门槛通过后直接清除，不再复制到自动备份区再删除。Store 升级异常会直接返回、宿主未完成或主界面未正常读取初始资料时不会进入清理；当前库 quick_check／foreign_key_check 不通过也会在删除前返回，恢复副本保留。手工备份、完整例句、原声和学习历史的边界不变。

2026-10-23 实际执行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features desktop --lib data_maintenance::tests::successful_upgrade_keeps_recovery_until_startup_then_clears_only_automatic_backups --offline
pnpm version:check
git diff --check
```

选中的 **1 项正常测试通过，0 失败**，其余 117 项未执行；桌面 feature 的测试编译、统一版本检查（0.26.4）及差异检查通过。最小合成场景确认：

| 阶段／内容 | 实际结果 |
| --- | --- |
| 升级前恢复材料 | schema 9 正常升级为 10；清理前读取自动快照，仍为 schema 9，原词条可读，证明尚未正常启动清理时恢复副本存在 |
| 成功后的自动材料 | 调用成功启动后的维护入口，清除升级前快照及模拟旧两份策略留下的另外两份自动快照；空 `backups/automatic` 目录移除 |
| 迁移和旧目录 | 已验证的迁移快照／临时包装目录、旧安装缓存和空旧安装库移除，不再产生迁移快照转存副本 |
| 手工及业务资料 | 手工备份和手工目录文件逐字节保持；例句／作答的完整导出与清理前相同，旧词条、原声音频字节及当前 WebView 缓存保持 |

本轮没有新增故障注入、失败／并发／链接矩阵、完整回归或桌面 GUI 启动验收；失败保留描述为现有控制流约束，不冒充已执行单独失败场景。前端与迁移脚本本轮未改动，未重复其上一轮已通过检查。未构建或安装发行包，未启动新版本操作或删除日常 Data；当前已有文件需运行包含本改动的新版并通过成功门槛后才会按新规则清除。

## 7 0.26.6 jobs 分层与引用回收最小验证

本轮按用户要求实现 NFR-02／NFR-06 的长期语料音轨与 OCR 中间文件分层。新导入由共用 `Store::install_corpus` 发布到 corpora；旧 jobs 中来源仍使用的语料／音轨先复制、核对摘要，保留完整正文与额外元数据，再用一个短事务按旧路径条件更新来源。回收重新读取来源、原声资产、持久任务和前台内存回执等引用，任务历史与学习历史不因清理而删除。

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features desktop --lib job_storage::tests::normal_publication_migration_and_reference_cleanup_keep_complete_learning_data --offline
pnpm version:check
git diff --check
```

2026-10-23 最终选中的 **1 项正常测试通过，0 失败**，其余 118 项未执行；包含桌面 feature 的原生代码编译通过。该合成正常场景覆盖：

| 内容 | 实际结果 |
| --- | --- |
| 新语料发布 | 新导入的完整语料及整集音轨发布到 corpora，来源路径有效，cached_source 可复用；完成任务的原工作目录可回收 |
| 旧资料迁移 | 旧 jobs/video 中仍被来源使用的语料／音轨迁移到 corpora；两条完整句子和额外 provenance 字段保持，仅 audioPath 改为长期位置，整集音轨字节相同 |
| 无引用产物回收 | 已迁移旧导入目录、无引用导入／OCR 残留和已发布新任务工作目录移除 |
| 任务引用保持 | 持久化运行任务的工作目录及前台终态回执仍引用的结果文件保持 |
| 完整学习与未知内容保持 | 已收录原声片段字节、含原声关联的词条／例句／一次作答完整导出保持；任务历史未删除，未知手工目录和语音缓存代表文件保持 |

第一次正常场景通过后，代码审查发现任务工作租约需要延续到终态保存完成；已修正该时序，再执行同一项正常验证通过。没有扩展为并发或故障测试。清理只在首屏成功后的后台入口或待维护任务完成事件触发；运行工作线程持共享文件租约，独占维护拿不到租约时延后，不主动终止任务或常驻轮询。

本轮前端代码和公开 DTO 未变化，不重复前端构建；未运行完整回归、实际新导入／OCR／播放／同步、文件占用／链接／竞争矩阵或 GUI 安装升级验收。桌面／移动资产可读范围的适配已接入，Android 原生包及真机链路未重建／重验，不据桌面编译声明其通过。没有对日常 `D:\SubtitleVocabularyList\Data` 执行迁移或删除，没有打包或安装。

## 8 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-11 | 记录真实资料和缓存迁移、路径修正、24 表比较及本地 debug exe 的实际启动，保留安装边界 |
| 0.2 | 2026-10-23 | 记录首屏成功后的旧目录回收、两份自动快照、迁移回执及手工／完整学习资料保持的最小正常验证，区分真实 Data 和实装边界 |
| 0.3 | 2026-10-23 | 记录 0.26.4 临时恢复策略的正常验证：升级前可恢复、首次成功启动后清除全部自动副本，保留手工和学习资料；旧策略记录明确标为历史 |
| 0.4 | 2026-10-23 | 记录 0.26.6 新／旧语料发布、来源与任务引用回收及原声／学习历史／未知文件保持的最小正常证据，区分实际 Data 与原生验收边界 |
