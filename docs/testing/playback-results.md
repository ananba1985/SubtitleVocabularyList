# SubtitleVocabularyList 播放列表与偏好增量验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-PLAYBACK-001 |
| 文档版本 | 0.3 |
| 更新日期 | 2026-10-21 |
| 状态 | 0.16.0 文件入口正常路径及 Computer Use 窗口核对通过；跨应用拖放受工具限制未实测，未生成安装包 |
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
