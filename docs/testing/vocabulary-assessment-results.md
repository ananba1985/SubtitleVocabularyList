# 高频水平测试增量验证记录

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-ASSESSMENT-001 |
| 文档版本 | 0.1 |
| 更新日期 | 2026-10-15 |
| 状态 | M-15 基础正常流程通过；M-16 至 M-19 与 TC-54 至 TC-58 尚未完成 |
| 对应版本 | 0.8.0 开发中；业务 schema 9，Google 参考 schema 2／ECDICT schema 1 |
| 追溯 | [PRD](../requirements/PRD.md) FR-20、FR-21、NFR-02；[详细设计](../design/vocabulary-assessment.md)；[开发计划](../planning/vocabulary-assessment-plan.md) |

## 1 实施与验证范围

本次只验证 M-15 的来源索引、独立题库、配置保存及审核发布。新增 009_assessments.sql 的领域基础表、assessment_bank.rs 和 CMD-46 至 CMD-49；会话、报告等表已预留，但后续业务逻辑并未据此宣称完成。题库准备任务接入既有 TaskManager 和 ModelGate，批大小／并发采用现有配置，已有候选复用、缺项记录和准备定位持久保存。

所有写入位于临时合成数据库或仓库忽略的 `.local/assessment-validation/Data`。日常安装版 `D:\SubtitleVocabularyList\Data` 没有迁移、替换或清空；未停止安装版进程，未生成 exe／安装包，未操作站点。

## 2 TC-53：一个正常升级与题库流程

命令：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib assessment_bank_tests::tc53_upgrade_catalog_and_reviewed_bank -- --nocapture
```

合成 schema 8 库预置一个个人词条、人工释义及到期／重学时间。打开新核心后迁移至 schema 9；合成词频目标保留原排名 1／2／3，其中排名 2 没有中文词典项，排名 3 有明确词形映射。保存独立测试配置，写入同一目标的辨识、填空和听音三类候选题，逐题明确核对批准，发布题库，退出重开再读取。

结果：**1 passed，0 failed**。原排名及中文缺项目标保留；发布题库为 1 个目标、3 道题，状态及题面重开后可读。个人词条仍为 1 条，人工释义未改，到期／重学时间仍为 123／456。合成声音文件只验证题库资源关联与发布，不作为真实声音、播放器或听感证据。

## 3 真实来源索引生成

在隔离目录生成实际 `base.sqlite3`，没有读取个人业务库进行分析；原始数据使用已固定下载内容及摘要。

| 口径 | 实际结果 |
| --- | --- |
| Google 单词来源 | chenny/english-word-10000 提交 a3bfa2903a2cbbbeb8cd3df5a3f6003578b08f8b |
| 原始单词行数／最大原排名 | 9,972／9,974；原文件空缺排名不重新补号 |
| Google 有效中文词典词头 | 9,306 |
| 频次目标没有 Google 中文词头释义的排名项 | 666；仍保留在完整词频目录 |
| 双词来源 | [Norvig 公开 Google 频次子集](https://norvig.com/ngrams/)，count_2w.txt |
| 双词文件 SHA-256 | 781c0596c3eea532d30bef9f3dba1d5137d652f00376260822c761a7584dfb8c |
| 原始双词记录 | 286,358；按频次降序、同频原文排序定义排名 |
| 初始自然短语候选 | 20；其余双词不作为合格自然短语 |

例如 `such as` 原排名 48、`make sure` 906、`in fact` 1520、`give up` 5924、`take off` 19762。这些排名属于原双词频次集合，不是把 20 条筛选结果重新编号。整体中文释义与自然例句为项目自拟；生成候选仍须逐题核对才能批准发布，不把频次命中称为正式题库已完成。

来源页面明确针对代码声明 MIT；该声明不自动扩展到原始语料。源文件、索引及个人题库均未提交源码仓库，本记录不声称取得完整 Google／LDC 语料的通用再分发许可。

## 4 最低必要编译检查

| 检查 | 结果 |
| --- | --- |
| cargo check --manifest-path src-tauri/Cargo.toml --features desktop | 通过；包含已注册的题库命令及 Windows 语音准备路径的编译 |
| pnpm exec tsc --noEmit | 通过；后台任务增加题库准备名称及分口径结果 |
| pnpm version:check | 通过；四处应用版本均为 0.8.0 |

核心测试的非 desktop 构建仍有既有查询窗口方法未使用警告；没有新增编译错误。未运行扩展回归、故障注入、压力、兼容矩阵或性能测试。

## 5 尚未完成的完整目标

M-16 的分层抽样、私有会话快照和判分，M-17 的真实桌面及实际声音，M-18 的报告／确认收录／复测，M-19 的真实站点完整双向同步，仍需继续实施并分别形成 TC-54 至 TC-58 证据。真实模型批量出题及真实候选的用户审核也尚未在本记录验证。M-15 基础通过不关闭全部 FR-20 至 FR-24，不表示可以开始完整水平测试或已经完成发行验收。

## 6 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-15 | 记录 M-15、TC-53、实际来源索引和编译结果，保持完整开发目标 |
