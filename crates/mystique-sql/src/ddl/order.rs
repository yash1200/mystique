//! Apply order for a schema diff: creates first, parents before children.

use std::collections::{HashMap, HashSet, VecDeque};

use super::types::{SchemaChange, TableDefinition};

pub(super) fn order_changes(
    changes: &mut Vec<SchemaChange>,
    drafts: &[TableDefinition],
) -> Result<(), String> {
    let creates: HashSet<String> = drafts
        .iter()
        .filter(|draft| draft.create)
        .map(|draft| draft.name.clone())
        .collect();
    let mut edges = Vec::new();
    for draft in drafts.iter().filter(|draft| draft.create) {
        for fk in &draft.foreign_keys {
            if creates.contains(&fk.ref_table) {
                edges.push((fk.ref_table.clone(), draft.name.clone()));
            }
        }
    }
    let create_order = topo(&creates.into_iter().collect::<Vec<_>>(), &edges)?;
    changes.sort_by_key(|change| {
        let group = match change.action.as_str() {
            "create" => 0,
            "alter" => 1,
            "unchanged" => 2,
            _ => 3,
        };
        let rank = create_order
            .get(&change.name)
            .copied()
            .unwrap_or(usize::MAX);
        (group, rank, change.name.clone())
    });
    Ok(())
}

pub(super) fn topo(
    nodes: &[String],
    edges: &[(String, String)],
) -> Result<HashMap<String, usize>, String> {
    let mut incoming: HashMap<String, usize> = nodes.iter().map(|name| (name.clone(), 0)).collect();
    let mut next: HashMap<String, Vec<String>> = HashMap::new();
    for (before, after) in edges {
        *incoming.entry(after.clone()).or_insert(0) += 1;
        next.entry(before.clone()).or_default().push(after.clone());
    }
    let mut queue: VecDeque<String> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(name, _)| name.clone())
        .collect();
    let mut order = HashMap::new();
    let mut rank = 0usize;
    while let Some(name) = queue.pop_front() {
        order.insert(name.clone(), rank);
        rank += 1;
        for child in next.get(&name).into_iter().flatten() {
            if let Some(count) = incoming.get_mut(child) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    queue.push_back(child.clone());
                }
            }
        }
    }
    if order.len() != nodes.len() {
        return Err("Foreign keys form a cycle. Apply these tables one at a time.".into());
    }
    Ok(order)
}
