# 工单入口

Ki-Model 的功能、维护和发布工单位于 GitHub `xlihub/Ki-Model`。通过 `gh issue view NUMBER --repo xlihub/Ki-Model --json title,body,comments,state,blockedBy,blocking,parent` 读取目标工单的最新需求与依赖。

跨仓需求以目标工单实际关联的父规格、依赖和 PR 为准，按链接核对仓库身份与最新内容。Ki-Model 负责自身 SDK 与发布维护；涉及 Ki-Core 消费或 Ki-Buddy 集成时，到对应仓库读取其需求和验收条件。

在实施 PR 中引用具体工单并记录检查结果。长期规范描述通用职责与维护规则，具体工单的阶段、版本选择和执行状态保留在工单、PR 与发布记录中。工单关闭或清单勾选不能代替分支、保护规则、CI 或 Release 的实际证据。
