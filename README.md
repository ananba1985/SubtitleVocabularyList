# SubtitleVocabularyList

面向 Windows 的本地英语单词本。通过剧集字幕、原声、划词和截图 OCR 收集生词，以测验和易忘词专项帮助复习，并与已有的英语练习站点同步。

## 项目状态

当前正在实施 0.1，已建立 Tauri 桌面工程、本地数据库和收录核心，正在验证采集与剧集导入。应用尚未完成完整功能验收；设计和验证记录随实施更新。

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
| [0.1 开发计划](docs/planning/development-plan.md) | 按依赖安排的实施阶段与完成依据 |
| [文档管理规范](docs/README.md) | 文档分类、评审状态、变更流程和需求追溯规则 |
| [开发约束](AGENTS.md) | 开发者与 AI 协作者必须遵守的项目规则 |

## 0.1 的使用目标

- 观看剧集前，按单词和短语确认需要学习的内容。
- 随时通过划词或截图 OCR 收录生词，并保存语境。
- 重复收录时合并词条，增加例句和音频，保留学习历史。
- 使用原声进行主动回忆测验，分别跟踪词义与听力表现。
- 核心流程离线运行；需要时进行联网查询和在线单词本同步。

具体需求与验收以 PRD 为准，技术选型以架构设计文档为准。

文档的阅读与维护顺序见 [文档管理规范](docs/README.md)。功能完成情况以实际验证记录为准，计划中的阶段不代表已经交付。

## 仓库维护

公开仓库用于维护文档与后续代码。个人词库、影视文件、原声音频、模型权重和凭据不作为源代码提交。

项目采用 [GPL-3.0-only](LICENSE)。第三方代码和工具的来源、版本与许可按架构设计核对，不因本项目许可而改变它们各自的义务。

第三方版本与来源见 [组件说明](THIRD_PARTY_NOTICES.md)。

## 开发入口

本机验证使用 Node.js 22、pnpm 10、Rust 1.95 和 Visual C++ 工具链。执行 `pnpm install` 后，可使用以下入口：

- `pnpm build`：前端类型检查与构建。
- `pnpm test:core`：不启动界面的 Rust 核心测试。
- `pnpm desktop:dev`：开发中的桌面程序；业务界面仍在接入。
- `pnpm tauri build --debug --no-bundle`：当前已验证的 Windows 调试程序构建。

媒体原型另需 FFmpeg、ffprobe 与英语 Tesseract。`scripts/setup-tools.ps1` 准备私有的 Whisper 程序和模型，并校验模型摘要；它用于开发资源准备，不是最终用户安装说明。
