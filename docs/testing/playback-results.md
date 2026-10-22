# SubtitleVocabularyList 播放列表与偏好增量验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-PLAYBACK-001 |
| 文档版本 | 0.2 |
| 更新日期 | 2026-10-21 |
| 状态 | 0.14.0／0.15.0 最小后台正常路径通过；人工窗口／听感未执行，不阻塞开发 |
| 依据 | FR-32 至 FR-36、M-28 至 M-32、TC-68 至 TC-72 |

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
