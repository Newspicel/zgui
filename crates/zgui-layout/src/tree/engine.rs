//! The layout tree as the engine reads it.
//!
//! Three traits: [`LayoutTree`] answers structure, styles and leaf measurements, [`CacheAccess`]
//! hands out each box's cache, and [`Incremental`] is what the scheduler marks through. The engine
//! runs its algorithms against these and nothing else, so what a box *is* to it — a block, a flex
//! container, a leaf — is decided by the engine style the store interned for the box.

use cephal::compute::block::BlockContext;
use cephal::schedule::Incremental;
use cephal::tree::{
    CacheAccess, ChildRequest, IdentSuffix, LayoutInput, LayoutOutput, LayoutTree as EngineTree,
    NodeCache, NodeId, RunMode,
};
use cephal::{Layout, Style};
use zgui_dom::side::BoxKey;
use zgui_profile::{Counter, counter};

use crate::key::{from_node_id, to_node_id};
use crate::measure::MeasureContent;
use crate::node::children::ChildIter;
use crate::node::kind::FormattingContext;
use crate::tree::LayoutTree;

/// Whether the engine may descend into a box's children.
///
/// A box that holds lines, a replaced box and a custom element are leaves to the engine: what is
/// inside them is laid out by the inline layer, the content, or the element's own source, and the
/// engine's algorithms must never see it as a list of blocks.
fn engine_sees_children(fc: FormattingContext) -> bool {
    !matches!(
        fc,
        FormattingContext::Inline
            | FormattingContext::Replaced
            | FormattingContext::Custom
            | FormattingContext::None
    )
}

impl<C: MeasureContent> EngineTree for LayoutTree<'_, C> {
    type ChildIter<'b>
        = ChildIter<'b>
    where
        Self: 'b;

    fn children(&self, node: NodeId) -> Self::ChildIter<'_> {
        let node = self.node_of(node);
        if engine_sees_children(node.fc) {
            ChildIter::new(&node.children)
        } else {
            ChildIter::new(&[])
        }
    }

    fn child_count(&self, node: NodeId) -> usize {
        let node = self.node_of(node);
        if engine_sees_children(node.fc) {
            node.children.len()
        } else {
            0
        }
    }

    fn child_at(&self, node: NodeId, index: usize) -> NodeId {
        to_node_id(self.node_of(node).children[index])
    }

    fn style(&self, node: NodeId) -> &Style {
        self.engine_style(from_node_id(node))
    }

    fn resolve_calc(&self, id: cephal::style::CalcId, basis: f32) -> f32 {
        LayoutTree::resolve_calc(self, id, basis)
    }

    fn suffixed_ident(
        &self,
        base: cephal::style::Ident,
        suffix: IdentSuffix,
    ) -> Option<cephal::style::Ident> {
        self.structure().suffixed_ident(base, suffix)
    }

    fn leaf_is_cheap(&self, node: NodeId) -> bool {
        // An empty block-level box measures nothing: its answer is its own padding and border.
        // Everything else that reaches the leaf path has content to ask, and the asking is
        // exactly what the cache exists to avoid.
        let node = self.node_of(node);
        matches!(
            node.fc,
            FormattingContext::Block | FormattingContext::Flex | FormattingContext::Grid
        ) && node.children.is_empty()
    }

    fn compute_leaf(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        crate::inline::leaf_shell::compute(self, node, inputs, block_ctx)
    }

    fn set_unrounded_layout(&mut self, node: NodeId, layout: &Layout) {
        if self.speculation > 0 {
            return;
        }
        let key = from_node_id(node);
        let generation = self.generation;
        let state = self.state_mut(key);
        if state.unrounded != *layout {
            if state.cache_generation() != generation {
                let before = state.unrounded;
                state.note_touched(generation);
                self.touched.push((key, before));
            }
            let state = self.state_mut(key);
            state.unrounded = *layout;
            // Until the snapping pass runs, the snapped result is the unrounded one, so a caller
            // that reads a layout between the two passes reads geometry rather than nothing.
            state.snapped = *layout;
        }
    }

    fn unrounded_layout(&self, node: NodeId) -> Option<Layout> {
        self.state(from_node_id(node)).map(|state| state.unrounded)
    }

    fn set_absolute_context(&mut self, node: NodeId, context: cephal::tree::AbsoluteContext) {
        if self.speculation > 0 {
            return;
        }
        self.state_mut(from_node_id(node)).absolute = Some(Box::new(context));
    }

    fn absolute_context(&self, node: NodeId) -> Option<cephal::tree::AbsoluteContext> {
        self.state(from_node_id(node))?.absolute.as_deref().cloned()
    }

    fn compute_child_layout(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        cephal::compute::compute_child_layout(self, node, inputs, block_ctx)
    }

    fn after_layout(
        &mut self,
        node: NodeId,
        inputs: &LayoutInput,
        output: &mut LayoutOutput,
        cached: bool,
    ) {
        observe(self, node, inputs, output, cached);
    }

    fn batches_help(&self) -> bool {
        self.batch_pool().is_some()
    }

    fn compute_child_layouts(&mut self, requests: &[ChildRequest], out: &mut Vec<LayoutOutput>) {
        self.run_batch(requests, out);
    }
}

