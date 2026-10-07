# SubtitleVocabularyList 接口契约设计

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-IF-001 |
| 文档版本 | 0.5 |
| 更新日期 | 2026-10-06 |
| 状态 | 实施中；导入与词库命令已实现，其余契约待实施 |
| 需求依据 | [PRD](../requirements/PRD.md) 的 FR-01 至 FR-12、NFR-02 至 NFR-05 |
| 架构依据 | [架构设计](architecture.md) 的 ARC-01 至 ARC-08 |
| 关联设计 | [数据模型](data-model.md)、[应用流程](application-flows.md) |

本契约定义界面与 Rust 核心的命令、任务查询、错误和外部适配边界。下列目录保留完整 0.1 的逻辑契约；当前已实现参数另列，不把后续提案当作现有接口。站点认证与协议仍需按 OPEN-04、TECH-06 对齐。

## 1 命令与数据约定

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

全局取词与截图触发直接进入同一核心采集入口，先取得原应用上下文再显示窗口。CMD-06、CMD-07 不是让弹窗取得焦点后重新猜测原窗口的操作。

### 2.1 当前已实现参数

实现位置为 `src-tauri/src/desktop.rs`，类型位于对应 Rust 模块及 `src/types.ts`。当前分页使用 offset/limit，成功 TaskSnapshot 使用 `id` 作为任务标识；后续命令不能假定已经存在。

| 命令 | 当前请求与结果 |
| --- | --- |
| `app_info`、`sources_list` | 应用目录、数据库版本及计数；来源列表含文字来源、对白与候选数量 |
| `import_start` | `{paths: string[], operationId}` → TaskSnapshot；递归扫描支持视频目录，文件逐项成功或失败 |
| `task_get`、`task_cancel`、`tasks_list` | 前两项使用 `{taskId}`；最后一项无参数，返回最近 30 次任务快照 |
| `candidates_list` | `{sourceId, search?, kind?, onlyPending?, offset?, limit?}` → 候选数组 |
| `candidate_examples`、`candidate_decide` | 前者 `{sourceId,key}`；后者 `{sourceId,key,exampleId,decision}`，当前决定为 familiar 或 uncertain |
| `source_example_update` | `{sourceId,exampleId,revision,text,startMs,endMs}` → 修订后的对白；保留已收录旧引用 |
| `entries_list`、`entry_get` | 前者 `{search?,offset?,limit?}`；后者 `{entryId}`，返回释义、例句、原声及计数 |
| `entry_update` | `{input:{id,expectedRevision,text,meanings:[{id,text}]}}` → 新词条版本；新释义 id 为 null；当前通过版本校验保护修改，未实现更新操作回执 |
| `collection_prepare` | `{input: CollectionInput}` → `{draftId,matches,input}`；input 含 operationId、kind、text、meaning、examples、targetEntryId、expectedRevision |
| `collection_from_example` | `{sourceId,exampleId,text,kind,meaning,operationId}` → 同一 PreparedCollection，核心取得当前对白 |
| `collection_commit` | `{draftId,targetEntryId,expectedRevision,saveAudio}` → TaskSnapshot；创建时目标与版本为 null，操作标识来自准备好的 input |
| `preview_start`、`media_path` | 前者 `{sourceId,exampleId,operationId}` → 片段准备任务；后者 `{assetId}` → 经位置、状态与摘要核对的路径 |
| `explain_start` | `{text,context,operationId}` → 本地解释任务，结果为 meaning、translation、notes 字符串；仅显式采用后进入收录内容 |
| `settings_get`、`settings_update` | 后者 `{settings}`，本地模型限回环地址，含离线开关与工具位置 |
| `capture_selection` | 无参数，先记录前台上下文，再返回取词 TaskSnapshot；成功发出 capture_completed，失败不返回旧内容 |
| `capture_ocr`、`capture_session_get` | 前者无参数 → screen_capture TaskSnapshot，成功结果 sessionId、stage=awaiting_selection；后者 `{sessionId}` → 本次快照路径与物理 ScreenBounds |
| `capture_ocr_submit`、`capture_ocr_cancel` | 前者 `{sessionId,rect:{x,y,width,height}}` → OCR TaskSnapshot，坐标相对本次快照；后者 `{sessionId}` → 取消本次选区，过期标识不能取消新会话 |
| `speech_voices`、`speech_start` | 前者列出本地英语声音；后者 `{text,operationId}` → 语音准备任务，结果含 path、kind=system、voice |
| `native_status`、`app_quit` | 前者返回当前快捷键注册与失败信息；后者请求取消后台任务并退出应用 |

