use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Clone, Default, Debug)]
struct DiGraph {
    in_edges: HashMap<usize, Vec<usize>>,
    out_edges: HashMap<usize, Vec<usize>>,
}

impl DiGraph {
    fn add_node(&mut self, node: usize) -> (&mut Vec<usize>, &mut Vec<usize>) {
        let in_edges = self.in_edges.entry(node).or_default();
        let out_edges = self.out_edges.entry(node).or_default();

        (in_edges, out_edges)
    }

    pub fn remove_node(&mut self, node: usize) -> bool {
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

    pub fn add_edge(&mut self, from: usize, to: usize) -> bool {
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

    fn visit(&self, node: usize, marks: &mut HashSet<usize>, temp: &mut HashSet<usize>) -> bool {
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
    use crate::DiGraph;

    #[test]
    fn test_digraph() {
        let mut graph = DiGraph::default();

        graph.add_edge(1, 2);
        graph.add_edge(2, 3);
        graph.add_edge(3, 4);
        graph.add_edge(5, 2);

        assert!(!graph.has_cycles());

        graph.add_edge(4, 2);

        assert!(graph.has_cycles());

        assert!(graph.remove_node(4));

        assert!(!graph.has_cycles())
    }
}
