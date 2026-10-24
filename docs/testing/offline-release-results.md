# SubtitleVocabularyList 0.1 离线安装与升级验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-REL-001 |
| 文档版本 | 0.13 |
| 更新日期 | 2026-10-23 |
| 状态 | 0.26.3 播放器解压工具专项检查通过；历史完整打包和预检记录保持，本轮未编译、打包或安装 |
| 依据 | M-08；NFR-01 至 NFR-05；TC-17/19/20/21/37/38/40 |

## 1 交付实现

发行程序使用资源目录内的 ffmpeg/ffprobe、Tesseract、Whisper CLI 与模型，不再通过全局 PATH 查找这些工具。用户偏好仍保存于本地数据库，工具路径从当前程序位置解析，旧版本保存的开发路径不再沿用。Qt/Java/Node/Rust/Visual Studio 不作为最终用户运行依赖。

安装配置采用当前用户 NSIS、简体中文和 WebView2 离线安装程序。工具与模型进入 tools 目录，个人词库、剧集、截图、同步凭据和测试数据不进入安装包。`package-release.ps1` 要求全部资源存在、生成摘要清单，并在打包完成后检查资源没有在构建中变化。

## 2 已执行的资源与代码检查

| 项目 | 实际结果 |
| --- | --- |
| 主程序 | Windows release 与 NSIS 构建通过；首轮候选包 368233622 bytes，SHA256 90644ac531795da91959919b3a94aad785932e7eac4b5110e60b8dd681730428。后续源码与许可资料变更需要重新打包，不将这个候选摘要当作最终交付摘要 |
| 设置升级 | 真实 SQLite 检查通过：旧工具路径被当前安装目录替换；模型标识、声音与联网偏好保留；再次保存不持久化工具路径 |
| OCR 资源 | 从固定上游源码构建 Tesseract 5.5.3、Leptonica 1.85.0、libpng 1.6.43、libtiff 4.6.0、libjpeg-turbo 3.0.1、zlib 1.3.1。实际 PGS 图片返回 “[ Claire ] Kids, breakfast!”；内部字体所需 TIFF/JPEG 已加入，最终测试无缺失字体错误 |
| 转写资源 | Whisper.cpp 1.8.3 与英语 base.en 模型准备完成，模型 SHA256 保持既有固定值。真实 18 秒片段转写输出；识别内容仍需用户纠错，不承诺自动准确率 |
| 媒体资源 | 从 n9.0.2 固定源码构建 FFmpeg/ffprobe；不启用网络、设备采集或额外外部编解码库。修正中文 MSVC 标识探测、构建路径和依赖跟踪，并使用实际静态 zlib |
| 全局工具隔离 | 将子进程 PATH 缩减为 Windows 系统目录，正式 bundled 适配实际完成 OCR 与 18 秒无字幕导入，7 段对白、local_speech。没有靠本机原先安装的工具补齐运行依赖 |
| DLL 核对 | dumpbin 对四个工具的实际导入检查仅为 Windows 系统 DLL；初版 ffprobe 意外引用 zlib.dll 的问题已修复，并重新执行上述隔离测试 |
| 当前源码验证 | 62 项 Windows 核心测试通过、1 项需要真实凭据的测试默认忽略；Clippy 全目标零警告检查、前端类型检查与构建、PowerShell 脚本语法检查通过。真实恢复测试的单独执行结果见同步记录 |

源码归档、构建脚本、实际构建参数、许可证及模型说明随交付整理。工具构建日志、原视频/音频和验证截图保留在私有目录，公开仓库只保存必要脚本与说明。

## 3 实际安装与旧词库迁移

本轮使用独立 `SVL_DATA_DIR`，没有打开、替换或删除用户的正式词库。以 NSIS `/S /D=<隔离安装目录>` 安装，进程退出码为 0。安装目录内的六项程序/模型逐项核对长度与 SHA256，全部符合资源清单。

安装后的主程序以仅有 Windows 系统目录的 PATH 启动，使用真实 Rust 命令与 WebView2 界面。使用新的私有任务进行 18.021 秒无字幕视频转写，得到 7 段对白；保存 breakfast、例句和 3.008 秒原声，完成词义和听力两个维度各一次正确作答。原声与 Windows 英语词条语音实际播放到结束，`readyState=4`、媒体错误为空。此证据证明真实播放器可用，未代替人工听辨。

