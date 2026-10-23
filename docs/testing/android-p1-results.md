# Android P1 开发与正常验证记录

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-ANDROID-P1-001 |
| 文档版本 | 0.1 |
| 日期 | 2026-10-23 |
| 状态 | 0.26.0 五项 P1 代码已接入，必要正常本机流程及新历史真实交换通过；SubDL 设置已核对，真实 key 下载未验证；未正式发布 |
| 依据 | [Android PRD](../requirements/android-PRD.md) AND-FR-17 至 21、AND-AC-13 至 17；[P1 复用计划](../planning/android-p1-plan.md) |

## 1 范围与环境

用户明确授权完成五项 P1，并要求尽量复用桌面底层。继续单仓库 Rust／React、同一 Store／迁移、查询会话、题库／判分、报告与 assessment-1；不增加手机专用业务库或桌面常驻依赖。Kotlin 仅提供文件／目录、声音、媒体控制和容器元数据适配。

实际设备为原 Mi 10 Pro、Android 13、arm64，沿用原手机资料及 Keystore 真人批准连接。使用本机已固定 SDK／NDK／JDK 和 Rust 1.95，构建 0.26.0 debug APK。Windows 使用 `pnpm desktop:dev`，数据目录仅为 `.local/android-sync-desktop/Data`，未触及日常 `D:\SubtitleVocabularyList\Data`。真实服务器沿用现有站点版本 32，本轮无服务端部署、协议或 schema 变更。

仅执行本轮必要正常路径及发现问题后的对应步骤，没有扩大 P0 回归、故障注入、压力／长时间、设备／格式兼容矩阵。具名合成材料及私有证据保留在 `.local/android-p1`，手机材料在 `Download/SVL-P1`；不提交真实账号标识、凭据、用户库、音频或视频。

## 2 逐项结果

| 需求／验收 | 实际正常结果 | 证据及边界 |
| --- | --- | --- |
| AND-FR-17／AND-AC-13 听读输入 | 系统选择器实际导入 TXT、未加密 EPUB 与 lesson.zip；共享解析展示内容，离线英语声音及课程原声均实际播放到完成。输入草稿强制关闭后重开保持；正确输入自动进入第 2 句并朗读；软键盘展开仍可看原句、输入、反馈及操作 | `draft-before-reopen.json`／`draft-after-reopen.json`、`epub-import.json`、`course-import.json`／`course-finished.txt`、`practice-next.json`；已查看真机 `keyboard-real.png`。抄写沿用原规则，不生成测验通过 |
| AND-FR-18／AND-AC-14 按词预习 | 本地第 2 集文本索引展示候选，展开 practice 原句并实际试听原声，统一确认后收录；progress 标为已掌握后默认从候选隐藏，本机已掌握列表可回读；旧整句的原例句与新增来源分别保留 | `preview-audio.json`、`preview-collected.json`／`preview-final.json`。新词 practice 保留来源、第 2 集、500–4500ms 和独立原声；没有启动整季 AI |
| AND-FR-19／AND-AC-15 高频测试 | 手动离线开启时，以项目自拟短语准备 2 道听音题，核对后发布合格题库；在共享页面完成两题实际听音，一题正确、一题“不知道”。报告有效 2、正确 1、未知 1、未计分 0，正确率 50%；待巩固项 as well 经报告预览并确认收录，保留例句和 system 声音标签 | `bank-candidates.json`、`listening1.txt`／`listening2.txt`、`assessment-complete.json`。题库 ID `143f8a48-0fd5-4500-aedd-68f06a87a2d0`；本次只测短语前 100 范围、2 题，不据此换算语言等级或宣称题库覆盖全部范围 |
| AND-FR-20／AND-AC-16 在线字幕 | 原 SubDL 共享命令、检索／下载／预览／核对／偏移已开放 Android；Media3 解析私有下载文本并接原 Store。手机“我的 → 在线字幕服务”入口存在，启用及导入自动检索均默认 false，API key 为空，文案限定免费基础服务 | `subdl-setting.json`／`final-state.json`。按用户决定由用户自行填写免费 key，本轮没有真实 key，也没有实际 SubDL 下载成功证据；不能标为真实服务全流程验收通过 |
| AND-FR-21／AND-AC-17 媒体 | 实际选择并授权测试目录，预览 2 个直接包含的视频后确认索引，两项成功且各有 1 英文／1 中文 cue。真机原生画面同时显示中英，26sp 与背景开关有效；章节 Progress 定位 3000ms；A=2000、B=4000，播放越过 B 后回到 2479ms，继续播放 | `folder-index-final.json`、`dual-native.png`（已查看）、`chapter-result.json`、`loop-result.json`。MP4 chpl 章节已实测；Matroska Chapters 解析已实现，尚无该类真机章节样本，不外推全部容器／ASS 样式兼容 |
| AND-FR-21／AND-AC-17 查询 | 在手动草稿明确勾选多服务并点击查询，Wiktionary 和 MyMemory 均实际 ready，结果以服务名分别展示；明确采用 Wiktionary 后仅回填该结果，未确认收录。配置恢复原先启用状态 | `query-live-result.json`／`query-live-adopted.json`、`network-final.json`；实际响应内容仍需用户核对，不承诺免费服务翻译质量或持续可达 |

