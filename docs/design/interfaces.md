# SubtitleVocabularyList 接口契约设计

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-IF-001 |
| 文档版本 | 0.56 |
| 更新日期 | 2026-10-21 |
| 状态 | 主要命令已实现；同步边界与整体验证继续 |
| 需求依据 | [PRD](../requirements/PRD.md) 的 FR-01 至 FR-24；FR-25 至 FR-29 已确认、尚未实现；NFR-02 至 NFR-05 |
| 架构依据 | [架构设计](architecture.md) 的 ARC-01 至 ARC-10；ARC-11 为计划模块 |
| 关联设计 | [数据模型](data-model.md)、[应用流程](application-flows.md) |

本契约定义界面与 Rust 核心的命令、任务查询、错误和外部适配边界。下列目录保留完整 0.1 的逻辑契约；当前已实现参数另列，不把后续提案当作现有接口。站点认证与协议仍需按 OPEN-04、TECH-06 对齐。

## 1 命令与数据约定

0.8.13 起，所有 106 个已注册命令的前端请求与成功结果统一由 Rust 生成的 `src/generated/ipc.ts` 中的 CommandMap 定义（替代 0.8.12 的手工表），`src/api.ts` 的 call 按命令名推导参数和结果，不接受由界面指定的任意返回类型。无参命令可直接调用，必填参数必须提供，Option 参数按表中可选字段传递；路径结果在前端为字符串。`practice_audio_save` 直接传 Uint8Array，保持原生 Raw 请求体，不包成 JSON。

例如 `call("task_get", { taskId })` 的结果为 TaskSnapshot，`call("practice_progress_save", { documentId, progress })` 的结果为 void。useAssessmentMutation 按命令约束不含 operationId 的输入，再补回稳定回执标识；同步和翻译替换的动态操作只接受明确的命令联合。`assessment_report_get` 的实际结果包括已完成报告及进行中状态两种形状；界面核对 report 字段后展示报告，进行中时提示完成或提前结束后查看。