为检查旧版本迁移，退出应用后按原始 001–004 SQL 创建真实 schema 4 词库，复制本轮 20 张旧表的完整数据，并保留已经实际生成的原声文件。用同一候选安装器重装到另一个隔离目录，安装退出码为 0；随后启动安装程序，实际自动迁移到 schema 5。

| 核对 | 结果 |
| --- | --- |
| 旧数据逐表逐列比较 | 原 schema 4 的 20 张表没有内容变化；词条、释义、例句、来源、主动收录与两次作答保留 |
| 数据库完整性 | `PRAGMA integrity_check=ok`；`foreign_key_check` 无错误 |
| 原声文件 | 安装与迁移前后摘要一致 |
| 当前工具路径 | 返回新安装目录 `installed-upgraded/tools`；不沿用旧安装目录 |
| 界面重开 | 仍显示 breakfast、例句与两个维度的历史 |
| 移走源视频后的播放 | 临时移走测试源文件，保存的 3.008 秒原声仍播完且无错误；源文件随后恢复 |

这是同一版本安装器的重装与真实旧数据库迁移检查，不声称已经执行两个已发布软件版本之间的升级。公开发布前还需核对最终源码匹配包。私有证据位于 `.tools/installer-validation`，界面截图位于 `output/playwright/installed-listening-correct.png`。

## 4 许可和源码交付材料

新增许可收集脚本按 Windows Rust 图和前端运行依赖生成清单；实际取得 316 项 Cargo 依赖、5 项前端运行依赖的条款。缺少文件时中止；未附在 crate 内的工作区条款按该 crate 的固定提交补取，MPL-2.0 使用正式条款，原始源码声明继续保留。

打包入口要求干净的已提交源码，同时导出应用、八份工具上游源码、实际 FFmpeg 配置、Cargo 原始源码包与前端运行依赖。源码归档实际执行通过，核对 1073 个材料文件的大小及 SHA256：316 份 Cargo 原始包与 Cargo.lock 一致，8 份工具源码存在，依赖条款 321 项，内层应用归档的 115 个文件与其 Git 树完全一致。未包含 `.tools`、个人词库、影视媒体或测试输出。修正了 Cargo 缓存校验文件缺失、PowerShell 5.1 绝对路径拼接及 npm 的 1970 年时间戳不适合 ZIP 等实际打包问题；只修改复制文件的时间元数据，不改变源码字节。

### 4.1 首轮源码材料候选

| 文件 | 字节数 | SHA256 |
| --- | --- | --- |
| SubtitleVocabularyList_0.1.0_x64-setup.exe | 368298464 | 69acca89f0bed0e531408f146ca4d369b20de98d53b1d33c4254459426dbac31 |
| SubtitleVocabularyList_0.1.0_source-materials.zip | 114584805 | cdd317d84029fa03c42af21ec866436f7042e44124eedd1e230e49ac4fe45d66 |

安装器构建提交为 `a920884ae4dcfa806300ad51639185a95b836a05`；源码材料提交为 `c5e18ddbebec4e0a5e8e396ad24732cfefca94d1`。两者 `src`、`src-tauri`、package.json 与 pnpm 锁文件完全相同，后续修改只修复源码导出脚本。交付摘要已逐项读取文件核对。尚未发布为已验收版本。

上述新包再次隔离安装，退出码 0；实际安装目录内六项运行资源均符合清单，依赖条款清单 321 项。原测试词库 20 张旧表逐列内容、原声摘要、两次作答记录仍一致；新程序冷启动进入词库，3.008 秒原声与 1.724354 秒系统词条语音都播放至结束，媒体错误为空，前端无错误或警告。台式机已下载同一摘要安装包，完成隔离安装与六项摘要核对，前台兼容测试正在执行。传包使用已有专用网络许可的工作区运行时，没有修改防火墙或网络；完成后临时服务已停止。

### 4.2 随包媒体资源样本

机器为 Windows 11 IoT Enterprise LTSC 26100、AMD Ryzen 7 8845HS、8 核 16 线程，系统报告物理内存约 61.8 GiB。正式 bundled 适配指向上述实际安装目录，PATH 仅含 Windows 系统目录，使用新的输出目录处理既有私有 PGS 图片与 18.021 秒无字幕视频：OCR 返回 “[ Claire ] Kids, breakfast!”，转写 7 段，exit 0。

