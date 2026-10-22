# SubtitleVocabularyList 播放列表与偏好增量验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-PLAYBACK-001 |
| 文档版本 | 0.6 |
| 更新日期 | 2026-10-22 |
| 状态 | 0.16.3 真实 Actor 后台字幕恢复、连续文件请求与自动连播通过；既有实际操作／原生合成图证据保留，剩余窗口操作待续，未生成安装包 |
| 依据 | FR-32 至 FR-37、M-28 至 M-33、TC-68 至 TC-74 |

## 1 实现与实际检查

播放操作继续交给已有 libmpv。新增折叠列表、多选与目录、自然排序、切集和队列编辑、默认开启且可关闭的自动连播；工具条提供当前倍速选择。保存队列、当前项、速度／音量／静音／连播及逐文件位置／音轨／主第二字幕／字幕延迟，恢复列表后保持暂停。换集进入新的学习会话，旧 OCR、查询和未到期收录计时失效。

| 检查 | 执行及结果 |
| --- | --- |
| 隔离资料正常保存 | `cargo run --quiet --locked --manifest-path src-tauri/Cargo.toml --no-default-features --example playback_acceptance -- output/playback-acceptance-0140 output/subtitle-import-acceptance/bilingual.mkv`：通过。Episode2 排在 Episode10 前；两个文件各自的位置、音轨、字幕延迟，1.25×／音量 32／静音／连播和当前项在重开资料库后保持 |
| 真实 libmpv 正常播放控制 | `python scripts/verify-playback-normal.py .tools/runtime/mpv/libmpv-2.dll output/playback-acceptance-0140`：通过。两个文件真实加载，经过 on_load 钩子，列表换序、上一集、移除正常；第一集 seek 至片尾后自然 EOF 自动加载第二集，倍速仍为 1.25×。使用 vo=null／ao=null／audio=no，无窗口、无声音 |
| 前端／IPC | `pnpm build` 通过，包括版本一致性、Rust IPC 签名及嵌套 DTO 检查、TypeScript 和 Vite 构建；有既有大 chunk 提示 |

隔离目录在仓库根目录的 `output/playback-acceptance-0140/`，`persistence.json`、`mpv.json`、`Data/` 和两份静音视频保留，不提交个人数据或视频到 Git。复跑核心样本须选新的命名目录，脚本不重置既有目录。

## 2 边界与后续使用

此次未启动桌面调试／安装客户端，未执行 Computer Use，也未生成安装包。真实原生引擎命令和存储结果不等同于已经人工点击过新增列表、全屏倍速提示或验证实际声音；本次没有这些新窗口实测证据。已有实际证据保持，后续使用反馈按 bug 修复，不等待人工确认或真实字幕服务配置。自动片头／片尾跳过不在本次范围。

## 3 0.15.0 常用播放控制

鼠标补通滚轮、水平滚轮、中键与侧键；右键／菜单键进入同一 Windows 菜单，工具条只增加“播放设置”。菜单集中提供音轨、主第二字幕及显示／样式、指定时间、逐句与章节、单集／列表／A–B 循环。继续调用 mpv 命令及内置 console，不新增播放器、外部脚本或运行时。

| 最小检查 | 实际结果 |
| --- | --- |
| `cargo test --quiet --locked --manifest-path src-tauri/Cargo.toml --lib --features ipc-types player::menu::tests::normal_native_menu -- --exact` | 1 项通过，114 项过滤；实际创建及释放 Windows 菜单并核对第二字幕／章节／列表循环／时间输入映射，不执行弹出和点击。编译时原有 Store 测试夹具漏初始化 subtitle_guard，补齐该测试字段后正常；运行时存储逻辑未因此改变 |
| `cargo run --quiet --locked --manifest-path src-tauri/Cargo.toml --no-default-features --example playback_acceptance -- output/player-controls-acceptance-0150 output/player-controls-fixture-0150/chapters.mkv` | 保存及重开通过；新增 1.25 字号比例、位置 90、file 循环等偏好随既有播放状态保持，时间格式 00:01:30.5 解析为 90.5 秒 |
| `python scripts/verify-playback-normal.py .tools/runtime/mpv/libmpv-2.dll output/player-controls-acceptance-0150 --controls` | 真实库滚轮音量、侧键下一集、aid／sid／secondary-sid、显示／样式／延迟、第二章节跳转、单集／列表循环属性及一次 A–B 回绕通过。mpv 内置时间输入框通过原生按键收到 00:00:02，CLIENT_MESSAGE submit 回传正确，随后原生绝对定位正常；vo=null／ao=null／audio=no，无窗口、无声音 |
| `pnpm build` | 版本、IPC 嵌套类型、TypeScript 与 Vite 通过；保留既有大 chunk 提示 |