impl<C> CacheAccess for LayoutTree<'_, C> {
    fn cache(&self, node: NodeId) -> &NodeCache {
        &self
            .state(from_node_id(node))
            .expect("every box the engine asks about holds layout state")
            .cache
    }

    fn cache_mut(&mut self, node: NodeId) -> &mut NodeCache {
        &mut self.state_mut(from_node_id(node)).cache
    }

    fn generation(&self) -> u32 {
        self.generation
    }

    fn speculation(&self) -> u32 {
        self.speculation
    }

    fn set_speculation(&mut self, depth: u32) {
        self.speculation = depth;
    }
}

impl<C: MeasureContent> Incremental for LayoutTree<'_, C> {
    fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.structure()
            .get(from_node_id(node))?
            .parent
            .map(to_node_id)
    }

    fn depth(&self, node: NodeId) -> u32 {
        // A box recycled since it was queued has no record and sits at the root's depth.
        let structure = self.structure();
        let mut depth = 0;
        let mut at = structure
            .get(from_node_id(node))
            .and_then(|node| node.parent);
        while let Some(parent) = at {
            depth += 1;
            at = structure.get(parent).and_then(|node| node.parent);
        }
        depth
    }

    fn flags(&self, node: NodeId) -> u8 {
        self.state(from_node_id(node))
            .map_or(0, |state| state.flags)
    }

    fn set_flags(&mut self, node: NodeId, flags: u8) {
        // A box removed since it was queued has no state; the scheduler skips it next.
        let key = from_node_id(node);
        if self.state(key).is_some() {
            self.state_mut(key).flags = flags;
        }
    }
}

/// Accounts for one answer and records the baselines it carries, on every answer the engine
/// hands out.
///
/// The baselines the algorithms do not report come from the content, and both are recorded on
/// the box — on every answer, because it may have come from the cache without any algorithm
/// running. A box whose baselines were recorded by one pass and served from the cache in the
/// next would otherwise keep whatever the first pass happened to see.
fn observe<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    node: NodeId,
    inputs: &LayoutInput,
    output: &mut LayoutOutput,
    cached: bool,
) {
    match (inputs.run_mode, cached) {
        (RunMode::PerformLayout, false) => counter::bump(Counter::NodesRelaidOut),
        (RunMode::ComputeSize, false) => counter::bump(Counter::SizesMeasured),
        (RunMode::ComputeSize, true) => counter::bump(Counter::SizesHeld),
        _ => {}
    }
    let key = from_node_id(node);
    let fc = tree.node_of(node).fc;
    let first = output
        .baselines
        .first
        .or_else(|| first_baseline_of_content(tree, key, fc));
    // A leaf reports its own last baseline. A container's is its last in-flow child's, which the
    // algorithms do not compute: they hand the first back as the last.
    let leaf = !engine_sees_children(fc);
    let last = leaf
        .then_some(output.baselines.last)
        .flatten()
        .or_else(|| last_baseline_of_content(tree, key, fc))
        .or(first);
    output.baselines.first = first;
    output.baselines.last = last;
    let state = tree.state_mut(key);
    state.first_baseline = first;
    state.last_baseline = last;
}

/// The inner formatting context of a box, an atomic inline's being the one its display names.
fn inner_context<C: MeasureContent>(
    tree: &LayoutTree<'_, C>,
    key: BoxKey,
    fc: FormattingContext,
) -> FormattingContext {
    if fc != FormattingContext::Atomic {
        return fc;
    }
    crate::style::convert::display::atomic_inner(tree.structure().node(key).style.get_box().display)
}

fn first_baseline_of_content<C: MeasureContent>(
    tree: &LayoutTree<'_, C>,
    key: BoxKey,
    fc: FormattingContext,
) -> Option<f32> {
    if inner_context(tree, key, fc) != FormattingContext::Block {
        return None;
    }
    let structure = tree.structure();
    structure.node(key).children.iter().find_map(|&child| {
        if !is_in_flow(tree, child) {
            return None;
        }
        let state = tree.state(child)?;
        let baseline = state.first_baseline?;
        Some(baseline + state.unrounded.location.y)
    })
}

/// Whether a box takes part in its parent's flow, as opposed to floating or being positioned out.
fn is_in_flow<C: MeasureContent>(tree: &LayoutTree<'_, C>, child: BoxKey) -> bool {
    let style = tree.engine_style(child);
    style.float == cephal::style::Float::None && style.position != cephal::style::Position::Absolute
}

fn last_baseline_of_content<C: MeasureContent>(
    tree: &LayoutTree<'_, C>,
    key: BoxKey,
    fc: FormattingContext,
) -> Option<f32> {
    if fc == FormattingContext::Inline {
        // A box that holds lines has a last baseline of its own, and it is not the same line as
        // its first whenever the content wrapped.
        let state = tree.state(key)?;
        let inset = state.unrounded.border.top + state.unrounded.padding.top;
        return tree
            .inline_resolution_of(key)
            .and_then(crate::inline::resolved::InlineResolution::last_baseline)
            .map(|baseline| baseline + inset);
    }
    if !inner_context(tree, key, fc).is_container() {
        return None;
    }
    let structure = tree.structure();
    structure
        .node(key)
        .children
        .iter()
        .rev()
        .find_map(|&child| {
            if !is_in_flow(tree, child) {
                return None;
            }
            let state = tree.state(child)?;
            let baseline = state.last_baseline?;
            Some(baseline + state.unrounded.location.y)
        })
}
