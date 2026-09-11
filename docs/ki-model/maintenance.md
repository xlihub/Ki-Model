# Ki-Model 产品维护

Ki-Model 沿用既有 Ki 产品的发布维护方式：只读维护技能核查来源、候选版本和当前状态；维护者分别选择上游基准与产品版本；产品发布技能使用 Git、gh 和仓库已有 CI 执行已确认的步骤。上游更新、SDK 开发、产品发布和下游采用各自按需要推进。

## 维护入口与事实来源

- 只读状态、候选和恢复分析：Ki-Buddy 工作区的 `.claude/skills/ki-release-maintenance/SKILL.md`，按 remote 身份发现 Ki-Model。
- 已确认的 Ki-Model 同步、发布准备与恢复：[release-ki-model](../../.claude/skills/release-ki-model/SKILL.md)。该入口只操作 `xlihub/Ki-Model`。
- 跨产品维护原则：Ki-Buddy 的独立发布 ADR 和维护手册；具体变更的需求与依赖通过[工单入口](../agents/issue-tracker.md)读取。

本指南定义长期维护规则。具体版本、选择日期、兼容性结论和检查结果记录在基准文件、关联 PR/run/Release 中，不在规范中维护状态副本。每次操作都从目标仓库重新读取事实，区分维护者选择、配置要求和已经验证的结果。

## 仓库与分支

| 仓库/分支 | 职责 |
| --- | --- |
| `iOfficeAI/aionrs` | 上游 SDK/CLI；正式 Release 提供可选择的发布基准 |
| `xlihub/Ki-Model:main` | 只跟踪上游历史；快进同步，不接受产品功能、版本或发布提交 |
| `xlihub/Ki-Model:product/main` | 长期默认分支及产品 PR base |
| 产品维护分支 | 从已核对的 product/main SHA 创建；经 PR 与 CI 合入 |

origin 指向 GitHub Ki-Model，upstream 指向 GitHub aionrs；规范化 remote 的 owner/repo 后确认身份，不能按目录名或 origin 名称推断。保留已有工作树与未提交文件，专用 worktree 用于每个同步/发布 PR。main 与上游分叉时停止；不强推、回退或替换已有历史。

首次建立 product/main 时，以维护者选定且核验过的上游 tag 的精确 peeled commit 为起点，记录选择依据。已有 product/main 时从其实际历史继续维护，不因重新选择上游基准而重建产品分支。

## 上游候选与基准选择

使用现有维护技能读取相关 Release、tag direct/peeled refs、run、compare 和 changed paths。候选完整性沿用 Ki 维护手册，以目标 tag 的发布 workflow 和资产契约为证据，不根据最高 SemVer、最新 main 或当前目录 HEAD 自动选择。

每个候选相对当前产品发布基准报告累计变化；存在待发布基准时另列其差异。尚无正式产品 Release 时，以已记录的初始基准进行比较；尚无基准时报告首次选择所需证据。特别核对公开 Rust API、provider 与流式协议、agent/session、下游实际消费的 crate、Cargo.lock、工具链、CI 与发布资产。分别记录“已发现风险”“未发现已知风险”“证据不足”。

维护者可以保留当前基准或跳过中间版本；保留时不创建同步 PR。选择新基准时记录精确 tag/peeled commit、compare、兼容性判断及实际检查结果，再按发布技能准备独立同步 PR。候选完整性例外必须记录批准者、日期、准确 tag/peeled commit、缺失项及接受理由；例外仅适用于该次选择，不自动延续到后续版本。

## main 同步与 workflow 隔离

main 快进和产品采用新基准是两项独立操作。按维护者确认逐项执行 Git/gh 命令，记录源、目标和前后 SHA；先确认 fast-forward，重复同步无变化时结束。

main 同步前核查当前及目标提交中的全部继承 workflow。上游的 CI、E2E、Release Please、发布及依赖维护入口在 Ki-Model 保持禁用；产品 workflow 使用独立路径和标识，以及 repository/base/ref guard。产品历史中的继承 workflow 也限制为上游仓库身份，避免误启用后执行。

main 的内容保持上游镜像，所以必须另外核对 GitHub 上的禁用状态。首次 fork 返回空 workflow 列表不等于已禁用。新增上游 workflow 或无法确认状态时，暂停该次同步并先处理隔离；必要时在明确确认后关闭 Actions 总开关，完成更新和逐项核验后恢复。main 同步与 Actions 恢复不得由不同维护操作同时进行，由维护者串行执行。

不得复用继承 workflow 的路径或标识创建产品入口。同步 main 不创建上游 v* tag、Release 或产品版本 PR。

## 基准与产品版本记录

按 Ki-Core 的文件命名与职责约定：

