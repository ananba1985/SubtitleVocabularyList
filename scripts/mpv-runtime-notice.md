# libmpv 播放资源

- Windows x86_64 构建：shinchiro/mpv-winbuild-cmake，20261021，mpv git e471496c56。
- 固定资源：https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261021/mpv-dev-x86_64-20261021-git-e471496c56.7z
- 资源包 SHA256：efc719eff3dc5954b4a5efc800632808bcec01cc9034940dd14e5eb7fe9ea633。
- 构建说明与对应源码入口：https://github.com/shinchiro/mpv-winbuild-cmake/releases/tag/20261021
- 播放引擎许可：https://github.com/mpv-player/mpv/blob/e471496c56/Copyright
- C API 头文件许可不代表整个 DLL 许可；实际分发时需将该构建的依赖许可及对应源码材料纳入项目既有交付流程。

运行期只加载安装目录 `tools/mpv/libmpv-2.dll`；调试读取项目 `.tools/runtime/mpv`。配置和缓存由本应用控制，放在所选资料目录 `Data/cache/mpv`，不读取外部 mpv 用户配置或脚本。此文件记录开发资源来源，不表示已生成安装包或完成安装验收。
