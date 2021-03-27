use std::collections::HashMap;
use std::collections::HashSet;

use crate::MutexId;

type Order = usize;

/// Directed Graph with dynamic topological sorting
///
/// Design and implementation based "A Dynamic Topological Sort Algorithm for
/// Directed Acyclic Graphs" by David J. Pearce and Paul H.J. Kelly which can
/// be found on [the author's website][paper].
///
/// Variable- and method names have been chosen to reflect most closely
/// resemble the names in the original paper.
///
/// This digraph tracks its own topological order and updates it when new edges
/// are added to the graph. After a cycle has been introduced, the order is no
/// longer kept up to date as it doesn't exist, but new edges are still
/// tracked. Nodes are added implicitly when they're used in edges.
///
/// [paper]: https://whileydave.com/publications/pk07_jea/
#[derive(Clone, Default, Debug)]
pub struct DiGraph {
    in_edges: HashMap<MutexId, Vec<MutexId>>,
    out_edges: HashMap<MutexId, Vec<MutexId>>,
    /// Next topological sort order
    next_ord: Order,
    /// Poison flag, set if a cycle is detected when adding a new edge and
    /// unset when removing a node successfully removed the cycle.
    contains_cycle: bool,
    /// Topological sort order. Order is not guaranteed to be contiguous
    ord: HashMap<MutexId, Order>,
}

impl DiGraph {
    /// Add a new node to the graph.
    ///
    /// If the node already existed, this function does not add it and uses the
    /// existing node data. The function returns mutable references to the
    /// in-edges, out-edges, and finally the index of the node in the topological
    /// order.
    ///
    /// New nodes are appended to the end of the topological order when added.
    fn add_node(&mut self, n: MutexId) -> (&mut Vec<MutexId>, &mut Vec<MutexId>, Order) {
        let next_ord = &mut self.next_ord;
        let in_edges = self.in_edges.entry(n).or_default();
        let out_edges = self.out_edges.entry(n).or_default();

        let order = *self.ord.entry(n).or_insert_with(|| {
            let order = *next_ord;
            *next_ord += next_ord.checked_add(1).expect("Topological order overflow");
            order
        });

        (in_edges, out_edges, order)
    }

    pub(crate) fn remove_node(&mut self, n: MutexId) -> bool {
        match self.out_edges.remove(&n) {
            None => false,
            Some(out_edges) => {
                for other in out_edges {
                    self.in_edges.get_mut(&other).unwrap().retain(|c| c != &n);
                }

                for other in self.in_edges.remove(&n).unwrap() {
                    self.out_edges.get_mut(&other).unwrap().retain(|c| c != &n);
                }

                if self.contains_cycle {
                    // Need to build a valid topological order
                    self.recompute_topological_order();
                }

                true
            }
        }
    }

    /// Add an edge to the graph
    ///
    /// Nodes, both from and to, are created as needed when creating new edges.
    pub(crate) fn add_edge(&mut self, x: MutexId, y: MutexId) -> bool {
        if x == y {
            // self-edges are not considered cycles
            return false;
        }

        let (_, out_edges, ub) = self.add_node(x);

        if out_edges.contains(&y) {
            // Edge already exists, nothing to be done
            return false;
        }

        out_edges.push(y);

        let (in_edges, _, lb) = self.add_node(y);

        in_edges.push(x);

        if !self.contains_cycle && lb < ub {
            // This edge might introduce a cycle, need to recompute the topological sort
            let mut visited = HashSet::new();
            let mut delta_f = Vec::new();
            let mut delta_b = Vec::new();

            if !self.dfs_f(y, ub, &mut visited, &mut delta_f) {
                self.contains_cycle = true;
                return true;
            }

            // No need to check as we should've found the cycle on the forward pass
            self.dfs_b(x, lb, &mut visited, &mut delta_b);

            // Original paper keeps it around but this saves us from clearing it
            drop(visited);

            self.reorder(delta_f, delta_b);
        }

        true
    }

    /// Forwards depth-first-search
    fn dfs_f(
        &self,
        n: MutexId,
        ub: Order,
        visited: &mut HashSet<MutexId>,
        delta_f: &mut Vec<MutexId>,
    ) -> bool {
        visited.insert(n);
        delta_f.push(n);

        self.out_edges[&n].iter().all(|w| {
            let order = self.ord[w];

            if order == ub {
                // Found a cycle
                false
            } else if !visited.contains(w) && order < ub {
                // Need to check recursively
                self.dfs_f(*w, ub, visited, delta_f)
            } else {
                // Already seen this one or not interesting
                true
            }
        })
    }