## 3 新测试历史、收录和原声的真实交换

本轮新增测试会话 `eba2d346-df74-460b-83a3-95dc92b02c6d`，完成版本 4。as well 词条 `324e1442-8be9-4570-8615-bcbbdc80a325` 来自报告确认，practice 词条 `e7aa0d15-16b0-42a9-91fa-bed837edd21c` 来自预习确认。均为具名合成资料，保留供检查。

第一次获准任务在账号检查阶段因 Wi-Fi 实际断开而失败，未进入原声传输。VPN 虚拟接口仍使客户端显示 connected；用系统 Wi-Fi 状态确认实际断开后，用户恢复网络并重新明确允许一次最多 20 MiB 原声交换。自动同步始终关闭，现有单任务授权不继承给后续任务。

恢复后的手机任务成功，发送 3 份词库变更，assessment pending=0、conflicts=0、pendingReferences=0，完整交换时间更新。隔离 Windows 正常同步取得 3 份词库变更及同一完整测试历史，assessment cursor=8，未提交新的手机授权或拷贝手机数据库。手机当时保留自己的 cursor=7；这是尚未拉取自身通知的游标，不据此声称两端游标相等。

- 两端完整报告 JSON 与修订列表完全相同。逐题冻结内容、作答事实及收录引用一致；唯一差别为 Android 本机生成语音的缓存 relativePath，在接收端为空，符合系统语音缓存不上传的原约定。
- as well 的报告项在 Windows 指向同一收录词条；接收方用 Windows 离线声音重建该题系统语音，实际播放推进约 0.56 秒。未把 system 音频标为原声，未新增作答或改写成绩。
- practice 的例句、来源、时间和原声在 Windows 可用；两端原声都是 65,568 字节，SHA-256 均为 `9b9fbeb8ce815585714d5b893009fa22ba9511bf12d464e30313bf0721a1e975`，Windows 实际播放推进约 0.52 秒。不是仅凭媒体回执或文件存在判断。

证据：`sync-result-reconnected.json`、`desktop-sync-result.json`、两端 `*-report.json`／`*-report-items.json`、`desktop-assessment-audio.json`、`synced-original-hash.json`／`desktop-audio.json`。本次只验证新增 P1 资料的正常发送与桌面接收，不重跑此前已通过的 P0 冲突／重复交换测试。

## 4 开发中修正与最终状态

实际课程导入发现 Android 未收到原二进制 Raw IPC，已改为两端共同的 `Vec<u8>`／`{bytes:number[]}` 接口并继续使用同一 Store 校验与保存，修正后仅重做该次课程导入、原声播放正常步骤。

实际目录素材发现 Media3 MP4 timed text 的 duration 未声明，紧接的空样本使有效对白丢失。索引改用同轨后续更晚时间戳确定结束，修正后只重新索引已确认的两个目录素材，两项均取得正确中英 cue；旧已收录内容不被删除。

早期网络断开时两个真实查询均不可用，采用流程先用现有格式的具名本地模型接口夹具验证。该证据仅为本地协议验证；临时 ADB reverse 和本地服务已关闭、服务配置已恢复。Wi-Fi 恢复后再完成同一未通过的真实两服务正常步骤，见第 2 节。测试驱动中的候选查询返回类型、带空格按钮标签与异步快照时机已修正，没有为此新增业务记录或异常测试。

最终 `pnpm build`（含版本／唯一 IPC、TypeScript 与 Vite）、Windows 共享宿主开发启动、Android Rust／Kotlin debug 构建通过。手机升级到最终 0.26.0 后仍有 14 个词条、12 份本机来源、课程第 2 句位置和上述测试历史，assessment pending／conflicts／pendingReferences 均为 0；原账号与凭据保留。SubDL 默认关闭且无 key，原服务启用状态已恢复，手动离线关闭用于本轮用户授权同步，自动同步保持关闭。

本轮不包含正式签名、发行打包、安装器或上架，也不增加后台自动同步、其他网络／设备兼容、全部题型组合或字幕格式矩阵测试。已有正常 P0 及剧集共享证据保持独立。当前五项 P1 开发已交付，SubDL 真实服务自配置、Matroska 章节样本及以上未测条件仍有明确边界。

## 5 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-23 | 记录五项 P1 共享实现、必要真机正常流程、新测试历史真实服务器交换及外部服务／媒体支持证据边界 |
