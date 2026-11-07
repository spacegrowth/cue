//! Lock-graph edges that lie on a cycle: two threads taking the same locks in opposite orders can each
//! wait on the other; a self-edge is one thread waiting on itself.

use std::collections::{BTreeMap, BTreeSet};

pub fn cyclic_edges<'e>(edges: impl Iterator<Item = &'e (String, String)>) -> BTreeSet<(String, String)> {
    let mut next: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let all: Vec<&(String, String)> = edges.collect();
    for (a, b) in &all {
        next.entry(a.as_str()).or_default().insert(b.as_str());
    }
    all.into_iter().filter(|(a, b)| a == b || reaches(&next, b, a)).cloned().collect()
}

fn reaches(next: &BTreeMap<&str, BTreeSet<&str>>, from: &str, to: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if seen.insert(n) {
            stack.extend(next.get(n).into_iter().flatten().copied());
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(a: &str, b: &str) -> (String, String) {
        (a.into(), b.into())
    }

    #[test]
    fn self_edges_and_two_way_orders_are_cycles_a_chain_is_not() {
        let edges = [e("A", "A"), e("B", "C"), e("C", "D"), e("D", "B"), e("X", "Y")];
        let cyclic = cyclic_edges(edges.iter());
        assert_eq!(cyclic, [e("A", "A"), e("B", "C"), e("C", "D"), e("D", "B")].into_iter().collect());
    }
}
