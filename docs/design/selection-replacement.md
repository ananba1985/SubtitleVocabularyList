# SubtitleVocabularyList 选区翻译并替换设计

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-DES-REPLACE-001 |
| 文档版本 | 0.2 |
| 更新日期 | 2026-10-11 |
| 状态 | 已获实施授权；0.6.0 首版实现，实际验证以增量记录为准 |
| 代码基线 | 0.6.0，schema 7 |
| 需求依据 | [PRD](../requirements/PRD.md) FR-19、NFR-01 至 NFR-05 |
| 架构依据 | [架构](architecture.md) ARC-02、ARC-06、ARC-09 |
| 契约与阶段 | [接口](interfaces.md) CMD-45；[开发计划](../planning/development-plan.md) M-14；[测试计划](../testing/test-plan.md) TC-48 |

## 1 目标与范围

用户在其他应用的输入框写好英文，选中一个句子并按专用快捷键，自动译为简体中文并替换原选区。未选内容保持，正常流程无需复制、切窗或再次确认；不会发送消息、提交表单或保存原应用文档。

样本 `Hi, Tom. I will arrive tomorrow. See you soon.` 只选中中间句，完成后为 `Hi, Tom. 我明天到。 See you soon.`；具体译文取决于服务返回。

首版要求控件同时提供 UI Automation TextPattern 和可编辑 ValuePattern，非密码、启用且为非空连续选区。源文最多 4000 字符，本机核对的控件文本最多 65536 字符；不能验证范围或编辑状态时停止自动替换。只读网页、多选区、OCR 回填、富文本样式及通用编辑器兼容不作为已交付能力。已掌握筛选不阻止此写作动作；不自动收录、记学习成绩、建立学习来源或同步。

## 2 入口与交互

| 项目 | 实际行为 |
| --- | --- |
| 快捷键 | Ctrl+Alt+R，与 Alt+C／Alt+E／Alt+D 独立；不是 Pot 默认键 |
| 设置 | 本地设置可修改或留空禁用；四组键不得重复，注册冲突提示原因 |
| 服务 | replacementProviderId 默认 qwen；可明确选择已启用且声明支持翻译的单个服务，词典不能回填 |
| 重复触发 | 等待中再按同一键取消，重复 keydown 忽略；已提交编辑不自动撤销或重试 |
| 反馈 | 非激活的原生状态提示及托盘状态；等待中不打开查询小窗，不写后台任务历史 |
| 手动查看 | 托盘“翻译替换…（查看）”或本地设置“查看译文”，打开现有小窗；不会恢复旧自动写入 |
| 撤销 | 原应用 Ctrl+Z；具体粒度以已测控件为准 |

用户移动光标、改变选区、编辑内容或切换焦点后，本次只保留译文，不夺回焦点或重新选句。功能仅在程序运行或常驻时可用，不修改现有退出偏好。

## 3 模块与持久化

| 模块 | 实际分工 |
| --- | --- |
| desktop_capture.rs／desktop.rs | 第四快捷键、即时窗口身份、开始／取消、托盘和 CMD-45 |
| desktop_replacement.rs | 单操作状态、翻译调用、期限、取消、非激活提示、结果和剪贴板收尾 |
| replacement_target.rs | 工作线程内 COM／UIA、可编辑目标、端点及原文核对、一次粘贴和读回 |
| queries.rs／providers.rs／model_gate.rs | 私有 replacement owner、translate 缓存、单服务适配、共享并发与交互优先 |
| desktop_query.rs | 自身剪贴板变化排除、既有结果小窗 |
| ReplacementSettings.tsx | 快捷键、服务、上次结果、取消及待恢复剪贴板入口 |

原生目标、范围和剪贴板快照仅在内存，网页不能传入 HWND、写入地址或待替换正文。replacement owner 不改变查询小窗 current，也不向小窗发 query_updated。用户手动查看才建立普通查询会话。

设置沿用 settings，旧配置缺失字段由反序列化默认值补齐，保存时写入；原有三键保持。完整有效译文沿用 schema 7 的 query_results，不新增编辑流水表或站点资料包字段。

## 4 原生目标核对

热键事件取得前台 HWND／进程，工作线程读取焦点元素、可编辑 ValuePattern、单个 TextRange 和范围克隆。CompareElements 核对 UIA 元素身份，CompareEndpoints 核对两端，文字原样核对；不能只凭同名句子判断位置。

订阅文本、选区和焦点事件，在等待和回填前重读实际状态。初始化或重复通知先核对是否真的改变；已经观察到的变化永久取消自动回填资格。控件完整值和选区前后文本只用于本机验证，不发送给服务；完整值读取不表示调用 SetValue 重写整个输入框。

