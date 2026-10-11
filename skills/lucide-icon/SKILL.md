---
name: lucide-icon
description: Add a new SVG icon to this project by downloading it from lucide.dev. Use whenever new UI icons are needed in code.
---

# Lucide Icon

本项目图标统一来自 [lucide](https://lucide.dev/icons/)。gpui-kit 内置图标（`IconName`）已有的优先用内置，确实没有才新增。

## Steps

1. 在 https://lucide.dev/icons/ 找到图标，记下它的 kebab-case 名称（如 `wrench`、`square-arrow-out-up-right`）。
2. 下载 SVG 存入 `assets/icons/`：

   ```
   curl -o assets/icons/<name>.svg https://unpkg.com/lucide-static@latest/icons/<name>.svg
   ```

3. 在 `src/custom_icons.rs` 注册两处：
   - `CustomIconName` 枚举加变体（大驼峰，如 `Wrench`）。
   - `IconNamed::path` 的 match 加一行：`CustomIconName::Wrench => "icons/wrench.svg".into()`。
4. 使用：`Icon::new(CustomIconName::Wrench)`。

## Rules

- 文件名保持 lucide 原名（kebab-case），不改内容、不改颜色（stroke 用 currentColor，由主题着色）。
- 下载后确认 SVG 以 `<svg` 开头且含 `viewBox="0 0 24 24"`，防止拿到 404 HTML。
