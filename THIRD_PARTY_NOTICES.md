# SubtitleVocabularyList 第三方来源说明

项目代码采用 GPL-3.0-only，完整条款见 [LICENSE](LICENSE)。下列第三方组件保持其原许可。当前工具原型使用本机程序与项目内的私有资源，尚未发布可分发安装包；安装包形成时核对实际随附程序、许可文件和相应来源材料。

| 组件 | 当前用途与版本依据 | 上游来源与许可 |
| --- | --- | --- |
| Tauri | Windows 桌面宿主，Rust 2.12.1，CLI/API 版本见锁文件 | [tauri-apps/tauri](https://github.com/tauri-apps/tauri)，MIT 或 Apache-2.0 |
| Tauri Dialog 与 Single Instance 插件 | 本机文件、目录选择和单实例激活，版本见锁文件 | [tauri-apps/plugins-workspace](https://github.com/tauri-apps/plugins-workspace)，MIT 或 Apache-2.0 |
| Tauri Global Shortcut 插件 | 全局快捷键注册，Rust 2.4.0；底层 global-hotkey 0.8.0 | [tauri-apps/plugins-workspace](https://github.com/tauri-apps/plugins-workspace)、[tauri-apps/global-hotkey](https://github.com/tauri-apps/global-hotkey)，MIT 或 Apache-2.0 |
| windows | Windows UI Automation、SAPI、GDI 截图与进程窗口 API 绑定，0.62.2 | [microsoft/windows-rs](https://github.com/microsoft/windows-rs)，MIT 或 Apache-2.0 |
| reqwest | 本机模型 HTTP 适配，0.12.28，采用 rustls | [seanmonstar/reqwest](https://github.com/seanmonstar/reqwest)，MIT 或 Apache-2.0 |
| scraper | 用户手动请求的 Wiktionary 英语词典 HTML 解析，0.27.0；关闭 CLI 默认功能 | [rust-scraper/scraper](https://github.com/rust-scraper/scraper)，ISC；传递依赖按 Cargo 锁文件及各自许可保留 |
| walkdir | 导入目录递归扫描，2.5.0 | [BurntSushi/walkdir](https://github.com/BurntSushi/walkdir)，Unlicense 或 MIT |
| Prettier | 开发源码格式化，3.9.9，不随应用作为运行工具调用 | [prettier/prettier](https://github.com/prettier/prettier)，MIT |
| React 与 React DOM | 本地界面，19.3.0 | [facebook/react](https://github.com/facebook/react)，MIT |
| Vite 与 TypeScript | 开发构建与类型检查，版本见 pnpm 锁文件 | [Vite](https://github.com/vitejs/vite)、[TypeScript](https://github.com/microsoft/TypeScript)，分别为 MIT 与 Apache-2.0 |
| rusqlite 与 SQLite | 本地持久化，rusqlite 0.37.0；SQLite 随其 bundled 功能编译 | [rusqlite](https://github.com/rusqlite/rusqlite)，MIT；[SQLite](https://www.sqlite.org/copyright.html)，public domain |
| libbitsub-core | PGS 解码与图像呈现，1.12.1 | [altqx/libbitsub](https://github.com/altqx/libbitsub)，MIT；参考审查提交 a280c0c2dc4dad0d7ae5aff138a967a07b25d83f |
| image | PGS 图像输出、屏幕快照与裁剪处理，版本见 Cargo 锁文件 | [image-rs/image](https://github.com/image-rs/image)，MIT 或 Apache-2.0 |
| whisper.cpp | 离线英语转写，当前原型使用 1.8.3 的 Windows x64 CPU 发行文件 | [ggml-org/whisper.cpp](https://github.com/ggml-org/whisper.cpp)，MIT |
| 英语 Whisper 模型 | ggml-base.en，资源下载与校验见 setup-tools.ps1 | [转换模型来源](https://huggingface.co/ggerganov/whisper.cpp)、[原始 Whisper](https://github.com/openai/whisper)，按上游模型许可保留说明 |
| Tesseract | 首台机器使用 5.4.0；新增桌面开发环境使用上游 5.5.3 的 Windows 英语 OCR | [tesseract-ocr/tesseract](https://github.com/tesseract-ocr/tesseract)，Apache-2.0；实际二进制的依赖分别核对 |
| FFmpeg 与 ffprobe | 首台机器使用 2022-12-15 构建；新增桌面开发环境使用 Gyan 9.0.2 essentials Windows 构建，启用 GPL | [FFmpeg 许可说明](https://ffmpeg.org/legal.html)、[Gyan 构建](https://www.gyan.dev/ffmpeg/builds/)，以实际配置和随附材料为准 |
| Windows 系统语音与 WebView2 | 使用系统能力及运行时 | Microsoft 系统组件按其许可使用，不纳入本项目代码许可 |

Rust 与前端的确切依赖版本分别保存在 `src-tauri/Cargo.lock` 与 `pnpm-lock.yaml`。后续新增依赖更新本说明；最终分发核对包含实际用到的传递依赖。当前未复制 Pot 项目源码。

用户剧集、原声片段、截图、个人词库与模型大文件不提交到公开源码仓库。它们的存在不改变源代码许可证，也不意味着本项目授予第三方素材的再分发权。
