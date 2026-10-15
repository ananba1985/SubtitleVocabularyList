# 基础词库安装资源流程验证

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-DICTIONARY-PACKAGE-001 |
| 文档版本 | 0.1 |
| 更新日期 | 2026-10-14 |
| 状态 | 独立资源准备、首次复制及更新正常路径通过；未生成安装包 |
| 对应版本 | 0.7.3；业务 schema 8 与底库 schema 1 不变 |
| 需求与设计依据 | FR-08、NFR-01、NFR-02、NFR-06、TC-52；[底库设计](../design/base-dictionary.md)、[安装说明](../release/installation.md)；用户要求修改流程而不立即打包 |

## 1 实施范围

根目录 `生成安装包.ps1` 在编译前独立准备 Google 高频词库和 ECDICT。`prepare-base-dictionary.py --release-dir` 从固定提交、固定 SHA-256 原始资料构建两个 SQLite，不打开日常的 `vocabulary.sqlite3`，也不从个人 Data 导出安装资料。来源缓存位于 `.tools/release-cache/dictionaries`，发行暂存位于 `.tools/release-dictionaries`。

本机准备复用了已经下载的六份固定原始资料，逐份核对摘要后复制到独立来源缓存；没有复制个人业务库。转换输出与原词库相同：Google 9,306 个有效中文词头、7,287 个明确词形；ECDICT 767,533 个有效中文词头、32,843 个明确词形。

Tauri 显式映射六份文件到所选安装目录的 `Data/dictionaries`：

| 文件 | 用途 |
| --- | --- |
| base.sqlite3 | Google 高频词库 |
| ecdict.sqlite3 | ECDICT 后备词库 |
| manifest.json | 应用版本、来源提交／摘要、输出摘要与转换统计 |
| notices/Google-README.md | 上游来源说明原文 |
| notices/ECDICT-README.md | 上游资料说明原文 |
| notices/ECDICT-LICENSE.txt | 固定提交的原 MIT LICENSE 与版权声明 |

安装时沿用 Tauri NSIS 的文件资源机制；当前锁定工具生成的模板在复制前检查运行程序，按资源文件处理覆盖及删除，目录只在空时移除。本次没有新增清空 Data 的 hook，`Data/vocabulary.sqlite3` 不出现在资源映射中。standalone 使用 `copy-release-dictionaries.ps1` 读取同一显式映射，复制六份文件，不整体替换 Data。

## 2 实际最小验证

| 动作 | 实际结果 |
| --- | --- |
| `python -X utf8 scripts/prepare-base-dictionary.py --release-dir .tools/release-dictionaries` | 正常退出，两个底库与三份来源说明及 manifest 已生成；输入摘要固定，未打开个人业务库 |
| `python -X utf8 scripts/test-release-dictionaries.py` | 一条正常流程通过：核对六份资源映射和清单一致；临时目录首次复制生成全部文件；写入合成个人词条和学习记录，将临时底库标为上一修订，再执行同一复制入口更新；六份文件摘要恢复为发行暂存值，个人数据库 SHA-256 和查询记录均保持。临时目录已自动清理 |
| PowerShell Parser | `生成安装包.ps1` 与 `scripts/copy-release-dictionaries.ps1` 无语法错误 |
| `生成安装包.ps1 -CheckOnly` | 正常列出底库准备、安装映射和既有构建计划，没有执行 Cargo 清理、编译或安装 |
| `pnpm version:check` | 0.7.3 各版本来源一致 |

资源步骤未读写本机日常业务库；没有启动模型、前端、GUI 或完整回归。资源准备结果仍保留在项目忽略的发行暂存中，供后续手动打包使用；原始数据不进入公开 Git 提交。

## 3 交付与未执行范围

下次用户手动执行根目录 `生成安装包.ps1` 时，将重新准备并校验底库，然后执行既有工具及 NSIS 构建；`-SkipToolBuild` 不跳过词库。构建前后的底库摘要会核对，standalone 同样含 `Data/dictionaries`。个人库、学习记录和原声不属于随包底库资源，应用仍通过原有迁移打开个人数据库。

**本轮没有生成安装包、独立 exe 或源码 ZIP，没有实际执行 NSIS 首次安装、覆盖升级或卸载。** 本地资源复制证明相同映射及 standalone 步骤，不冒充安装器实装验收；真正安装行为留到用户要求生成版本包时验证。已安装旧版不会因源码更新而自动获得新资源。

Google 中文词表的来源声明与 ECDICT 原条款随包保留，代码仓库只提交转换和打包流程。本轮按用户要求准备本机安装使用，不据此声明 Google 上游已授予通用公开分发许可，详见第三方来源说明。

## 4 修订记录

| 版本 | 日期 | 变更 |
| --- | --- | --- |
| 0.1 | 2026-10-14 | 记录双底库独立发行准备、首次复制及更新时保留个人数据的正常验证；保持不打包边界 |