    /// Backwards depth-first-search
    fn dfs_b(
        &self,
        n: MutexId,
        lb: Order,
        visited: &mut HashSet<MutexId>,
        delta_b: &mut Vec<MutexId>,
    ) {
        visited.insert(n);
        delta_b.push(n);

        for w in &self.in_edges[&n] {
            if !visited.contains(w) && lb < self.ord[w] {
                self.dfs_b(*w, lb, visited, delta_b);
            }
        }
    }

    fn reorder(&mut self, mut delta_f: Vec<MutexId>, mut delta_b: Vec<MutexId>) {
        self.sort(&mut delta_f);
        self.sort(&mut delta_b);

        let mut l = Vec::with_capacity(delta_f.len() + delta_b.len());
        let mut orders = Vec::with_capacity(delta_f.len() + delta_b.len());

        for w in delta_b {
            orders.push(self.ord[&w]);
            l.push(w);
        }

        for v in delta_f {
            orders.push(self.ord[&v]);
            l.push(v);
        }

        // Original paper cleverly merges the two lists by using that both are
        // sorted. We just sort again. This is slower but also much simpler.
        orders.sort_unstable();

        for (node, order) in l.into_iter().zip(orders) {
            self.ord.insert(node, order);
        }
    }

    fn sort(&self, ids: &mut [MutexId]) {
        // Can use unstable sort because mutex ids should not be equal
        ids.sort_unstable_by_key(|v| self.ord[v]);
    }

    pub fn has_cycles(&self) -> bool {
        self.contains_cycle
    }

    /// Attempt to recompute a valid topological order.
    ///
    /// This method implements the DFS method to find leave nodes to find the reverse order
    fn recompute_topological_order(&mut self) {
        // This function should only be called when the graph contains a cycle.
        debug_assert!(self.contains_cycle);

        let mut permanent_marks = HashSet::with_capacity(self.out_edges.len());
        let mut temporary_marks = HashSet::new();
        let mut rev_order = Vec::with_capacity(self.out_edges.len());

        for node in self.out_edges.keys() {
            if permanent_marks.contains(node) {
                continue;
            }

            if !self.visit(
                *node,
                &mut permanent_marks,
                &mut temporary_marks,
                &mut rev_order,
            ) {
                // Cycle found, no order possible
                return;
            }
        }

        // We didn't find a cycle, so we can reset
        self.contains_cycle = false;
        // Newly allocated order is contiguous 0..rev_order.len()
        self.next_ord = rev_order.len();

        self.ord.clear();
        self.ord
            .extend(rev_order.into_iter().rev().enumerate().map(|(k, v)| (v, k)))
    }

    /// Helper function for `Self::recompute_topological_order`.
    fn visit(
        &self,
        v: MutexId,
        permanent_marks: &mut HashSet<MutexId>,
        temporary_marks: &mut HashSet<MutexId>,
        rev_order: &mut Vec<MutexId>,
    ) -> bool {
        if permanent_marks.contains(&v) {
            return true;
        }

        if !temporary_marks.insert(v) {
            return false;
        }

        if !self.out_edges[&v]
            .iter()
            .all(|&w| self.visit(w, permanent_marks, temporary_marks, rev_order))
        {
            return false;
        }

        temporary_marks.remove(&v);
        permanent_marks.insert(v);

        rev_order.push(v);

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MutexId;

    #[test]
    fn test_digraph() {
        let id: Vec<MutexId> = (0..5).map(|_| MutexId::new()).collect();
        let mut graph = DiGraph::default();

        // Add some safe edges
        graph.add_edge(id[0], id[1]);
        graph.add_edge(id[1], id[2]);
        graph.add_edge(id[2], id[3]);
        graph.add_edge(id[4], id[2]);

        // Should not have a cycle yet
        assert!(!graph.has_cycles());

        // Introduce cycle 3 → 1 → 2 → 3
        graph.add_edge(id[3], id[1]);
        assert!(graph.has_cycles());

        // Removing 3 should remove that cycle
        assert!(graph.remove_node(id[3]));
        assert!(!graph.has_cycles())
    }
}
