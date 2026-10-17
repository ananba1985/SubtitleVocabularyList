# 离线基础词库设计

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-DES-DICTIONARY-001 |
| 文档版本 | 0.6 |
| 更新日期 | 2026-10-17 |
| 状态 | 双词库及匹配优化实现、只读统计与最小正常流程通过；未打包 |
| 对应版本 | 0.8.10 开发中；业务 schema 9，Google 底库 schema 2／ECDICT schema 1 |
| 需求依据 | FR-02、FR-08、FR-14；用户确认优先采用已有中文 Google 高频词表，底库命中不调用本地 AI |

M-15 已在生成器中增加 frequency_catalogs/targets 完整排名索引，包括无中文单词及可追溯双词频次；仅经自然用法筛选的短语 eligible=1。dictionary_entries/aliases 及查询顺序不变，启动不加载整个频次索引。隔离生成实证见[水平测试记录](../testing/vocabulary-assessment-results.md)；未生成安装包、未更新日常 Data 中的既有底库。

## 1 范围与查询顺序

基础词库是独立的只读参考资料，不自动加入生词本、学习记录或同步载荷。保留既有人工收录内容和已保存解释。词语查询先复用已保存解释，再依次查 Google 高频词头、Google 词形映射、基础缩写校订、ECDICT 词头、ECDICT 词形映射；仍缺失时尝试 §3.1 中列明的变体目标，最后才调用本地 AI。Google 的词形命中也优先于 ECDICT 的直接词头命中。有效中文释义命中后直接返回，不能为了重复生成通用释义调用模型。

后台准备在排队时排除底库命中的词语，执行时再次复用本地结果；手动解释和 Qwen 词语查询也走同一读取逻辑。完整句子译文独立准备；选区翻译替换和明确的整句翻译模式仍使用既有翻译流程。词典通用释义不标成当前对白中的特定含义。

## 2 来源与文件

中文释义优先采用 `chenny/english-word-10000` 的 `english-word-10000.md`，锁定提交 `a3bfa2903a2cbbbeb8cd3df5a3f6003578b08f8b`。按用户追加授权，后备采用 ECDICT 同一固定提交 `bc015ed2e24a7abef49fc6dbbb7fe32c1dadaf8b` 的 `ecdict.csv` 基础版全部有效中文词条，并复用 `lemma.en.txt` 和 CSV 的明确 exchange 词形。不通过模型翻译词表。

本机准备时，原始文件、下载 URL、提交和 SHA-256 保存在 `Data/dictionaries/sources/`。本地转换输出 Google 的 `Data/dictionaries/base.sqlite3` 和 ECDICT 的 `Data/dictionaries/ecdict.sqlite3`，与业务库 `vocabulary.sqlite3` 分离。导入工具只解析 Markdown、CSV 和词形文本，不执行第三方 SQL。用户后续授权本机安装包携带两份底库，发行暂存与个人 Data 隔离；第三方数据不提交公开源码仓库，不把该授权表述为已取得第三方数据的通用再分发许可。

## 3 数据与运行方式

底库 schema 1：`dictionary_entries` 保存规范化词头、中文释义、英美音标、词性、频率排名；`dictionary_aliases` 保存表层词形到词头的映射；`metadata` 保存来源信息。只收录含中文释义的词条；一个词形对应多个词头时不自行猜义，不计为词形命中。精确词头优先于别名。

Store 打开时只读加载 Google 小词表为内存索引；ECDICT 保留只读 SQLite 连接，使用词头和词形索引按需查询，不在启动时把全部大词典加载入内存。查询不产生模型请求，notes 标注命中的实际词典。没有外部底库时仍可读取内置基本缩写补充，其余词语保持原流程。更换或首次准备底库后重新打开程序加载。业务 schema 保持 9，不迁移或改写用户词条。`explanation_get.meaningScope` 使用 `dictionary`，小窗结果使用 `cacheSource=dictionary` 区分模型结果。

### 3.1 书写变体与基本缩写补充