最初无窗口样本未像产品一样显式启用 input-default-bindings，滚轮断言等待超时；补齐相同选项后同一正常样本通过，产品启动选项原本已开启，无需改默认绑定。fixture 从既有 8 秒静音双语样本复制流并加入两章，不生成噪音；原视频、测试 Data 和 JSON 输出均保留在命名的 output 目录，未读取日常资料库，不提交媒体／个人资料。

当前结果证明菜单构建、引擎正常控制及存储，不声称已实际点击 Windows 弹出菜单、全屏输入或文件选择；新窗口排版、逐句缓存可定位范围和学习计时的完整宿主操作未人工验收，后续按用户反馈修复，不作为开发阻塞。此次没有额外回归矩阵、故障注入、压力或安装检查。

## 4 0.16.0 拖放与文件打开

对应 FR-37、M-33、TC-73／74。本轮按用户后续授权使用 Computer Use，运行隔离开发客户端，未停止或操作安装版的个人资料。开发 identifier 为 com.subtitlevocabularylist.fileentry0160，Data 位于 `output/file-open-acceptance-0160/Data`，静音偏好在启动前已保存。置顶只用于隔离窗口的拖放检查布局，不改变产品配置。

| 最小正常检查 | 实际结果 |
| --- | --- |
| 核心文件分类及参数 | `cargo run --quiet --locked --manifest-path src-tauri/Cargo.toml --no-default-features --example file_open_acceptance -- output/file-open-acceptance-0160 output/subtitle-import-acceptance/bilingual.mkv` 通过。--data-dir 及其值被排除，相对路径按调用 cwd 解析；目录自然排序、字幕分类和指定首视频正常。记录 files.json |
| IPC／前端 | `pnpm commands:generate`、`pnpm build` 通过；包括真实 Rust 导出、TS 与 Vite。既有 no-default dead-code／unused 提示及 Vite 大 chunk 提示仍在，无新增失败 |
| 已运行时打开 | 再次调用隔离 debug exe，传 --data-dir 和含空格的 Start sample.mkv，第二进程正常返回；Computer Use 的真实窗口显示进入“边看边学”、列表 1 项及当前 Start sample |
| 目录与字幕同批 | 再次传 Series 目录和 Chinese.srt，实际窗口显示列表 3 项及当前 Episode2。只读核对隔离 SQLite：原 Start sample 保留，Episode2 在 Episode10 前；Episode2 的字幕 ID 为 3，externalSubtitles 保存 Chinese.srt，position=0，muted=true。记录 native-open.json；没有把这些结果称为实际鼠标拖放通过 |
| 冷启动带文件 | 结束隔离客户端后，用同一 debug exe、同一隔离 --data-dir 与 Start sample.mkv 冷启动；Computer Use 显示“边看边学”、当前 Start sample、原 3 项列表及暂停提示。启动日志保存在 cold-stderr.log |
| 安装钩子 | NSIS `/PPO` 对最小引入文件正常展开安装／卸载宏，hooks-expanded.nsi 仅包含本程序 Applications／SupportedTypes／OpenWithProgids。没有编译安装器、执行安装宏或修改注册表；安装后的 Explorer 菜单展示留待明确打包安装时验收 |
| 真实拖放边界 | Computer Use 可以读取窗口，但跨窗口 drag 首先报告终点越界，调整布局后又拒绝落点进入另一应用（“not target window explorer.exe”）；没有成功投递拖放。视频／字幕进入同一队列后的真实处理已核对，WebView DragDrop 与原生 WM_DROPFILES 的实际鼠标投递仍未确认 |

本轮没有实际声音听感或原生视频画面显示通过证据：窗口捕获中的原生视频区域为空白，不能据此声称画面已验收。未扩展复测此前功能，未生成发布 exe／安装包、未改变系统默认播放器。源码补齐两种拖放入口及启动链路，后续实际使用反馈按缺陷修复，不以工具不能跨应用拖动阻塞开发。命名的隔离 Data、视频和记录保留，未提交到公共仓库。

## 5 0.16.0 实际操作复核与后台续查

本轮用户明确授权 Computer Use 检查播放暂停、进度条、全屏、右键菜单、字幕、连播及拖放。随后用户需要使用电脑，要求改为后台检查；从该指示起停止桌面输入，并结束本轮隔离 debug 客户端，没有操作正式安装客户端或 `D:\SubtitleVocabularyList\Data`。没有修改播放器产品代码，没有递增应用版本或生成安装包。