| 文件或标识 | 职责 |
| --- | --- |
| `ki-model-upstream-pending.json` | 维护者选定的待发布上游 repository/tag/peeledCommit，以及适用的选择依据和例外 |
| `ki-model-upstream.json` | 产品版本对应的上游基准，由版本准备流程从 pending 提升 |
| `ki-model-versions.json` | 按独立产品版本追加的上游来源映射；保留已发布历史 |
| `ki-model-version.txt` | 维护者选择的独立产品 SemVer，与上游 Cargo workspace version 分离 |
| `CHANGELOG.ki-model.md` | 产品版本的变更与兼容性说明 |
| `ki-model-vX.Y.Z` | 指向固定产品 release commit 的不可变 tag |

同步 PR 只更新 pending 及必要兼容代码；产品版本和已发布历史不变。版本 PR 提升待发布基准并追加映射。版本准备中的文件不代表已经发布：已发布基准须从实际成功发布的 tag、Release 及该 commit 的映射核对。没有正式产品 Release 时区分初始待发布基准和已发布基准，不补造版本映射。

发布前读取目标仓库实际配置与验证规则。首次发布所需文件和 workflow 应通过普通配置 PR 建立并验证；已有发布流程时沿用其协议。缺失配置或状态不一致时报告具体缺口，不为通过检查修改历史或临时降低 CI。

## 产品 PR 与 CI

所有产品 PR base 为 product/main。PR 正文记录当前/目标基准、40 位 SHA、累计 compare、API/协议影响、实际检查命令及失败或未验证项。修改 SDK 扩展时还要验证旧构造入口、扩展接口和下游消费契约；验证范围以实际代码与需求为准。

`.github/workflows/ki-model-ci.yml` 复用选定上游基准的 CI，保留其工具链、检查、报告和平台构建步骤。采用新基准时比较上游 CI 与发布 workflow 的变化，同步必要更新；产品差异集中在 workflow/check 名称、repository/product/main guard 与 SDK 消费所需检查。现有检查入口包括：

```bash
vx just fmt-check
vx just lint
vx just test-ci
vx cargo audit
vx cargo metadata --locked --format-version 1 --no-deps
vx cargo build --release --target TARGET -p aion-cli
# Use the upstream cross build step for targets that require it.
```

具体命令、平台矩阵、阻塞规则和工具链以目标提交的 workflow、`justfile` 和 `vx.toml` 为准。维护时保留上游检查的原有语义；变更检查范围或失败处理须在 PR 中说明理由。SDK 消费检查与 CLI 构建分别说明结果，本地通过不代替真实跨平台 CI。

实际 check 成功出现后再配置 product/main 的 PR/check 保护，禁止强推与删除；main 只允许受控维护更新，并禁止强推与删除。不要把尚不存在或尚未成功登记的 check 设成永久无法满足的门槛。保护规则与默认分支通过 gh/API 回读确认，记录在关联 PR。

## 独立产品发布

发布配置以选定 aionrs 的 Release Please 与 release workflow 为参考，复用已有工具链、平台构建、打包、checksum 和资产汇总步骤。调整范围集中在 Ki-Model 产品语义：product/main、独立版本/CHANGELOG、ki-model-vX.Y.Z、pending 提升与上游映射、ki-model-stable 审批和不可变已发布资产。产品发布 workflow 使用独立标识及仓库/分支校验，main 同步不启用继承的发布入口。

维护者按产品变更选择首版或后续稳定 SemVer；上游更新不自动触发产品发版，产品发版也不要求更新上游。版本准备、Release Please、tag 和发布任务遵循实际仓库协议，参考 Ki-Core/Ki-Buddy 的已有实现。GitHub Environment 由维护者亲自审批，聊天确认不能代替它。

SDK 发布完成条件以目标 tag 声明的来源清单、crate、适用测试和跨平台构建为准；承诺 CLI 资产时核对完整声明矩阵与 checksums。只有 tag/Release 对象不代表完整可消费。公开 tag、资产和来源映射不移动、不覆盖。

## 中断恢复与下游交接

按 PR、run 或 tag 恢复时重新读取 repository、base/head SHA、workflow、attempt、checks、版本映射和 Release，不依赖旧会话。存在冲突时保留 merge 状态并交给 resolving-merge-conflicts；确定性失败通过修复 PR 处理，同一 commit 的瞬时失败才考虑经确认重跑。已公开版本需要内容修正时发布新产品版本。

发布交接提供固定产品 tag、40 位 release commit、上游 tag/peeled commit、来源证明和实际验证结果。下游集成 PR 按其依赖清单核对所有直接 SDK crate 同源同 revision，并验证锁文件和消费行为。SDK 发布不自动更改 Core pin 或触发桌面/项目包发布；下游由各自维护者选择采用时间。
