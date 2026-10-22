# SubtitleVocabularyList 本地中英字幕增量验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-SUBTITLE-001 |
| 文档版本 | 0.1 |
| 更新日期 | 2026-10-21 |
| 状态 | 0.10.0 本地文本正常路径通过；在线与图片字幕未验证 |
| 依据 | FR-30／31、M-25／27、TC-65／67 |

## 1 环境和素材

Windows 本机开发程序，独立标识 `com.subtitlevocabularylist.subtitles.acceptance`，独立资料目录 `output/subtitle-import-acceptance/Data-passing`。日常安装版及其词库未变更，不生成安装包。该命名测试目录与首次发现问题的 `Data` 保留供手动查看。

合成 `bilingual.mkv` 为 8 秒绿底视频、静音 AAC、英文 ASS 与中文 SRT；同名外置英文／中文文件也存在。媒体 SHA256 为 `61ca30970a37a44c059151e9de5afd7f1f02ce074af21f12f87e409cc443d457`。英文两句，中文三段，第一英文对应两段中文；ASS 原文包含 `pos(230,120)`。不使用真实用户媒体或外放测试音。

## 2 最小检查及结果

| 检查 | 实际结果 |
| --- | --- |
| 核心真实 FFmpeg 导入 | `cargo run --manifest-path src-tauri/Cargo.toml --no-default-features --example subtitle_acceptance -- output/subtitle-import-acceptance/Data-passing output/subtitle-import-acceptance/bilingual.mkv` 成功退出；显式音轨 1／字幕轨 2 导入可再次打开，选项指纹不再误当媒体摘要 |
| 原始资产与学习文本 | 保存中英原轨及同名外置文件，共 4 项；原 ASS 位置保持，学习原文为 `I will arrive tomorrow.`，不含 FFmpeg 遗留的 `{\\an8}` |
| 时间对应及缓存 | 第一英文对应 `我明天会到。`、`请放心。`；第二句缓存为 `请等我。`；待准备工作中没有这两句的模型翻译任务 |
| 旧资料保持 | 导入前后人工词条 `keep / 保留的人工释义` 的完整读取结果一致；导入后个人词条仍 1 项，没有整片自动收录 |
| Computer Use 真实界面 | 原生鼠标进入边看边学、展开学习设置、点击“补充本地中英字幕”；聚合任务 `fe0cbeca-f339-4176-85ab-a5273bd32cb7` 成功，4 项资产，无警告 |
| 中文选择与重开 | 原生操作选择 `bilingual.zh.srt · 外置 · 3 条`；开发程序重启后仍为该选择，原生点击打开剧集成功 |
| 暂停复用 | 为精确定位，通过调试 IPC 静音、定位 1.5 秒并暂停，随后取消本次自动收录；快照和实际窗口学习面板均显示第一句及对应的两段中文，不含下一句；不把 IPC 定位说成真实按键操作 |
| 编译与类型 | `pnpm build` 通过（版本／IPC 签名核对、TypeScript、Vite）；原生开发程序编译并启动成功 |

## 3 发现和修正

首次导入发现 FFmpeg 的 SRT 含 ASS 控制标记，导致规范句缓存键不一致。仅在学习文本解析时去除标记，原 ASS 不变；修正后上述核心检查通过。

真实选择外置中文时发现容器导入时间有 21 毫秒偏移，下一句中文的极小重叠被拼入当前句。改为排除边缘重叠后，在同一原生程序重启、同一片段复核，第一句准确为两段中文；选择持久化也保持。没有扩大为全格式或偏移矩阵测试。

## 4 证据边界

本机 Computer Use 的窗口捕获未取得 OpenGL 原生视频子平面，截图中的视频区域为空白；本轮截图只证明 WebView 学习面板，不能冒充画面字幕的视觉验收。0.9.0 的原生控制条点击／拖动／空格与渲染缓冲证据另见[核心记录](watch-and-learn-results.md)。

本轮无在线检索／下载、图片轨独立提取／OCR、不同发行版校正、可听音频或发行安装验收，也未进行故障注入、性能或兼容矩阵。完整计划继续实施，不将局部正常结果当作全模块完成。
