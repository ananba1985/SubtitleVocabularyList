# SubtitleVocabularyList 资料目录与本机迁移

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-DATA-DIR-001 |
| 文档版本 | 0.5 |
| 更新日期 | 2026-10-23 |
| 状态 | 既有迁移记录保持；成功启动后的自动清理已接入，最小验证结果见验证记录；本轮未打包或操作日常资料 |
| 依据 | NFR-02、NFR-06；[架构](../design/architecture.md)；[实际验证](../testing/data-directory-results.md) |

## 1 已确认方案

安装位置由用户在安装器中选择，不固定 D 盘。本机用户决定安装到 `D:\SubtitleVocabularyList`，安装目录保存程序和 tools，资料保存到其 `Data` 子目录。本轮按用户后续缩小的要求调整资料路径和测试入口，不执行发行打包或安装，也不改安装器的路径选择流程。

| 运行方式 | 未显式指定资料目录时 |
| --- | --- |
| 本地调试脚本／debug 程序 | `D:\SubtitleVocabularyList\Data` |
| 本地打包 exe／生成的 run-local.ps1 | `D:\SubtitleVocabularyList\Data` |
| 安装版／发行 standalone exe | 当前可执行程序所在目录下的 `Data` |

因此安装到本机选定的 D 盘位置后，三者默认共用一份资料。正式程序选择其他安装位置时仍跟随该位置；本机测试默认值不自动跟随以后更换的安装位置。隔离测试可显式用 `-DataDirectory` 或 `SVL_DATA_DIR`，环境变量优先于程序默认值，不修改系统级环境变量。

0.8.31 的开机启动登记会携带 `--data-dir "<开启时当前绝对资料目录>"`，该显式参数优先于 SVL_DATA_DIR 和上述默认值，避免登录时进程工作目录／环境变化导致读到另一份词库。没有该参数时沿用原规则；资料文件不移动、不复制、不迁移。登记属于当前 Windows 用户，不写入可迁移的词库配置；移动程序或资料目录后需关闭开关再开启，更新登记到实际路径。

## 2 内容与缓存

Data 根目录保存 `vocabulary.sqlite3`，以及原有 media、jobs、credentials 等资料目录；例句、中文解释、学习状态、设置和同步元数据沿用数据库。`cache/webview` 为主窗口及查询小窗共同使用的 WebView2 资料，`cache/temp` 为本进程及子工具的 TEMP／TMP。启动前明确创建并使用这些目录，不通过 AppData 选择业务库或 WebView 缓存位置；无法访问选定位置时显示启动错误，不退回其他空词库。

0.7.3 安装配置将 Google 和 ECDICT 基础词库作为独立资源写入 `Data/dictionaries`，随包带两份 SQLite、清单和来源说明；升级只替换列明资源，不打包或覆盖 `vocabulary.sqlite3`。发行准备不读取本机日常 Data；standalone 复用同一映射，保留已存在的个人资料。实际 NSIS 包本轮按用户要求未生成，资源流程结果见[记录](../testing/dictionary-packaging-results.md)。

主窗口通过绝对路径 WebviewWindowBuilder 创建，小窗使用同一目录，避免 WindowConfig 的相对目录处理。开发配置与发行配置采用同一应用标识，复用现有单实例机制；关闭或从托盘完全退出上一种运行方式后再切换。数据库结构升级沿用 Store 的迁移规则，旧程序不能读取更高版本结构。

源剧集是用户导入时选取的文件，不复制所有外部剧集到 Data；已经准备的音轨和已保存音频随资料目录迁移。Windows 自身的运行时安装、系统声音和操作系统记录不作为本程序业务资料。

## 3 迁移方式

迁移前退出客户端，选定一份实际使用的源目录；不把各类验收用的隔离词库合并到日常资料。执行：

```powershell
.\scripts\migrate-data.ps1 -SourceDirectory '旧资料目录' -DataDirectory 'D:\SubtitleVocabularyList\Data'
```

脚本要求目标不存在，核对绝对路径、源目录链接和进程状态，先用 SQLite backup 保存包含 WAL 提交内容的一致性快照，再移动整个源目录。仅改写 sources 的 corpus_path／audio_path 和 tasks 的 snapshot_json 中的旧根路径；逐表比较所有记录并检查数据库完整性。旧库快照保存到 Data/backups，用户词条与释义不作文本替换。

新脚本只在上述逐表比较及 integrity_check 成功后，在迁移快照目录写入 `migration-verified.json`，记录目标资料根目录和通过结果。启动清理读取回执时要求目标目录与当前选定 Data 一致；没有回执的旧格式仍按下一节列明的严格范围转存，数据库快照先保留再回收目录。

