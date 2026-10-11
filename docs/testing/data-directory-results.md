# SubtitleVocabularyList 资料目录迁移验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-DATA-DIR-001 |
| 文档版本 | 0.1 |
| 更新日期 | 2026-10-11 |
| 状态 | 本机迁移及本地 debug exe 正常启动通过；新版发行包未构建或安装 |
| 对应版本 | 0.6.1，schema 7 |
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

## 5 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-11 | 记录真实资料和缓存迁移、路径修正、24 表比较及本地 debug exe 的实际启动，保留安装边界 |
