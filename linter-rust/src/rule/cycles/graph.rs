use std::collections::{BTreeSet, VecDeque};
pub(super) fn components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut visited = vec![false; edges.len()];
    let mut order = Vec::new();
    for start in 0..edges.len() {
        let mut stack = vec![(start, false)];
        while let Some((node, exit)) = stack.pop() {
            if exit {
                order.push(node);
                continue;
            }
            if visited[node] {
                continue;
            }
            visited[node] = true;
            stack.push((node, true));
            stack.extend(edges[node].iter().rev().map(|next| (*next, false)));
        }
    }
    let mut reverse = vec![Vec::new(); edges.len()];
    for (from, targets) in edges.iter().enumerate() {
        for to in targets {
            reverse[*to].push(from);
        }
    }
    visited.fill(false);
    let mut output = Vec::new();
    for start in order.into_iter().rev() {
        if visited[start] {
            continue;
        }
        let mut stack = vec![start];
        let mut component = Vec::new();
        while let Some(node) = stack.pop() {
            if visited[node] {
                continue;
            }
            visited[node] = true;
            component.push(node);
            stack.extend(&reverse[node]);
        }
        component.sort_unstable();
        output.push(component);
    }
    output.sort();
    output
}
pub(super) fn cycle(start: usize, component: &[usize], edges: &[Vec<usize>]) -> Option<Vec<usize>> {
    let members: BTreeSet<_> = component.iter().copied().collect();
    for next in &edges[start] {
        if !members.contains(next) {
            continue;
        }
        if *next == start {
            return Some(vec![start, start]);
        }
        let mut parents = vec![None; edges.len()];
        let mut queue = VecDeque::from([*next]);
        parents[*next] = Some(*next);
        while let Some(node) = queue.pop_front() {
            if edges[node].contains(&start) {
                let mut path = vec![node];
                let mut current = node;
                while current != *next {
                    current = parents[current]?;
                    path.push(current);
                }
                path.reverse();
                path.insert(0, start);
                path.push(start);
                return Some(path);
            }
            for target in &edges[node] {
                if members.contains(target) && parents[*target].is_none() {
                    parents[*target] = Some(node);
                    queue.push_back(*target);
                }
            }
        }
    }
    None
}