包括启动、监测与收尾的墙钟用时 4605 ms。监测媒体探针及其直接工具子进程，每轮采样后等待 500 ms，另有 WMI 开销；4 个实际样本的并发 working set 最大 335663104 bytes、private bytes 最大 833138688。这里只报告该媒体样本的采样最大值，未覆盖完整 GUI、Qwen 服务或其他进程，不能代替全机峰值或百分位性能目标。私有原始记录位于 `.tools/installer-validation/final-resource-probe`。

### 4.3 包含大资料同步的前轮候选

源码与安装器统一来自 `047a48f67148017d793a7ae5a1b3fa2ea774cc64`，完整 package-release 入口执行通过。内部 Rust 库已区分为 subtitle_vocabulary_list_core，release 编译与 NSIS 本轮没有 PDB 名称碰撞告警。

| 文件 | 字节数 | SHA256 |
| --- | --- | --- |
| SubtitleVocabularyList_0.1.0_x64-setup.exe | 368313540 | a87fee60aa6b8bcd86c52ac0406979a3f38beaf45b4dc623459b1c8f7cc22977 |
| SubtitleVocabularyList_0.1.0_source-materials.zip | 114595964 | a41e1cc4e1ec62bef6c468b2a471ebd16a8747118a070f79f4f7e3f6f9133829 |

1073 个材料文件摘要核对通过，内层 117 个应用文件与上述 Git 树一致，316 份 Cargo、8 份工具源码、321 项依赖条款保持完整。新包在 `.tools/packet-installer-validation/installed` 实际安装，退出码 0，六项运行资源的长度与摘要一致。

安装后的正式程序在 Windows 系统目录 PATH 下打开大资料验证的私有接收副本，实际调用同步发出一次约 3.54 MB 归档资料；再次同步 pushed=0、pulled=1。pending=0，另有 1 条此前独立恢复夹具的归档冲突仍待选择，不把任务成功描述为无冲突的整体验收。该过程没有修改正式词库。原声 1.710 秒与系统词条语音 1.724354 秒均实际播完、媒体错误为空；历史接口每页返回 30 条，前端错误/警告均为 0。

该轮默认 62 项核心测试通过，3 项依赖独立 HTTP 后台或真人批准凭据的测试默认忽略；其中新增 HTTP 与真实大资料测试已分别显式执行通过，见[同步验证](synchronization-results.md)。

### 4.4 包含媒体路径去重的前轮候选

程序与源码材料统一来自 `87ab4ea2996af9c65572493535827a2904be9d8e`。本轮修复文件夹与其中显式文件重复处理的问题，实际媒体与目录结果见[媒体支持矩阵](media-support-matrix.md)。格式检查、全目标 Clippy 和 64 项核心测试通过，3 项外部条件测试默认忽略。

| 文件 | 字节数 | SHA256 |
| --- | --- | --- |
| SubtitleVocabularyList_0.1.0_x64-setup.exe | 368270682 | cc2a3c2c9bb86bad830fc11270ed3486fc0789af38cdcdc457de3779b689b1c0 |
| SubtitleVocabularyList_0.1.0_source-materials.zip | 114621158 | af50ce90df65721b56127997f0814d709eddc99f3ddc37498ef69a6699caf0af |

完整打包入口退出码 0；1073 个源码材料文件的大小与摘要通过，内层 126 个应用文件与该提交的 Git 树一致，316 份 Cargo、8 份工具源码和 321 项条款保持完整。后续文档提交记录本次结果，不改变这个程序与源码材料基线。

退出测试程序后，在既有私有安装目录静默覆盖安装，退出码 0。Tauri 安装器内的主程序带 `TAURI_BUNDLE_TYPE_VAR_NSS` 标记，构建目录输出恢复为 `TAURI_BUNDLE_TYPE_VAR_UNK`；二者仅这三个字节不同。按 NSIS 标记核对全部程序字节一致，六项随包资源大小及摘要均一致。实际程序摘要为 `c70f440c7e36f9f95cf8711afdd6a81c4a4a45cba07bfe0422e0fc8bd21c8c3a`。