当前原声和已生成的系统语音由 WebView2 audio 元素报告实际播放及错误，暂停直接作用于播放器；系统语音准备可通过 task_cancel 取消。speech_start 的成功表示文件已准备，不等于已经听到声音；CMD-13、CMD-14 的独立统一播放命令仍未实现。复习、查询与同步命令仍是后续设计。

设置新增 selectionShortcut、ocrShortcut、systemVoice，旧 JSON 使用默认值读取，不改变数据库 schema。两个快捷键不得相同；更新先尝试注册新值，成功后释放旧值并保存，失败保留原设置。capture_completed 携带本次 CollectionSeed，包括文字、可取得语境和来源；capture_failed 携带本次 AppError。当前草稿未关闭时，新结果保留等待确认，不覆盖输入；当前只保留一份待确认采集结果，连续采集队列仍需完善。

截图使用先快照后选区的独立会话。提交校验矩形和显示器配置，成功裁剪后关闭选区窗口；recognize 失败结果为空。取消选区调用 capture_ocr_cancel，识别已运行时调用 task_cancel。识别成功生成 OCR 文字来源，经显式确认才写入词库；图像在本轮结束后清理。

## 3 收录请求与提交结果

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

## 4 任务与事件

任务查询是持久化状态的权威入口。事件建议采用以下最小结构：

| 事件 | 字段与用途 |
| --- | --- |
| `task_updated` | `taskId`、状态、阶段、递增序号、可选进度、结果或错误；更新运行中的界面 |
| `library_changed` | 已提交的实体类型、标识与新版本；让相关视图重新读取数据 |
| `playback_changed` | 播放标识、音频类型、状态与错误；反馈实际播放情况 |

事件可能被错过或迟到。界面重新打开时查询真实任务与实体版本，不能仅凭曾经收到成功事件就推断文件和数据库已经保存。结果数据较大时使用结果引用与分页读取。

当前 `task_updated` 发出完整 TaskSnapshot，未实现递增事件序号；界面通过任务查询刷新并核对持久化终态。library_changed 与 playback_changed 尚未实现，保存后的视图重新查询，播放直接读取 audio 元素状态。程序启动将遗留非终态任务标为 interrupted 失败，保留部分结果；重试建立新的任务执行，同一业务操作仍去重。

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
| `network_error` | 查询或同步网络失败 | 保留待处理内容，稍后重试 |
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

本地模型地址与兼容接入方式唯一维护在 [架构设计](architecture.md)。接口适配核对服务模型和实际响应，并处理超时、取消、结构不完整及必要兼容参数。查询服务及识别引擎未选定前，不写成已有可调用地址。

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

认证方式、实际路径、版本字段与游标形式需要与站点实现对齐。现有 `/api/vocabulary` 的限制见架构设计，本文不把自行发送认证头或更改 Origin 当成可行认证方案。

### 7.2 数据边界与重试

词条、释义、类型、例句、原声音频和学习记录已确认必须双向同步。复习状态需要随学习记录保持一致；实际字段版本、删除传播和首次对齐规则仍属于 OPEN-04 的协议设计部分。

本地提交与待同步记录关联保存，远端确认前保留待处理状态。远端已提交但响应丢失时，以原变更标识重试，服务返回已处理回执。拉取后内容按统一合并规则应用，不增加新的主动收录计数。

同一字段的冲突不得静默按客户端时间覆盖。追加资料可以按稳定标识与来源去重，语义不同的内容保留供用户处理。此处理提案在确认协议时需连同字段范围评审。

媒体传输契约应使用资料标识、摘要和内容，不发送本机绝对文件路径；大小、上传恢复和站点保存策略在实施中确定。模拟服务只能验证契约行为，不能代替真实站点认证与双向验收。

## 8 契约验证

命令实施前核对请求、成功结果、错误与版本行为，并用 [测试计划](../testing/test-plan.md) 中的重试、取消、并发修改和服务异常场景验证。协议调整同步更新数据模型与流程，具体参数由实现中的类型定义和契约测试约束。

## 9 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-06 | 建立本地命令、任务、错误、适配和站点同步契约提案 |
| 0.2 | 2026-10-06 | 根据用户确认，将原声与学习记录纳入必需双向同步契约 |
| 0.3 | 2026-10-06 | 对齐当前导入、收录、浏览、修改、媒体与任务实现参数，区分后续命令与事件 |
| 0.4 | 2026-10-06 | 对齐划词事件、系统语音准备、声音与快捷键设置及退出命令 |
| 0.5 | 2026-10-06 | 对齐截图会话、裁剪、取消与识别命令，明确当前待确认结果边界 |