TextPattern 可出现在只读内容，能读取并不等于能编辑。[Microsoft Text and TextRange](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-implementingtextandtextrange)、[CompareEndpoints](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextrange-compareendpoints)

异步通知和键盘输入无法形成跨应用原子事务，不承诺零竞态。首个真实控件及其他未覆盖项见验证记录，Alt+C 的读取支持不自动成为回填支持。

## 5 翻译与回填

1. 固定 translate／en／zh-CN，仅发送选中句子，复用按内容、模式、语言和服务配置区分的缓存；meaning 不代替整句译文。
2. 缺失时沿用已有请求和共享模型许可；总期限 60 秒，含排队。取消／超时释放订阅，不静默外网兜底。
3. 仅接受完整非空且含中文的 translation；Qwen 返回 length 截断时拒绝。不写流式片段或错误说明，保留选区首尾空白。
4. 等用户释放修饰键，不强制释放用户按住的键；原目标和剪贴板仍有效才取得一次回填权。
5. Unicode 纯文本，单次 Ctrl+V；不使用 Ctrl+A、Enter、Ctrl+Enter 或发送按钮。读回匹配“原前缀＋译文＋原后缀”才报告 replaced。

本地失败不转发外网；明确选择外部服务时沿用既有启用、联网和手动离线规则。缓存可本机复用；服务选择列表不表示外部服务已经实际验收。

### 5.1 剪贴板边界

首版使用 Windows 内存快照，完整复制可物化为 HGLOBAL 的格式（普通文本、HTML 等），总上限 64 MiB；使用本程序窗口作为剪贴板所有者。普通截图的 DIB 内存格式可保留，Windows 从 DIB 合成的 CF_BITMAP 不重复保存，恢复后由系统再合成；无 DIB 的独立位图、元文件、私有／显示对象或无法读取格式，在清空前拒绝自动回填。未采用初稿 OLE 数据对象方案，不声称任意格式无损恢复。

临时写入及恢复在 ClipboardWatcher 自身操作排除锁内更新剪贴板序号，监听投递前重查，避免回环。写入关闭后取得发布序号，并核对剪贴板所有者，避免 Windows 补全格式造成自身变化误判。读回确认目标消费后，仅在序号仍属于本次操作时恢复；用户复制新内容则保留新内容。最多等待读回 2 秒，不以固定睡眠代表已消费。[Microsoft Using the Clipboard](https://learn.microsoft.com/en-us/windows/win32/dataxchg/using-the-clipboard)、[格式合成](https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats)

已经发送但无法确认时不重发、不自动撤销，也不提前恢复可能未消费的译文。保留译文和原剪贴板快照，显示 unverified；用户核对原输入后在本地设置恢复。尚持有该未核实剪贴板时不开始新的自动替换。退出取消等待、等待收尾并尝试恢复自身剪贴板；重启不重放。

SendInput 返回值只说明事件插入，普通权限不能保证写入管理员应用，不自动提权。纯文本样式和一次撤销粒度以控件样本为准。[Microsoft SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)

## 6 状态与契约

| 状态 | 含义 |
| --- | --- |
| capturing／translating／validating | 采集、缓存／请求等待、回填前核对；可取消 |
| applying | 一次回填权已提交，不接受自动重写 |
| replaced | 完整控件读回匹配；剪贴板恢复问题另行提示 |
| result_only | 有译文但目标或剪贴板不适合自动回填 |
| unverified | 编辑已发送但无法确认，需用户核对，不能声称原文未变 |
| cancelled／failed | 已取消或写入前失败，保留原因，不自动重试 |

CMD-45 接通 status_get、按 operationId 取消、手动查看和恢复剪贴板。状态为 `{operationId,state,originalText,translation,providerId,error,clipboardPending}`，无操作时 id／state 为空。replacement_updated 只携带 operationId／state；native_status 增加 replacementRegistered。完整字段见接口文档。

## 7 实施与最小验收

用户在设计提交后以“ok，please do”授权 M-14。TC-48 仅在一个真实普通输入框核对中间句替换、两侧保持、焦点保持、不发送和一次原生撤销，核对无词条、来源或同步记录增加。必要类型检查及桌面调试编译随本次实现完成，不生成发行物。执行证据和首次失败修正见 [增量验证](../testing/selection-replacement-results.md)。

浏览器、富文本、权限、剪贴板竞争、焦点竞态、服务故障及压力等均为未覆盖边界；额外测试需要提前取得用户明确授权。

## 8 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-11 | 建立 FR-19 提案及最小实施计划，未修改源码 |
| 0.2 | 2026-10-11 | 按授权接入 0.6.0，记录实际模块、UIA 通知核对、内存剪贴板范围及命令，不预填兼容结论 |
