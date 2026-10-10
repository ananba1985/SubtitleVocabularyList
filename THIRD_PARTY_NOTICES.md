# SubtitleVocabularyList 第三方来源说明

项目代码采用 GPL-3.0-only，完整条款见 [LICENSE](LICENSE)。下列第三方组件保持其原许可。开发环境与发行资源分别管理；当前安装候选包已经生成并完成隔离安装，完整 0.1 仍在验收。最终交付同时提供源码材料包、组件说明、许可证、固定版本及摘要。

| 组件 | 当前用途与版本依据 | 上游来源与许可 |
| --- | --- | --- |
| Tauri | Windows 桌面宿主，Rust 2.12.1，CLI/API 版本见锁文件 | [tauri-apps/tauri](https://github.com/tauri-apps/tauri)，MIT 或 Apache-2.0 |
| Tauri Dialog 与 Single Instance 插件 | 本机文件、目录选择和单实例激活，版本见锁文件 | [tauri-apps/plugins-workspace](https://github.com/tauri-apps/plugins-workspace)，MIT 或 Apache-2.0 |
| Tauri Global Shortcut 插件 | 全局快捷键注册，Rust 2.4.0；底层 global-hotkey 0.8.0 | [tauri-apps/plugins-workspace](https://github.com/tauri-apps/plugins-workspace)、[tauri-apps/global-hotkey](https://github.com/tauri-apps/global-hotkey)，MIT 或 Apache-2.0 |
| windows | Windows UI Automation、SAPI、GDI 截图与进程窗口 API 绑定，0.62.2 | [microsoft/windows-rs](https://github.com/microsoft/windows-rs)，MIT 或 Apache-2.0 |
| reqwest | 本机模型 HTTP 适配，0.12.28，采用 rustls | [seanmonstar/reqwest](https://github.com/seanmonstar/reqwest)，MIT 或 Apache-2.0 |
| scraper | 用户手动请求的 Wiktionary 英语词典 HTML 解析，0.27.0；关闭 CLI 默认功能 | [rust-scraper/scraper](https://github.com/rust-scraper/scraper)，ISC；传递依赖按 Cargo 锁文件及各自许可保留 |
| walkdir | 导入目录递归扫描，2.5.0 | [BurntSushi/walkdir](https://github.com/BurntSushi/walkdir)，Unlicense 或 MIT |
| dunce | 简化 Windows 扩展路径，1.0.5，避免原生工具误读路径 | [kornelski/dunce](https://github.com/kornelski/dunce)，CC0-1.0 或 MIT-0 或 Apache-2.0 |
| Prettier | 开发源码格式化，3.9.9，不随应用作为运行工具调用 | [prettier/prettier](https://github.com/prettier/prettier)，MIT |
| React 与 React DOM | 本地界面，19.3.0 | [facebook/react](https://github.com/facebook/react)，MIT |
| JSZip | 本机 EPUB 与 `.lesson.zip` 读取，3.10.1 | [Stuk/jszip](https://github.com/Stuk/jszip)，本项目选择 MIT；原版权与条款见 npm 包 `LICENSE.markdown`，传递依赖随 pnpm 锁文件和现有发行许可脚本收集 |
| Vite 与 TypeScript | 开发构建与类型检查，版本见 pnpm 锁文件 | [Vite](https://github.com/vitejs/vite)、[TypeScript](https://github.com/microsoft/TypeScript)，分别为 MIT 与 Apache-2.0 |
| rusqlite 与 SQLite | 本地持久化，rusqlite 0.37.0；SQLite 随其 bundled 功能编译 | [rusqlite](https://github.com/rusqlite/rusqlite)，MIT；[SQLite](https://www.sqlite.org/copyright.html)，public domain |
| libbitsub-core | PGS 解码与图像呈现，1.12.1 | [altqx/libbitsub](https://github.com/altqx/libbitsub)，MIT；参考审查提交 a280c0c2dc4dad0d7ae5aff138a967a07b25d83f |
| image | PGS 图像输出、屏幕快照与裁剪处理，版本见 Cargo 锁文件 | [image-rs/image](https://github.com/image-rs/image)，MIT 或 Apache-2.0 |
| whisper.cpp | 发行资源从 1.8.3 固定源码编译 Windows x64 CPU CLI，静态 CRT | [ggml-org/whisper.cpp](https://github.com/ggml-org/whisper.cpp)，MIT |
| 英语 Whisper 模型 | ggml-base.en，下载与 SHA256 见 build-offline-whisper.ps1，模型附 MIT 条款 | [转换模型来源](https://huggingface.co/ggerganov/whisper.cpp)、[原始 Whisper](https://github.com/openai/whisper)，MIT |
| Tesseract 与英语数据 | 发行资源从 5.5.3 源码构建；tessdata_fast 4.1.0 的 eng.traineddata | [Tesseract](https://github.com/tesseract-ocr/tesseract)、[tessdata_fast](https://github.com/tesseract-ocr/tessdata_fast)，Apache-2.0 |
| OCR 静态依赖 | Leptonica 1.85.0、libpng 1.6.43、libtiff 4.6.0、libjpeg-turbo 3.0.1、zlib 1.3.1 | 固定源码与摘要见 build-offline-ocr.ps1；分别保留 BSD 类、libpng、TIFF、IJG/BSD/zlib 及 zlib 条款，不替换成项目许可 |
| FFmpeg 与 ffprobe | 发行资源从 n9.0.2 固定源码构建，未开启 GPL、nonfree、网络或外部编解码库，使用静态 zlib；历史开发环境的 Gyan 构建不进入安装包 | [FFmpeg](https://github.com/FFmpeg/FFmpeg)、[许可说明](https://ffmpeg.org/legal.html)，本次构建 LGPL-2.1-or-later；原始源码、适配脚本与配置随源码资料交付 |
| selectors 等传递依赖 | selectors 0.38.0 用于 HTML 解析；完整版本见生成的依赖清单 | [servo/stylo](https://github.com/servo/stylo)，MPL-2.0；保留源码许可声明，并附[正式条款](https://www.mozilla.org/media/MPL/2.0/index.txt) |
| Windows 系统语音与 WebView2 | 使用系统能力及运行时 | Microsoft 系统组件按其许可使用，不纳入本项目代码许可 |

Rust 与前端的确切依赖版本分别保存在 `src-tauri/Cargo.lock` 与 `pnpm-lock.yaml`。后续新增依赖更新本说明；最终分发核对包含实际用到的传递依赖。当前未复制 Pot 项目源码。

`scripts/prepare-release-licenses.ps1` 根据 Windows Rust 依赖图和前端运行依赖收集条款，写入 `tools/licenses/dependencies.json`。未随 crate 发布的许可从该 crate 记录的固定上游提交补取；缺少条款会中止打包。`scripts/export-release-source.ps1` 从干净的已提交源码导出应用、八份媒体/OCR/ASR 源码、校验过的 Cargo 原始包及前端运行依赖。模型文件的固定地址、摘要和许可证在构建脚本及随包资源中保留。微软运行时与系统组件继续按微软许可使用。

用户剧集、原声片段、截图、个人词库与模型大文件不提交到公开源码仓库。它们的存在不改变源代码许可证，也不意味着本项目授予第三方素材的再分发权。
