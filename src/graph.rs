use std::cell::Cell;
use std::collections::hash_map::Entry;
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
#[derive(Debug)]
pub struct DiGraph<V, E>
where
    V: Eq + Hash + Copy,
{
    nodes: HashMap<V, Node<V, E>>,
    // Instead of reordering the orders in the graph whenever a node is deleted, we maintain a list
    // of unused ids that can be handed out later again.
    unused_order: Vec<Order>,
}

#[derive(Debug)]
struct Node<V, E>
where
    V: Eq + Hash + Clone,
{
    in_edges: HashSet<V>,
    out_edges: HashMap<V, E>,
    // The "Ord" field is a Cell to ensure we can update it in an immutable context.
    // `std::collections::HashMap` doesn't let you have multiple mutable references to elements, but
    // this way we can use immutable references and still update `ord`. This saves quite a few
    // hashmap lookups in the final reorder function.
    ord: Cell<Order>,
}

impl<V, E> DiGraph<V, E>
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
    fn add_node(&mut self, n: V) -> (&mut HashSet<V>, &mut HashMap<V, E>, Order) {
        // need to compute next id before the call to entry() to avoid duplicate borrow of nodes
        let fallback_id = self.nodes.len();

        let node = self.nodes.entry(n).or_insert_with(|| {
            let order = if let Some(id) = self.unused_order.pop() {
                // Reuse discarded ordering entry
                id
            } else {
                // Allocate new order id
                fallback_id
            };

            Node {
                ord: Cell::new(order),
                in_edges: Default::default(),
                out_edges: Default::default(),
            }
        });

        (&mut node.in_edges, &mut node.out_edges, node.ord.get())
    }

    pub(crate) fn remove_node(&mut self, n: V) -> bool {
        match self.nodes.remove(&n) {
            None => false,
            Some(Node {
                out_edges,
                in_edges,
                ord,
            }) => {
                // Return ordering to the pool of unused ones
                self.unused_order.push(ord.get());

                out_edges.into_keys().for_each(|m| {
                    self.nodes.get_mut(&m).unwrap().in_edges.remove(&n);
                });

                in_edges.into_iter().for_each(|m| {
                    self.nodes.get_mut(&m).unwrap().out_edges.remove(&n);
                });

                true
            }
        }
    }

    /// Attempt to add an edge to the graph
    ///
    /// Nodes, both from and to, are created as needed when creating new edges. If the new edge
    /// would introduce a cycle, the edge is rejected and `false` is returned.
    ///
    /// # Errors
    ///
    /// If the edge would introduce the cycle, the underlying graph is not modified and a list of
    /// all the edge data in the would-be cycle is returned instead.
    pub(crate) fn add_edge(&mut self, x: V, y: V, e: impl FnOnce() -> E) -> Result<(), Vec<E>>
    where
        E: Clone,
    {
        if x == y {
            // self-edges are always considered cycles
            return Err(Vec::new());
        }

        let (_, out_edges, ub) = self.add_node(x);

        match out_edges.entry(y) {
            Entry::Occupied(_) => {
                // Edge already exists, nothing to be done
                return Ok(());
            }
            Entry::Vacant(entry) => entry.insert(e()),
        };

        let (in_edges, _, lb) = self.add_node(y);

        in_edges.insert(x);

        if lb < ub {
            // This edge might introduce a cycle, need to recompute the topological sort
            let mut visited = [x, y].into_iter().collect();
            let mut delta_f = Vec::new();
            let mut delta_b = Vec::new();

            if let Err(cycle) = self.dfs_f(&self.nodes[&y], ub, &mut visited, &mut delta_f) {
                // This edge introduces a cycle, so we want to reject it and remove it from the
                // graph again to keep the "does not contain cycles" invariant.

                // We use map instead of unwrap to avoid an `unwrap()` but we know that these
                // entries are present as we just added them above.
                self.nodes.get_mut(&y).map(|node| node.in_edges.remove(&x));
                self.nodes.get_mut(&x).map(|node| node.out_edges.remove(&y));

                // No edge was added
                return Err(cycle);
            }

            // No need to check as we should've found the cycle on the forward pass
            self.dfs_b(&self.nodes[&x], lb, &mut visited, &mut delta_b);

            // Original paper keeps it around but this saves us from clearing it
            drop(visited);

            self.reorder(delta_f, delta_b);
        }

        Ok(())
    }

    /// Forwards depth-first-search
    fn dfs_f<'a>(
        &'a self,
        n: &'a Node<V, E>,
        ub: Order,
        visited: &mut HashSet<V>,
        delta_f: &mut Vec<&'a Node<V, E>>,
    ) -> Result<(), Vec<E>>
    where
        E: Clone,
    {
        delta_f.push(n);

        for (w, e) in &n.out_edges {
            let node = &self.nodes[w];
            let ord = node.ord.get();

            if ord == ub {
                // Found a cycle
                return Err(vec![e.clone()]);
            } else if !visited.contains(w) && ord < ub {
                // Need to check recursively
                visited.insert(*w);
                if let Err(mut chain) = self.dfs_f(node, ub, visited, delta_f) {
                    chain.push(e.clone());
                    return Err(chain);
                }
            }
        }

        Ok(())
    }

    /// Backwards depth-first-search
    fn dfs_b<'a>(
        &'a self,
        n: &'a Node<V, E>,
        lb: Order,
        visited: &mut HashSet<V>,
        delta_b: &mut Vec<&'a Node<V, E>>,
    ) {
        delta_b.push(n);

        for w in &n.in_edges {
            let node = &self.nodes[w];
            if !visited.contains(w) && lb < node.ord.get() {
                visited.insert(*w);

                self.dfs_b(node, lb, visited, delta_b);
            }
        }
    }

    fn reorder(&self, mut delta_f: Vec<&Node<V, E>>, mut delta_b: Vec<&Node<V, E>>) {
        self.sort(&mut delta_f);
        self.sort(&mut delta_b);

        let mut l = Vec::with_capacity(delta_f.len() + delta_b.len());
        let mut orders = Vec::with_capacity(delta_f.len() + delta_b.len());

        for v in delta_b.into_iter().chain(delta_f) {
            orders.push(v.ord.get());
            l.push(v);
        }

        // Original paper cleverly merges the two lists by using that both are sorted. We just sort
        // again. This is slower but also much simpler.
        orders.sort_unstable();

        for (node, order) in l.into_iter().zip(orders) {
            node.ord.set(order);
        }
    }

    fn sort(&self, ids: &mut [&Node<V, E>]) {
        // Can use unstable sort because mutex ids should not be equal
        ids.sort_unstable_by_key(|v| &v.ord);
    }
}

