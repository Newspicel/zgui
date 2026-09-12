//! The store as the scheduler sees it between passes.
//!
//! Marking a box dirty asks the scheduler for its depth, its parent and its flags, and for
//! nothing that lays anything out. This view answers those from the store alone, so a mark can be
//! taken by anything holding the store — a box-tree patch, a gutter revision — without a pass
//! being open. The layout methods are unreachable by construction: a marker never computes.

use cephal::compute::block::BlockContext;
use cephal::schedule::Incremental;
use cephal::tree::{CacheAccess, LayoutInput, LayoutOutput, LayoutTree, NodeCache, NodeId};
use cephal::{Layout, Style};

use crate::key::{from_node_id, to_node_id};
use crate::node::children::ChildIter;
use crate::tree::store::LayoutStore;

/// A borrow of the store that the scheduler can mark through.
pub(crate) struct Marker<'a>(pub(crate) &'a mut LayoutStore);

impl LayoutTree for Marker<'_> {
    type ChildIter<'b>
        = ChildIter<'b>
    where
        Self: 'b;

    fn children(&self, node: NodeId) -> Self::ChildIter<'_> {
        ChildIter::new(&self.0.node(from_node_id(node)).children)
    }

    fn child_count(&self, node: NodeId) -> usize {
        self.0.node(from_node_id(node)).children.len()
    }

    fn child_at(&self, node: NodeId, index: usize) -> NodeId {
        to_node_id(self.0.node(from_node_id(node)).children[index])
    }

    fn style(&self, node: NodeId) -> &Style {
        self.0.structure().engine_style(from_node_id(node))
    }

    fn set_unrounded_layout(&mut self, _: NodeId, _: &Layout) {
        unreachable!("a marker never lays out")
    }

    fn compute_child_layout(
        &mut self,
        _: NodeId,
        _: LayoutInput,
        _: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        unreachable!("a marker never lays out")
    }
}

impl CacheAccess for Marker<'_> {
    fn cache(&self, node: NodeId) -> &NodeCache {
        &self
            .0
            .state(from_node_id(node))
            .expect("a queued box holds layout state")
            .cache
    }

    fn cache_mut(&mut self, node: NodeId) -> &mut NodeCache {
        &mut self.0.state_mut(from_node_id(node)).cache
    }

    fn generation(&self) -> u32 {
        self.0.generation
    }

    fn speculation(&self) -> u32 {
        0
    }

    fn set_speculation(&mut self, _: u32) {}
}

impl Incremental for Marker<'_> {
    fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.0.get(from_node_id(node))?.parent.map(to_node_id)
    }

    fn depth(&self, node: NodeId) -> u32 {
        depth_of(self.0, from_node_id(node))
    }

    fn flags(&self, node: NodeId) -> u8 {
        self.0
            .state(from_node_id(node))
            .map_or(0, |state| state.flags)
    }

    fn set_flags(&mut self, node: NodeId, flags: u8) {
        let key = from_node_id(node);
        if self.0.state(key).is_some() {
            self.0.state_mut(key).flags = flags;
        }
    }
}

/// How many boxes lie between `key` and the root, the root being at depth zero.
pub(crate) fn depth_of(store: &LayoutStore, key: zgui_dom::side::BoxKey) -> u32 {
    let mut depth = 0;
    let mut at = store.get(key).and_then(|node| node.parent);
    while let Some(parent) = at {
        depth += 1;
        at = store.get(parent).and_then(|node| node.parent);
    }
    depth
}