本机原开发库已移到 D 盘选定位置；旧安装库为空，单独存档到 `backups/legacy-installed-data`。最新开发 WebView 缓存移到 cache/webview，旧安装缓存独立存档到 `backups/legacy-installed-cache`。其他隔离测试目录保持独立。实际行数、路径和启动证据见验证记录。

## 4 成功启动后的自动清理

对应 NFR-02、NFR-06。安装器覆盖结束不触发删除；桌面宿主全部 setup 成功，主界面成功取得 `app_info` 和 `settings_get` 后调用既有 `app_ready`，每次启动仅安排一次后台维护。`Store::open` 已完成 schema 迁移，维护再次对当前库执行 `quick_check` 及 `foreign_key_check`；任一步未通过均不开始清理。校验后释放数据库锁，再处理文件，不阻塞首屏等待文件回收。

| 路径／内容 | 保留和回收规则 |
| --- | --- |
| `backups/automatic/snapshot-<毫秒时间>-<UUID>.sqlite3` | 程序管理的自动快照命名空间；非空旧 schema 升级前由 SQLite backup API 保存含 WAL 已提交内容的一致性副本，通过完整性验证后才发布。按文件名中的快照时间排序，仅保留最新两份，旧副本连同其 SQLite sidecar 回收 |
| 同一自动目录内的上述 `.partial` 临时文件和已无对应主快照的 sidecar | 当前资料启动及校验成功后回收程序拥有的未发布残留；其他名称不参与 |
| `backups/migration-<UUID>` | 只认迁移脚本使用的 `before.sqlite3`、其 sidecar 和可选验证回执；先用 SQLite backup 转存至自动快照区并验证，再删除旧包装目录。UUID 与原快照时间组成稳定目标名，重试不重复复制；包含额外未知文件时整目录保留 |
| `backups/legacy-installed-cache` | 回收本项目此前保存的旧安装 WebView 缓存；当前 `cache/webview` 和 `cache/temp` 不参与 |
| `backups/legacy-installed-data` | 仅在目录只含旧词库及其 sidecar、库内全部业务表为空且完整性检查通过时回收；含记录、额外文件或凭据则保留，不假定它已合并 |
| 手工备份及其他旧文件 | 自动命名空间之外的文件／目录原样保留；不依据 `.sqlite3` 后缀或大小猜测。此前人工修复生成且未登记为自动快照的 `before-chinese-repair-…` 也保留 |

当前 `vocabulary.sqlite3`、完整例句／词义／学习历史、`media` 原声、`jobs`、`subtitles`、`credentials`、`dictionaries` 和正在使用的缓存均不属于删除候选。用户手工备份请放在 `backups` 下自定文件名／目录，不放入程序保留的 `automatic/snapshot-…` 命名空间。

所有删除目标均检查解析后的绝对路径仍位于选定 Data 内，拒绝目录链接／重解析点；删除目录前还检查其后代链接。清理错误只记录 `[data-maintenance]` 并延后至下次启动，既有业务资料不受影响。清理不联网、不启动定时轮询，也不每次升级复制完整原声；此规则当前接入 Windows 桌面，Android 仍沿用其独立沙盒缓存规则。

代码更新本身不会执行日常 Data 清理；需要运行含此实现的新桌面版本并达到上述成功门槛。开发验证只使用合成资料目录，不自动构建、安装或启动新版操作日常资料。实际证据见[验证记录第 5 节](../testing/data-directory-results.md#5-成功启动后的自动清理最小验证)。

## 5 使用与交付边界

三个中文入口仍按原职责使用。运行调试／本地构建不需要再传资料路径；生成安装包仍为手动入口。用户后续明确授权后，已通过同一脚本生成[0.6.1 安装测试包](../release/0.6.1-release-notes.md)，等待用户选择 D 盘安装并验证。历史 0.1 安装包不包含新规则，不能拿旧包验证本次行为。

## 6 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-11 | 记录可选安装目录、本机默认共用资料、绝对 WebView 缓存、离线迁移与当前交付边界 |
| 0.2 | 2026-10-11 | 关联后续授权生成的 0.6.1 安装测试包，保持实装未验证边界 |
| 0.3 | 2026-10-14 | 补充独立随包底库、显式更新范围与个人数据库保持规则 |
| 0.4 | 2026-10-20 | 明确开机启动命令固定当前绝对资料目录及显式参数优先级，不改变默认目录或迁移资料 |
| 0.5 | 2026-10-23 | 增加成功启动与验证门槛、自动快照保留两份、旧迁移／缓存回收及手工备份和完整资料保持规则 |
