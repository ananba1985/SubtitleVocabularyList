# SubtitleVocabularyList Pot 替代增量验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-POT-001 |
| 文档版本 | 0.5 |
| 更新日期 | 2026-10-18 |
| 状态 | 首版最小正常流程通过；桌面声音与采集已获用户确认 |
| 对应版本 | 0.5.0 首轮；0.8.1 划词、0.8.6 专名说明、0.8.7 流式及 0.8.30 自动查询方式增量；当前 schema 10 |
| 对应需求 | FR-14 至 FR-18、NFR-01 至 NFR-05 |
| 依据 | [设计](../design/pot-replacement.md)、[计划](../planning/pot-replacement-plan.md)、[测试计划](test-plan.md) |

## 1 范围与环境

本轮实现独立查询窗口、自动取词与截图查询、剪贴板监听、多服务及声明式翻译／词典插件。首版插件使用单个 JSON 配置，不执行脚本／DLL，不直接兼容 Pot 包。模型和 OCR／语音插件的其他兼容范围未据此扩大。

采用本机 Windows、Tauri 2.12.1、现有 Qwen3.5-9B 回环服务、Tesseract 和 Windows 英语语音。桌面验证使用独立 `.local/pot-acceptance` 词库和自编英文，未操作日常词库、真实剧集或同步账号。用户已退出 Pot，native_status 确认 Alt+C、Alt+E、Alt+D 均已注册，error 为 null。

## 2 已执行的最小正常检查

| 检查 | 实际结果与边界 |
| --- | --- |
| Rust 核心 | `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test query_normal -- --nocapture`：1 passed。两个正常合成服务返回各自释义，查询和准备收录前词库为空；显式保存插件释义后重开直接复用两份缓存；卸载插件保持已收录词条；查询历史数量为 0 |
| 前端类型 | `pnpm exec tsc --noEmit` 通过 |
| 桌面构建与运行 | 本地开发入口完成 Windows debug 编译、启动和真实 IPC；没有发行包或安装器 |
| 独立窗口 | 首次创建可正常返回，查询窗口实际 WebView 尺寸为 420×320；默认核心操作可见，内容区域可滚动；未执行其他屏幕／缩放矩阵 |
| 实际模型 | 原生小窗输入 reluctant 并 Enter 查询，返回“不情愿的”及用法；查询前后 entryCount 和 sourceCount 均为 0，schemaVersion 为 7 |
| 明确收录 | 在真实小窗查询 apprentice，选定“学徒”后进入主窗口收录确认；检查已有词条时数量仍为 0，点击“确认收录”后 entryCount 为 1，列表显示 apprentice／学徒；sourceCount 仍为 0 |
| 内容修正 | 正常无原句输入曾得到虚构句子译文，修正提示和展示规则：explain 模式无 context 时 translation 清空，缓存指纹包含解析版本；最终实际小窗只显示词义和用法 |
| 窗口线程修正 | 首次创建暴露 Windows WebView 同步调用挂起，按已安装 Tauri 源码的已知限制改为独立线程；截图选区的同类创建路径一并修正 |
| 收尾 | 已关闭隔离测试程序及自编英文窗口，开发启动脚本返回 0；退出仍打印既有 WebView2 Chrome_WidgetWin_0 注销错误 1412，本轮不据退出码声明该诊断已解决 |

核心检查最初因合成收录操作标识未使用 UUID 而失败，修正夹具后通过；没有把失败记录写成通过。后续核心调整后仅重跑同一正常用例，没有执行全套回归、故障注入、性能或长期监听检查。

## 3 桌面场景状态

