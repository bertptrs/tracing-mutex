use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;

type Order = usize;

/// Directed Graph with dynamic topological sorting
///
/// Design and implementation based "A Dynamic Topological Sort Algorithm for Directed Acyclic
/// Graphs" by David J. Pearce and Paul H.J. Kelly which can be found on [the author's
/// website][paper].
///
/// Variable- and method names have been chosen to reflect most closely resemble the names in the
/// original paper.
///
/// This digraph tracks its own topological order and updates it when new edges are added to the
/// graph. If a cycle is added that would create a cycle, that edge is rejected and the graph is not
/// visibly changed.
///
/// [paper]: https://whileydave.com/publications/pk07_jea/
#[derive(Clone, Default, Debug)]
pub struct DiGraph<V>
where
    V: Eq + Hash + Copy,
{
    in_edges: HashMap<V, HashSet<V>>,
    out_edges: HashMap<V, HashSet<V>>,
    /// Next topological sort order
    next_ord: Order,
    /// Topological sort order. Order is not guaranteed to be contiguous
    ord: HashMap<V, Order>,
}

impl<V> DiGraph<V>
where
    V: Eq + Hash + Copy,
{
    /// Add a new node to the graph.
    ///
    /// If the node already existed, this function does not add it and uses the existing node data.
    /// The function returns mutable references to the in-edges, out-edges, and finally the index of
    /// the node in the topological order.
    ///
    /// New nodes are appended to the end of the topological order when added.
    fn add_node(&mut self, n: V) -> (&mut HashSet<V>, &mut HashSet<V>, Order) {
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

    pub(crate) fn remove_node(&mut self, n: V) -> bool {
        match self.out_edges.remove(&n) {
            None => false,
            Some(out_edges) => {
                for other in out_edges {
                    self.in_edges.get_mut(&other).unwrap().remove(&n);
                }

                for other in self.in_edges.remove(&n).unwrap() {
                    self.out_edges.get_mut(&other).unwrap().remove(&n);
                }

                true
            }
        }
    }

    /// Attempt to add an edge to the graph
    ///
    /// Nodes, both from and to, are created as needed when creating new edges. If the new edge
    /// would introduce a cycle, the edge is rejected and `false` is returned.
    pub(crate) fn add_edge(&mut self, x: V, y: V) -> bool {
        if x == y {
            // self-edges are not considered cycles
            return true;
        }

        let (_, out_edges, ub) = self.add_node(x);

        if !out_edges.insert(y) {
            // Edge already exists, nothing to be done
            return true;
        }

        let (in_edges, _, lb) = self.add_node(y);

        in_edges.insert(x);

        if lb < ub {
            // This edge might introduce a cycle, need to recompute the topological sort
            let mut visited = HashSet::new();
            let mut delta_f = Vec::new();
            let mut delta_b = Vec::new();

            if !self.dfs_f(y, ub, &mut visited, &mut delta_f) {
                // This edge introduces a cycle, so we want to reject it and remove it from the
                // graph again to keep the "does not contain cycles" invariant.

                // We use map instead of unwrap to avoid an `unwrap()` but we know that these
                // entries are present as we just added them above.
                self.in_edges.get_mut(&y).map(|nodes| nodes.remove(&x));
                self.out_edges.get_mut(&x).map(|nodes| nodes.remove(&y));

                // No edge was added
                return false;
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
    fn dfs_f(&self, n: V, ub: Order, visited: &mut HashSet<V>, delta_f: &mut Vec<V>) -> bool {
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
    fn dfs_b(&self, n: V, lb: Order, visited: &mut HashSet<V>, delta_b: &mut Vec<V>) {
        visited.insert(n);
        delta_b.push(n);

        for w in &self.in_edges[&n] {
            if !visited.contains(w) && lb < self.ord[w] {
                self.dfs_b(*w, lb, visited, delta_b);
            }
        }
    }

    fn reorder(&mut self, mut delta_f: Vec<V>, mut delta_b: Vec<V>) {
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

        // Original paper cleverly merges the two lists by using that both are sorted. We just sort
        // again. This is slower but also much simpler.
        orders.sort_unstable();

        for (node, order) in l.into_iter().zip(orders) {
            self.ord.insert(node, order);
        }
    }

    fn sort(&self, ids: &mut [V]) {
        // Can use unstable sort because mutex ids should not be equal
        ids.sort_unstable_by_key(|v| self.ord[v]);
    }
}

#[cfg(test)]
mod tests {
    use rand::prelude::SliceRandom;
    use rand::thread_rng;

    use super::*;

    #[test]
    fn test_digraph() {
        let mut graph = DiGraph::default();

        // Add some safe edges
        assert!(graph.add_edge(0, 1));
        assert!(graph.add_edge(1, 2));
        assert!(graph.add_edge(2, 3));
        assert!(graph.add_edge(4, 2));

        // Try to add an edge that introduces a cycle
        assert!(!graph.add_edge(3, 1));

        // Add an edge that should reorder 0 to be after 4
        assert!(graph.add_edge(4, 0));
    }

    /// Fuzz the DiGraph implementation by adding a bunch of valid edges.
    ///
    /// This test generates all possible forward edges in a 100-node graph consisting of natural
    /// numbers, shuffles them, then adds them to the graph. This will always be a valid directed,
    /// acyclic graph because there is a trivial order (the natural numbers) but because the edges
    /// are added in a random order the DiGraph will still occassionally need to reorder nodes.
    #[test]
    fn fuzz_digraph() {
        // Note: this fuzzer is quadratic in the number of nodes, so this cannot be too large or it
        // will slow down the tests too much.
        const NUM_NODES: usize = 100;
        let mut edges = Vec::with_capacity(NUM_NODES * NUM_NODES);

        for i in 0..NUM_NODES {
            for j in i..NUM_NODES {
                edges.push((i, j));
            }
        }

        edges.shuffle(&mut thread_rng());

        let mut graph = DiGraph::default();

        for (x, y) in edges {
            assert!(graph.add_edge(x, y));
        }
    }
}