// Manual `Default` impl as derive causes unnecessarily strong bounds.
impl<V, E> Default for DiGraph<V, E>
where
    V: Eq + Hash + Copy,
{
    fn default() -> Self {
        Self {
            nodes: Default::default(),
            unused_order: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    use super::*;

    fn nop() {}

    #[test]
    fn test_no_self_cycle() {
        // Regression test for https://github.com/bertptrs/tracing-mutex/issues/7
        let mut graph = DiGraph::default();

        assert!(graph.add_edge(1, 1, nop).is_err());
    }

    #[test]
    fn test_digraph() {
        let mut graph = DiGraph::default();

        // Add some safe edges
        assert!(graph.add_edge(0, 1, nop).is_ok());
        assert!(graph.add_edge(1, 2, nop).is_ok());
        assert!(graph.add_edge(2, 3, nop).is_ok());
        assert!(graph.add_edge(4, 2, nop).is_ok());

        // Try to add an edge that introduces a cycle
        assert!(graph.add_edge(3, 1, nop).is_err());

        // Add an edge that should reorder 0 to be after 4
        assert!(graph.add_edge(4, 0, nop).is_ok());
    }

    /// Fuzz the DiGraph implementation by adding a bunch of valid edges.
    ///
    /// This test generates all possible forward edges in a 100-node graph consisting of natural
    /// numbers, shuffles them, then adds them to the graph. This will always be a valid directed,
    /// acyclic graph because there is a trivial order (the natural numbers) but because the edges
    /// are added in a random order the DiGraph will still occasionally need to reorder nodes.
    #[test]
    fn fuzz_digraph() {
        // Note: this fuzzer is quadratic in the number of nodes, so this cannot be too large or it
        // will slow down the tests too much.
        const NUM_NODES: usize = 100;
        let mut edges = Vec::with_capacity(NUM_NODES * NUM_NODES);

        for i in 0..NUM_NODES {
            for j in i..NUM_NODES {
                if i != j {
                    edges.push((i, j));
                }
            }
        }

        edges.shuffle(&mut thread_rng());

        let mut graph = DiGraph::default();

        for (x, y) in edges {
            assert!(graph.add_edge(x, y, nop).is_ok());
        }
    }
}