| 场景 | 当前证据 | 状态 |
| --- | --- | --- |
| TC-43 | 真实小窗、Qwen 结果及明确收录闭环通过；用户确认“窗口和朗读正常”，未确认收录前数量保持 0 | 最小正常流程通过 |
| TC-44 | 自编英文 WPF 窗口中选取 reluctant 按 Alt+C，再用 Alt+E 框选英文；用户确认“划词和截图自动翻译都正常” | 最小正常流程通过 |
| TC-45 | 用户确认复制 apprentice 后“自动查询正常，已关闭监听”；收尾时原生 clipboard_listener_get 核对 enabled 为 false、error 为 null | 最小正常流程通过 |
| TC-46 | 两个合成服务、选定释义、保存与重开缓存通过；真实外部账号／服务没有测试 | 核心正常路径通过 |
| TC-47 | 合成声明式插件安装、配置、查询、卸载及资料保持通过；没有运行现有 Pot 插件或带真实凭据的外部插件 | 核心正常路径通过 |

## 4 保持与未覆盖项

既有 explanations 和词库保持；本地 explain 结果仍按目标与完整语境保存，通用多服务缓存使用 query_results。只有明确确认收录增加词条和学习单元，缓存、监听、服务配置不进入词库同步。

不把本地模拟 HTTP 当成真实外部翻译服务验收，也不把声音文件生成当成真人听辨。没有额外执行多屏、压力、长期监控、凭据故障或扩大兼容矩阵。声明式插件首版的配置和迁移边界见 [插件说明](../../examples/plugins/README.md)。

## 5 0.8.1 单词重复语境查询修复

对应 FR-14／FR-15、TC-43／TC-44 的查询处理增量。用户在已安装的 0.6.2 版本划取 `Certified`，界面报告“模型解释缺少有效中文词义或译文”。该安装版尚未包含后续基础词库优先查询。

使用现有查询适配器的同一请求参数直接请求本机 Qwen3.5-9B，`target=Certified, context=Certified` 返回了中文 meaning“认证的”，但 translation 仍为英文 `Certified`；原先校验将重复单词视为非空原句，要求中文句子译文，因此整条有效词义被拒绝。空 context 请求正常。此项复现说明同一失败原因，不声称取得了用户截图当次的原始响应。

修复后重复语境按无句查词处理，原始采集内容保留；真实不同原句的校验不放宽。已执行最小正常验证：

| 检查 | 实际结果 |
| --- | --- |
| 正常核心用例 | `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test query_normal captured_word_is_explained_without_a_sentence_and_saved_for_reuse -- --nocapture`：1 passed。合成服务收到空语境，中文词义成功持久化；会话保留 `Certified` 原始语境和 selection 来源；重开后以小写、无语境查询复用通用解释，不增加个人词条 |
| 真实模型 | 隔离空词库通过生产 QueryRuntime 和本机 `http://127.0.0.1:8096` 查询相同的 `Certified` 划词输入，ready／provider，meaning“有证书的”，translation 为空，约 6.95 秒；通用解释已保存，原始语境保持，个人词条为 0 |
| 真实底库 | 在另一隔离词库中加载现有 Google 底库，相同输入返回 ready／dictionary，meaning“认证”，translation 为空，个人词条为 0；由底库分支返回，不请求模型 |

隔离正常验证数据保留在 `.local/query-certified-validation`，不写入日常词库。只执行本次查询正常路径，未扩大到窗口兼容、故障、压力或全套回归；未重新打包、安装或替换用户运行中的 0.6.2 安装版。当前记录不冒充用户已升级后的划词验收。

## 6 专名中文说明字段整理（0.8.6）

FR-08、FR-14。用户输入 `headscale` 后，0.8.4 安装版显示“未获得有效中文词义；英文展开式或拼写诊断不能代替释义，请核对原文或图片字幕”。该文案来自 `Explanation::validate`，不能证明模型返回空白或输入有误。

按现有生产提示、参数、JSON schema，向已配置的本机 Qwen3.5-9B 重现一次请求：HTTP 成功、finish_reason=stop，content 是完整 JSON：

```json
{
  "meaning": "headscale",
  "translation": "",
  "notes": "HeadScale 是一个开源的 Tailscale 控制服务器，用于管理 Tailscale 网络。"
}
```

