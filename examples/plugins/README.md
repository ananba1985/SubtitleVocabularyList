# 声明式查询插件

0.5.0 首版插件使用单个 JSON 配置文件，支持翻译和词典 HTTP 接口。查询小窗 → 选项 → 安装插件，选择 JSON；安装后默认关闭，展开插件设置、填好必填项并启用后查询。

[local-qwen.json](local-qwen.json) 是本项目编写的 CC0-1.0 示例，连接默认回环地址。模型标识填写本机服务的实际值；它独立于内置 Qwen 服务，结果分别显示。访问同一回环模型接口时仍使用项目的模型请求许可。

manifest 的 protocolVersion 为 1；填写版本、最低主程序版本、许可、translation／dictionary 能力、allowedHosts、固定 endpoint 和 GET／POST 方法。外部地址使用 HTTPS；HTTP 只允许回环地址。请求只读取 headers、params、body，响应按 JSON Pointer 提取 meaning／translation／notes，至少配置一种有效结果。

模板变量有 `{{text}}`、`{{context}}`、`{{sourceLanguage}}`、`{{targetLanguage}}`，及在 fields 中声明的普通字段；secret 字段使用 `{{secret:字段id}}`。未允许发送语境时 context 为空。字段设为 secret 时由 Windows DPAPI 保护保存；不要把真实凭据写进插件文件。

禁用或卸载停止调用，原词条和缓存保留。升级同一 id 的插件后需要重新确认启用，必填项按新配置检查。首版不执行脚本／DLL，也不直接兼容 Pot 的 .potext 包；OCR、语音插件和插件市场不属于本版。
