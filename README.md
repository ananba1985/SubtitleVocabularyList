# SubtitleVocabularyList

面向 Windows 的本地英语单词本。通过剧集字幕、原声、划词和截图 OCR 收集生词，以测验和易忘词专项帮助复习，并与已有的英语练习站点同步。

## 项目状态

当前 0.1 未完成。台式机按用户要求完成当前功能点并停止开发，剩余任务在笔记本继续，当前入口见[笔记本交接](docs/development/laptop-handoff.md)。真实桌面已接入剧集导入、按词预习、本地解释、收录合并、词条修改、浏览、独立原声和 Windows 英语语音。全局划词及截图 OCR 已在首台机器 WPF 窗口实际验证；台式机前台操作、其他屏幕配置与应用仍需核对。主动回忆测验、词义与听力独立安排、提示、人工判分修正、历史及易忘词专项已接通并验证核心和部分真实桌面流程。完整站点同步主流程已在笔记本接通并通过真实账号验证；轨道选择、外置字幕、采集队列和手动词典/翻译已补齐；应用/屏幕支持矩阵、同步故障与容量边界、安装和离线整体验收继续进行。

产品名称、公开仓库名称和本地工作目录名称统一为 **SubtitleVocabularyList**。

## 文档入口

| 文档 | 职责 |
| --- | --- |
| [产品需求文档](docs/requirements/PRD.md) | 产品目标、0.1 范围、用户流程、需求和验收标准 |
| [需求审查记录](docs/requirements/review.md) | 文档层审查与开放事项的推进方式 |
| [架构设计文档](docs/design/architecture.md) | 已确认的技术栈、系统边界、模块职责、数据与集成设计 |
| [数据模型设计](docs/design/data-model.md) | 词条、例句、原声、收录、测验及同步数据的关系与一致性 |
| [应用流程设计](docs/design/application-flows.md) | 预习、采集、确认、播放、测验、专项和异常状态 |
| [接口契约设计](docs/design/interfaces.md) | 本地命令、任务、错误及外部适配与同步契约 |
| [测试与验收计划](docs/testing/test-plan.md) | 验证环境、计划场景、需求覆盖与执行证据规则 |
| [首轮原型验证](docs/testing/prototype-results.md) | 实际核心与工具链测试，真实 PGS 剧集结果和覆盖边界 |
| [桌面导入与收录验证](docs/testing/desktop-import-results.md) | 项目内剧集副本、实际界面、合并、重启与取消结果 |
| [Windows 划词与语音验证](docs/testing/windows-capture-results.md) | 用户参与的跨进程取词、系统语音播放、失败与取消结果 |
| [屏幕 OCR 验证](docs/testing/ocr-results.md) | 真实截图、DPI 对齐、识别合并与兼容边界 |
| [复习策略设计](docs/design/review-policy.md) | 初始间隔、掌握和易忘词规则，保留策略版本 |
| [测验与专项验证](docs/testing/review-results.md) | 41 项核心、实际测验、两种听力音频、人工确认、提示和重启证据 |
| [完整同步设计](docs/design/synchronization.md) | 真实连接、完整资料、冻结重试、别名、冲突和媒体契约 |
| [同步增量验证](docs/testing/synchronization-results.md) | 当前核心与真实双向、原声、历史、冲突及未覆盖项 |
| [导入选择与查询验证](docs/testing/import-query-results.md) | 多轨/外置/转写、连续采集、实际模型和手动联网请求 |
| [离线安装与升级验证](docs/testing/offline-release-results.md) | 随包工具、实际安装、原声播放、旧库迁移与发布余项 |
| [安装与使用说明](docs/release/installation.md) | 当前候选包、数据目录、系统资源和基本使用流程 |
| [0.1 开发计划](docs/planning/development-plan.md) | 按依赖安排的实施阶段与完成依据 |
| [文档管理规范](docs/README.md) | 文档分类、评审状态、变更流程和需求追溯规则 |
| [开发约束](AGENTS.md) | 开发者与 AI 协作者必须遵守的项目规则 |

## 0.1 的使用目标

- 观看剧集前，按单词和短语确认需要学习的内容。
- 随时通过划词或截图 OCR 收录生词，并保存语境。
- 重复收录时合并词条，增加例句和音频，保留学习历史。
- 使用原声进行主动回忆测验，分别跟踪词义与听力表现。
- 核心流程离线运行；需要时进行联网查询和在线单词本同步。

当前同一原句可保留多个已确认语境，复用原句和原声。完整同步、网页资料桥接与冲突选择已接通，真人批准后的双向例句、原声和学习历史主流程已通过；现场中断、删除、账号切换、容量和整体离线交付仍有余项。具体需求与验收以 PRD 为准，技术选型以架构设计文档为准。

文档的阅读与维护顺序见 [文档管理规范](docs/README.md)。功能完成情况以实际验证记录为准，计划中的阶段不代表已经交付。

## 仓库维护

公开仓库用于维护文档与后续代码。个人词库、影视文件、原声音频、模型权重和凭据不作为源代码提交。

项目采用 [GPL-3.0-only](LICENSE)。第三方代码和工具的来源、版本与许可按架构设计核对，不因本项目许可而改变它们各自的义务。

第三方版本与来源见 [组件说明](THIRD_PARTY_NOTICES.md)。

## 开发入口

本机验证使用 Node.js 22、pnpm 10、Rust 1.95 和 Visual C++ 工具链。执行 `pnpm install` 后，可使用以下入口：

新 Windows 机器的开发与媒体环境脚本见[开发环境说明](docs/development/windows-environment.md)，包含已实际验证的依赖版本、资源位置与权限要求。

- `pnpm build`：前端类型检查与构建。
- `pnpm test:core`：不启动界面的 Rust 核心测试。
- `pnpm desktop:dev`：开发中的桌面程序，已接入导入、预习与词库界面。
- `pnpm tauri build --debug --no-bundle`：当前已验证的 Windows 调试程序构建。

媒体原型另需 FFmpeg、ffprobe 与英语 Tesseract。`scripts/setup-tools.ps1` 准备私有的 Whisper 程序和模型，并校验模型摘要；它用于开发资源准备，不是最终用户安装说明。

当前划词默认使用 `Ctrl+Alt+Shift+W`，截图使用 `Ctrl+Alt+Shift+S`，可在本地设置修改；声音选项列出已安装的 Windows 英语声音。关闭主窗口后应用留在托盘，可从托盘菜单打开或退出。已验证支持范围与未覆盖项见 Windows 与 OCR 验证记录。