程序原来要求 meaning 含中文，并拒绝以“无此单词”等开头的诊断；真实 context 另要求中文译文。这个结果在 meaning 的中文检查处失败，notes 中已有的解释被整体拒绝，前端只收到错误，没有收到可展示的 QueryValue。本机没有已保存的 headscale 解释，排除了这次从旧解释缓存返回的情况。上述是按相同请求重现的原始响应，不是对用户此前那一次未留存 HTTP 响应的读取。

修复共享字段整理，原样移用 notes 中的现成中文解释；查询提示补充专名规则。校验失败改为给出具体字段原因及 content 原文，最多 16,000 字符，便于用户核对。没有为 headscale 加词条特判或改写模型的解释内容。

| 最小验证 | 实际结果 |
| --- | --- |
| 真实响应的正常回放 | `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test query_normal named_project_explanation_in_notes_is_displayed_and_saved -- --nocapture`：1 passed、2 filtered。使用上面的实际响应，生产查询返回 ready，中文解释保存为通用释义，无重复 notes；另开 Headscale 查询以 stored 复用，个人单词本保持为空 |
| 真实本机 Qwen | 新建隔离词库，经生产 QueryRuntime 查询 headscale，约 7.10 秒，ready／provider，中文结果成功保存，个人词条为 0。隔离资料及请求／响应证据保留在 `.local/headscale-query-0.8.6` |