运行 `pnpm desktop:dev`，指定 `output/player-ui-acceptance-0161/Data` 和独立 identifier `com.subtitlevocabularylist.playerui0161`；目录后缀是检查编号，应用仍为 0.16.0。全局取词快捷键和自动收录在该隔离库中关闭，播放静音。使用两段 60 秒绿／蓝底合成视频，无音轨，内含英文及中文 SubRip 轨。原夹具、隔离 Data 和结果文件保留，不提交至公共仓库。

Computer Use 执行真实键盘、鼠标点击和拖动。局部窗口捕获未包含 OpenGL 视频层，补充从隔离客户端读取状态和 `GL_FRONT` 渲染缓冲；这些读数用于观察，不以调用播放／定位命令代替鼠标操作。缩放为 200%，坐标操作使用对应 screenshotId。

| 用户要求的检查 | 本轮实际结果及边界 |
| --- | --- |
| 播放／暂停 | 通过。聚焦视频后 Space 开播，再次 Space 暂停在 13.500 秒；mpv 原生 OSC 的播放、暂停按钮也实际点击成功，位置从 10.917 秒继续到 36.833 秒，状态分别为 playing=true／false |
| 进度条 | 通过。原生 OSC 点击从 13.500 秒定位到 30.083 秒；真实拖动后稳定读数为 10.417 秒，仍保持暂停。拖动刚结束的首次快照尚未刷新，依据随后稳定状态及对应画面核对，没有把旧快照当作定位失败 |
| 全屏 | 已实际进入。窗口查询 fullscreen=true，视频区域为整个 1280×800 逻辑窗口；原生渲染缓冲为 2560×1600，显示合成画面与英文。退出全屏尚未执行，不能将进入结果扩大为完整往返通过 |
| 右键菜单 | 已实际打开并选择。右击视频后出现 Windows 原生菜单，经“字幕 → 第二字幕 → zho subrip”选中轨道 2，菜单关闭，快照 secondary=true。其余菜单项未逐项扩展检查 |
| 字幕 | 选轨动作通过，宿主显示仍待复核。选中第二字幕后及再次定位后的 GL_FRONT 图均未见中文，英文正常；同一视频在无窗口 libmpv 中可返回“我明天会到。”，软件内存渲染也同时显示中英文。已排除该夹具中文字幕为空或不能解码，但后台软件渲染不能证明原生 OpenGL 宿主正常；保留该疑点，尚未认定或修复具体产品缺陷 |
| 自动连播 | 后台正常链路通过，窗口片尾连播尚未操作。真实 libmpv 自动从首项 EOF 加载第二项，on_load 钩子顺序与加载顺序一致，倍速保持 1.25×；没有将命令驱动的后台结果写成 Computer Use 通过 |
| 视频／目录／字幕拖放 | 本轮尚未投递真实跨应用拖放。已完成的隔离文件分类结果保留：目录自然排序 Episode2 在 Episode10 前，视频和字幕分别入对应批次，--data-dir 未被当作媒体。WebView DragDrop 和原生 WM_DROPFILES 共用文件队列的实现已读查；代码核对不能替代实际鼠标拖入 |

### 5.1 后台执行与证据

1. `cargo run --quiet --locked --manifest-path src-tauri/Cargo.toml --no-default-features --example playback_acceptance -- output/player-ui-acceptance-0161/background-playback "output/player-ui-acceptance-0161/UI Series/Episode1.mkv"`：通过。新隔离库中列表、逐文件进度／轨道、偏好和当前项保存后重开一致。编译仅有既有 no-default unused／dead-code 提示。
2. `python scripts/verify-playback-normal.py .tools/runtime/mpv/libmpv-2.dll output/player-ui-acceptance-0161/background-playback`：通过。vo=null／ao=null／audio=no，不创建窗口，不播放声音；自动 EOF 连播、返回上一项、移除列表项和偏好读数正常。输出 `background-playback/mpv.json`。
3. 本轮命名目录内 `diagnose-subtitles.py` 调用同一固定版本 libmpv，先在暂停中选择第二字幕，再正常定位。`secondary-sub-text` 两次均为中文；使用 render API 的软件后端将画面写入内存及 PNG，无窗口、无声音。输出 `subtitle-diagnosis.json`、`secondary-selected-paused-software.png` 和 `secondary-after-seek-software.png`。该诊断用于区分字幕解码和原生宿主显示，不改产品渲染后端。

