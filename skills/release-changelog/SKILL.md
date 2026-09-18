---
name: "release-changelog"
description: "为 NetAssistant 版本发布更新网站文档（仅 docs，不碰 GitHub Release Notes）。Invoke when用户发布了新版本或新 tag（如 '我发布了 vX.Y.Z'、'更新下文档'、'更新 changelog'），需要按 git 提交更新 docs/changelog.md 与 docs/en/changelog.md，视情况更新功能说明（download.md / features.md / guide/*）。"
---

# 版本发布文档更新

为新版本系统地读取 git 提交、审阅改动，在不歪曲事实的前提下生成中英文 changelog，并视情况同步功能说明文档。**只管 `docs/`，不要改动 GitHub Release Notes（自动生成，由 CI 负责）。**

## 何时使用

- 用户说"发布了新的版本/tag"，需要更新文档
- 需要为新版本更新 `docs/changelog.md` / `docs/en/changelog.md`

## 工作流程

### 1. 确定版本范围

- 用 `git tag` 拿到最新 tag（通常是用户刚发布的新版本）
- 用 `git log --oneline <上一版本tag>..<新版本tag>` 拿到本次发布的全部提交
- 用 `git show -s --format=%ci <新版本tag>` 拿到 tag 提交时间，作为 changelog Badge 的日期

若用户没给版本号，用最新 tag；若返回值混乱，先问用户确认目标版本。

### 2. 审阅改动源码，确认用户可见的更新内容

- `git diff --stat <上一版本>..<新版本>` 看整体改动分布（区分 `src/` 代码改动 vs `docs/` 文档改动 vs `packaging/` 打包改动）
- 对每个 `feat:`/`fix:`/`perf:` 提交，结合实际改动的文件读关键 diff，**确认真实的用户可见行为**，不要臆测
- 重点判断：
  - 新增功能（要写"功能名" + 一句话讲清用途）
  - 修复 bug（写清修了什么、影响哪个场景）
  - 性能/架构优化（写清优化的场景与效果）
  - 打包/分发变化（installer、winget、AppImage 等入口变化）
- `docs:` 提交通常只是 `docs/changelog.md` 自身或站点配置，不需要重复写进本期 changelog

### 3. 更新中文 changelog（docs/changelog.md）

- 在文件顶部第一条（即最新的版本条目之上）插入新版本小节
- 格式遵循现有惯例：
  ```
  ## vX.Y.Z <Badge type="tip" text="YYYY-MM-DD" />

  - **功能一**：一句话说明
  - **功能二**：一句话说明
  - 次要项，不用加粗
  ```
- 按"用户可见的重要程度"而非提交数量排列；每个粗体项开头用一个醒目的短名
- 语言精炼，符合已有的中文措辞风格

### 4. 镜像到英文 changelog（docs/en/changelog.md）

- 保持版本号、日期、条目数量、顺序完全一致
- 专业英文翻译，功能短名用已习惯的英文术语（如 Inno Setup installer、stress testing）

### 5. 视情况更新功能说明

只有当某条改动**改变了用户的操作入口、下载方式或功能描述**时才更新对应页面：

- 新增 install 方式 / 打包变化 → 更新 `docs/download.md` 与 `docs/en/download.md`
- 新增/改动功能 → 更新 `docs/features.md`、`docs/en/features.md` 及对应 `docs/guide/*` 用法页
- 纯性能/内部架构优化、不改变用户用法 → **不**改功能说明，只写进 changelog

> 若只是普通功能项（不涉及上次文档描述的行为变化），不要为凑篇幅改动功能页。
> 遵循项目约定：不要擅自改动用户配置的报文、解码器等文本语义；英文与中文保持一致。

### 6. 自查

- 中英两条版本条目的数目、顺序、日期一致
- 所有描述都能从 git diff / 源码中找到依据，不含编造内容
- 功能说明的改动确实改动了用户可见的入口或描述