词典专用规范化折叠弯／直撇号，业务库的原文、候选键、收录、掌握和同步标识保持原样。`src-tauri/resources/dictionary-supplements.json` 保存少量经核对的缺撇号、口语省音、明确词形映射，以及自拟中文的标准缩略解释；Rust 编译读取与 Python 统计使用同一文件，不调用模型建立补充资料。补充说明参考 [Cambridge 缩略形式](https://dictionary.cambridge.org/grammar/british-grammar/contractions)。Google 有效结果仍优先，已核对的基础缩写补充作为 ECDICT 的缺项／错项校订层；注明来源，不改写原始下载文件或已有解释。

没有精确／明确词形结果时才尝试列明的书写变体，并要求目标存在有效中文，不使用编辑距离或无界后缀猜测。不统一去掉 `’s`，不擅自选择 `’d` 的展开，不自动纠正疑似 OCR，不根据姓名或语言猜测改写原句。`chillin → chilling`、`kiddin → kid`、`remotes → remote` 等当前词库释义不合适的映射不加入补充名单。一个已确认错误的 `they'd` 词义通过基本缩写解释校订，保留 had／would 两种可能。

### 3.2 安装与升级资源

根目录 `生成安装包.ps1` 在构建前调用 `prepare-base-dictionary.py --release-dir .tools/release-dictionaries`，独立使用 `.tools/release-cache/dictionaries/sources` 中经 SHA-256 核对的固定来源；没有缓存时在打包机下载，安装／运行时不下载。即使使用 `-SkipToolBuild` 也重新准备底库，失败时不继续编译。release 模式不读取个人业务库或本机 Data。

发行暂存只生成两份 SQLite、带来源提交／摘要／词条统计的 manifest，以及 Google README、ECDICT README 和原 LICENSE。Tauri 配置逐文件映射六份输出到 `<安装目录>/Data/dictionaries`，不把整个 Data 或原始 CSV 纳入资源；打包脚本同时核对构建前后的资源摘要。底库作为普通 NSIS 安装资源在首次安装时落盘、覆盖安装时替换，安装路径由用户选择，无需额外启动脚本。

standalone 输出使用 `scripts/copy-release-dictionaries.ps1` 读取同一显式映射，更新列明的文件，不递归删除 Data。个人 `vocabulary.sqlite3`、词条例句、人工／缓存解释、学习历史、同步资料和原声均不进入打包输入，也不由底库更新覆盖。调试入口继续复用本机 Data，不通过本轮修改迁移或重建个人资料。

资源映射采用 [Tauri bundled resources](https://v2.tauri.app/develop/resources/)；NSIS 安装前检查运行进程、按资源文件安装／删除，目录仅在空时移除，未添加删除 Data 的脚本。卸载和真实覆盖安装不在本轮执行，后续生成版本包时核对。来源说明保留上游原文；Google 表 README 声明中文释义与发音的来源，未标注独立开源许可证，不冒称其采用本项目 GPL 或 ECDICT 的 MIT。当前按用户要求准备本机使用流程，公开分发许可仍需单独核对。

### 3.3 可选资源故障隔离（0.8.10）

Google 和 ECDICT 分别只读加载；Google 的词头、映射及来源全部读取成功后才发布内存索引，ECDICT 在启动时预编译既有查询检查所需表和字段，不扫描完整大词典。任何一份资源的打开或结构读取错误仅停用该资源，不让可选资源错误中断个人业务库初始化。内置补充独立加载；外部底库不存在仍按既有可选资源方式运行。

ECDICT 运行期查询失败时，本进程释放并停用该连接，记录一次可见原因，查询按未命中继续原有解释流程。健康的 Google、内置补充、已保存解释和个人资料继续使用。app_info.dictionaryWarnings 返回当前进程的资源提示，主窗口显示；该提示不写进词条或同步资料，不自动删除、下载或修复文件。资源修复后重开程序重新加载。

此变更不修改个人数据库 schema 或底库文件。[最小正常验证及未执行边界](../testing/base-dictionary-results.md#6-0810-资源隔离与最小正常验证)分别记录，不以代码降级路径冒充损坏资源实测。

## 4 统计与最小验证

统计只读快照中有语料文件、未归档例句所对应的 `(kind, candidate_key)`，跨集去重；Google 直接／词形、ECDICT 直接／词形、基础缩写补充及均未命中分别统计，来源分类互斥且合计为候选总数。额外记录字形规范化和列明变体的方法，不能把自拟缩写解释冒充第三方词典原文。未命中不直接等于实际 AI 请求量：须进一步扣除已保存词义和已掌握词语。整句译文和 HTTP 请求数不混入词语覆盖数。

最小正常验证覆盖：底库精确与词形查询、字形／明确变体／基础缩写补充、后台排队排除、手动解释与小窗返回、未命中沿用模型、原语境解释与句子译文保持。只使用合成资料及本机一次只读匹配，不启动全量模型任务、不打包。

实际覆盖数、来源校验和本地准备命令见[基线记录](../testing/base-dictionary-results.md)，487 项初分与优化后的实际覆盖见[未命中复核](../testing/unmatched-vocabulary-results.md)。

## 5 修订记录

| 版本 | 日期 | 变更 |
| --- | --- | --- |
| 0.1 | 2026-10-14 | 按用户后续确认设计已翻译高频词底库优先和只读匹配 |
| 0.2 | 2026-10-14 | 按追加授权增加 ECDICT 后备，明确跨词典优先级和大词典按需只读查询 |
| 0.3 | 2026-10-14 | 复核未命中形态和对白，明确字形规范化、列明变体、基本缩写校订及不误改语义的边界 |
| 0.4 | 2026-10-14 | 按授权加入独立发行暂存、显式安装映射和保留个人资料的底库升级流程 |
| 0.5 | 2026-10-15 | Google 参考库增加独立词频索引，既有中文查词优先级保持 |
| 0.6 | 2026-10-17 | 分别隔离 Google／ECDICT 加载与查询错误，公开资源提示并保留个人库可用；业务 schema 仍为 9 |
