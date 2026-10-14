# 未命中候选复核与匹配优化记录

| 项目 | 内容 |
| --- | --- |
| 文档编号 | DOC-TEST-DICTIONARY-REVIEW-001 |
| 文档版本 | 0.1 |
| 更新日期 | 2026-10-14 |
| 状态 | 487 项初分、可靠匹配优化及最小正常流程通过；未打包 |
| 对应版本 | 0.7.2；业务 schema 8，底库 schema 1 |
| 需求／设计依据 | FR-02、FR-08、FR-14；[基础词库设计](../design/base-dictionary.md)、TC-51；用户要求复核剩余 487 项并优化匹配 |

## 1 范围和口径

沿用[双词库统计基线](base-dictionary-results.md)：24 集，分集候选合计 21,273 项，跨集按 `(kind, candidate_key)` 去重为 6,140 项，原先 Google／ECDICT 覆盖 5,653 项、未命中 487 项。

本次对 487 项读取各自最多两条已保存对白，按字面形态优先作互斥初分。专名和 OCR 分类是基于这些对白的疑似判断，未逐帧核对视频或重跑识别，不作为自动删除、改写或掌握判断。原业务库只读；底库转换只更新独立词典文件。

完整候选、初分、原句和匹配结果留在本机 `.local/dictionary-coverage/unmatched-audit/`，不公开个人语料；报告仅列汇总和孤立词形示例。`contexts.json` 为取样对白，`review.csv`／`review.json` 为逐项复核，`review-summary.json` 为汇总；同目录 notebook 提供取样、规则和统计的可检查代码，未单独执行 notebook。

## 2 487 项原因初分

| 互斥主分类 | 数量 | 示例与说明 |
| --- | ---: | --- |
| `’s` 结构 | 177 | `baby’s`、`dad’s`、`cameron’s`；所属、is／has 缩写等需语境，不能统一删尾 |
| 其他带撇号形式 | 71 | `can’t`、`couldn’t`、`how’d`；包含标准缩写、字形变体和识别疑点 |
| 非英语对白词 | 52 | `afuera`、`esposa`、`queso`、`mangia`；英语底库未收录并不表示是英语生词 |
| 疑似专名及姓名派生形式 | 47 | `Benihana`、`DiCaprio`、`Fizbo`、`Dunphys`、`SkyMall` |
| 疑似 OCR 或分词问题 | 57 | `mitk`、`sald`、`wedaing`、`iittle`、`lalk`；需原字幕或画面进一步确认 |
| 普通词形或口语省音 | 33 | `feelin`、`fishin`、`lowballed`、`nerdier`、`repurposed` |
| 字幕元数据 | 1 | `psdh`，来自字幕标识 |
| 其他俚语、拟声、合成词或缺词项 | 49 | `bajillion`、`coinkydink`、`helpaholic`、`microsuede`；也包含缺撇号的书写形式 |
| 合计 | **487** | 名字后接 `’s` 的候选计入第一类，不重复计为专名 |

带撇号两组合计 **248 项，占 50.92%**；这不是 248 个确定的缩写。分类时间为 2026-10-14 14:37:21 UTC，规则优先级及逐项结果可从本机证据检查。

## 3 已实施的可靠匹配改进

1. 只在查词层将弯撇号等统一为直撇号，不改变原文、业务键或同步标识。
2. 共用 JSON 记录 30 条明确映射：10 条缺撇号、13 条口语省音、6 条明确词形、1 条口语缩略。仅在普通查词缺失时尝试，并要求目标有有效中文。
3. 补充 12 个基本缩写的自拟中文解释，标注为补充资料；保留 `it’d`、`they’d` 中 had／would 的歧义。其中 `they'd` 用基本缩写解释校订词典中的无关错义，未改原始下载文件或已有缓存。
4. Rust 查询与 Python 覆盖统计共用 `src-tauri/resources/dictionary-supplements.json`，后台排队、手动解释及 Qwen 词语查询继续复用同一底库入口。

缩写和所有格规则参考 [Cambridge contractions](https://dictionary.cambridge.org/grammar/british-grammar/contractions) 与 [British Council possessive ’s](https://learnenglish.britishcouncil.org/free-resources/grammar/a1-a2/possessive-s)。补充中文为本项目编写，不是复制来源文章；不把通用解释冒充对白中的唯一含义。

没有加入模糊拼写纠正、普遍删除 `’s` 或随意还原后缀。已排除目标词义不适合的 `chillin → chilling`（底库为“冷却”）、`kiddin → kid`（“孩子”）等映射。专名、外语和疑似 OCR 项保持原样，不能为减少剩余数量而虚报有效匹配。

## 4 优化后的实际覆盖

在 2026-10-14 14:34:21 UTC 执行一次 `python -X utf8 scripts/prepare-base-dictionary.py --analyze`。使用与基线相同固定提交及 SHA-256 的 Google／ECDICT 原始资料，候选仍为 6,140 项；当前输出保存在 `.local/dictionary-coverage/optimized/`，历史基线保留。

| 新增匹配方法（互斥） | 数量 |
| --- | ---: |
| 撇号字形统一后查到原词典 | 45 |
| 基本缩写补充／校订 | 18 |
| 口语省音 | 13 |
| 缺撇号 | 10 |
| 明确词形 | 6 |
| 口语缩略 | 2 |
| 新增覆盖合计 | **94** |

| 优化后来源分类（互斥） | 数量 |
| --- | ---: |
| Google 直接 | 3,301 |
| Google 词形 | 397 |
| ECDICT 直接 | 2,029 |
| ECDICT 词形 | 0 |
| 基本缩写补充 | 20 |
| 仍未命中 | 393 |
| 合计 | **6,140** |

覆盖 **5,747／6,140＝93.60%**，从 92.07% 增加约 **1.53 个百分点**，未命中 **487 → 393**。方法分类与来源分类是不同维度，例如缺撇号形式也可能命中基本缩写补充，因此“方法 18”与“来源 20”不矛盾。

393 项中，2 项已掌握，385 项已有通用词义缓存，目前仅 **6 项**仍缺词义：`getitony`、`how’d`、`lalk`、`tolot`、`what'd`、`yamos`。其中有疑似识别错误及有歧义的缩写，保留后续人工核对。本次不自动删除或改写这些内容。**6 是该快照中缺少的词义项目数，不包括独立的整句译文，也不是 HTTP 请求次数。**

## 5 最小正常验证及边界

命令：`cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test base_dictionary_normal -- --nocapture`，结果 **1 passed，0 failed**。

同一正常路径验证 Google／ECDICT 优先级、弯撇号、明确省音与缺撇号、基本缩写及 `they'd` 校订；手动解释和查询小窗直接返回底库，后台仅为确实缺失的合成词请求一次合成模型。原语境解释与已保存句子译文保持，未新增个人词条，重开可读。未启动真实全量模型任务，未重做 GUI 或打包验证。

已有解释缓存继续优先；此次匹配改进不覆盖用户已保存的解释。底库有效中文字段覆盖不等于全部释义已逐项校对。0.7.2 源码在下次调试运行时使用新逻辑，已安装旧版程序不会自动获得代码更新。

## 6 修订记录

| 版本 | 日期 | 变更 |
| --- | --- | --- |
| 0.1 | 2026-10-14 | 记录 487 项初分、94 项可靠匹配增量、剩余项目及正常验证 |
