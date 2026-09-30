// Licensed under Apache-2.0. Copyright Darmok contributors.
//! Iterative resource validation and disposal, independent of AST recursion.

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, string::String, vec, vec::Vec};

/// Owned node; exported for generated implementations, including no_std builds.
pub type NodeBox<T> = Box<T>;
/// Work list for iterative destruction. No recursive AST remains in a disposed node.
pub type NodeQueue = Vec<Box<dyn AstNode>>;

/// Exhaustive structural access generated together with `Visit`.
/// Adding a field without structural support is a compile error.
pub trait AstNode: 'static {
    /// Report immediate fields only; the caller controls traversal.
    fn children<'a>(&'a self, children: &mut dyn FnMut(&'a dyn AstNode));
    /// Move immediate fields onto the work list and drop the now-empty parent.
    fn dismantle(self: Box<Self>, pending: &mut NodeQueue);
}

/// Generated code uses this function to avoid importing allocator types.
#[doc(hidden)]
pub fn enqueue_node<T: AstNode>(node: T, pending: &mut NodeQueue) {
    pending.push(Box::new(node));
}

/// Discard a possibly deep tree without recursively invoking its destructors.
/// Used for rejected input and unchecked rewrites, not the normal admitted path.
pub fn drop_ast<T: AstNode>(node: T) {
    let mut pending: NodeQueue = vec![Box::new(node)];
    while let Some(node) = pending.pop() {
        node.dismantle(&mut pending);
    }
}

/// Limits include container and metadata nodes, consistently across AST types.
#[derive(Debug, Clone, Copy)]
pub struct AstLimits {
    /// Root depth is one. Zero admits no tree.
    pub max_depth: usize,
    /// Total visited nodes. Zero admits no tree.
    pub max_nodes: usize,
}

/// The structural resource limit exceeded by a tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstLimitExceeded {
    /// A root-to-leaf path is too deep.
    Depth,
    /// Too many total AST fields and containers.
    Nodes,
}

/// Check without recursively visiting, cloning, formatting or dropping the AST.
pub fn check_ast(node: &dyn AstNode, limits: AstLimits) -> Result<(), AstLimitExceeded> {
    let mut pending = vec![(node, 1usize)];
    let mut nodes = 0usize;
    while let Some((node, depth)) = pending.pop() {
        if depth > limits.max_depth {
            return Err(AstLimitExceeded::Depth);
        }
        if nodes == limits.max_nodes {
            return Err(AstLimitExceeded::Nodes);
        }
        nodes += 1;
        // A node's fan-out can be large; stop queueing once the remaining node
        // budget is exhausted instead of allocating an unbounded work list.
        let mut over_limit = false;
        node.children(&mut |child| {
            if pending.len() >= limits.max_nodes - nodes {
                over_limit = true;
            } else {
                pending.push((child, depth.saturating_add(1)));
            }
        });
        if over_limit {
            return Err(AstLimitExceeded::Nodes);
        }
    }
    Ok(())
}

impl<T: AstNode> AstNode for Box<T> {
    fn children<'a>(&'a self, children: &mut dyn FnMut(&'a dyn AstNode)) {
        children(self.as_ref());
    }
    fn dismantle(self: Box<Self>, pending: &mut NodeQueue) {
        pending.push(*self);
    }
}

impl<T: AstNode> AstNode for Option<T> {
    fn children<'a>(&'a self, children: &mut dyn FnMut(&'a dyn AstNode)) {
        if let Some(node) = self {
            children(node);
        }
    }
    fn dismantle(self: Box<Self>, pending: &mut NodeQueue) {
        if let Some(node) = *self {
            enqueue_node(node, pending);
        }
    }
}

impl<T: AstNode> AstNode for Vec<T> {
    fn children<'a>(&'a self, children: &mut dyn FnMut(&'a dyn AstNode)) {
        for node in self {
            children(node);
        }
    }
    fn dismantle(self: Box<Self>, pending: &mut NodeQueue) {
        for node in *self {
            enqueue_node(node, pending);
        }
    }
}

macro_rules! leaves {
    ($($t:ty),+) => { $(impl AstNode for $t {
        fn children<'a>(&'a self, _: &mut dyn FnMut(&'a dyn AstNode)) {}
        fn dismantle(self: Box<Self>, _: &mut NodeQueue) {}
    })+ };
}
leaves!(u8, u16, u32, u64, i8, i16, i32, i64, char, bool, String);
#[cfg(feature = "bigdecimal")]
leaves!(bigdecimal::BigDecimal);