私有实装回执位于 `.tools/packet-installer-validation/latest-install-verification.json`。本轮没有打开界面，没有把前轮播放/同步结果冒充新包整体验收；实际受限制网络和剩余界面条件继续保留。

### 4.5 最终技术验收候选

安装器、程序与源码材料统一来自 `2fbe3c83eba72811d444f640be93191547d95be0`。包含全部未结束任务的列表修正和 0.1 版本显示，格式/Clippy/前端生产构建通过，65 项核心通过、3 项条件测试默认忽略。完整打包退出 0，1073 项材料摘要及 127 个应用文件与该 Git 树一致，316 份 Cargo、8 份工具源码和 321 项条款完整。

| 文件 | 字节数 | SHA256 |
| --- | --- | --- |
| SubtitleVocabularyList_0.1.0_x64-setup.exe | 368313839 | 2b5b803f4f4e4df15997cdfaf6b6c96a3d6d0bed032de58ea2317fc5076cc0e6 |
| SubtitleVocabularyList_0.1.0_source-materials.zip | 114627929 | f1372f3f9deb677a221b381abd427ebf2745f94b2dede9f3afb71317328aa1f3 |

实际静默覆盖安装退出 0，按 NSIS 标记核对主程序字节一致，六项资源大小及摘要一致。安装主程序 SHA256 为 `291b74f11a35cd27dfdc592abf06415b5cf8abb145cd9801bd923a0d4c041ef7`。检查此前主程序 PE 直接依赖均为 Windows 系统库，没有单独的 Node/Rust/Visual Studio 运行依赖；识别工具依赖见既有资源验证，不把开发环境准备当作产品依赖。

本轮最终包未再次打开界面重播全部旧场景：业务接口沿用既有实际执行，任务列表新增门控回归，版本文字有生产构建。实际出站限制、冷启动、用户听辨及 OCR 补测在[离线记录](restricted-network-results.md)，台式机重启/两次重装的数据保持另有[记录](desktop-machine-results.md)。最终安装回执在 `.tools/packet-installer-validation/final-install-verification.json`。

### 4.6 用户试用后的长词表详情修订