真实桌面观察保存在 `observations.jsonl`；进度拖动的稳定记录为 `seek-drag-settled`。原生图包含 `fullscreen-osc.png`、`seek-click.png`、`seek-drag.png`、`secondary-subtitle.png`、`subtitle-after-seek.png`。当前暂停的是桌面操作验证，正式个人资料保持隔离，后续继续针对未完成正常操作核对，不重跑已通过项目或据后台结果宣布全部验收完成。

## 6 0.16.1 后台定位与调试取帧修复

2026-10-22 按用户要求在后台继续定位，未发送鼠标／键盘输入，未启动可见窗口，也没有读取或修改正式 Data。对应 FR-35／TC-71，本轮确认的是调试取帧缺陷，尚不能把前次缺字截图认定为真实屏幕不显示字幕。

### 6.1 原因及修复范围

原 capture_frame 在 SwapBuffers 之后直接读取 GL_FRONT。后台同一 OpenGL 会话中，绘制缓冲和 mpv 原生 window 截图均包含“我明天会到。”，GL_FRONT 读取却可得到空白图；该读取无法可靠代表当前合成帧。上一节缺少中文字幕的图来自这一旧诊断入口，现将该疑点改记为截图证据不足，不能据其认定字幕解码或渲染失败。

`Engine::capture` 改为调用同一活动渲染器的 `screenshot-to-file … window`，从 mpv 获取包含两路字幕及 OSD 的合成图。仅 debug 构建提供该入口，不修改选轨、播放、暂停学习或自动收录逻辑。生成的图属于渲染输出证据；实际窗口是否显示仍留待用户有空后的桌面操作核对。

### 6.2 一个原生正常流程的前后结果

新增 `src-tauri/examples/player_subtitle_acceptance.rs`，直接包含并调用产品实际使用的 `player/mpv.rs`，不复制另一套渲染实现。示例创建未设置 WS_VISIBLE 的 960×600 HWND，从不调用 ShowWindow 或焦点／输入 API，前后均断言窗口不可见；选择无音轨的既有合成视频，并在加载前关闭音轨、静音。资料与 GPU 缓存只位于新的命名输出目录，不打开业务资料库。

| 检查 | 实际结果 |
| --- | --- |
| 修复前正常流程 | 暂停在 20 秒，选中英文主轨和中文第二轨，旧 capture 不能取得含中文的图，正常流程的“字幕像素应存在”断言失败；输出保留在 `output/player-subtitle-opengl-0161/engine-before` |
| 修复后同一正常流程 | `cargo run --quiet --locked --manifest-path src-tauri/Cargo.toml --example player_subtitle_acceptance -- output/player-subtitle-opengl-0161/engine-after "output/player-ui-acceptance-0161/UI Series/Episode1.mkv"` 通过。产品 Engine／libmpv OpenGL 返回主字幕 `I will arrive tomorrow.`、第二字幕 `我明天会到。`，暂停及位置 20.000 秒保持；第二字幕区域有 1,647 个变化像素，实际 PNG 同时显示中英文 |
| 隔离及退出 | 示例全程无可见窗口、无声音和桌面输入，退出销毁引擎及原生 HWND；结果为 windowVisible=false、audioOutput=false。不启动调试客户端或安装版、不改变正式播放列表及个人词库 |

证据包括 `engine-after/result.json`、`primary.png`、`secondary.png`，后台初步定位的 Python 脚本及图保留在同一 `output/player-subtitle-opengl-0161` 命名目录，均不提交媒体或数据到公共仓库。示例要求 Windows debug 构建及明确的静音双语夹具，已有输出目录拒绝覆盖。

本轮没有观察到需要修改 mpv 双字幕渲染的后台故障。已修复调试取帧并取得可靠合成图；真实窗口呈现、退出全屏和跨应用拖放仍按用户要求等待后续操作，不声明全部桌面验收完成。

## 7 0.16.3 后台字幕、连播与文件队列检查

用户要求继续后台检查三条链路并修复问题，实际窗口留待有空时继续。本轮未操作鼠标／键盘，未启动应用可见窗口或正式安装版，未读取正式 Data。隔离结果目录编号沿用 `0162`，本轮最终应用版本为 0.16.3；同工作区查询翻译的 0.16.2 已由另一任务提交，本次保留其内容。

### 7.1 定位与修复