Rust 命令统一注册在 `desktop/commands/mod.rs` 的 catalog，命令函数使用 svl-ipc-macros::command，分工见[架构 4.1.1](architecture.md#411-桌面命令适配层与类型表0812)。新增或调整接口时在 Rust 修改签名和 DTO，执行 `pnpm commands:generate`，提交生成文件及必要调用修改；不得手改生成文件。`pnpm commands:check`（亦由 pnpm build 执行）用同一注册和真实 Rust 类型图重新导出，只读比较整个文件；嵌套字段、结果新增／遗漏、枚举、数组、映射和空值变化都会使旧生成文件不再匹配。正常桌面编译不需要启用 ipc-types；导出工具不访问个人库。

生成规则沿用 IPC JSON：i64／u64 等数字继续映射 JavaScript number，PathBuf 为 string，Option 字段为 T|null；顶层 Option 参数同时允许省略。Rust String 如有业务限定仍由核心既有校验判定，不由另一份前端 DTO 枚举假定其取值。Serde flatten 用于听读进度、已准备解释、任务事件及同步 entry 扩展字段；二进制请求保持 Uint8Array，Result 的成功分支生成返回类型，拒绝仍为 AppError。TaskSnapshot 现在包含此前前端遗漏的 requestHash；Entry 的 createdAt／updatedAt、MediaTools 的 pgsTessdata、PluginManifest 的完整配置字段也随 Rust 定义导出。

水平测试会话分页、报告／明细／历史／比较、作答修正、音频回执、题库核对、同步冲突／保留两份，以及截图会话改为明确的 Rust 返回 DTO。旧核心 JSON 结果在 IPC 边界核对嵌套结构并确认重新序列化不丢字段，不匹配时返回 internal_error；音频回执 error 的正常值可为 null。同步冲突中 entry 的必要展示字段类型明确，其他 entry 字段通过 flatten 保留。

TaskSnapshot.result、插件 body、同步表行与 entry 扩展字段具有业务决定的动态结构，生成类型明确为递归 JsonValue 或 JSON 映射，不使用 any；生成检查不校验这些动态对象的每种业务内容，也不取代运行期标识、版本、权限及核心输入校验。CollectionSeed 等纯界面数据与按任务类型解读结果的辅助类型不作为新的 Rust 命令 DTO。

0.8.11 的 task_updated 事件保留原 TaskSnapshot 顶层字段，并增加 visibleInList:boolean，由原生临时／业务任务规则计算；该字段仅用于事件展示，不写 tasks 记录，不改变 tasks_list／task_get 结果和持久化 schema。主窗口按事件维护任务展示，快照读取期间收到的事件在读取结束后重放；单项等待先订阅再查询 task_get，终态释放监听。仅可见的未结束操作保留无事件 10 秒后的补查。

听读 practice_get／practice_import／practice_progress_save 的参数与公开结果保持。正文和进度的内部键及旧记录事务转换见[数据模型](data-model.md#71-本机听读练习fr-13)；进度保存不读取完整正文。

0.8.10 起数据库相关命令通过既有 background／spawn_blocking 执行：主线程不等待这些命令的业务连接互斥锁；命令名称、参数、结果与幂等语义保持。窗口／焦点操作和纯运行期查询会话不据此统一迁移线程。app_info.dictionaryWarnings 仅报告当前进程可选资源异常，健康资源和个人库仍可用，修复底库后重启重新加载。

界面通过 Tauri 命令调用核心。命令接受类型明确的请求，Rust 成功返回数据，失败返回可序列化错误；前端统一处理 Promise 成功或拒绝。[Tauri 命令说明](https://v2.tauri.app/develop/calling-rust/)

| 类型 | 约定 |
| --- | --- |
| `EntityId` | 稳定 UUID 字符串，对应实体的真实标识 |
| `OperationId` | 一次明确业务操作的标识，重试时保持不变 |
| `Revision` | 实体非负整数版本，更新时传入读取到的版本 |
| `PageRequest` | 本地分页大小与游标；排序规则稳定，大小由核心限制 |
| `TaskHandle` | `taskId`、任务类型与当前状态；不等于处理成功 |
| `AppError` | `code`、`message`、`retryable`、可选结构化定位信息 |

请求与响应字段使用 camelCase，Rust 内部名称可按语言约定转换。时间、词条类型、能力维度与状态值以数据模型和应用流程为准，不在界面另建语义。

界面引用核心管理的来源、草稿和音频标识。核心核对标识与当前数据，不以界面传来的 SQL、任意工具命令或媒体路径作为业务接口。

## 2 本地命令目录

| 编号 | 命令提案 | 输入与输出摘要 | 需求 |
| --- | --- | --- | --- |
| CMD-01 | `import_start` | 文件或文件夹输入、启动操作标识与已支持选项，返回 TaskHandle | FR-01 |
| CMD-02 | `task_get` | 任务标识，返回阶段、进度、终态、结果引用或错误 | NFR-03 |
| CMD-03 | `task_cancel` | 任务标识，返回已接受请求或已有终态 | NFR-03 |
| CMD-04 | `candidates_list` | 来源标识与分页，返回去重候选及出现位置摘要 | FR-02 |
| CMD-05 | `candidate_decide` | 来源范围、候选与熟悉或不确定判断，返回保存结果 | FR-02 |
| CMD-06 | `capture_selection` | 本次原生采集上下文，返回采集任务 | FR-03 |
| CMD-07 | `capture_ocr` | 本次选区上下文，返回裁剪与 OCR 任务 | FR-04 |
| CMD-08 | `collection_prepare` | 原文、类型、语境与资料标识，返回草稿及匹配建议 | FR-05、FR-06 |
| CMD-09 | `collection_commit` | 草稿、目标词条选择、版本和操作标识，返回收录任务 | FR-05、FR-06、FR-07 |
| CMD-10 | `entries_list` | 基本文本查询与分页，返回词条摘要 | FR-09 |
| CMD-11 | `entry_get` | 词条标识，返回释义、例句、音频及学习状态 | FR-05、FR-09 |
| CMD-12 | `entry_update` | 待纠正内容、操作标识与预期版本，返回新版本 | FR-05、FR-06 |
| CMD-13 | `playback_start` | 音频标识，或明确的系统语音文本，返回播放标识与类型 | FR-07、FR-09 |
| CMD-14 | `playback_stop` | 播放标识，返回停止或已有终态 | FR-07 |
| CMD-15 | `review_next` | 复习或专项范围，返回题目及作答标识，不返回可直接展示的隐藏答案 | FR-10、FR-11 |
| CMD-16 | `review_submit` | 作答标识、回答、提示情况与操作标识，返回评估任务 | FR-10 |
| CMD-17 | `review_correct` | 作答、预期结果版本、人工结果与理由，返回修正及复习状态 | FR-10 |
| CMD-18 | `leeches_list` | 基本分页，返回专项条目、维度和触发原因 | FR-11 |
| CMD-19 | `explain_start` | 草稿或词条例句与请求目的，返回本地解释任务 | FR-08 |
| CMD-20 | `online_query_start` | 用户明确请求与已配置服务，返回查询任务 | FR-08、NFR-05 |
| CMD-21 | `sync_start` | 已配置连接、此次范围与操作标识，返回同步任务 | FR-12 |
| CMD-22 | `sync_status` | 连接标识，返回已确认状态、待同步和冲突摘要 | FR-12 |
| CMD-23 | `sync_resolve_conflict` | 冲突标识、两端版本与用户选择，返回处理结果 | FR-12 |
| CMD-24 | `connection_start` | `{operationId,replace}` → 真实站点待批准连接任务 | FR-12 |
| CMD-25 | `connection_check` | `{operationId}` → 真实账号批准/过期/拒绝/撤销状态任务 | FR-12 |
| CMD-26 | `connection_open` | 无参数 → 打开已保存请求的系统浏览器连接页 | FR-12 |
| CMD-27 | `connection_status` | 无参数 → 仅本机保存状态，不发送网络请求 | FR-12、NFR-05 |
| CMD-28 | `known_target_get/set`、`known_targets_list` | 查询/设置已掌握筛选偏好及分页管理，不修改测验成绩 | FR-02 至 FR-04、NFR-02 |
| CMD-29 | `explanation_get` | 按目标和完整语境读取本地解释，不依赖模型 | FR-02、FR-08、NFR-02 |
| CMD-30 | `explanation_import` | 按需迁入旧浏览器解释，不覆盖数据库资料或产生收录行为 | FR-08、NFR-02 |
| CMD-31 | `explanations_prepare` | 安排/继续聚合中文准备，可优先指定来源，返回已有或新任务及全部就绪状态 | FR-02、FR-08、NFR-03 |
| CMD-32 | `explanations_status` | 本地读取 running/paused/failed/idle，不探测模型或联网 | FR-08、NFR-03 |
| CMD-33 | `practice_get` | 无参数 → 本机保存的当前阅读与位置，首次为 null | FR-13、NFR-02 |
| CMD-34 | `practice_import` | `{document}` → 保存阅读并返回 PracticeState；同来源继续位置 | FR-13 |
| CMD-35 | `practice_progress_save` | `{documentId,progress}` → 保存章节、句子、草稿、速度、音频来源与声音；忽略已更换阅读的旧更新 | FR-13、NFR-02 |
| CMD-36 | `practice_audio_save` | 原始二进制请求 → 写入本机课程音轨并返回 `media/practice/{digest}.m4a` 相对路径 | FR-13、NFR-01 |
| CMD-37 | `practice_example` | `{documentId,chapter,sentence}` → 按已保存课程创建／复用原句，返回 CandidateExample，进入原收录确认和原声保存流程 | FR-05 至 FR-07、FR-13 |

FR-13 的 `PracticeDocument` 包含 `id,title,kind,chapters:[{title,sentences:[{text,startMs,endMs}]}],audioFile`；`PracticeState` 增加 `chapterIndex,sentenceIndex,copy,speed,audioMode,voiceId`。时间单位为毫秒；无原声的句子时间为 null。`speech_start` 新增可选 `voiceId`，仅覆盖本次请求的系统声音，旧调用继续使用全局设置；速度由本地播放器设置。

全局取词与截图触发直接进入同一核心采集入口，先取得原应用上下文再显示窗口。CMD-06、CMD-07 不是让弹窗取得焦点后重新猜测原窗口的操作。

### 2.1 当前已实现参数

实现位置为 `src-tauri/src/desktop.rs`，类型位于对应 Rust 模块及 `src/types.ts`。candidates_list 增加可选 showKnown（默认 false），返回候选增加 isKnown；已掌握过滤在 SQL 分页前完成，包含已处理但要求查看的已掌握项。当前分页使用 offset/limit，成功 TaskSnapshot 使用 `id` 作为任务标识；后续命令不能假定已经存在。

| 命令 | 当前请求与结果 |
| --- | --- |
| `app_info`、`sources_list` | app_info 返回 version（编译时应用版本字符串）、dataDirectory、schemaVersion、dictionaryWarnings（当前进程基础词库资源提示字符串数组）与词条/来源计数；应用版本独立于数据库版本。来源列表含文字来源、对白与候选数量 |
| `import_start` | `{paths: string[], operationId, options?}` → TaskSnapshot；options 包含 audioStream、subtitleStream、subtitleMode（auto/embedded/external/speech）、externalSubtitle。目录递归扫描六个视频后缀、现有文件统一路径身份去重；显式坏文件独立报告，错误选择不静默改轨。具体样本见[媒体矩阵](../testing/media-support-matrix.md) |
| `media_inspect` | `{path}` → 音轨、字幕轨的索引/语言/名称/编码及同名外置字幕；为选择提供数据，不直接收录 |
| `online_query_start` | CMD-20：`{text,provider?,operationId}` → 任务；provider 为 dictionary（Wiktionary）或 translation（MyMemory），无参默认前者。只发送 text，拒绝手动离线，自动降级后允许重试；返回 query、source、sourceUrl、definitions，建议需人工采用 |
| `task_get`、`task_cancel`、`tasks_list` | 前两项使用 `{taskId}`，同时支持业务后台执行和在内存保存的临时操作；最后一项无参数，返回全部未结束业务后台任务和最近一条可展示终态。preview/speech/review_audio/explanation 只有 failed 进入任务列表；正常执行/成功/取消不进入列表或侧栏数量；仍按 updatedAt、id 降序 |
| `tasks_history` | `{offset?,limit?}` → `{items:TaskSnapshot[],total}`；读取业务 succeeded/failed/cancelled 及上述临时类型的 failed，旧版正常临时记录同样排除，总数使用相同条件。默认 offset=0、limit=10，limit 为 1–100；页内数据及总数同一读事务取得，超出末页返回空 items，业务历史不受最近 30 条限制 |
| `candidates_list` | `{sourceId, search?, kind?, onlyPending?, offset?, limit?}` → 候选数组 |
| `known_target_get`、`known_target_set` | CMD-28：`{kind,text}` → boolean；设置额外传 known:boolean，返回空成功。kind 为 word/phrase/sentence，设置文本非空且最多 4000 字符；不自动建立词条或测验记录 |
| `known_targets_list` | CMD-28：`{search?,offset?,limit?}` → `{items:[{kind,text,matchKey,markedAt}],total}`；默认 offset=0、limit=20，limit 为 1–100，按标记时间/键排序；总数与页内数据同一读事务 |
| `candidate_examples`、`candidate_decide` | 前者 `{sourceId,key}`；后者 `{sourceId,key,exampleId,decision}`，当前决定为 familiar 或 uncertain |
| `source_example_update` | `{sourceId,exampleId,revision,text,startMs,endMs}` → 修订后的对白；保留已收录旧引用 |
| `entries_list`、`entry_get` | 前者 `{search?,offset?,limit?}`；后者 `{entryId}`，返回释义、例句、原声及计数 |
| `entry_update` | `{input:{id,expectedRevision,text,meanings:[{id,text}]}}` → 新词条版本；新释义 id 为 null；当前通过版本校验保护修改，未实现更新操作回执 |
| `collection_prepare` | `{input: CollectionInput}` → `{draftId,matches,input}`；input 含 operationId、kind、text、meaning、examples、targetEntryId、expectedRevision |
| `collection_from_example` | `{sourceId,exampleId,text,kind,meaning,operationId}` → 同一 PreparedCollection，核心取得当前对白 |
| `collection_commit` | `{draftId,targetEntryId,expectedRevision,saveAudio}` → TaskSnapshot；创建时目标与版本为 null，操作标识来自准备好的 input |
| `preview_start`、`media_path` | 前者 `{sourceId,exampleId,operationId}` → 片段准备任务；后者 `{assetId}` → 经位置、状态与摘要核对的路径 |
| `explain_start` | `{text,context,operationId}` → 本地解释任务；优先返回对应数据库记录，缺失才请求模型；结果为 meaning、translation、notes 字符串，成功前提交解释资料；仅显式采用后进入收录内容 |
| `explanation_get` | `{text,context}` → 展示资料或 null；优先语境义，其次规范化通用义，独立合并句子译文。附加 meaningScope 为 context／general／translation；只有译文就绪时 meaning 为空，不自动收录。不连接模型或产生任务 |
| `explanation_import` | `{text,context,value:{meaning,translation,notes},modelUrl,modelName}` → 数据库 Explanation；兼容旧浏览器缓存，校验中文及长度，只追加缺失记录，重复导入返回原结果，不创建词条或学习记录 |
| `settings_get`、`settings_update` | 后者 `{settings}`，本地模型限回环地址，保存离线开关、声音等偏好；返回当前工具位置供检查，忽略传入工具路径且不将路径写入用户设置 |
| `capture_selection` | 无参数，先记录前台上下文，再返回取词 TaskSnapshot；成功发出 capture_completed，失败不返回旧内容 |
| `capture_ocr`、`capture_session_get` | 前者无参数 → screen_capture TaskSnapshot，成功结果 sessionId、stage=awaiting_selection；后者 `{sessionId}` → 本次快照路径与物理 ScreenBounds |
| `capture_ocr_submit`、`capture_ocr_cancel` | 前者 `{sessionId,rect:{x,y,width,height}}` → OCR TaskSnapshot，坐标相对本次快照；后者 `{sessionId}` → 取消本次选区，过期标识不能取消新会话 |
| `speech_voices`、`speech_start` | 前者列出本地英语声音；后者 `{text,operationId}` → 语音准备任务，结果含 path、kind=system、voice |
| `native_status`、`app_quit` | 前者返回当前快捷键注册与失败信息；后者请求取消后台任务并退出应用 |
| `tray_action_get` | 返回并取走最近待处理托盘操作，返回 string 或 null，仅用于主窗口导航和手动收录 |
| `review_units` | `{mode:'due'/'all'/'leech',dimension:'meaning'/'listening'/'',offset,limit}` → 分页学习单元、独立状态、可练习标识与专项原因，兼容 CMD-18 的专项列表语义 |
| `review_question` | `{unitId,expectedRevision}` → CMD-15 的当前题目实现，返回题目 id、维度、题面、可见语境与音频类型；不返回参考答案 |
| `review_hint` | `{questionId}` → 首字提示，同时持久化提示事实；已提交题目不能补写提示 |
| `review_audio_start` | `{questionId,operationId}` → 听力音频任务；核心读取私有快照，结果仅含 path、kind；原声检查摘要，无适用原声时准备已标识系统语音 |
| `review_submit` | `{input:{operationId,questionId,answer,unable}}` → CMD-16 的当前实现，直接返回已保存作答、答案依据及当前安排；未明确匹配进入 needs_confirmation，不需要模型 |
| `review_correct` | `{input:{operationId,attemptId,expectedRevision,outcome,reason}}` → 修正后的作答与重算状态；请求摘要去重，提示事实不能清除 |
| `review_history` | `{unitId:string/null,offset,limit}` → 原回答、原判分、有效结果、答案快照及历次修正；支持待确认结果在重启后处理 |
| `connection_start`、`connection_check` | CMD-24、CMD-25 已接入；结果为 siteUrl、state、deviceName、requestId、displayCode、authorizationUrl、accountScope、expiresAt，不返回设备令牌；离线配置拒绝网络操作 |
| `connection_status`、`connection_open` | CMD-27 读取 DPAPI 本机凭据；CMD-26 校验 origin 并打开 HTTPS 连接页，登录与批准由真人完成 |
| `sync_start` | CMD-21：`{operationId}` → TaskSnapshot；使用已批准账号，交换全部资料。结果为 pulled、pushed、conflicts、cursor；存在冲突不能宣称全部资料一致 |
| `sync_status`、`sync_conflicts` | CMD-22：无参数 → 当前账号 cursor、pending、conflicts，或完整待确认版本、目标候选及实际当前本地内容；不返回设备令牌 |
| `sync_resolve` | CMD-23 的实际名称：`{input,operationId}` → TaskSnapshot；input 包含 conflictId、choice、targetEntryId、expectedRemoteRevision、expectedLocalRevision、expectedTargetRevision。文字可选 local/remote；同形词可选 merge/new；移除可选 archive/restore。过期展示版本拒绝处理 |

当前原声和已生成的系统语音由 WebView2 audio 元素报告实际播放及错误，暂停直接作用于播放器；系统语音准备可通过 task_cancel 取消。speech_start 的成功表示文件已准备，不等于已经听到声音；CMD-13、CMD-14 的独立统一播放命令仍未实现。复习命令已按上表接入；查询与同步命令仍是后续设计。

设置包含 selectionShortcut、ocrShortcut、inputShortcut、systemVoice，不改变数据库 schema。三组启用的快捷键不得相同；更新先尝试注册新值，成功后释放旧值并保存，失败保留原设置。capture_completed 携带本次 CollectionSeed，包括文字、可取得语境和来源；capture_failed 携带本次 AppError。当前草稿未关闭时，新结果保留等待确认，不覆盖输入；当前只保留一份待确认采集结果，连续采集队列仍需完善。

快捷键默认值为 selectionShortcut=Alt+C、ocrShortcut=Alt+E、inputShortcut=Alt+D；与本机 Pot 的已核实配置一致。旧 JSON 尚无 inputShortcut 时，settings_get 一次补写该字段，并仅将旧工厂值 Ctrl+Alt+Shift+W／Ctrl+Alt+Shift+S 分别升级为对应新默认值，保留其他自定义字段。native_status 增加 inputRegistered:boolean，区分输入键的注册状态。启动按键分别尝试注册并保留错误；显式保存仍按三键整体配置处理，失败不保存新值。输入键向 main 发出已有 tray_action 操作，tray_action_get 返回 collect；不执行模型调用、联网查询或词条保存。

设置增加 `closeToTray:boolean`，缺省为 false；旧设置缺少该字段时采用默认完全退出，不改变 schema。`settings_update` 与其他偏好共同保存，主窗口 CloseRequested 读取已保存值：true 隐藏窗口，false 或设置读取失败调用共享退出流程。`app_quit` 和托盘“退出”始终取消未结束任务，最多等待 3 秒后请求应用退出；不关闭调用者已存在的终端窗口。

托盘左键恢复当前窗口，show_menu_on_left_click 为 false，右键展示系统菜单。页面类菜单先保存运行期操作，再显示主窗口并向 main 发出 tray_action 通知；前端建立监听后主动读取一次，后续通知使用 tray_action_get 单次消费。可返回 library、episodes、practice、reviews、leech、sync、tasks、settings，以及 collect（词库页手动收录）和 import（预习页展开导入）。截图菜单直接沿用 NativeDesktop.ocr_start 和已有采集完成／失败事件；退出沿用完整退出流程。

`offlineMode:boolean` 默认 false，表示是否手动禁止外网；旧资料显式保存的 true 保留，无设置或缺字段采用新默认。`app_info` 增加 `networkUnavailable:boolean`，只返回当前进程最近完成的在线任务连接降级状态，不持久化、不触发联网。网络任务报 `network_unavailable` 时置 true；成功及可取得响应的 `network_error`/`auth_required`/`not_found`/`invalid_data` 置 false；取消和其他本地控制错误保持原状态。手动离线检查只依赖 offlineMode，自动降级不拒绝重试；离线偏好与降级状态本身不改变数据库 schema。

预习只调用 explanation_get，并按需迁移旧缓存；缺失时轮询本地资料和 explanations_status，不因页面打开而发起 explain_start。explanations_prepare 的参数为 `{sourceId?:string}`，返回 `{task:TaskSnapshot|null}`；null 表示请求范围没有缺失资料，指定来源可提高队列优先级，已有聚合任务被复用。status 返回 `{state:"running"|"paused"|"failed"|"idle"}`。没有新增 HTTP 入口，0.6.2 的本地缓存结构为 schema 8。

explanation_batch 通过 task_get/list/history 查询进度与 result.preparation；通用词义或句子译文先保存再计入进度，不创建逐项成功任务。取消保存暂停偏好，退出停止当前运行，重开补齐。输入分别按词键、完整原句去重，已有值不覆盖，已掌握过滤与预习一致。CollectionSeed.meaning 仅为草稿初值，确认后走原收录接口；准备语料不自动收录或同步。

0.8.25 的 result.preparation 固定结构由 preparation/progress.rs 的 PreparationProgress、SourceProgress、PreparationFailure 定义，并纳入同一个 Rust 类型导出工具。任务页引用生成类型，不再手写词／句／来源及失败明细结构；失败明细也由后端结构体序列化，source/text/message 字段保持。任务通用 result 仍是开放 JSON，各任务的其他固定结果与前端等待断言未因此全部类型化，旧历史和动态协议行也不改变；此边界与已完成的固定进度类型分别记录。

0.8.27 将 13 个已知任务启动命令的返回值关联为 TaskHandle<T>，T 来自实际工作函数的 Rust 返回类型：本地解释、在线查询、预览原声、复习／系统／测试／题库音频、普通／测试收录、连接申请／检查、同步及冲突处理。TaskManager.start_typed 和网络任务适配复用原任务生命周期，只将具体 DTO 序列化为原 result；Rust 的 PhantomData<T> 不序列化，生成类型中的 __taskResult 仅为编译期关联，不是 JSON 字段或第二份结果。前端 waitTask 根据启动返回类型推导完成值，不再由各调用处声明结果结构；未关联的通用 TaskSnapshot 只能得到 unknown。

重新启动同一成功任务时，核对旧结果可按 T 完整读回且不丢字段，继续返回同一回执；进行中进度仍为开放 JSON，不误按完成 DTO 读取。task_get、任务事件、历史及旧快照仍使用原 TaskSnapshot，不增命令、不增数据库字段、不重写历史。新增固定 DTO 由同一工具生成，完整测试收录回执及同步的 assessment 状态也纳入该类型图。对应最小正常证据见[性能记录 §28](../testing/performance-results.md#28-0827-已知异步任务结果类型关联)。

0.6.2 的 current/total 为通用词义与句子译文项之和，包含生成、复用、跳过和未完成；sources[].current/total 为处理归属，failures 保留前 20 条原因。新任务分别显示词／句数量，不用分集候选总和代替全局词数。旧批次仍按旧词语语境口径显示，不重写历史。候选统计与任务轮询分开读取。

临时模型请求失败返回 `provider_unavailable` 且 `retryable:true`：连接、超时、请求/响应读取中断、HTTP 408/429/5xx；其他 HTTP 拒绝或客户端配置错误不自动无限重试。`explanation_batch` 内部在 2/4/8/16/32/60 秒等待后重试当前项目，后续间隔维持 60 秒，任务 `state` 保持 running；全部在途项目都等待时 `stage` 为 `retry_wait`，否则为 `translations`，`message` 含当前处理及等待信息，`error` 不写为终态错误，`current/total` 不因请求尝试而增长。输出 JSON 或中文校验的 `invalid_data` 仅补试一次；最终无效才计入未完成。等待不新增任务记录。

存在等待批次时，显式 `explanations_prepare` 复用并唤醒同一 taskId；没有活跃批次时重新查询缺失资料启动新执行。`explanations_status` 在等待期间仍为 running，历史失败不当作实时健康检查。取消与退出检查适用于等待，并在下一次请求前复核已掌握偏好。重试唤醒标志为进程内状态；暂停偏好和回执继续持久化，中文缓存结构见 schema 8 设计。

Settings 增加 `modelConcurrency:number`，默认 1，接受 1～16 的整数；settings_get 返回已保存值，settings_update 校验并保存后更新共享请求许可。该上限覆盖 explain_start 及准备线程；已有请求不因降低上限被取消。`result.preparation` 增加 `concurrency`、`inFlight`、`waiting`，表示当前上限、已分配且未结束的项目和其中等待重试的项目，不等同于服务端实际计算数；历史结果缺少这些字段时继续显示原统计。工作线程只发内部状态/结果事件，由协调线程单独提交聚合快照。唤醒用运行期递增序号，使一次显式重试通知全部等待工作线程。

0.6.2 通用词义使用 targets:[{id,text}] 和空 context，输出 translation 为空、items:[{id,meaning,notes}]；句子译文使用 sentences:[{id,text}]，输出 items:[{id,translation}]。两类各自合批，id 必须完整、唯一对应输入。result.preparation.unit 为 words_and_sentences，新增 wordsTotal/wordsCurrent、sentencesTotal/sentencesCurrent；current/total 是两类待处理项之和，旧任务仍保留其词语语境口径。inFlight/waiting 为在途项目数，batchesInFlight 为请求批次数；并发限制不按词数计量。

Settings.modelBatchSize 默认 8，接受 1～64 的整数，和 modelConcurrency 独立。settings_update 保存并更新运行期偏好，下一次调度重组待执行队列，不打断已分配请求；旧 JSON 缺字段补默认值。batchSize 为词条或句子数上限，同一批不混合两类，数量可以小于上限。

本机解释请求使用 response_format=json_schema，要求 meaning、translation、notes 三个必需字符串且不增加字段；提示明确词义用中文、缩写不只返回英文展开式、译句仅来自当前语境。本机 Qwen 服务实际支持该结构化输出；依据见 [llama.cpp 的结构化接口测试](https://github.com/ggml-org/llama.cpp/blob/master/scripts/server-test-structured.py)。不支持的服务仍以原有错误反馈处理，不将非 JSON 内容直接用于收录。

缩略词提示补充中文释义示例，避免模型仅返回 What did 等英文展开式。中文与 JSON 校验保持：非法输出不写入解释库，真实模型单个样本通过不代表所有 OCR 词语或释义已验收。

截图使用先快照后选区的独立会话。提交校验矩形和显示器配置，成功裁剪后关闭选区窗口；recognize 失败结果为空。取消选区调用 capture_ocr_cancel，识别已运行时调用 task_cancel。识别成功生成 OCR 文字来源，经显式确认才写入词库；图像在本轮结束后清理。

schema 4 的 Entry.examples 对原句去重，每项增加 `contexts:[{scopeKey,meaningId,contextMeaning}]`，返回全部已确认关联；旧 contextMeaning 兼容字段保留最早一条。收录请求保持原结构，核心自行复用原句和媒体并追加关联范围。

只读完整资料包导出保留为 Rust 库/私有检查入口；桌面同步通过上述独立命令执行，HTTP 路由和字段唯一维护在[同步设计](synchronization.md)。新增 `sync_probe` 私有检查入口调用正式 HTTP 适配器，要求已获浏览器批准的 DPAPI 连接，不生成或伪造个人身份。

### 2.2 Pot 替代查询命令

CMD-38 至 CMD-44 已在 0.5.0 接通，字段、事件与取消契约见 [Pot 替代设计](pot-replacement.md#6-命令与事件提案)。

| 编号 | 命令 | 职责 | 需求 |
| --- | --- | --- | --- |
| CMD-38 | translation_session_open | 创建查询会话，保持查询与收录分离 | FR-14、FR-15 |
| CMD-39 | translation_query_start | 对当前输入和允许的服务开始查询 | FR-14 至 FR-17 |
| CMD-40 | translation_session_get | 读取当前版本的状态、流式预览或完整有效结果 | FR-14 至 FR-17 |
| CMD-41 | translation_query_cancel | 取消相应查询订阅，保留其他会话 | FR-14、FR-16 |
| CMD-42 | clipboard_listener_set/get | 显式开关与真实运行状态 | FR-16 |
| CMD-43 | providers_list/update | 非敏感服务配置、调用范围和凭据引用 | FR-17、FR-18 |
| CMD-44 | plugin_install/remove | 版本与能力明确的 JSON 配置安装／卸载 | FR-18 |

实际参数：translation_session_open 为 `{input:QueryInput,sessionId?:string}`；QueryInput 包含 target、context、mode（auto／explain／translate）、sourceLanguage、targetLanguage 和 origin。0.8.30 查询入口默认 auto，查询时统一解析实际 explain／translate，QuerySession.input 保留用户选择；显式模式保持。get 的 sessionId 可空，空时返回当前会话。query_start 为 `{sessionId,expectedInputRevision,providerIds,automatic,persist}`；providerIds 空表示所有已启用且支持本次能力的服务，automatic 时还筛选 autoQuery，整段翻译排除仅支持词典的服务。cancel 为 `{sessionId,queryId?:string}`。返回完整 QuerySession，含输入版本、查询标识和各服务独立状态／值／错误；query_updated 只通知标识，当前快照通过 get 读取。mode 仍为原字符串字段，不新增 DTO 字段或命令。

0.8.7 起，内置本地模型的 running 结果可以带临时 QueryValue，字段形状不变，仅含已经解码的正文片段；ready 才表示完整且通过校验的有效结果。客户端收到流式 query_updated 后重新 get，沿用原有输入版本及查询标识核对。临时值不能收录或写入查询缓存，失败清除临时值；持久化表结构不变。具体等待状态和输出规则见 [Pot 替代设计第 4 节](pot-replacement.md#4-会话取消与进度)。

clipboard_listener_set 为 `{enabled,preferences:{debounceMs,persistResults}}`，间隔 100～2000 ms，返回运行期 enabled、error 和偏好；get 无参数。providers_update 为 `{update:{id,enabled,autoQuery,sendContext,order,maxConcurrency,values,secrets}}`；order 为 0～1000，外部并发为 1～16，本机模型沿用全局许可。返回配置不含 secrets，仅 credentialFields 表示已配置的凭据字段。插件为 `{path}` 安装单个小于 512 KB 的 JSON，或 `{pluginId}` 卸载；升级默认关闭，协议为 1，首版仅 translation／dictionary。

辅助桌面命令 translation_open、translation_window_hide、translation_window_pin（pinned）、query_copy（text）及 translation_collect（sessionId、inputRevision、providerId）服务于小窗交互。collect 仅把当前完整结果送入原收录确认，hide 取消小窗订阅并停止本窗播放。新监听状态不随重启开启，复制本程序结果按序号排除。

### 2.3 选区翻译并替换契约

FR-19 下列命令、设置和事件已在 0.6.0 接通；完整流程和原生边界见 [选区翻译并替换](selection-replacement.md)。

| 编号 | 命令 | 输入／输出及边界 |
| --- | --- | --- |
| CMD-45 | replacement_status_get／replacement_cancel／replacement_result_open／replacement_clipboard_restore | get 无参数，返回 `{operationId,state,originalText,translation,providerId,error,clipboardPending}`，未操作时 id／state 为空；cancel 接收 `{operationId}`，忽略其他操作 id，不撤销已提交编辑；open 无参数，用户主动查看内存结果，不重新请求或写入；restore 无参数，用户核对未确认输入后恢复尚属本次操作的剪贴板 |

settings_get／settings_update 增加 replacementShortcut（默认 Ctrl+Alt+R，可空禁用）和 replacementProviderId（默认 qwen）；旧配置读时补默认值，保存时写入，不改既有三键。native_status 增加 replacementRegistered，注册错误沿用 error。原生快捷键读取即时选区并提交，不提供网页传入任意 HWND、范围或待写文字的回填接口。replacement_updated 只通知 operationId／state，详情用 CMD-45 读取；不改变现有 query_updated 或小窗 current。设置改变替换键／服务时取消等待中的替换。

### 2.4 高频水平测试命令（基础已实现，后续提案保留）

FR-20 至 FR-24 的 CMD-46 至 CMD-60 在[高频水平测试设计第 7 节](vocabulary-assessment.md)唯一维护：来源/配置、题库准备核对、预览与开始、公开题面、作答与音频、会话状态、报告修正/历史，以及确认收录。新命令沿用 camelCase、分页、operationId、预期版本及结构化 AppError；CMD-46 至 CMD-49 已注册 assessment_catalog_get、assessment_settings_get/update、assessment_bank_prepare/review，并新增 assessment_bank_items(bankId,state?,offset,limit)；CMD-50 至 CMD-55、CMD-57 的核心已注册，另有 assessment_session_list(unfinishedOnly,offset,limit)；CMD-56、CMD-58 至 CMD-60 已接入报告、历史、比较和可信确认收录；另有 assessment_report_items 逐题分页命令。题库审核／发布需操作标识和预期修订，准备返回 TaskSnapshot；真实桌面入口已由 M-17 接入，M-18 报告和确认收录继续复用原词库事务。M-19 增加 assessment_audio_prepare、assessment_sync_status、assessment_sync_conflicts 与 assessment_sync_keep_both，输入输出及事务边界见[详细设计 M-19 补充](vocabulary-assessment.md)。完整网络包唯一维护于 protocol/svl-assessment-1.json，独立于旧词条包。

开始时冻结题目及私有答案，IPC 不下发未作答答案；判分事实保存在核心。收录适配复用既有事务，增加可信 assessment 来源关联，不由浏览器自报成绩或伪造剧集来源。具体请求/响应和不足/版本变化处理见详细设计，不在本目录重复一套字段。

assessment_preview/start 使用后台线程计算并在开始事务内冻结；session_get 不返回未作答答案。submit 只接受本题选项 ID、填空原文或明确 unknown/skip，提示与播放事实由核心读取。session_transition 支持 pause/resume/end_early/request_hint/save_draft，后者只保存草稿。audio_start 返回 playbackId、可播放 path 或资源 error，不下发听音文本；audio_event 接受 started/progress/ended/error 及秒数，成功结束才保存 played。播放事件不改变作答用 expectedRevision；result_correct 追加事实，保留原回答与提示，未播放听音不能修正为计分成功。

0.8.18 的 assessment_report_get、assessment_history_list 与 assessment_report_items 参数及 DTO 字段保持。报告与历史摘要中的 revision 表示本机报告查询位置，用于详情及旧版本选择；不要求等于 session_get 的会话 revision。默认读取按业务内容选择已有或新增报告，单纯同步修订及本机音频定位变化复用旧版本；收到的原报告 JSON 和摘要不改写。比较及可信收录也使用选中的本机报告位置，详见[报告设计第 5、6 节](vocabulary-assessment.md#5-判分报告与复测)。

### 2.5 边看边学命令（预留，未实现）

FR-25 至 FR-29 的 CMD-63 至 CMD-70 唯一维护于[边看边学设计 §7.2](watch-and-learn.md#72-拟新增命令)：打开、状态、播放控制、视频视图区、暂停上下文、帧 OCR 重试、准备统一收录及关闭。播放器上下文带 sessionId 和 contextRevision，查询、OCR 和草稿核对其预期版本；事件只通知标识后读取快照。

翻译复用 CMD-38 至 CMD-41 等 translation 会话命令，确认收录继续用 CMD-09；实现时扩展 watch 来源及必要 DTO，并使用现有类型生成链，不建立平行查询或提交协议。上述命令仅预留编号，当前未注册、未生成类型，不能作为可调用接口。

## 3 收录请求与提交结果

CMD-29 的 `meaningScope` 增加 `dictionary`：词义来自独立底库，`notes` 保留来源与词形匹配信息，`translation` 只组合已保存的整句译文，不查询模型。小窗 Qwen `explain` 入口命中底库时 `cacheSource=dictionary`，直接返回，既有 `stored/cache/provider` 保持；`translate` 模式不将词典释义冒充完整译文。底库文件、导入与统计口径见[离线基础词库设计](base-dictionary.md)。

草稿包含类型、原文、用户确认释义、例句、来源位置、音频准备情况以及匹配建议。修改原文或类型后，需要重新核对匹配建议。

当前提交请求如下，示例标识仅用于说明结构，业务操作标识保存在已准备的草稿 input 中：

```json
{
  "draftId": "00000000-0000-4000-8000-000000000002",
  "targetEntryId": null,
  "expectedRevision": null,
  "saveAudio": true
}
```

targetEntryId 为空表示新建；合并时必须提供匹配词条标识和 expectedRevision。草稿信息由核心读取，并在提交前再次校验。没有原声与存在但保存失败属于不同结果。当前草稿保存在运行中的应用内存，失败可在窗口中重试；关闭程序后需要重新准备未提交草稿。

收录任务的成功结果包含词条标识、主动收录标识、提交后版本、新增和复用的例句与音频标识。失败保留草稿；同一操作正在执行或已执行完成时，重试返回对应任务或已提交结果，不产生另一条主动收录记录。

0.8.14 保持既有 sync_status／assessment_sync_status 参数与结果字段。水平测试状态由完整资料的变更缓存取得，变更后重算；只读缓存不代表网络确认。SyncResult.pulled 是处理过的变更通知数，已保存的相同／更早版本可跳过整包下载后继续推进游标。发送、回执、取消与完整资料范围见[同步设计 4.2](synchronization.md#42-同步增长控制0814)。

0.8.15 保持 IPC 命令、嵌套 DTO 和既有同步结果不变。HTTP account 新增可选 bundleDelta=json-delta-1 能力；词条／会话 PUT 可发送 changeId、baseRevision、entryId／sessionId 和 delta，完整字段与顺序操作见[共享差量契约](../../protocol/svl-delta-1.json)。站点返回 412／delta_base_missing 时，以相同逻辑变更退回完整请求；成功回执和 409 完整冲突文档仍沿用原契约。不得把缺少 delta 中的字段解释为业务删除。

0.8.16 保持原 IPC 返回字段。HTTP account 可声明 bundleDeltaDownload=json-delta-1；桌面在已有匹配基准且 chunks-1 参数匹配时，对变更详情 GET 发送 x-svl-delta 与 x-svl-base-revision。词条／会话详情 GET 同样支持该协商。响应保留原实体标识、目标 revision 及适用的 cursor／changeId，新增 baseRevision 与 delta 或 deltaRef；无匹配基准／差量不更小时沿用原 bundle／packetRef。deltaRef 描述符和分片端点沿用 chunks-1，内容是完整序列化的差量对象。桌面核对基准版本、实体和目标版本，校验分片长度／摘要，补回完整 bundle，再进入原持久化和冲突流程。契约唯一维护在[共享差量契约](../../protocol/svl-delta-1.json)。

0.8.17 不改变 IPC、业务包或 json-delta-1 线上字段。相邻确认版本的下载可直接返回服务端保存的已校验 delta／deltaRef；完整下载和冲突文档仍必须先从内部检查点／差量还原完整业务包，不能把 history 封装作为 bundle 返回。内部字段见[站点历史存储契约](../../protocol/svl-history-1.json)，不暴露为新增前端 DTO。

## 4 任务与事件

任务查询是当前执行状态和结果的权威入口。业务后台与失败诊断读取 SQLite，正常临时操作读取有限内存回执；事件建议采用以下最小结构：

| 事件 | 字段与用途 |
| --- | --- |
| `task_updated` | `taskId`、状态、阶段、递增序号、可选进度、结果或错误；更新运行中的界面 |
| `library_changed` | 已提交的实体类型、标识与新版本；让相关视图重新读取数据 |
| `playback_changed` | 播放标识、音频类型、状态与错误；反馈实际播放情况 |

事件可能被错过或迟到。界面重新打开时查询真实任务与实体版本，不能仅凭曾经收到成功事件就推断文件和数据库已经保存。结果数据较大时使用结果引用与分页读取。

capture_completed 的采集资料增加 alreadyKnown:boolean；匹配完整目标及类型，不删除 context 中的熟词。命中目标不自动激活主窗口，前端提供“仍要查看”；筛选偏好读取失败时继续普通确认，避免丢弃未知采集。

当前 `task_updated` 发出完整 TaskSnapshot，未实现递增事件序号；界面通过任务查询刷新，业务终态与失败诊断核对持久化记录，临时操作核对当前回执。library_changed 与 playback_changed 尚未实现，保存后的视图重新查询，播放直接读取 audio 元素状态。程序启动将数据库遗留非终态任务标为 interrupted 失败，保留部分结果；重试建立新的执行，同一业务操作仍去重。正常临时回执无需重启恢复，已保存中文解释与音频资料独立保留。

TaskSnapshot 新增可空的 subject，保存在既有 snapshot_json 中，不改变数据库版本；旧快照缺字段时按空值读取。导入、收录、原声、解释、系统语音和在线查询的新任务记录目标对象，最多 240 个字符。听力准备仅记录“听力测验”，任务页显示音频类型，不展示待作答目标，保持 FR-10 的答案显示规则。旧 preview 快照可通过 result.asset.id 的例句和来源关联取得展示对象；只读补充，不改写旧记录或音频。任务时间沿用 createdAt/updatedAt，终态 updatedAt 用作结束时间，取消和恢复规则保持。

## 5 错误契约

| 错误码提案 | 含义 | 恢复方式 |
| --- | --- | --- |
| `invalid_input` | 空输入、越界片段或不合法状态 | 保留输入，用户修正 |
| `unsupported` | 当前未支持的应用、素材或引擎能力 | 显示实际边界，选择可用入口 |
| `not_found` | 实体或任务标识不存在 | 刷新对应视图，不复用旧缓存 |
| `conflict` | 实体版本变更或操作标识对应不同请求 | 显示版本差异并重新确认 |
| `resource_missing` | 音频、模型或本地语音资源不可用 | 补齐资源或明确选择可用替代 |
| `permission_denied` | 采集或文件操作权限不足 | 说明本次失败，不自动扩大权限 |
| `provider_unavailable` | 本地模型、OCR 或播放适配失败 | 保留数据，按支持能力重试 |
| `network_unavailable` | 外部连接、超时或响应传输中断 | retryable=true，运行期离线降级，保留内容并允许重试 |
| `network_error` | 外部服务操作未完成、HTTP 错误或配额问题 | 单独显示服务错误，保留内容，不自动禁止联网 |
| `auth_required` | 站点未认证或认证失效 | 重新连接账号，本地继续使用 |
| `internal_error` | 无法归类的处理错误 | 记录可定位信息，显示失败 |

取消是任务状态，不伪装成成功内容。错误信息应针对操作说明原因；凭据和完整私有请求不写入公开诊断输出。

## 6 语言与媒体适配

| 适配能力 | 输入 | 输出与失败边界 |
| --- | --- | --- |
| 选区与取词 | 原应用采集上下文或屏幕区域 | 当前文字或裁剪结果；失败不返回旧内容 |
| 媒体检查与字幕读取 | 用户素材与已选音轨字幕 | 来源、对白、时间位置与能力说明 |
| 本地语音识别 | 本地音频、已准备模型和取消信号 | 带时间的可纠正文本，不直接创建词条 |
| 原声截取 | 来源片段与目标位置 | 可解码的临时音频与元数据，确认保存由收录核心完成 |
| 本地解释 | 原文、有限语境和解释目的 | 可修订建议及服务信息，不直接覆写用户内容 |
| 辅助判分 | 已提交回答、题目快照和答案依据 | 建议结果与理由；不确定时进入人工确认 |
| 播放与系统语音 | 已保存音频或英语文本与声音选择 | 播放标识、实际状态，区分原声与系统语音 |

本地模型地址与兼容接入方式唯一维护在 [架构设计](architecture.md)。接口适配核对服务模型和实际响应，并处理超时、取消、结构不完整及必要兼容参数。联网查询已选择上述两个服务，人工触发并如实注明来源；失败保留原文和草稿，未使用这些服务执行本地识别。

## 7 站点同步契约提案

### 7.1 逻辑能力

同步适配至少需要可验证的账号身份、读取远端变化、提交本地变化，以及逐项确认处理结果。下列能力为协议设计输入，不是已部署 HTTP 路由：

| 能力 | 必需语义 |
| --- | --- |
| 认证连接 | 获取真实账号范围与权限；不能由桌面任意填写身份请求头 |
| 拉取变化 | 输入已确认游标；返回稳定标识、版本及变化数据，只有应用成功后推进游标 |
| 推送变化 | 输入变更标识、实体、基准版本及数据；返回成功、重复、冲突或拒绝的逐项回执 |
| 首次对齐 | 将旧站点词键映射到本地实体；匹配歧义需确认，不能重复建立词条 |
| 冲突处理 | 保留两端版本及变更来源，用户处理后提交新的明确修改 |

账号连接、资料包、游标、回执与媒体路由已按[同步设计](synchronization.md)部署。网页 `/api/vocabulary` 已桥接完整资料包，新增例句、原声关联和判分修正进入同一变更流。自行发送平台身份头或更改 Origin 不能代替真实认证。

### 7.2 数据边界与重试

词条、释义、类型、例句、原声音频和学习记录已确认必须双向同步。字段、版本、软归档与首次对齐按[同步设计](synchronization.md)实施，复习状态由保留的原事件及历次修正重算。

本地提交与待同步记录关联保存，远端确认前保留待处理状态。远端已提交但响应丢失时，以原变更标识重试，服务返回已处理回执。拉取后内容按统一合并规则应用，不增加新的主动收录计数。

同一字段的冲突不得静默按客户端时间覆盖。追加资料可以按稳定标识与来源去重，语义不同的内容保留供用户处理。此处理提案在确认协议时需连同字段范围评审。

媒体传输契约应使用资料标识、摘要和内容，不发送本机绝对文件路径；大小、上传恢复和站点保存策略在实施中确定。模拟服务只能验证契约行为，不能代替真实站点认证与双向验收。

## 8 契约验证

命令实施前核对请求、成功结果、错误与版本行为，选择直接确认改动的最小成功路径。测试计划中的额外异常、边界、压力或扩大回归场景须先取得用户明确授权；通过必要检查后交付。协议调整同步更新数据模型与流程，具体参数由实现中的类型定义和契约测试约束。

## 9 Windows 开机启动（0.8.31）

NFR-02／NFR-03／NFR-04，沿用 system 适配层、background 和同一生成类型表，增加两条命令；Settings DTO 与数据库不增加字段：

| 编号 | 命令与契约 | 语义 |
| --- | --- | --- |
| CMD-61 | `autostart_get`：无参数 → boolean | 读取当前用户 HKCU Run 中本产品的非空登记；不存在时为 false，不创建或修复登记 |
| CMD-62 | `autostart_set`：`{enabled:boolean}` → boolean | 开启以当前 exe 和当前 Store 根目录登记，关闭仅移除本产品的值，回读实际登记后返回；不写词库，不退出程序 |

使用 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 的 `SubtitleVocabularyList`、REG_SZ，当前用户范围，不触碰其他程序或机器级启动项。命令格式为 `"<当前 exe>" --data-dir "<当前绝对资料目录>"`，按 Windows 参数规则引用含空格／中文及末尾反斜杠的路径，限制为 260 个 UTF-16 单元。依据[微软 Run 文档](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys)，由 Windows 登录流程启动，不是开机前的系统服务；不增加计划任务或启动脚本。

原生错误码为 `autostart_failed`，包含具体动作及 Windows 错误编号；路径无法表示或超过容量时不写入登记。读取错误只影响本项组件，不使正常设置读取、学习或模型服务失败。这里的 boolean 表示本产品启动项已登记，不代替任务管理器／系统策略的外部审批状态，也不自动重启或重新启用外部禁用的登记。相关守卫仅接入代码，不把未执行的故障／边界分支写成验收通过。

## 10 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-06 | 建立本地命令、任务、错误、适配和站点同步契约提案 |
| 0.2 | 2026-10-06 | 根据用户确认，将原声与学习记录纳入必需双向同步契约 |
| 0.3 | 2026-10-06 | 对齐当前导入、收录、浏览、修改、媒体与任务实现参数，区分后续命令与事件 |
| 0.4 | 2026-10-06 | 对齐划词事件、系统语音准备、声音与快捷键设置及退出命令 |
| 0.5 | 2026-10-06 | 对齐截图会话、裁剪、取消与识别命令，明确当前待确认结果边界 |
| 0.6 | 2026-10-07 | 对齐实际复习列表、题目、提示、听力音频、作答、修正和历史命令 |
| 0.7 | 2026-10-07 | 增加实际连接请求、检查、浏览器和本机状态契约，保持完整同步待实施状态 |
| 0.8 | 2026-10-07 | 补充 contexts 读取与关联规则；HTTP 同步服务端初版和桌面未接通分别标注 |
| 0.9 | 2026-10-07 | 对齐实际推拉、状态、冲突及带版本处理命令，记录网页桥接与正式适配检查入口 |
| 0.10 | 2026-10-07 | 增加实际导入选择、轨道检查和手动在线查询参数及外发边界 |
| 0.11 | 2026-10-07 | 明确工具位置由当前宿主解析，偏好更新不持久化工具路径 |
| 0.12 | 2026-10-07 | 对齐递归目录、现有路径身份去重与显式坏文件结果 |
| 0.13 | 2026-10-07 | 保留全部未结束任务，确保旧任务可观察、可取消及退出时清理 |
| 0.14 | 2026-10-07 | 增加完整终态历史分页，收紧默认摘要并记录兼容旧快照的处理对象 |
| 0.15 | 2026-10-07 | 增加全局筛选查询/设置/分页、候选 isKnown 与采集 alreadyKnown 契约 |
| 0.16 | 2026-10-07 | app_info 返回真实编译版本，界面显示与统一应用版本管理对齐 |
| 0.17 | 2026-10-07 | 增加 closeToTray 默认值、持久化与主窗口/完全退出命令的行为契约 |
| 0.18 | 2026-10-07 | 明确 offlineMode 默认 false、app_info 运行期状态和连接失败/服务错误分类，保持 schema 与重试入口 |
| 0.19 | 2026-10-08 | 明确自动预习解释复用接口、中文结果缓存及可选草稿 meaning，保持原生事件和数据库兼容 |
| 0.20 | 2026-10-08 | 新增解释数据库读取与旧缓存导入，明确 schema 6、模型来源与身份分离、保存成功后返回 |
| 0.21 | 2026-10-08 | 明确临时操作查询和取消、正常操作不入历史、旧记录过滤与业务分页一致性 |
| 0.22 | 2026-10-08 | 新增聚合准备/状态命令，明确自动后台、页面只读、批次取消暂停及结果先入库的契约 |
| 0.23 | 2026-10-08 | 明确语境进度、来源候选统计关联及旧记录兼容，补充缩略词中文提示与校验保持 |
| 0.24 | 2026-10-08 | 明确模型错误可重试分类、retry_wait、同 taskId 唤醒及输出有限补试，保持 schema 和缓存规则 |
| 0.25 | 2026-10-09 | 增加 modelConcurrency、聚合并发统计及协调提交契约，按用户规则限定最小正常路径验证 |
| 0.26 | 2026-10-10 | 补充同句多词请求与编号对应契约，区分请求批数和词语进度 |
| 0.27 | 2026-10-10 | 增加 FR-13 练习命令、二进制原声 IPC、位置数据与单次声音覆盖 |
| 0.28 | 2026-10-10 | 增加 modelBatchSize 设置契约及运行期待执行分组调整 |
| 0.29 | 2026-10-10 | 增加主窗口托盘操作通知和单次消费接口 |
| 0.30 | 2026-10-11 | 增加 inputShortcut／inputRegistered，定义默认升级及三键注册契约 |
| 0.31 | 2026-10-11 | 为 Pot 替代登记 CMD-38 至 CMD-44 提案，保持与现有接口分离 |
| 0.32 | 2026-10-11 | 登记查询小窗、服务插件和监听的实际命令与 schema 7，保持后续兼容边界 |
| 0.33 | 2026-10-11 | 预留 FR-19 的 CMD-45、独立快捷键及替换状态提案，不声明已实现 |
| 0.34 | 2026-10-11 | 接通 CMD-45，补足上次结果、剪贴板待恢复、按操作取消及实际设置命令 |
| 0.35 | 2026-10-12 | 登记 CMD-29 的通用／语境／译文展示、词／句分批协议及各自进度 |
| 0.36 | 2026-10-14 | 增加 dictionary 释义来源与查询缓存来源，词库命中跳过模型 |
| 0.37 | 2026-10-14 | 索引 CMD-46 至 CMD-60 高频水平测试提案，区分现行已注册接口 |
| 0.38 | 2026-10-15 | 登记已注册的目录、配置、题库准备审核及分页读取命令 |
| 0.39 | 2026-10-15 | 登记预览、开始、公开题面、提交、音频事件、会话状态及修正核心 |

| 0.40 | 2026-10-15 | 对齐水平测试 M-17／18 实施与 TC-55 部分、TC-56／57 核心证据，M-19 继续实施 |

| 0.41 | 2026-10-15 | 接通 assessment-1 完整测试历史、版本 27 真实往返及独立同步状态；人工体验按用户要求延期 |
| 0.42 | 2026-10-17 | 明确 running 临时 QueryValue、流式事件快照及 ready 的完整结果契约 |
| 0.43 | 2026-10-17 | 记录 dictionaryWarnings 可见资源状态和数据库相关 IPC 的后台执行边界，保持既有命令契约 |
| 0.44 | 2026-10-17 | 记录 task_updated 的 visibleInList、订阅与快照顺序、按需补查，保持听读命令外部结构 |
| 0.45 | 2026-10-17 | 统一已注册命令的参数／结果类型与调用入口，记录二进制音频、报告联合结果及跨语言目录检查边界 |
| 0.46 | 2026-10-17 | 以 Rust 自动生成请求及嵌套返回结构，记录固定 JSON 投影、字段补齐、空值与生成检查的真实边界 |
| 0.47 | 2026-10-17 | 明确同步状态缓存与已确认版本复用的结果语义，保持命令及完整包协议不变 |
| 0.48 | 2026-10-17 | 补充 json-delta-1 能力、词条／会话上传字段和基准缺失回退，保持 IPC 返回契约 |
| 0.49 | 2026-10-17 | 补充下载能力、基准请求头、内联／分片差量与完整响应回退，保持 IPC 和完整应用契约 |
| 0.50 | 2026-10-17 | 明确站点内部检查点存储和已校验差量复用不改变线上或 IPC 字段，保持完整回退响应 |
| 0.51 | 2026-10-17 | 明确报告按业务内容复用及本机查询位置语义，保持命令参数、DTO 字段和原报告 JSON |
| 0.52 | 2026-10-17 | 准备进度、分来源统计与失败明细由 Rust 生成给任务页，保留通用动态结果与旧历史边界 |
| 0.53 | 2026-10-17 | 13 个已知启动命令通过 TaskHandle 关联具体完成 DTO，移除等待结果手写断言，保留原 JSON、进度及历史 |
| 0.54 | 2026-10-18 | 查询 mode 增加 auto 并统一解析实际能力，保留用户选择、DTO 字段及原缓存身份 |
| 0.55 | 2026-10-20 | 增加 CMD-61／62 当前用户启动登记命令、原生错误及固定资料目录参数，保持 Settings 和存储契约 |
| 0.56 | 2026-10-21 | 索引边看边学 CMD-63 至 CMD-70 与上下文版本提案，复用原查询及确认提交；未注册实现 |
