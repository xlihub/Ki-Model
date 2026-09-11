---
name: release-ki-model
description: 按既有 Ki 产品发布规范处理 xlihub/Ki-Model 的已选定上游同步、SDK 版本准备与发布恢复。只读候选分析使用 ki-release-maintenance；仅操作 Ki-Model。
---

# Ki-Model 发布维护

遵循既有 Ki 产品的维护者选择、逐项确认与状态核验方式，使用 Git、gh、目标仓库已有 workflow 和 CI 执行维护。长期规则见 `docs/ki-model/maintenance.md`；具体版本、选择依据和执行证据从仓库记录读取。

## 权威资料与范围

开始前读取选定 Ki-Model clone 的 `AGENTS.md`、`docs/ki-model/maintenance.md`、基准/版本文件、CI 和发布 workflow。跨仓候选分析使用 Ki-Buddy 工作区的 `ki-release-maintenance`，候选完整性以其维护手册为准；该技能不可用时报告缺口，不编造候选结论。

只接受 remote 规范化为 `xlihub/Ki-Model` 的目标，核对上游为 `iOfficeAI/aionrs`。不要调用只允许 Ki-Core 的发布工具。工程文档和报告使用简体中文。

## 只读准备

读取 `git status --short --branch`、`git worktree list --porcelain`、remote、HEAD，检查 `gh auth status --active --hostname github.com` 和当前账号权限；回读 product/main、已有 PR/run/tag/Release。按文件与 SHA 判断当前处于上游同步、版本准备、发布或恢复阶段，不按标题猜测，不重复创建对象。

维护者分别选择上游 tag/peeled commit 与 Ki-Model 稳定 SemVer；已有明确选择继续有效，不再从 latest 或 Release Please 建议代选。基准选择中的完整性例外只适用于记录中获批准的准确 tag/peeled commit 与缺失项，不降低后续完整性要求。没有正式产品版本时如实报告“尚未发布”。

## 每次写操作的确认

与既有 Ki 产品发布技能一致，改变文件、refs、PR、Actions 状态、tag、Release 或下载目录前，列出目标仓库及绝对路径、base SHA、维护分支、当前/目标基准、当前/目标产品版本、精确修改清单和命令；等待维护者确认该项。已有针对同一具体操作的明确授权可沿用，命令或目标变化后重新确认。

commit、just push、创建 PR、合并 PR、dispatch/retry、删除 worktree/branch 分别保留确认点；GitHub Environment 必须由维护者在 GitHub 亲自审批。不要用一次“发版”代替后续独立决定。

## 上游同步

维护者保留基准时不创建同步 PR。选择新基准后：

1. 核对目标正式 Release、tag direct/peeled refs、同 commit 的发布 run 和资产契约；需要字节核验时经确认下载并核对 checksums。缺失证据不在该基准的明确批准例外范围内时停止同步，并报告缺口。
2. 先核对继承 workflow 禁用状态和 main 的纯上游历史；main 只快进且由维护者串行操作。新 workflow、分叉或状态不明时停止，不自动恢复 Actions。
3. 经确认获取确切 origin/product/main 和选定 upstream tag；从核对过的产品 SHA 建立独立分支及专用 worktree，保留原工作树改动。
4. 以 `git merge --no-commit --no-ff <peeled-commit>` 合入选定 SHA；不合入 tag 之后的浮动 main。冲突时保留现场并交给 resolving-merge-conflicts，不选择整份 ours/upstream 覆盖。
5. 按目标仓库当前协议只更新 `ki-model-upstream-pending.json` 与必要兼容改动；保留产品扩展、独立版本、发布文件和已发布映射。
6. 运行已有 Rust 检查与实际 CI；SDK 扩展存在时验证旧构造入口、扩展接口和 Core 消费。PR 正文写入 compare、累计变化、兼容性和真实结果；经确认提交、just push 并建立面向 product/main 的 PR。
7. 回读 PR head SHA 和 required checks，全部成功后才进入合并确认。重复同步已包含的精确 commit 时报告无改动，继续已有 PR，禁止重复创建。

## SDK 版本与发布

读取 `ki-model-version.txt`、`ki-model-upstream-pending.json`、`ki-model-upstream.json`、`ki-model-versions.json`、产品 CHANGELOG、Release Please 和稳定版 workflow，并与实际 tag/Release 核对。首次发布时检查配置是否齐备；后续发布时核对最近成功发布版本及其来源。配置缺失或记录不一致时列出具体缺口，通过普通配置或修复 PR 处理，在解决前停止发版。

建设或更新发布配置时，优先复用选定 aionrs 的 Release Please 与 release workflow 的构建、打包、checksum 和资产汇总，调整 Ki 产品独立版本、tag、branch、基准映射、审批和不可变资产语义。

维护者选定的产品 SemVer 独立于 Cargo workspace version；后续版本须高于已发布稳定版，不得复用已有 tag 或历史映射。采用新上游的同步 PR 应先合并；版本 PR 按实际 Release Please 协议提升 pending、追加新版本映射，保持已发布历史不变。版本准备文件不代表发布成功，须以实际 Release 及其 commit 的映射判定已发布状态。保留可解析分支、标题、正文区块和 autorelease 标签；确定性状态机缺口由普通修复 PR 处理。

版本 PR 检查通过并经确认合并后，核对 release commit。tag/Release 应由实际发布 workflow 在 `ki-model-stable` Environment 获维护者审批后创建，并显式触发后续验证；不手工创建产品 tag，不把 GITHUB_TOKEN 创建 tag 等同于另一个 workflow 已启动。

SDK 完成条件以目标 tag 声明的来源清单、crate、适用测试和跨平台构建为准。若承诺 CLI 资产，再核对完整声明矩阵与 checksums；不强制 SDK 用户安装 CLI。只有 tag/Release 对象不代表完整可消费。

## 恢复与完成

按 PR/run/tag 重新建立状态，核对 repository、head/base、commit、attempt、workflow、版本映射和资产。只对同 commit 的瞬时故障在单独确认后重跑；源码或配置需修改的确定性失败使用修复 PR 和新产品版本，不覆盖已公开 tag、资产或映射。

正式发布完成必须核对固定 ki-model-vX.Y.Z、40 位 release commit、上游 tag/peeled commit、workflow 与来源证明，且声明的全部检查/资产满足契约。未验证项逐项写明。证据保留在现有映射、产品 CHANGELOG、关联 PR/run/Release；不另建本地发布日志。

发布完成后向下游提供固定 SDK tag/commit、来源映射和验证结果；下游按实际依赖清单核对全部直接 SDK crate 同源同 revision。Core pin 更新和下游发布由各自维护流程决定。PR 合并/关闭前保留 worktree，清理和分支删除分别确认。