| 问题 | 原因与处理 |
| --- | --- |
| 刚选好的字幕及延迟在切集后回退 | 真实 Actor 正常流程复现：选择主字幕 1 并设延迟 0.25 秒，切到下一集后保存值却回到旧的字幕 3。即时保存只更新 SQLite，内存 progress 仍是旧快照；旧文件卸载后 END_FILE／START_FILE 用该缓存覆盖新记录。save_progress 现在同时更新最近缓存和持久化值 |
| 播放中加载字幕后程序误标已暂停 | 实际引擎 pause=false，但 invalidate 清理学习上下文后留下 paused 状态，refresh_paused 对继续播放没有处理。复现断言为实际播放中、程序状态 paused；修复后按引擎状态回到 playing，字幕操作不改变原播放意图 |
| 加载视频期间连续字幕请求被拒绝或覆盖 | 原目标判断只看界面 path；START_FILE 尚未处理时会把已有加载目标当作无视频，且 pending_subtitles 后批直接覆盖前批。现在从引擎路径或活动列表项绑定目标，同一目标下按接收顺序合并去重，加载完成后第一份主字幕、第二份第二字幕。不同视频仍丢弃旧绑定 |

没有改写 mpv 原生播放、选轨及 EOF 能力，没有新增按钮或实际窗口操作。

### 7.2 实际正常流程

新增 `player/native_tests.rs::normal_background_flow`，为显式运行的 ignored 检查。直接构造真实 Actor、Engine、FileInbox、Application 和独立 Store；Tauri 上下文去除全部窗口配置，不运行桌面 setup、插件或事件循环。只有未设置 WS_VISIBLE 的原生 OpenGL HWND，前后断言始终不可见；合成视频无音轨、引擎 aid=no 且静音，自动收录关闭。

同一个正常流程执行以下步骤并通过：

1. 依次将视频目录、英文 SRT、中文 SRT 作为三个 FileInbox 请求交给实际分类／消费流程，在加载完成前接收字幕。队列自然排序 Episode2、Episode10；主字幕 `We are ready.`，第二字幕 `我们准备好了。`。
2. 切到内置主字幕 1、延迟设为 0.25 秒后进入下一集，再返回。两份外置字幕、最近主轨及延迟保持；下一集不继承前集的第二字幕；切集刷新会话并清空旧上下文和倒计时。
3. 设 1.25×，定位到首集片尾后主动播放；实际 END_FILE／START_FILE／on_load／FILE_LOADED 路径自动进入第二集并继续播放，倍速不变，不建立学习暂停或自动收录计时。
4. 在第二集继续播放时提交外置中文字幕文件请求。没有切集或暂停，程序状态为 playing；主动暂停后可读到该中文字幕，外置路径及字幕轨写入第二集自己的记录，不把字幕加入视频列表。

运行命令（输出目录必须是新的命名目录）：

```powershell
$env:SVL_PLAYER_TEST_OUTPUT = 'D:/Projects/codex/SubtitleVocabularyList/output/player-background-0162-fixed-c'
$env:SVL_PLAYER_TEST_VIDEO = 'D:/Projects/codex/SubtitleVocabularyList/output/player-ui-acceptance-0161/UI Series/Episode1.mkv'
cargo test --quiet --locked --manifest-path src-tauri/Cargo.toml --lib --features ipc-types player::native_tests::normal_background_flow -- --ignored --exact --nocapture
```

最终 **1 项通过，115 项过滤**，约 6 秒；`result.json` 记录 nativeActor=true、windowVisible=false、audioOutput=false、两份逐文件进度及播放列表。第一集结束位置为 60 秒、字幕 1／4、延迟 0.25 秒；第二集独立保存新外置字幕，播放偏好为 1.25×／静音／自动连播。

原生检查进程初启因缺少应用二进制资源中的 Common Controls v6 清单而发生入口找不到；在 ipc-types 检查构建中补齐清单链接后，直接 Cargo 命令可启动，不再依赖临时手动嵌入清单。测试日志仍有测试 exe 无应用图标资源的提示，不影响结果；该补充不改正常发行构建。

已保存的修复前输出及所有命名隔离目录均保留，不提交媒体和 Data 到公共仓库。本轮证明实际播放器状态机的后台处理；没有实际投递 WM_DROPFILES 或 WebView 拖放事件，真实鼠标拖放、窗口画面及听感仍待用户方便时继续。

## 8 修订记录

| 版本 | 日期 | 内容 |
| --- | --- | --- |
| 0.4 | 2026-10-21 | 补充真实鼠标／按键播放、进度、全屏和菜单证据，记录用户暂停桌面操作后的后台连播／字幕检查及待复核边界 |
| 0.5 | 2026-10-22 | 定位并修复调试前台缓冲取帧误判，补充产品实际 Engine 的不可见窗口原生双字幕合成图及待人工边界 |
| 0.6 | 2026-10-22 | 修复最近字幕快照覆盖、播放状态及连续文件请求，记录真实 Actor 后台正常流程与窗口待确认边界 |