程序、安装器与源码材料统一来自 `0cce883a215eb16f5c402a17feb74cccd8928bc9`。本轮修复预习和词库中长列表滚动后看不到当前详情的问题，区分悬停与选中，切换后复位详情滚动并忽略旧选择的延迟语境/试听结果。实际生产 React 组件与样式的浏览验证见[界面记录](desktop-import-results.md#5-用户验收发现的长列表展示修正)。本轮未改变 Rust 或数据库结构，没有重跑全部旧业务场景；前端生产构建、Windows Release 与 NSIS 打包通过。

| 文件 | 字节数 | SHA256 |
| --- | --- | --- |
| SubtitleVocabularyList_0.1.0_x64-setup.exe | 368269753 | d1f9442cd983d1b977dac59958c106486d15b74f810a6827cb9ff5005f073866 |
| SubtitleVocabularyList_0.1.0_source-materials.zip | 114635473 | 8b666adda61a86a8564aa1d69bdecaeb84ae86a3c652b22c68acff455cfbc868 |

完整打包退出 0，1073 项材料摘要及 128 个应用文件与该 Git 树一致，316 份 Cargo、8 份工具源码和 321 项条款完整。用户授权退出后直接更新，检查发现主窗口关闭后仍驻留托盘；确认无进行中任务，先备份正式词库，再由安装器处理驻留实例并覆盖原 `.tools/packet-installer-validation/installed` 目录。静默安装退出 0，按 NSIS 标记核对主程序全部字节一致，六项资源大小及摘要均一致。实装主程序 SHA256 为 `a2cad4fd22c33ed0300ab4607bffbbf9ddd3661e32d09487b5df30cd2c5f3d8f`。

更新前后只读比较正式数据目录的 19 张表全部行内容与两份媒体文件摘要，均保持一致；SQLite quick_check 为 ok。没有复制隔离测试数据库到正式目录，没有启动更新后的程序抢占用户窗口。私有实装回执为 `.tools/packet-installer-validation/layout-install-verification.json`，更新前备份为同目录的 `before-layout-update.sqlite3`。

### 4.7 用户试用后的任务摘要与完整历史分页

程序、安装器与源码材料统一来自 `1245bf482ce282b106d3f41ff715030eb9fca368`。主区保留全部未结束任务和最近一条终态记录，完整终态历史默认折叠并按页读取；任务对象兼容旧 JSON，旧原声可从保存关联补充对象。批量结果先汇总，明细按需展开；听力准备不在摘要中显示待作答目标。需求 NFR-03、FR-10 与 TC-39 的核心和实际前端证据见[任务界面记录](desktop-import-results.md#6-用户验收发现的任务历史展示修正)。67 项核心通过、3 项独立条件默认忽略，格式、全目标 Clippy、生产前端及 Windows Release/NSIS 通过。

| 文件 | 字节数 | SHA256 |
| --- | --- | --- |
| SubtitleVocabularyList_0.1.0_x64-setup.exe | 368307805 | f39b5873f33d49246af00b6e6dcdb2424e2478709672d673046e98c6cdacc9e1 |
| SubtitleVocabularyList_0.1.0_source-materials.zip | 114644651 | b4ba7b0b8b49de301f4af9796f604020d31d5bc9e3beb637e648da3a36b67e0a |

完整打包退出 0，1073 项材料摘要及 129 个应用文件与该 Git 树一致，316 份 Cargo、8 份工具源码和 321 项条款完整。确认客户端未运行、无进行中任务后，备份正式数据并静默覆盖原 `.tools/packet-installer-validation/installed` 目录，退出 0。按 NSIS 标记核对主程序全部字节一致，六项资源大小和摘要一致；安装主程序 SHA256 为 `a6345168751ae827935a6073d9780360601a3e15d384d01e9c1d8f806fa4c766`。

更新前后的 19 张数据库表全部行内容和两份媒体摘要一致，SQLite quick_check 为 ok，没有替换为合成测试词库。未启动新程序干扰用户桌面；本轮界面验证使用生产组件和可控接口，未重新执行全部原生导入、播放、测验及联网场景。私有回执为 `.tools/packet-installer-validation/task-history-install-verification.json`，备份为同目录的 `before-task-history-update.sqlite3`。

### 4.8 本机全局已掌握筛选与采集跳过

程序、安装器与源码材料统一来自 `cca5738507cc486e30c2c3465432f64d3718f84b`。新增独立自评筛选偏好，在本机已有/未来剧集默认隐藏目标，划词/OCR 跳过完整已掌握目标的确认，可主动查看、搜索分页恢复；新收录原子取消跳过，旧回执不撤销后来标记。schema 仍为 5，原数据/媒体/学习历史不删除，手动标记不计测验成绩，本地配置偏好不进入现有站点词条资料包。需求 FR-02 至 FR-04、FR-10、NFR-02 的核心和生产前端证据见[筛选记录](desktop-import-results.md#7-全局自评已掌握筛选与采集跳过)。71 项核心通过、3 项独立条件默认忽略，格式、全目标 Clippy、前端生产及 Windows Release/NSIS 通过。

| 文件 | 字节数 | SHA256 |
| --- | --- | --- |
| SubtitleVocabularyList_0.1.0_x64-setup.exe | 368340388 | 6d0c5a84f23a25d7fb783b64b421dcd13d1e1e52f7eb63880f62833775059f49 |
| SubtitleVocabularyList_0.1.0_source-materials.zip | 114655687 | 172a250d4f54185429267ae3d9e79111db8ff96206b475a106a1304beafb7463 |

打包退出 0，1073 个材料摘要及 131 个应用文件与该 Git 树一致，316 份 Cargo、8 份工具源码和 321 项条款完整。确认客户端关闭、无进行中任务，备份正式数据后静默覆盖原 `.tools/packet-installer-validation/installed` 目录，退出 0；按 NSIS 标记核对全部程序字节一致，六项资源大小和摘要一致。安装主程序 SHA256 为 `7b6cf3fd69fa788f7096c5d88052a9685696f851ca278d038bbe294150405466`。

更新前后正式数据的 19 张表全部行内容及两份媒体摘要一致，SQLite quick_check 为 ok，没有替换为合成测试词库。未自动启动新程序；本轮未重跑 OS 前台/快捷键/OCR 窗口、播放和联网场景。私有回执为 `.tools/packet-installer-validation/known-targets-install-verification.json`，备份为同目录的 `before-known-targets-update.sqlite3`。

## 5 用户验收与保留边界

此前缺少的真实出站限制、听辨、实装性能、台式机媒体/测验/重装已补测，参见[发布说明](../release/0.1.0-release-notes.md)和[当前覆盖](acceptance-status.md)。用户正式使用验收仍待完成；浏览器、物理多屏等未测配置不进入兼容承诺，Chromium/WebView2 1412 退出日志保留其已观察边界。

以下段落保留前轮交付时尚未完成的条件，不覆盖上面的最终状态：

仍须执行断外网冷启动全流程、应用/屏幕支持矩阵、真实播放听感、完整资源/响应性能目标及最终需求逐项审计。临时隔离外网脚本已准备，需要用户确认管理员 UAC；在未执行前不将“离线模式”代替受限制网络条件。台式机整体验证尚未结束，不据传包、安装及笔记本结果声称台式机完成。

此前的 PDB 输出碰撞在第 4.3 节对应源码中已消除。断外网、界面工具停止后的剩余台式机场景、完整异常恢复/性能条件及最终需求审计仍需继续，不据这个增量关闭完整 0.1。

## 6 0.8.2 默认打包入口修复与实际生成

用户直接运行根目录 `生成安装包.ps1` 时，在 libpng 的 CMake 配置阶段收到弃用警告并触发 `NativeCommandError`。之前的 0.8.1 打包使用 `-SkipToolBuild`，没有覆盖此默认路径。本次修正原生重定向命令的错误策略，按实际退出码判定，警告仍完整保留在日志。

2026-10-15 使用本机 Windows PowerShell **5.1.26100.9711** 执行：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\生成安装包.ps1
```

没有传入跳过工具构建的参数，编译并行量为 2。zlib、libpng、JPEG、TIFF、Leptonica、Tesseract、Whisper 和 FFmpeg 均正常构建；原来 libpng 的弃用警告仍可在 configure.log 中看到，未中断后续流程。release 编译、NSIS、standalone、源码导出和摘要步骤完整完成，默认脚本退出码 **0**。

| 产物／核对 | 实际结果 |
| --- | --- |
| 应用与源码版本 | 0.8.2；构建源码为 `f1578cee6457dc02ddfbd5d981f0c2ce4caedced`，打包时工作区干净 |
| 安装包 | `release/0.8.2/SubtitleVocabularyList_0.8.2_x64-setup.exe`，404,605,267 bytes |
| 安装包 SHA-256 | `16b02fce52c5e4a87a99b02c94726da316de2ec6bea3370120d06a066c4cf906`；与生成的 SHA256SUMS.txt 核对一致 |
| 独立运行版 | `release/0.8.2/standalone/SubtitleVocabularyList.exe`，文件／产品版本均为 0.8.2；工具和 Data/dictionaries 随目录生成 |
| 源码材料 | `release/0.8.2/SubtitleVocabularyList_0.8.2_source-materials.zip`；内部应用源码 ZIP 名称对应上述完整提交 |
| 构建记录 | `.tools/package-0.8.2-windows-powershell.log`，保留本次默认流程输出 |

已执行四个相关脚本的 PowerShell 5.1 语法检查、统一版本检查和上述一次真实默认打包正常流程；未执行故障注入、扩展兼容矩阵或完整回归。本轮未运行安装器、启动独立运行版或改写日常个人词库；既有 0.1 安装验收不作为 0.8.2 实装证据。本文的后续记录提交不改变上述已生成产物对应的源码身份。

## 7 0.8.32 打包窗口保留与实际生成

用户从资源管理器右键“使用 PowerShell 运行”启动根目录脚本，窗口退出后无法查看结果。0.8.32 增加默认结束后等待 Enter、可双击的 CMD 入口、入口执行记录和异常位置／调用栈；终端调用可用 `-NoPause` 保留退出码并直接返回。

初期只执行过 `-CheckOnly`，不构成真实打包验证。用户随后提供的日志在干净源码检查处停止，原因是本轮修复文件当时尚未提交。提交修复后，2026-10-20 使用 Windows PowerShell **5.1.26100.9711** 实际执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\生成安装包.ps1 -NoPause
```

本次没有传入 `-CheckOnly` 或 `-SkipToolBuild`；`-NoPause` 只取消结束后的按键等待。默认流程完成底库准备、Cargo 清理、OCR／Whisper／FFmpeg 重建、335 个锁定依赖的许可准备、前端与 Rust release 编译、NSIS、独立运行目录、源码材料和摘要生成，脚本退出码 **0**。Rust release 编译日志显示耗时 9 分 44 秒；此为一次构建记录，不作为性能结论。

| 产物／核对 | 实际结果 |
| --- | --- |
| 应用与源码版本 | 0.8.32；构建源码 `f553cd79b94c28be0f1afc1abc4f3a428a24d1ec`，打包前后工作区干净 |
| 安装包 | `release/0.8.32/SubtitleVocabularyList_0.8.32_x64-setup.exe`，415,951,102 bytes（约 396.68 MiB） |
| 安装包 SHA-256 | `c0cd8b468b5074e1aadfc63d637cdb400541ee97c5192564b8a92430dd8a0cac`；独立计算后与 `SHA256SUMS.txt` 一致 |
| 独立运行版 | `release/0.8.32/standalone/SubtitleVocabularyList.exe`，27,790,336 bytes，文件／产品版本均为 0.8.32；工具及 `Data/dictionaries` 随目录生成 |
| 源码材料 | `release/0.8.32/SubtitleVocabularyList_0.8.32_source-materials.zip`，115,810,348 bytes；内部应用源码 ZIP 名称对应上述完整提交 |
| 摘要文件 | `release/0.8.32/SHA256SUMS.txt`，80,614 bytes |
| 入口执行记录 | `.local/package-logs/package-20261020-021847-817-469096.log`，包含最终产物路径及 transcript 结束标记 |

入口记录保留 PowerShell 输出、原生 stdout 和捕获的异常；离线工具的详细构建输出仍在其 `diagnostics` 日志。Windows PowerShell transcript 未收录本次 Tauri／Cargo 的全部原生 stderr 编译进度，不把入口文件称为每个子进程的完整原始日志。控制台观察到的 CMake 弃用、Vite 分块大小及 Rust 未使用导入警告均未阻止本次完成。

最小入口检查包括 Windows PowerShell 5.1 语法、CMD 的正常 `-CheckOnly` 调用与停留、入口计划和日志、统一版本检查，以及上述一次真实默认打包和交付物核对。没有运行安装器或独立应用，没有执行登录／开机启动／覆盖安装验收，也没有对日常词库、原声和学习历史执行操作；本次不据打包通过声明这些功能已重新验收。后续文档提交不改变上述产物对应的源码身份。

## 8 0.8.33 打包前置检查

本轮按用户要求将发行入口的 `-CheckOnly` 从计划预览改为真实前置检查，正式打包调用同一函数，避免预检遗漏源码和进程状态。验收条件为：在清理／准备资源／编译之前检查版本、Git 状态、实际 Cargo 构建目录及运行进程、开发工具／前端依赖和所选模式的工具／资源；发现问题用中文集中说明原因及处理办法，通过退出 0、阻塞退出 1。

2026-10-20 完成三个相关脚本的 Windows PowerShell 5.1 语法检查、`pnpm version:check`（0.8.33）及源码差异检查。提交 `9e9e644` 后，在工作区干净的实际仓库执行以下最小正常验证：

| 实际调用 | 结果与执行记录 |
| --- | --- |
| `powershell -NoProfile -ExecutionPolicy Bypass -File .\生成安装包.ps1 -CheckOnly` | 默认重建模式，实际检查版本、干净源码、空闲 Cargo 目录和开发前置条件，退出 0；`.local/package-logs/package-20261020-030547-904-286320.log` |
| `pnpm desktop:package -CheckOnly` | 原快捷入口传递 `-SkipToolBuild -NoPause -CheckOnly`，复用模式额外检查六个关键资源文件存在，退出 0；`.local/package-logs/package-20261020-030550-129-441576.log` |

两份日志均有预检通过、未开始构建的说明及 transcript 结束标记；调用前后 Cargo.lock 的 SHA-256 一致，Git 工作区仍干净。Cargo metadata 使用 `--locked --offline`，不更新锁文件或下载依赖。辅助属性核对引用的历史 `src-tauri/target/release/SubtitleVocabularyList.exe` 在调用前已不存在，因此不将该项属性比较作为产物保持证据，也未为此重跑预检或构建。

本轮不为验证制造未提交文件、启动占用程序或删除资源，也不追加故障注入／兼容矩阵／完整回归。源码和进程的中文阻塞分支属于已实现代码约束，不冒充本轮已执行这些额外场景。没有清理 Cargo、重建工具、编译、生成安装包、运行或安装应用；0.8.32 交付物和个人资料不参与修改。预检不代替完整打包、联网下载、资源／许可／源码摘要和实装验证。

## 9 0.26.3 播放器解压工具检测

用户在 0.26.1 默认安装包入口的预检阶段遇到“未找到开发命令 7z”，尚未开始编译。播放器 `.7z` 解压早已使用 7-Zip，打包接入提交 `a8a73a5` 增加了无条件 PATH 预检；本机已有 7-Zip，但不同终端的 PATH 使这项检查误报，且现有 DLL 的固定摘要正确，本次根本无需解压。

修复将工具定位收敛到 `archive_tools.py`，覆盖命令路径、7-Zip 安装注册表、标准目录及 Scoop；预检通过 `prepare-mpv-release.py --check-extractor` 只读检查 DLL 缓存及解压需要，DLL 有效即不要求 7-Zip。正式准备需要解压时使用识别出的绝对路径，开发资源入口复用定位规则。没有安装新工具、修改系统 PATH 或降低摘要／架构／许可要求。

最小正常验证：Windows PowerShell 5.1 解析两个修改入口通过，统一版本检查通过；当前缓存的 `--check-extractor` 返回 `required=false`。在子进程移除 PATH 中 Scoop／7-Zip 条目后，定位到已有 `D:\Program Files\7-Zip\7z.exe` 并执行信息查询成功；同一正常环境运行实际播放器资源准备，复用已有 DLL 和源归档，PE／摘要／API 校验及许可复制完成，返回 client API 2.6，退出 0。

本轮未执行解压兼容矩阵、下载新归档、完整工具重建、Rust 编译、安装包生成或安装；正式资料库不参与。仓库另有进行中的资料维护改动，未为了跑完整打包预检而提交或撤销这些改动，干净源码要求继续保留。

## 10 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.1 | 2026-10-07 | 记录随包资源、实际依赖、PATH 隔离及设置升级；安装与整体目标继续执行 |
| 0.2 | 2026-10-07 | 记录 NSIS 实装、安装资源摘要、实际原声/系统语音和双维度作答、20 表旧库迁移及源码许可交付实施 |
| 0.3 | 2026-10-07 | 记录实际源码材料与文件摘要、新包实装和播放、台式机传包进展及随包媒体采样；继续完整验收 |
| 0.4 | 2026-10-07 | 记录统一源码的大包同步新安装器、源材料校验、实际同步/播放与 PDB 告警修复，保留整体验收余项 |
| 0.5 | 2026-10-07 | 记录媒体路径修正后的匹配新包、源码摘要、静默实装和程序标记核对，完整验收仍未达成 |
| 0.6 | 2026-10-07 | 记录最终候选与匹配材料、静默实装及已补测门槛，进入用户验收 |
| 0.7 | 2026-10-07 | 记录用户试用后的详情修复、匹配新包、原目录覆盖安装及正式词库与媒体保持 |
| 0.8 | 2026-10-07 | 记录任务摘要与完整分页历史的新包、匹配材料、原目录更新与正式数据保持 |
| 0.9 | 2026-10-07 | 记录全局已掌握筛选的匹配新包、原目录更新及原资料保持，不扩大 OS 操作验证范围 |
| 0.10 | 2026-10-15 | 记录 0.8.2 Windows PowerShell 5.1 默认打包修复、无跳过参数的完整正常流程和匹配产物身份 |
| 0.11 | 2026-10-20 | 区分入口预览与实际执行，记录 0.8.32 默认完整打包、匹配源码／产物摘要及未安装边界 |
| 0.12 | 2026-10-20 | 记录 0.8.33 共用打包前置检查、中文阻塞提示、最小正常验证和未打包边界 |
| 0.13 | 2026-10-23 | 记录播放器缓存有效时不要求 7-Zip、非 PATH 安装识别及实际缓存资源准备的最小正常结果 |
