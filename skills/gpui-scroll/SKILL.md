---
name: "gpui-scroll"
description: "GPUI scrolling patterns and best practices. Invoke when implementing scrollable areas, fixing scroll issues, or using overflow_y_scrollbar/Scrollbar in GPUI framework."
---

# GPUI Scrolling Patterns & Best Practices

## Core Principle

**`overflow_y_scrollbar()` requires a determined height constraint to work.** If the container's height is flexible (e.g. `flex_1()` without a fixed-height parent), the container will grow to fit all content and scrolling will never activate.

## Pattern 1: Simple Scrollable Area (Most Common)

Use when: A container needs to scroll its content vertically.

```rust
div()
    .h(px(300.0))              // FIXED height - required!
    .overflow_y_scrollbar()     // Enables scrollbar
    .child(content)
```

**Key rules:**
- The container MUST have a determined height (`h()`, not `max_h()` alone)
- `overflow_y_scrollbar()` handles both overflow detection and scrollbar rendering
- Import: `use gpui_component::scroll::ScrollableElement;`

## Pattern 2: Flex Layout with Scrollable Content Area

Use when: A panel has a header + scrollable body (e.g. list panel with title bar).

```rust
div()
    .h(px(300.0))              // Outer: fixed height
    .overflow_hidden()         // Outer: clip overflow (CRITICAL!)
    .flex()
    .flex_col()
    .child(header_div)         // Fixed header (~36px)
    .child(search_div)         // Fixed search (~44px)
    .child(
        div()
            .flex_1()                  // Takes remaining space
            .overflow_y_scrollbar()    // Scrolls only this section
            .child(content)
    )
```

**Key rules:**
- Outer container: `h()` + `overflow_hidden()` — prevents content from leaking out
- Inner scrollable area: `flex_1()` + `overflow_y_scrollbar()` — fills remaining space and scrolls
- Without `overflow_hidden()` on the outer container, content will visually overflow even if `overflow_y_scrollbar()` is set on the inner area

## Pattern 3: Full-Page Sidebar Scroll

Use when: A sidebar or panel fills the full height and needs to scroll.

```rust
div()
    .w(px(200.0))
    .h_full()                  // Full height from parent
    .overflow_y_scrollbar()
    .child(content)
```

**Key rule:** Parent must have a determined height. If parent is `flex_1()`, ensure the grandparent has a fixed or determined height.

## Pattern 4: Custom Scrollbar with ScrollbarState

Use when: You need control over scrollbar visibility, position, or styling.

```rust
// In your state struct:
pub scroll_state: ListState,  // or use ScrollHandle

// In render:
div()
    .relative()
    .w_full()
    .flex_1()
    .child(
        div()
            .pr_8()           // Padding for scrollbar space
            .size_full()
            .child(scrollable_content)
    )
    .child(
        div()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .w(px(12.0))
            .child(
                Scrollbar::vertical(&scrollbar_state)
                    .scrollbar_show(ScrollbarShow::Always),
            ),
    )
```

**Import:** `use gpui_component::scroll::{Scrollbar, ScrollbarShow, ScrollableElement};`

## Pattern 5: Horizontal Overflow Hidden

Use when: Text or content should not overflow horizontally.

```rust
div()
    .max_w(px(60.0))
    .whitespace_nowrap()
    .overflow_x_hidden()       // Clip horizontal overflow
    .child(text)
```

For text ellipsis:
```rust
div()
    .max_w(px(150.0))
    .overflow_hidden()
    .text_ellipsis()
    .whitespace_nowrap()
    .child(text)
```

## Common Pitfalls & Solutions

### Pitfall 1: Using `max_h()` instead of `h()`

```rust
// WRONG - container grows beyond max_h, no scrollbar appears
div().max_h(px(300.0)).overflow_y_scrollbar()

// CORRECT - container is exactly 300px, content scrolls
div().h(px(300.0)).overflow_y_scrollbar()
```

**Why:** `max_h()` allows the container to be smaller than the max. If content pushes it to max_h, the container may still expand because `overflow_y_scrollbar()` doesn't constrain — it only adds a scrollbar IF the container has a fixed size.

### Pitfall 2: Missing `overflow_hidden()` on parent

```rust
// WRONG - content overflows the 300px panel visually
div()
    .h(px(300.0))
    .flex().flex_col()
    .child(header)
    .child(
        div().flex_1().overflow_y_scrollbar().child(content)
    )

// CORRECT - parent clips, inner area scrolls
div()
    .h(px(300.0))
    .overflow_hidden()        // <-- CRITICAL
    .flex().flex_col()
    .child(header)
    .child(
        div().flex_1().overflow_y_scrollbar().child(content)
    )
```

### Pitfall 3: `flex_1()` child without constrained parent

```rust
// WRONG - parent has no height, flex_1() resolves to 0 or infinite
div()
    .flex().flex_col()
    .child(
        div().flex_1().overflow_y_scrollbar().child(content)
    )

// CORRECT - parent has determined height
div()
    .h_full()                 // or h(px(500.0))
    .flex().flex_col()
    .child(
        div().flex_1().overflow_y_scrollbar().child(content)
    )
```

### Pitfall 4: Popup panel covered by sibling borders

When a popup panel (absolute positioned) is rendered inside a container with `border_t_1()`, the border renders on top of the popup.

**Solution:** Move the popup rendering to the root-level container (e.g. main_window), so it renders after all other elements and naturally appears on top. Use absolute coordinates from `MouseDownEvent.position` for positioning.

## Quick Reference

| Scenario | Height | Overflow | Scrollbar |
|----------|--------|----------|-----------|
| Simple scrollable box | `h(px(N))` | — | `overflow_y_scrollbar()` |
| Panel with header+scroll | `h(px(N))` + `overflow_hidden()` | outer: hidden | inner: `flex_1()` + `overflow_y_scrollbar()` |
| Full-height sidebar | `h_full()` | — | `overflow_y_scrollbar()` |
| Text no-wrap clip | — | `overflow_x_hidden()` | — |
| Text with ellipsis | — | `overflow_hidden()` + `text_ellipsis()` | — |

## Required Imports

```rust
use gpui_component::scroll::ScrollableElement;  // for overflow_y_scrollbar()
use gpui_component::scroll::{Scrollbar, ScrollbarShow};  // for custom scrollbar
```
