# Evidence Review: AI Code-Agent Skills and Plugins

## Research Metadata

- Research date: 2026-07-25
- Scope: reusable skills/plugins for Codex, Claude Code, and compatible coding agents that have empirical evidence of programming-task impact.
- Inclusion rule: a claim of usefulness must identify a task set, baseline, agent/model, and executable or otherwise objective verifier.
- Evidence ceiling: the directly relevant studies below are 2026 preprints. They are useful experimental reports, not peer-reviewed validation or proof that an arbitrary installed skill will help a repository.

## Executive Index

| Item | What was evaluated | Evidence status | Result relevant to programming |
| --- | --- | --- | --- |
| [SWE-Skills-Bench](https://arxiv.org/abs/2603.15401) | 49 public SWE skills, paired with/without skill runs, pinned repositories, deterministic tests | Direct and reproducible; preprint | Mean pass-rate delta +1.2 percentage points; 39/49 skills had zero delta; seven had positive deltas from +7.1 to +30.0 points. |
| [SkillsBench](https://arxiv.org/abs/2602.12670) | 84 tasks in 11 domains, 7 agent-model configurations, deterministic verifiers | Controlled benchmark; preprint | Curated skills added +4.5 points in its software-engineering subset. Self-generated skills had no average benefit. |
| [CODESKILL](https://arxiv.org/abs/2605.25430) | Learned maintenance of a coding-skill bank on EnvBench, SWE-Bench Verified, and Terminal-Bench 2 | Controlled research system; preprint; not a drop-in Codex/Claude Code plugin | +9.69 average pass-rate points versus its no-skill baseline, and +4.01 over its strongest prompt/memory baseline. |
| [Agent Skills specification](https://github.com/agentskills/agentskills) | Interoperable `SKILL.md` format | Format specification, not an effectiveness study | A skill directory can contain instructions, scripts, references, and assets; Codex supports the format. |

## What Counts as a Track Record

For this review, the useful unit is an evaluated skill artifact, rather than star count, marketplace rank, author reputation, model-generated testimonials, or a vendor demonstration. A claim is materially checkable when it has all of the following:

1. An explicit with-skill and without-skill control condition.
2. A fixed agent/model, task prompt, repository revision, and runtime environment.
3. An executable acceptance test, rather than an LLM judging its own output.
4. Per-skill outcomes and token/cost reporting.
5. Published skill content and harness sufficient to rerun or inspect the experiment.

SWE-Skills-Bench meets those properties most closely for Claude Code skills. It uses Claude Code with Claude Haiku 4.5 in Docker containers and places the skill for autonomous discovery, rather than mentioning the skill in the task prompt. [Method and setup](https://arxiv.org/html/2603.15401#S4) [evaluation repository](https://github.com/GeniusHTX/SWE-Skills-Bench)

## Other Empirical Research to Read

The studies below answer different questions from a marketplace-skill evaluation. They provide experimental or observational evidence about AI-assisted programming, agent scaffolds, and benchmark validity.

| Research type | Source | Design | What it measures | Main boundary |
| --- | --- | --- | --- | --- |
| Human productivity RCT | [Peng et al., 2023](https://arxiv.org/abs/2302.06590) | 95 recruited programmers randomly assigned Copilot access for one JavaScript HTTP-server task | Time to completion and task success | Controlled task, 2022 Copilot, not repository-scale maintenance. |
| Human productivity RCT | [METR, 2025](https://metr.org/blog/2025-07-10-early-2025-ai-experienced-os-dev-study/) | 16 experienced maintainers and 246 real issues in their own repositories; random AI-allowed/no-AI assignment | Completion time on real maintenance work | A historical early-2025 tool snapshot; METR marks the result out of date for current models. |
| Skill artifact ablation | [SWE-Skills-Bench](https://arxiv.org/abs/2603.15401) | 49 public skills, paired skill/no-skill conditions, pinned repositories, deterministic tests | Marginal pass-rate and token impact of particular skills | Preprint; generated task/test pipeline; one Claude Code configuration. |
| Cross-domain skill ablation | [SkillsBench](https://arxiv.org/abs/2602.12670) | 86 tasks, 7 agent-model configurations, no-skill/curated/self-generated conditions | Whether curated procedural instructions help | Software engineering is a subset of the benchmark. |
| Agent harness benchmark | [Terminal-Bench 2.0](https://arxiv.org/abs/2601.11868) | 89 terminal tasks, unique environments, human-written solutions, comprehensive tests | Long-horizon terminal-agent capability | It does not isolate individual skill effects. |
| Agent trajectory study | [Majgaonkar et al., 2025](https://arxiv.org/abs/2511.00197) | Successful and failed traces from OpenHands, SWE-agent, and Prometheus on SWE-Bench | Failure modes and action patterns | Observational analysis of traces, not a human-productivity or skill ablation. |
| Agent traceability study | [Ceka et al., 2025](https://arxiv.org/abs/2506.08311) | Five agents; taxonomy, component analysis, code-clone analysis | Bug localization, patch generation, and reproduction-test generation | Does not evaluate a portable skill package. |
| Product telemetry | [Anthropic, 2026](https://www.anthropic.com/research/claude-code-expertise) | Privacy-preserving analysis of approximately 400,000 Claude Code sessions | Work composition, decision attribution, and stated success signals | Observational data with no no-skill control. |
| Benchmark validity audit | [OpenAI, 2026](https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/) | Audit of 138 frequently unsolved SWE-bench Verified tasks over 64 independent runs | Test validity and training contamination | An evaluation of a benchmark, not a measurement of a coding product. |

### Human Programming Studies

[Peng et al.](https://arxiv.org/abs/2302.06590) randomly assigned 95 recruited programmers to Copilot access or control for a JavaScript HTTP-server implementation task. Among completers, the treatment condition took 71.17 minutes versus 160.89 minutes for the control condition, reported as a 55.8% reduction in time with a 95% confidence interval of 21% to 89%. The study has a randomized intervention, telemetry-confirmed Copilot use, a fixed task, and repository test history. [Study design and results](https://arxiv.org/abs/2302.06590)

[METR's 2025 study](https://metr.org/blog/2025-07-10-early-2025-ai-experienced-os-dev-study/) used the converse setting: 16 experienced open-source maintainers working on 246 issues from repositories to which they had contributed for years. It randomly assigned each issue to AI-allowed or no-AI conditions. Its reported result was 19% longer completion time with the early-2025 tools, while developers' prior and post-task estimates predicted a speedup. METR explicitly labels those results historical and states that they no longer reflect current models. This is evidence on human-plus-tool output rather than a skill package.

Reading both RCTs together isolates a useful axis for further literature searches: standardized unfamiliar tasks versus maintenance tasks in familiar, mature codebases. The two designs differ in participant pool, tool generation, task shape, and repository familiarity.

### Agent-Scaffold and Trace Research

[Terminal-Bench 2.0](https://arxiv.org/abs/2601.11868) is a current agent evaluation set with 89 terminal tasks, a human-written solution for each task, and comprehensive tests. It reports frontier agents below 65% on the suite. It is suitable reading for how an agent harness is assessed across long terminal workflows, but it does not establish whether `SKILL.md` packages improve performance.

[Majgaonkar et al.](https://arxiv.org/abs/2511.00197) analyze agent execution trajectories instead of only final pass rates. Across OpenHands, SWE-agent, and Prometheus, they report that failed trajectories are longer and have higher variance; problematic-file localization occurs in 72-81% of trajectories even when the task fails. This work supplies categories for examining a candidate skill's mechanism: context gathering, localization, edit selection, test generation, and recovery behavior.

[Ceka et al.](https://arxiv.org/abs/2506.08311) study five agents through a decision-path taxonomy and analyze bug localization, patch generation, and reproduction-test generation. Their methods provide a basis for asking whether a skill changes an intermediate behavior rather than only a final test outcome.

### Evaluation-Validity Research

Benchmark scores can be invalidated by defects in the verifier, ambiguous task specification, or model exposure to task solutions. OpenAI's 2026 SWE-bench Verified audit found material test-design and/or task-description issues in 59.4% of an audited 138-task subset and reports evidence that frontier models had seen some task solutions in training. [Audit details](https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/)

This literature is relevant when assessing any claimed plugin score: require task publication dates after a model's training cutoff where feasible, private or live holdouts, human review of requirements and tests, and an audit that accepts functionally correct alternate implementations.

## Directly Measured Public Skills

The seven positive entries from SWE-Skills-Bench are below. The study evaluates these artifacts on its own generated, repository-pinned tasks. The deltas should not be treated as transfer estimates for a different codebase, model, version, or task distribution.

| Skill | Domain | Tasks | With skill | Without skill | Delta | Token change |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `risk-metrics-calculation` | financial calculations | 10 | 100.0% | 70.0% | +30.0 pp | -34.8% |
| `gitlab-ci-patterns` | GitLab CI | 14 | 78.6% | 64.3% | +14.3 pp | +58.6% |
| `prompt-engineering-patterns` | prompt patterns | 10 | 100.0% | 90.0% | +10.0 pp | +46.4% |
| `similarity-search-patterns` | vector search | 10 | 100.0% | 90.0% | +10.0 pp | -32.4% |
| `distributed-tracing` | observability | 13 | 100.0% | 92.3% | +7.7 pp | -30.4% |
| `tdd-workflow` | testing | 14 | 28.6% | 21.4% | +7.1 pp | +78.6% |
| `istio-traffic-management` | service mesh | 14 | 100.0% | 92.9% | +7.1 pp | -22.0% |

Source: [the paper's complete per-skill result table](https://arxiv.org/html/2603.15401#S4.T2). The benchmark repository publishes the 49 skill documents and runner commands for its with-skill/control groups. [Task list and evaluation instructions](https://github.com/GeniusHTX/SWE-Skills-Bench)

Three tested skills had negative deltas: `springboot-tdd` (-10.0 pp), `linkerd-patterns` (-9.1 pp), and `django-patterns` (-9.1 pp). Forty-nine was the evaluated sample, not a census of all available skills.

## Findings That Transfer to Codex and Claude Code Workflows

### Narrow, domain-specific skills

The positive SWE-Skills-Bench entries encode concrete operating knowledge: formulas, an external configuration language, framework-specific configuration, or a repeatable test workflow. This is the supported artifact class for reuse. The same experiment reports zero pass-rate change for generic artifacts including `security-review`, `fix`, `mcp-builder`, `python-packaging`, `github-actions-templates`, and `turborepo`. [Complete table](https://arxiv.org/html/2603.15401#S4.T2)

### Curated skills versus agent-authored skills

SkillsBench ran no-skill, curated-skill, and self-generated-skill conditions over 7,308 trajectories. Curated skills increased overall pass rate by 16.2 points and the software-engineering subset by 4.5 points. Its self-generated skills had no average gain. It also reports that focused skills with two or three modules outperformed comprehensive documentation. [Paper abstract and results](https://arxiv.org/abs/2602.12670)

### Persistent, evaluated skill-bank management

CODESKILL is research code rather than a published plugin for Codex or Claude Code. It provides evidence for a maintenance procedure: extract skills from trajectories, add/merge/drop them, and use downstream executable task feedback to train the manager. Its evaluation reports 29.57% to 39.26% average pass rate under its primary downstream policy, and 25.88% to 34.12% on held-out Terminal-Bench 2. [Results section](https://arxiv.org/html/2605.25430#S4)

## Codex and Claude Code Compatibility

The Agent Skills format defines a directory with a `SKILL.md` file, with optional `scripts/`, `references/`, and `assets/`. It uses progressive disclosure: discovery metadata, full instruction activation, then optional resources/scripts. [Specification repository](https://github.com/agentskills/agentskills)

OpenAI documents that Skills are supported in Codex and that OpenAI Skills follow the Agent Skills open standard. The documentation also states that uploaded external skills may contain instructions, supporting files, and code, and recommends source review even after platform scanning. [OpenAI Skills documentation](https://help.openai.com/en/articles/20001066)

No study found in this pass evaluates the exact same third-party skill under both Codex and Claude Code. Portability of the package format does not establish portability of measured performance.

## Screening Procedure for Candidate Skills

Use this before installation or adoption:

1. Require an exact commit/tag or immutable artifact, readable `SKILL.md`, and full bundled file inventory.
2. Read every executable file and each instruction that can invoke network, shell, package-manager, credential, or destructive filesystem actions.
3. Pin the target repository revision and create an acceptance-test harness before enabling the candidate.
4. Run paired trials with the same model, token budget, prompt, tool permissions, environment, and seeds where supported.
5. Record task pass rate, test failures, wall time, input/output tokens, changed files, and action log.
6. Retain a skill only if its measured delta is positive for the repository/task slice where it will be triggered. Re-test after framework, toolchain, or model changes.

Minimal paired-run record:

```text
candidate: org/skill@commit
agent/model: <exact identifier>
repo/revision: <commit>
task-set: <paths to requirements and tests>
control: no skill
treatment: skill enabled
verifier: <command and exit status>
metrics: pass rate, tokens, duration, diff, tool log
```

## Supply-Chain and Evaluation Limits

An Agent Skill can bundle executable code and agent-facing instructions. The format repository describes both optional scripts and progressive loading. [Format contents](https://github.com/agentskills/agentskills#what-are-agent-skills)

The May/June 2026 MalSkillBench preprint constructs runtime-verified malicious agent skills and reports that code-injection detection is substantially easier than prompt-injection and agent-control attacks in its dataset. Its claims concern benchmarked detection, not a claim that all public skills are malicious. [MalSkillBench](https://arxiv.org/abs/2606.07131)

The directly useful result is procedural: treat a third-party skill as a combined prompt-and-code supply-chain dependency, inspect it before enablement, and constrain its execution permissions in the trial harness.

## Current Product and Usage Evidence

Anthropic's June 2026 observational report covers approximately 400,000 Claude Code sessions, not individual skills. It reports that people make about 70% of planning decisions and 20% of execution decisions, and that 56% of observed sessions involve code writing, fixing, testing, or orchestration. It does not establish that a particular skill improves programming performance. [Report and methodology summary](https://www.anthropic.com/research/claude-code-expertise)

## Research Gaps

- No peer-reviewed study was found that demonstrates general performance improvement from an arbitrary marketplace skill on arbitrary repositories.
- No controlled study was found that compares the same public programming skill on both Codex and Claude Code.
- SWE-Skills-Bench is a preprint and uses task requirements and test files generated by its pipeline; inspect and rerun the published harness before relying on a reported delta.
- CODESKILL has measured results but is not a packaged, directly installable Codex/Claude Code plugin.

## Gap-Filling Research Program

### 1. Reproduce the public-skill result table on the actual target stack

The first experiment should use the published SWE-Skills-Bench corpus because it already supplies a control condition, pinned repositories, container images, requirements, and deterministic tests. Freeze a commit of the benchmark repository, then run each selected skill under a fixed Codex configuration and a fixed Claude Code configuration. Preserve the original no-skill condition.

The experimental unit is a `(task, agent stack, skill condition)` run:

| Factor | Levels |
| --- | --- |
| Agent stack | Codex; Claude Code |
| Skill condition | no skill; the same pinned skill artifact |
| Task | benchmark task, held-out repository task, or real issue |
| Model configuration | exact model/version, reasoning setting, token cap, tool permissions |
| Trial | repeated independent run where the product supports it |

For every run, retain the prompt, skill commit, repository commit, Docker image digest, full tool/action log, generated diff, test command and result, token count, duration, and termination reason. The primary effect for each agent is `pass(with skill) - pass(without skill)`. The cross-agent question is the difference between those two effects, rather than a raw Codex versus Claude Code pass-rate comparison.

### 2. Establish whether marketplace results generalize

The literature currently reports selected public artifacts, not a representative marketplace sample. Construct a frozen sampling frame from one or more registries on a stated date. Partition by task type before selection, for example: language/framework guidance, CI/CD, testing, security, observability, data/ML, repository navigation, and generic workflow prompts. Sample a fixed number from every stratum, including low-visibility artifacts, and publish selection queries and exclusions.

Use an audit table per artifact:

| Field | Purpose |
| --- | --- |
| Registry URL and artifact commit/hash | Immutable identity |
| Text and bundled-file size | Context and execution exposure |
| Executable files and declared commands | Security review surface |
| Framework/tool versions named by the skill | Version-mismatch analysis |
| Task family and target repository revision | Domain-fit analysis |
| Paired pass-rate and cost delta | Outcome |
| Failure category | Mechanism: irrelevance, stale instruction, conflicting convention, tool failure, unsafe action, or test failure |

Report all sampled skills, including zero and negative deltas. This directly addresses selection bias created by collections that show only favorable examples.

### 3. Measure transfer rather than only in-distribution success

For each skill, create three task slices:

1. Tasks from the framework/repository family named by the skill.
2. Tasks from a different repository using the same framework/version.
3. Tasks with a nearby but incompatible version or convention.

The third slice measures the version-conflict failure reported by SWE-Skills-Bench. It provides a direct estimate of whether an artifact is an instruction package for a particular ecosystem version, a broadly reusable procedure, or a source of interference outside its original context.

### 4. Separate execution correctness from programming assistance

Executable tests measure whether the produced patch satisfies the task. They do not measure whether a skill helps a programmer understand, plan, review, or safely supervise the work. A separate randomized crossover user study can measure that layer.

Give experienced programmers equivalent task sets in randomized order with: no skill, a target skill, and a neutral information control of similar token length. Log task completion, final test pass, review-detected defects, time to first correct plan, interventions, reverted changes, and whether the user can accurately identify the skill's constraints. Predefine exclusion and stopping rules.

Anthropic's usage report is observational and does not isolate skill effects, so it supplies context but cannot substitute for this study. [Methodology summary](https://www.anthropic.com/research/claude-code-expertise)

### 5. Test safety as an outcome, not only source-review hygiene

Run third-party skills in a disposable container with a synthetic repository and canary credentials that have no external access. Instrument shell execution, filesystem writes, subprocesses, outbound DNS/HTTP attempts, and attempts to read credentials or Git configuration. Score both task completion and prohibited actions.

MalSkillBench provides a motivation for this combined instruction-and-code threat model, but its dataset is a detection benchmark. A programming-skill study needs a separate, published policy for what counts as an unsafe action. [MalSkillBench](https://arxiv.org/abs/2606.07131)

### 6. Make claims reviewable

Pre-register the primary metric, task sampling, trial count, aggregation method, and planned subgroup analyses. Release an executable harness, container digests, task fixtures, artifact snapshots, raw run logs subject to credential removal, and a machine-readable results table. Label preprints as preprints and submit the completed study for peer review.

## Concrete Starting Sequence

1. Fork [SWE-Skills-Bench](https://github.com/GeniusHTX/SWE-Skills-Bench) at a fixed commit.
2. Select the seven positive skills and the three negative skills from its reported table as a falsifiable initial set.
3. Write one adapter per agent stack that receives a task directory, a skill directory or empty directory, and a fixed policy/configuration.
4. Run the benchmark's no-skill and skill conditions before changing the corpus or creating new tasks.
5. Compare per-task paired outcomes, then add a predeclared stratified marketplace sample and the three transfer slices.
6. Publish the run manifest and raw artifacts before interpreting aggregate effects.

The final result should make a claim at this granularity: `artifact A@commit improves/does not improve task family B under agent configuration C, test harness D, and stated execution constraints.`

## Source Links

- [SWE-Skills-Bench paper](https://arxiv.org/abs/2603.15401) and [HTML result table](https://arxiv.org/html/2603.15401#S4.T2)
- [SWE-Skills-Bench evaluation code and skill corpus](https://github.com/GeniusHTX/SWE-Skills-Bench)
- [SkillsBench](https://arxiv.org/abs/2602.12670)
- [CODESKILL](https://arxiv.org/abs/2605.25430)
- [Agent Skills specification](https://github.com/agentskills/agentskills)
- [OpenAI Skills documentation](https://help.openai.com/en/articles/20001066)
- [Anthropic Claude Code usage study](https://www.anthropic.com/research/claude-code-expertise)
- [MalSkillBench](https://arxiv.org/abs/2606.07131)
