use std::collections::HashMap;
use std::collections::HashSet;

use crate::MutexID;

#[derive(Clone, Default, Debug)]
pub struct DiGraph {
    in_edges: HashMap<MutexID, Vec<MutexID>>,
    out_edges: HashMap<MutexID, Vec<MutexID>>,
}

impl DiGraph {
    fn add_node(&mut self, node: MutexID) -> (&mut Vec<MutexID>, &mut Vec<MutexID>) {
        let in_edges = self.in_edges.entry(node).or_default();
        let out_edges = self.out_edges.entry(node).or_default();

        (in_edges, out_edges)
    }

    pub(crate) fn remove_node(&mut self, node: MutexID) -> bool {
        match self.out_edges.remove(&node) {
            None => false,
            Some(out_edges) => {
                for other in out_edges {
                    self.in_edges
                        .get_mut(&other)
                        .unwrap()
                        .retain(|c| c != &node);
                }

                for other in self.in_edges.remove(&node).unwrap() {
                    self.out_edges
                        .get_mut(&other)
                        .unwrap()
                        .retain(|c| c != &node);
                }

                true
            }
        }
    }

    /// Add an edge to the graph
    ///
    /// Nodes, both from and to, are created as needed when creating new edges.
    pub(crate) fn add_edge(&mut self, from: MutexID, to: MutexID) -> bool {
        if from == to {
            return false;
        }

        let (_, out_edges) = self.add_node(from);

        out_edges.push(to);

        let (in_edges, _) = self.add_node(to);

        // No need for existence check assuming the datastructure is consistent
        in_edges.push(from);

        true
    }

    pub fn has_cycles(&self) -> bool {
        let mut marks = HashSet::new();
        let mut temp = HashSet::new();

        self.out_edges
            .keys()
            .copied()
            .any(|node| !self.visit(node, &mut marks, &mut temp))
    }

    fn visit(
        &self,
        node: MutexID,
        marks: &mut HashSet<MutexID>,
        temp: &mut HashSet<MutexID>,
    ) -> bool {
        if marks.contains(&node) {
            return true;
        }

        if !temp.insert(node) {
            return false;
        }

        if self.out_edges[&node]
            .iter()
            .copied()
            .any(|node| !self.visit(node, marks, temp))
        {
            return false;
        }

        temp.remove(&node);

        marks.insert(node);

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MutexID;

    #[test]
    fn test_digraph() {
        let id: Vec<MutexID> = (0..5).map(|_| MutexID::new()).collect();
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
