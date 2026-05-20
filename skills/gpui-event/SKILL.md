---
name: "gpui-event"
description: "GPUI event handling patterns for click-through, stop propagation, and overlay. Invoke when fixing click penetration issues, implementing click-outside-to-close, or handling nested mouse events in GPUI."
---

# GPUI Event Handling Patterns

## Core Concept

In GPUI, mouse events **bubble up** from child to parent by default. When a child element and its parent both have `on_mouse_down` handlers, clicking the child triggers both handlers. This is the root cause of "click-through" (点击穿透) issues.

## Pattern 1: Block Event Bubbling with Empty Handler

Use when: A popup/overlay should not pass clicks through to underlying elements.

```rust
div()
    .absolute()
    .inset_0()
    .on_mouse_down(MouseButton::Left, |_, _, _| {})  // Empty handler blocks bubbling
    .child(popup_content)
```

**How it works:** The empty `on_mouse_down` handler consumes the event at this layer, preventing it from reaching elements below.

**Common mistake:** Forgetting this handler causes clicks on the popup to also trigger handlers on elements behind it.

## Pattern 2: Stop Propagation in Nested Elements

Use when: A child element's click should NOT trigger the parent's click handler.

```rust
// Parent with click handler
div()
    .on_mouse_down(MouseButton::Left, cx.listener(|app, _event, _window, cx| {
        // This handles "click on the whole item"
        do_something();
        cx.notify();
    }))
    .child(
        // Child delete button
        div()
            .on_mouse_down(MouseButton::Left, cx.listener(|app, _event, _window, cx| {
                cx.stop_propagation();  // CRITICAL: prevent bubbling to parent
                delete_item();
                cx.notify();
            }))
            .child(Icon::new(IconName::Close))
    )
```

**Key API:** `cx.stop_propagation()` — Must be called inside the listener closure, on the `cx` parameter (which is `&mut Context<T>`).

**Common mistake:** Trying to chain `.stop_propagation()` after `cx.listener(...)` — this does NOT work because `stop_propagation` is a method on `Context`, not on the closure.

```rust
// WRONG - stop_propagation is not a method on the closure
.on_mouse_down(MouseButton::Left, cx.listener(|..| { ... }).stop_propagation())

// CORRECT - call inside the closure
.on_mouse_down(MouseButton::Left, cx.listener(|.., cx| { cx.stop_propagation(); ... }))
```

## Pattern 3: Click-Outside-to-Close (Overlay Pattern)

Use when: A popup should close when clicking anywhere outside of it.

```rust
// Outer overlay - covers entire window, closes on click
div()
    .absolute()
    .inset_0()
    .on_mouse_down(MouseButton::Left, cx.listener(|app, _event, _window, cx| {
        app.show_popup = false;
        cx.notify();
    }))
    .child(
        // Inner popup - blocks the overlay's close handler
        div()
            .absolute()
            .left(pos_x)
            .top(pos_y)
            .w(px(320.0))
            .on_mouse_down(MouseButton::Left, |_, _, _| {})  // Block bubbling to overlay
            .child(popup_content)
    )
```

**How it works:**
1. Overlay covers the whole screen with a close handler
2. Popup sits inside the overlay with an empty `on_mouse_down` that blocks the close handler
3. Clicking outside the popup → hits overlay → closes
4. Clicking inside the popup → blocked by empty handler → stays open

## Pattern 4: Nested Buttons in Clickable Items

Use when: A list item is clickable, but contains action buttons (delete, edit, etc.) that should not trigger the item click.

```rust
div()
    .on_mouse_down(MouseButton::Left, cx.listener(|app, _event, _window, cx| {
        // Item click: select / navigate
        select_item();
        cx.notify();
    }))
    .child(content_div)
    .child(
        div()
            .on_mouse_down(MouseButton::Left, cx.listener(|app, _event, _window, cx| {
                cx.stop_propagation();  // Prevent item selection when clicking delete
                delete_item();
                cx.notify();
            }))
            .child(delete_icon)
    )
```

## Pattern 5: Escape Key to Close

Use when: A popup should close on Escape key press.

```rust
div()
    .on_key_down(cx.listener(|app, event: &KeyDownEvent, _window, cx| {
        if event.keystroke.key.as_str() == "escape" {
            app.show_popup = false;
            cx.notify();
        }
    }))
    .child(popup_content)
```

**Note:** For global Escape handling (works regardless of focus), add the handler at the root container level in `main_window.rs`.

## Quick Reference

| Problem | Solution | Code |
|---------|----------|------|
| Click passes through popup to elements behind | Empty handler on popup | `.on_mouse_down(MouseButton::Left, \|_, _, \| {})` |
| Child click triggers parent handler | Stop propagation in child | `cx.stop_propagation()` inside listener |
| Click outside to close popup | Overlay + inner block | Overlay close handler + inner empty handler |
| Nested button in clickable item | Stop propagation on button | `cx.stop_propagation()` on button handler |
| Escape to close | Key handler | `.on_key_down()` with escape check |

## Common Pitfalls

### Pitfall 1: `stop_propagation()` on the wrong thing
```rust
// WRONG - method on closure, doesn't compile
cx.listener(|..| { ... }).stop_propagation()

// CORRECT - method on Context, inside closure
cx.listener(|.., cx| { cx.stop_propagation(); ... })
```

### Pitfall 2: Forgetting the empty handler on popups
```rust
// WRONG - clicks on popup content bubble to elements behind
div().absolute().child(popup_content)

// CORRECT - empty handler blocks bubbling
div().absolute().on_mouse_down(MouseButton::Left, |_, _, _| {}).child(popup_content)
```

### Pitfall 3: Overlay without inner blocking
```rust
// WRONG - clicking popup content also closes it
div().absolute().inset_0()
    .on_mouse_down(cx.listener(|app, ..| { app.show = false; }))
    .child(popup)  // No blocking handler!

// CORRECT - popup has its own handler to block overlay's close
div().absolute().inset_0()
    .on_mouse_down(cx.listener(|app, ..| { app.show = false; }))
    .child(
        div().absolute()
            .on_mouse_down(MouseButton::Left, |_, _, _| {})  // Block!
            .child(popup)
    )
```
