# SubtitleVocabularyList 保存中断与取消提交边界验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-REC-001 |
| 文档版本 | 0.1 |
| 更新日期 | 2026-10-07 |
| 状态 | 本增量通过；完整 0.1 验收继续 |
| 依据 | FR-05、FR-06、FR-07、NFR-02、NFR-03；TC-18、TC-38、TC-39 |

## 1 环境与方法

Windows 11 x64，使用当前正式 Rust 媒体、收录和任务核心，实际 SQLite WAL 和随安装器提供的 FFmpeg/ffprobe。私有测试根目录是 `.tools/crash-recovery-465456b2af794d11ba580ee8d3772923`。输入来自已解析的合法私有 18 秒片段；程序复制所需音轨到这个新目录，不改原素材、正式词库或其他进程。

`crash_recovery_probe` 建立已确认的 breakfast、原声与一次正确作答，再排队保存另一条 yelling 对白。Python 父进程持有独立 SQLite 写事务，在新 m4a 已经移动到正式媒体目录、对应 media_assets 尚未提交时，终止由它启动的唯一探针进程。它不操作窗口、键盘、剪贴板或网络。

重开探针时执行正式 `recover_interrupted`，核对旧任务报告 interrupted，再以同一业务操作重试。完成后再执行一次，核对成功回执与收录计数。示例中的故障门闩只用于确定验证时机，不进入发行程序。

## 2 实际结果

| 场景 | 结果 |
| --- | --- |
| 确认中断边界 | 新音频文件存在，数据库对其路径的 media_assets 数为 0；任务状态 running，随后终止已核对 PID 的探针 |
| 中断后旧资料 | 1 条词条、1 份已引用原声、1 次收录、1 次作答保留；没有新词条或虚假可播放引用 |
| 启动恢复 | 旧执行标记 interrupted；以原操作标识建立恢复尝试 |
| 重试收录 | 得到 2 条词条、2 份原声、2 次收录；原作答仍为 1 条，旧原声摘要相同 |
| 再次重试 | 复用同一成功任务和结果，没有新增收录；yelling 的主动收录计数为 1 |
| 文件与数据库 | 成功恢复后，两份媒体文件与引用集合一致，无未引用媒体输出；integrity_check=ok、foreign_key_check 无错误 |

原始 JSON 位于私有目录 `verification.json`。本次实际终止的是使用正式核心的原生测试进程，证明具体文件/SQLite 边界；不将其写成 Tauri 窗口操作或所有异常条件均已执行。

## 3 取消与提交竞态

`tasks::tests::cancellation_at_collection_commit_boundary_keeps_truthful_state_and_one_receipt` 使用正式任务管理和 SQLite 收录，并以通道确定两个时机，结果通过：

- 在收录事务之前收到取消：终态 cancelled，没有词条；同一操作重试可收录一次。
- 收录已提交、任务结果尚未返回时收到取消：最终返回 succeeded 和真实保存结果，不把已保存内容报告为丢弃。
- 两种情况下再次开始相同操作均复用成功回执，词条和收录计数都只有 1。

这补充 TC-39 的提交边界；实际导入运行中浏览与取消仍使用先前桌面记录，互不替代。

## 4 重现入口与检查

从项目根目录构建并使用自己的私有素材与新输出目录：

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --example crash_recovery_probe
python scripts/test-crash-recovery.py --tools <安装目录/tools> --corpus <私有corpus.json> --video <私有视频> --output <项目/.tools/新目录>
cargo test --manifest-path src-tauri/Cargo.toml --lib cancellation_at_collection_commit_boundary_keeps_truthful_state_and_one_receipt
```

输出必须在本项目 `.tools` 内且之前不存在。这个固定边界夹具要求语料中含 breakfast 和 yelling 的不同时间段；其他语料需先按相同结构准备。脚本只终止自己的 Popen 进程句柄，重复执行需另选新目录。探针编译、定向提交边界测试及全目标 Clippy `-D warnings` 通过；没有为新增测试工具重复无关界面验收。

## 5 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-07 | 记录真实进程中断、WAL/文件边界恢复、成功回执与取消提交竞态的实际证据 |
