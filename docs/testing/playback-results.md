# SubtitleVocabularyList 播放列表与偏好增量验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-PLAYBACK-001 |
| 文档版本 | 0.1 |
| 更新日期 | 2026-10-21 |
| 状态 | 0.14.0 最小后台正常路径通过；人工窗口／听感未执行，不阻塞开发 |
| 依据 | FR-32／33、M-28／29、TC-68／69 |

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
