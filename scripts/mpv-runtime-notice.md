# libmpv 播放资源

固定版本、归档与 DLL 摘要由同目录 `mpv-runtime.json` 维护。开发及安装包准备均核对归档和实际 DLL；安装包还检查 Windows x64 架构及实际加载依赖，不初始化播放或窗口。

- Windows x86_64 构建：shinchiro/mpv-winbuild-cmake，20261021，mpv git e471496c56。
- 固定资源：https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261021/mpv-dev-x86_64-20261021-git-e471496c56.7z
- 资源包 SHA256：efc719eff3dc5954b4a5efc800632808bcec01cc9034940dd14e5eb7fe9ea633。
- 构建说明与对应源码入口：https://github.com/shinchiro/mpv-winbuild-cmake/releases/tag/20261021
- 播放引擎许可：https://github.com/mpv-player/mpv/blob/e471496c56/Copyright
- C API 头文件许可不代表整个 DLL 许可；实际分发时需将该构建的依赖许可及对应源码材料纳入项目既有交付流程。

0.16.3 本机安装试用包包含上述 DLL、mpv Copyright／GPL／LGPL 原文及固定清单；随包源码材料包含 mpv e471496c56 和提供方构建配方 1f8d2e76255abc48f0456d9be99b4cab9c6566e6（含补丁、配置）。提供方配方还跟踪多个依赖的移动分支，目前未收齐所有静态依赖的确切对应源码和条款，**这些材料不是完整对应源码**，对外分发前仍需补齐；不以一个 mpv 源码归档代替完整依赖材料。

运行期只加载安装目录 `tools/mpv/libmpv-2.dll`；调试读取项目 `.tools/runtime/mpv`。配置和缓存由本应用控制，放在所选资料目录 `Data/cache/mpv`，不读取外部 mpv 用户配置或脚本。是否已生成安装包及安装验收结果以该版本实际交付记录为准。
