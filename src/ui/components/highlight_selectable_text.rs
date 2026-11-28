//! 可高亮的可选择文本。
//!
//! 复制自 `gpui-base 0.8.0` 的 `SelectableText`（Apache-2.0），仅增加 `highlights`
//! 构造能力：把搜索命中的字节区间以背景色 run 的形式交给 `StyledText`。
//! 选区绘制、`TextSelectionHandle` 注册与绘制顺序全部保持原样，因此拖选、双击选词、
//! Ctrl/Cmd+C 等既有交互不受影响。
//!
//! 维护提示：gpui-kit 升级时需对照 `gpui-base/src/selectable_text.rs` 同步本文件。

use std::ops::Range;

use gpui_kit::base::{
    TextSelection, TextSelectionHandle, TextSelectionRegistration, TextSelectionRun,
};
use gpui_kit::{
    App, BorderStyle, Bounds, Corners, Edges, Element, ElementId, GlobalElementId, HighlightStyle,
    Hitbox, HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, PaintQuad, Pixels,
    Point, SharedString, StyledText, TextStyleRefinement, Window, transparent_black,
};

/// Plain text that participates in the window-scoped [`TextSelection`] and can
/// additionally paint background highlights over arbitrary byte ranges.
pub struct HighlightSelectableText {
    id: ElementId,
    handle: Option<TextSelectionHandle>,
    text: SharedString,
    styled_text: StyledText,
    document_order: u64,
    text_style: Option<TextStyleRefinement>,
    selection_color: Option<Hsla>,
}

// 复制上游实现时保留了完整构造能力（with_handle / document_order / text_style），
// 当前项目只用其子集，避免为未用方法报 dead_code。
#[allow(dead_code)]
impl HighlightSelectableText {
    /// Creates a run that owns its own selection.
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self::build(id.into(), None, text.into())
    }

    /// Creates a run that joins the document `handle` belongs to.
    pub fn with_handle(
        id: impl Into<ElementId>,
        handle: TextSelectionHandle,
        text: impl Into<SharedString>,
    ) -> Self {
        Self::build(id.into(), Some(handle), text.into())
    }

    fn build(id: ElementId, handle: Option<TextSelectionHandle>, text: SharedString) -> Self {
        Self {
            id,
            handle,
            styled_text: StyledText::new(text.clone()),
            text,
            document_order: 0,
            text_style: None,
            selection_color: None,
        }
    }

    /// Sets the highlight runs. Ranges must be sorted, non-overlapping and on
    /// UTF-8 char boundaries; empty ranges should be filtered by the caller.
    pub fn highlights(
        mut self,
        highlights: impl IntoIterator<Item = (Range<usize>, HighlightStyle)>,
    ) -> Self {
        let highlights: Vec<(Range<usize>, HighlightStyle)> = highlights
            .into_iter()
            .filter(|(r, _)| !r.is_empty())
            .collect();
        if !highlights.is_empty() {
            self.styled_text = StyledText::new(self.text.clone()).with_highlights(highlights);
        }
        self
    }

    /// Places this run in reading order among the others sharing its handle.
    pub fn document_order(mut self, order: u64) -> Self {
        self.document_order = order;
        self
    }

    /// Sets the text style the run is laid out and painted with.
    pub fn text_style(mut self, style: TextStyleRefinement) -> Self {
        self.text_style = Some(style);
        self
    }

    /// Overrides the selection background, which defaults to the theme's
    /// `colors.selection` token.
    pub fn selection_color(mut self, color: Hsla) -> Self {
        self.selection_color = Some(color);
        self
    }

    fn paint_selection(
        layout: &gpui_kit::TextLayout,
        range: Range<usize>,
        color: Hsla,
        window: &mut Window,
    ) {
        let (Some(start), Some(end)) = (
            layout.position_for_index(range.start),
            layout.position_for_index(range.end),
        ) else {
            return;
        };
        for bounds in selection_quad_bounds(start, end, layout.bounds(), layout.line_height()) {
            window.paint_quad(PaintQuad {
                bounds,
                background: color.into(),
                corner_radii: Corners::default(),
                border_widths: Edges::default(),
                border_color: transparent_black(),
                border_style: BorderStyle::default(),
            });
        }
    }
}

fn selection_quad_bounds(
    start: Point<Pixels>,
    end: Point<Pixels>,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
) -> Vec<Bounds<Pixels>> {
    if start.y == end.y {
        return vec![Bounds::from_corners(
            start,
            Point::new(end.x, end.y + line_height),
        )];
    }

    let mut quads = vec![Bounds::from_corners(
        start,
        Point::new(bounds.right(), start.y + line_height),
    )];
    if end.y > start.y + line_height {
        quads.push(Bounds::from_corners(
            Point::new(bounds.left(), start.y + line_height),
            Point::new(bounds.right(), end.y),
        ));
    }
    quads.push(Bounds::from_corners(
        Point::new(bounds.left(), end.y),
        Point::new(end.x, end.y + line_height),
    ));
    quads
}

impl IntoElement for HighlightSelectableText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for HighlightSelectableText {
    type RequestLayoutState = TextSelectionHandle;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let handle = self.handle.clone().unwrap_or_else(|| {
            window.with_element_state(
                global_id.expect("HighlightSelectableText must have a stable element id"),
                |retained: Option<TextSelectionHandle>, _| {
                    let handle =
                        retained.unwrap_or_else(|| TextSelectionHandle::new(self.text.clone(), cx));
                    (handle.clone(), handle)
                },
            )
        });
        let (layout_id, ()) = if let Some(style) = self.text_style.clone() {
            window.with_text_style(Some(style), |window| {
                self.styled_text
                    .request_layout(global_id, inspector_id, window, cx)
            })
        } else {
            self.styled_text
                .request_layout(global_id, inspector_id, window, cx)
        };
        (layout_id, handle)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled_text
            .prepaint(global_id, inspector_id, bounds, &mut (), window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let registration = TextSelectionRegistration::new(hitbox.clone(), bounds)
            .with_document_order(self.document_order)
            .with_text_bounds(vec![bounds])
            .with_rendered_element(handle, window, cx);
        handle.register(registration, window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.styled_text.layout().clone();
        let selected_text_before = TextSelection::selected_text(window, cx);
        let projection = handle.update_runs(
            &[
                TextSelectionRun::new(self.text.clone(), layout.clone(), bounds)
                    .with_document_order(self.document_order),
            ],
            cx,
        );
        if selected_text_before != TextSelection::selected_text(window, cx) {
            window.refresh();
        }
        let color = self
            .selection_color
            .unwrap_or_else(|| gpui_kit::base::Theme::global(cx).tokens.colors.selection);
        for range in projection.ranges().iter().flatten().cloned() {
            Self::paint_selection(&layout, range, color, window);
        }
        self.styled_text.paint(
            global_id,
            inspector_id,
            bounds,
            &mut (),
            &mut (),
            window,
            cx,
        );
    }
}
