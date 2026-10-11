# SubtitleVocabularyList 资料目录与本机迁移

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-DATA-DIR-001 |
| 文档版本 | 0.1 |
| 更新日期 | 2026-10-11 |
| 状态 | 0.6.1 实现；本机资料迁移已执行，启动结果见验证记录 |
| 依据 | NFR-02、NFR-06；[架构](../design/architecture.md)；[实际验证](../testing/data-directory-results.md) |

## 1 已确认方案

安装位置由用户在安装器中选择，不固定 D 盘。本机用户决定安装到 `D:\SubtitleVocabularyList`，安装目录保存程序和 tools，资料保存到其 `Data` 子目录。本轮按用户后续缩小的要求调整资料路径和测试入口，不执行发行打包或安装，也不改安装器的路径选择流程。

| 运行方式 | 未显式指定资料目录时 |
| --- | --- |
| 本地调试脚本／debug 程序 | `D:\SubtitleVocabularyList\Data` |
| 本地打包 exe／生成的 run-local.ps1 | `D:\SubtitleVocabularyList\Data` |
| 安装版／发行 standalone exe | 当前可执行程序所在目录下的 `Data` |

因此安装到本机选定的 D 盘位置后，三者默认共用一份资料。正式程序选择其他安装位置时仍跟随该位置；本机测试默认值不自动跟随以后更换的安装位置。隔离测试可显式用 `-DataDirectory` 或 `SVL_DATA_DIR`，环境变量优先于程序默认值，不修改系统级环境变量。

## 2 内容与缓存

Data 根目录保存 `vocabulary.sqlite3`，以及原有 media、jobs、credentials 等资料目录；例句、中文解释、学习状态、设置和同步元数据沿用数据库。`cache/webview` 为主窗口及查询小窗共同使用的 WebView2 资料，`cache/temp` 为本进程及子工具的 TEMP／TMP。启动前明确创建并使用这些目录，不通过 AppData 选择业务库或 WebView 缓存位置；无法访问选定位置时显示启动错误，不退回其他空词库。

主窗口通过绝对路径 WebviewWindowBuilder 创建，小窗使用同一目录，避免 WindowConfig 的相对目录处理。开发配置与发行配置采用同一应用标识，复用现有单实例机制；关闭或从托盘完全退出上一种运行方式后再切换。数据库结构升级沿用 Store 的迁移规则，旧程序不能读取更高版本结构。

源剧集是用户导入时选取的文件，不复制所有外部剧集到 Data；已经准备的音轨和已保存音频随资料目录迁移。Windows 自身的运行时安装、系统声音和操作系统记录不作为本程序业务资料。

## 3 迁移方式

迁移前退出客户端，选定一份实际使用的源目录；不把各类验收用的隔离词库合并到日常资料。执行：

```powershell
.\scripts\migrate-data.ps1 -SourceDirectory '旧资料目录' -DataDirectory 'D:\SubtitleVocabularyList\Data'
```

脚本要求目标不存在，核对绝对路径、源目录链接和进程状态，先用 SQLite backup 保存包含 WAL 提交内容的一致性快照，再移动整个源目录。仅改写 sources 的 corpus_path／audio_path 和 tasks 的 snapshot_json 中的旧根路径；逐表比较所有记录并检查数据库完整性。旧库快照保存到 Data/backups，用户词条与释义不作文本替换。

本机原开发库已移到 D 盘选定位置；旧安装库为空，单独存档到 `backups/legacy-installed-data`。最新开发 WebView 缓存移到 cache/webview，旧安装缓存独立存档到 `backups/legacy-installed-cache`。其他隔离测试目录保持独立。实际行数、路径和启动证据见验证记录。

## 4 使用与交付边界

三个中文入口仍按原职责使用。运行调试／本地构建不需要再传资料路径；生成安装包仍为用户手动执行。现存 0.1 安装包不包含 0.6.1 新规则，不能拿旧包验证本次行为；待用户生成新包并选择 D 盘安装目录后再进行安装验收。

## 5 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-11 | 记录可选安装目录、本机默认共用资料、绝对 WebView 缓存、离线迁移与当前交付边界 |
