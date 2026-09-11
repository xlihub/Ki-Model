# OpenAIProvider 可选网关配置

`aion-providers` 通过 `openai::OpenAIProvider` 提供模型调用。`new(api_key, base_url, compat)` 使用默认 Bearer 鉴权和 HTTP client；`with_options` 接受请求头、鉴权方式及调用方 HTTP client。消息、工具、图片、Chat Completions 与 Responses 由 SDK 统一处理。

## 公开构造接口

```rust
use std::time::Duration;
use aion_config::compat::ProviderCompat;
use aion_providers::openai::{OpenAIAuth, OpenAIOptions, OpenAIProvider};
use reqwest::Client;

let mut compat = ProviderCompat::openai_defaults();
compat.transport.api_path = Some(String::new()); // Full endpoint URL
compat.transport.include_stream_options = Some(false);
let client = Client::builder()
    .no_proxy()
    .connect_timeout(Duration::from_secs(10))
    .read_timeout(Duration::from_secs(60))
    .build()?;
let provider = OpenAIProvider::with_options(
    None,
    "http://127.0.0.1:8080/gateway/chat?tenant=synthetic",
    compat,
    OpenAIOptions {
        auth: OpenAIAuth::None,
        headers: vec![("X-Synthetic-Key".into(), "synthetic-secret".into())],
        client: Some(client),
    },
)?;
```

示例全部是合成配置；其中超时数值仅展示 API，不是客户环境的推荐值。`with_options` 返回 `Result<OpenAIProvider, OpenAIConfigError>`，调用方可匹配结构化构造错误。配置类型从 `aion_providers::openai` 导出；transport、parser、projector 和脱敏模块属于内部实现。

## 鉴权、默认值和错误

| 配置 | 行为 |
| --- | --- |
| `OpenAIOptions::default()` | Bearer；无附加头；使用 `reqwest::Client::new()` 的网络策略 |
| `auth: Bearer` | Bearer 唯一来源是 `with_options` 的 `api_key: Some(...)`；附加头同时发送 |
| `auth: None` | SDK 不生成 Bearer；不要求 API Key，也不使用传入的 API Key；可仅使用自定义头或完全不鉴权 |
| `headers` | 有序名称/值列表，保留重复输入用于校验；名称大小写不敏感 |
| `client: Some(client)` | 原样使用调用方构造的 client，保留代理、TLS、连接/读取/总超时和连接池策略 |

构造时拒绝重复名称、非法名称/值、Bearer 与自定义 Authorization 并存。Bearer 的 key 缺失或仅含空白时归为缺失凭据，包含非法头字符归为非法凭据。仅头模式允许显式 Authorization。`Content-Type`、`Content-Length`、`Transfer-Encoding`、`Host` 由传输层控制，不能覆盖。`OpenAIConfigError` 的错误类型区分这些情况，索引为从零开始的输入位置，错误不回显名称或值。

所有显式头都标记为 sensitive；options 的 Debug 只显示鉴权模式、头数量和是否注入 client，不输出 client Debug。已配置 key/头值出现在 HTTP 或 SSE 错误中时，在返回错误及重试日志之前替换为 `[REDACTED]`。HTTP 错误不附带请求 URL，防止查询参数进入异常。脱敏对象是已知凭据的原值；不尝试识别服务端对凭据任意编码后的形式。正常文本、工具内容与 provider metadata 不作字符串替换。

调用方应把鉴权放在上述显式字段中，client 只负责网络策略。`reqwest::Client` 不公开其 `default_headers`，SDK 无法枚举其中的隐藏鉴权头、校验它们与显式头的重复关系或为它们收集脱敏值；不能通过 client 的默认头、cookie 或中间件配置另一个凭据来源。client 中的非鉴权默认头遵循 reqwest 的请求头合并语义。

## 地址和协议选项

`ProviderCompat` 控制请求语义：

- `api_path: Some("")` 表示 `base_url` 已经是完整调用地址，不添加 `/chat/completions`，保留查询参数和末尾斜线。
- 非空 `api_path` 追加在基础地址路径之后、查询参数之前。例如 `/v1?tenant=synthetic` 与 `/chat/completions` 组合为 `/v1/chat/completions?tenant=synthetic`。
- Chat Completions 默认路径为 `/chat/completions`；Responses 默认路径为 `/responses`，可通过 `api_path` 指定自定义路径。
- `include_stream_options` 默认为 true；false 时实际请求不含 `stream_options`。
- `ProviderCompat` 显式决定协议选项。OpenAI 官方根地址标准化为 `/v1`；其他网关的差异通过配置表达。构造配置不提供任意请求体覆盖、动态脚本或响应转换 DSL。

`new` 直接返回 `OpenAIProvider`，凭据校验在请求阶段进行。`with_options` 在构造阶段校验配置并返回 `Result`。对合法 key，默认 options 与 `new` 发送相同的 URL、头和请求体。

## 流、超时和取消

共享 SSE 分帧遵循 [WHATWG event stream 规则](https://html.spec.whatwg.org/multipage/server-sent-events.html#event-stream-interpretation)：冒号后移除至多一个空格，接受 LF/CRLF/CR，多行 data 用换行连接，以空行触发事件，忽略注释和未知字段，处理起始 BOM 与 UTF-8 跨网络分段；非法 UTF-8 按 [Encoding 标准的 replacement 模式](https://encoding.spec.whatwg.org/#utf-8-decode)替换后继续解析，仅保留不完整尾部字节。该分帧规则适用于 Chat Completions、Responses 和 Anthropic，与网关选项无关。单个 data 行不是独立事件边界；缺少结束空行的输入属于未完成事件。

Chat Completions 支持工具分段参数、opaque metadata、reasoning、usage-only 末尾事件与结束状态。`finish_reason` 的 Done 延迟到 `[DONE]`，让最后的 usage 有机会更新。无 `[DONE]` 时，完整 `finish_reason` 加干净 EOF 仍可完成；无结束状态、仅 `[DONE]`、非法 JSON、事件截断或显式错误都不会报告空成功。

超时直接使用 [reqwest ClientBuilder](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html) 的含义：`connect_timeout` 覆盖连接建立（含 TLS），`read_timeout` 限制每次读取等待并在成功读取后重置，`timeout` 从单次请求开始覆盖到响应体结束。SDK 默认不设置这些时限，代理默认使用 reqwest 行为。总请求 timeout 不是跨重试的总预算；调用方需要整体预算时应约束完整 stream 调用及接收过程。

SDK 的通用重试策略为：初始连接失败最多重试 2 次；HTTP 5xx 最多重试 5 次；没有输出内容的可重试流失败最多重试 2 次。各层可组合但次数有限。429 初始响应直接返回错误；解析错误不重试。已输出文本、reasoning 或工具事件后不自动重放。具体等待序列由通用 `retry` 模块定义。

`stream()` 返回前，取消其 future 会释放尚未完成的请求。返回后，丢弃 `mpsc::Receiver<LlmEvent>` 会取消网络读取、重试等待和重新发送，后台任务结束。provider/client 的其他 clone 仍有效；连接池由 reqwest 的共享引用管理，调用方不需要每次请求重建 client。取消不会伪造 Done 事件。

## 调用方职责

Ki-Core 负责凭据持久化、可序列化连接配置和 provider 装配；SDK 负责协议请求与响应处理。产品发布遵循[Ki-Model 维护指南](maintenance.md)，下游采用经过发布验证的固定产品 tag/commit。