真实后测另一次回答把该项目称作“Headless 客户端”，属于模型知识错误；[Headscale 官方说明](https://headscale.net/stable/)定义其为 Tailscale 控制服务器的开源自托管实现。本次通过只说明中文字段能被正确处理、展示和保存，不宣称模型输出的事实均正确。程序没有把这一条错误说法改写成正确知识或写入个人词库，保留隔离结果供核对。

查询失败原文展示分支已接入，未另行执行故障注入或错误响应测试。本轮没有重启当前安装版、修改日常词库、做兼容矩阵或全量回归，也没有打包或安装；0.8.6 是源码修复版本。

## 7 本地交互查询流式展示（0.8.7）

FR-14。原本本地查询发送 `stream:false`，等完整 JSON 返回、解析和校验后才向窗口提供结果。模型许可不足时还会在交互队列等待，不能把所有等待都归因于模型生成。此次将本地查词和整段翻译切换为流式请求与正文预览，显示排队、等待回复及正在生成；底库／缓存命中仍直接返回。

| 最小正常检查 | 实际结果 |
| --- | --- |
| 核心流式与保存 | `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test query_stream_normal -- --nocapture`：1 passed。合成服务确认 stream=true，先发未完成 meaning；会话 running 时已显示“维”，explanations 与 query_results 尚无记录。随后完成中文词义、译文及含引号的 notes，ready 后保存完整结果；相同查询复用 cache，个人词库为空 |
| 前端类型 | `pnpm exec tsc --noEmit` 通过 |
| 桌面构建与启动 | 使用根目录脚本对应的 `pnpm desktop:dev -DataDirectory ...`，按既有规则清理 Cargo 后完成 debug 编译，真实 app_info 为 0.8.7／schema 9，资料库位于 `.local/query-stream-0.8.7/Data` |
| 真实 Qwen 与窗口 | 通过实际 Tauri 查询窗口选择“整段翻译”、输入下述自编英文并点击“查询”。DOM 首次显示中文“请在”及“正在生成…”约为 3,764 ms，随后持续追加正文；6,611 ms 出现完整结果及复制／收录按钮。最终状态 ready、来源 provider，未使用缓存；生成中的收录按钮未显示 |

真实查询输入为：

> Please review the deployment checklist before restarting the server. We need to back up the database, verify the configuration, and notify the team when the maintenance is complete.

最终译文为“请在重启服务器之前检查部署清单。我们需要备份数据库、验证配置，并在维护完成后通知团队。”已查看真实 WebView 的生成中与完成截图，正文逐步显示，未展示 JSON 结构。计时从点击查询前开始，包含客户端调度、模型等待、传输和渲染；仅为一次正常查询观测，不用于断言模型计算速度提升或所有输入的延迟。流式展示使本次能在完成前约 2.85 秒开始阅读。

用户确认无任务和未保存输入后，已退出原安装版并保留隔离调试版运行。真实查询前后 entryCount／sourceCount 均为 0，没有向日常单词本写入测试数据。独立资料、`ui-result.txt`、`partial.png`、`complete.png` 及启动记录保留在 `.local/query-stream-0.8.7`。未打包或安装；未增加故障注入、压力、兼容矩阵或全套回归。

## 8 默认查询方式与完整段落翻译（0.8.30）

2026-10-18，FR-14 至 FR-17、TC-43／44。用户截图输入包含两句话、末尾没有句号，但小窗为“词义解释”，只给出其中一个词的释义。源码核对发现手动输入固定默认 explain，采集／剪贴板主要按末尾标点判断句子；缺少额外语境时又按词典提示要求简短词义，无法满足整段翻译。这里未读取截图当次的原始 HTTP 响应，不把代码路径核对写成当次响应复现。

修复默认 auto，手动输入、划词、截图、剪贴板共用 QueryInput 的实际模式判断；长段落不要求最后必须带标点。UI 保留自动选择，仍可显式指定解释／翻译。完整原文送入翻译，使用翻译服务及原 2048 token 输出预算，底库和已存词义不替代段落译文；旧解释、有效翻译缓存和个人学习资料保留。启发式规则及手动纠正边界见[设计](../design/pot-replacement.md#4-会话取消与进度)，不宣称自动判断具备完整语法理解能力。

| 最小正常检查 | 实际结果 |
| --- | --- |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test query_normal automatic_paragraph_translates_complete_text_and_reuses_translation_cache -- --exact` | 1 passed、0 failed，0.10 秒，不含编译。合成多句原文末尾无句号；模型替身收到完整 target、翻译提示和 2048 token 预算，返回两句完整译文；已有词义原文保持，显式翻译复用新译文缓存，个人词条为 0 |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test query_normal captured_word_is_explained_without_a_sentence_and_saved_for_reuse -- --exact` | 1 passed、0 failed，0.07 秒，不含编译。原正常流程改用 auto，单词重复语境仍按无原句查词，词义正常保存并复用，原 selection 来源与语境保持 |
| `pnpm exec tsc --noEmit` | 通过；默认模式、原显式模式及调用类型一致，未增加命令或 DTO 字段 |
| 配置中的真实本机 Qwen3.5-9B | 只读取得配置，按生产翻译提示、参数及 SSE 请求截图原文一次，7.01 秒，finish_reason=stop，返回覆盖两句话的完整中文译文；meaning／notes 为空。此项为直接模型请求，没有通过新 QueryRuntime 或 GUI，不写词库／学习历史 |

真实模型把 TL 译为“时间线”，包含逃离、街头请求和制作卡片两句内容；缺少原上下文，未据此确认缩写含义或全部语义准确。私有输入／响应及请求口径保留在 `.local/query-auto-0.8.30/model-response.json`，不提交用户查询文字。单次模型时间不作为速度改善证据。版本与差异核对随交付完成；未重启或替换正在运行的安装版，没有打包、GUI、故障、压力、并发或兼容矩阵验证。

## 9 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-11 | 记录独立查询、双服务缓存／收录／卸载、真实桌面明确收录及用户确认的声音／采集／监听，注明未覆盖范围 |
| 0.2 | 2026-10-15 | 记录 Certified 重复语境的失败原因、0.8.1 修复和隔离真实模型／底库正常验证 |
| 0.3 | 2026-10-17 | 记录 headscale 原始返回、字段校验误拒绝、0.8.6 整理与真实查询验证，区分模型知识准确性 |
| 0.4 | 2026-10-17 | 记录流式核心、完整保存及真实查询窗口首段和完成时间，保留隔离调试数据 |
| 0.5 | 2026-10-18 | 记录段落误走词义流程、默认自动识别修复、两条最小正常验证及一次真实模型完整译文，保留缩写和安装版边界 |
