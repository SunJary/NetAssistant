// 回复规则引擎（L1 纯逻辑 + L2 执行器）—— 见 docs/plan-reply-rules.md
//
// 模块地图：
//   model.rs    数据模型（ReplyRule / MatchNode / ReplyPayload…）+ 保存期校验 + 条件摘要
//   frame.rs    RxFrame 帧快照 + RxContext 取值层（`rx.*` 的唯一语义来源）
//   matcher.rs  evaluate() 纯函数匹配器 + MatchTrace 匹配轨迹
//   store.rs    ReplyRulesStore 运行期状态（无锁快速路径 + 原子命中计数）
//   exec.rs     handle_frame() / render_reply()：副作用在此收口
//   expr.rs     单表达式引擎（`${= ... }`）
//
// **本模块不 `use gpui`**：规则模型、匹配器、求值器全部可无头单测。
// 网络层只调用 L1 的纯函数，UI 只渲染与交互。

pub mod exec;
pub mod expr;
pub mod frame;
pub mod matcher;
pub mod model;
pub mod store;

/// 精简的再导出面：只暴露"跨模块确实会用到"的少数类型。
///
/// 其余类型请走完整路径（`crate::reply::model::MatchNode` 等）—— 引擎模块内部
/// 类型数量多（15 种 `MatchNode` 变体 + 动作 + 约束 + 轨迹…），
/// 全量再导出会让 `use crate::reply::*` 的读者无法分辨某个名字来自哪一层。
pub use frame::{FrameMeta, FrameOrigin, RxFrame};
pub use store::ReplyRulesStore;
