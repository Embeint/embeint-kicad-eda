// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_rtree::{BulkEntry, DynamicRtree, Error, NodeData};
#[derive(Clone, Debug, PartialEq, Eq)]
struct Item(u32);
impl NodeData for Item {}
type Tree = DynamicRtree<Item, i32, 2, 4>;
/// Node budget for tests that do not exercise it.
const NODES: usize = 1 << 16;
fn point(i: u32) -> ([i32; 2], [i32; 2]) {
    let x = ((i * 97) % 997) as i32 - 500;
    let y = ((i * 43) % 787) as i32 - 400;
    ([x, y], [x + 9, y + 13])
}
#[test]
fn structural_mutations_and_delete_cascade() -> Result<(), Error> {
    let mut tree = DynamicRtree::<Item, i32, 2, 8>::new(2000, NODES)?;
    for i in 0..800 {
        let (lo, hi) = point(i);
        tree.insert(lo, hi, Some(Item(i)))?;
    }
    let stats = tree.branches();
    assert!(stats.leaf_splits > 0 && stats.internal_splits > 0 && stats.nonroot_reinserts > 0);
    let level = tree
        .node(&tree.root_handle().ok_or(Error::Invariant)?)?
        .level;
    assert!(level >= 3);
    for i in (0..800).step_by(2) {
        assert!(tree.remove([10000; 2], [10001; 2], Some(Item(i)))?);
    }
    let mut visited = Vec::new();
    assert_eq!(
        tree.search([-1000; 2], [1000; 2], &mut |x| {
            if let Some(x) = x {
                visited.push(x.0)
            }
            true
        })?,
        400
    );
    visited.sort_unstable();
    assert_eq!(visited, (1..800).step_by(2).collect::<Vec<_>>());
    for i in (1..800).step_by(2) {
        let (lo, hi) = point(i);
        assert!(tree.remove(lo, hi, Some(Item(i)))?);
    }
    assert!(tree.is_empty() && tree.root_handle().is_none());
    let b = tree.branches();
    println!("mutation branch counts: {b:?}");
    assert!(
        b.leaf_orphans > 0
            && b.internal_orphans > 0
            && b.root_condensations > 0
            && b.empty_child_frees > 0
    );
    let memory = tree.memory_usage()?;
    assert!(memory > 0);
    tree.insert([0; 2], [0; 2], None)?;
    tree.clear()?;
    assert!(tree.memory_usage()? >= memory);
    Ok(())
}
#[test]
fn moves_transfer_handles_and_allocator_pages() -> Result<(), Error> {
    let mut source = Tree::new(100, NODES)?;
    source.insert([0; 2], [1; 2], Some(Item(1)))?;
    let handle = source.root_handle().ok_or(Error::Invariant)?;
    let memory = source.memory_usage()?;
    let mut target = Tree::move_construct(&mut source)?;
    assert_eq!(source.len(), 0);
    assert_eq!(source.memory_usage()?, 0);
    assert_eq!(target.memory_usage()?, memory);
    assert_eq!(target.node(&handle)?.count, 1);
    assert!(source.node(&handle).is_err());
    source.insert([7; 2], [8; 2], None)?;
    let old = source.root_handle().ok_or(Error::Invariant)?;
    source.move_assign(&mut target)?;
    assert_eq!(source.node(&handle)?.count, 1);
    assert!(source.node(&old).is_err());
    assert_eq!(target.memory_usage()?, 0);
    assert!(target.is_empty());
    source.move_assign_self();
    assert_eq!(source.node(&handle)?.count, 1);
    Ok(())
}
#[test]
fn outputs_visitors_lazy_cursors_and_equal_boundaries() -> Result<(), Error> {
    let mut tree = Tree::new(100, NODES)?;
    for i in 0..30 {
        tree.insert([0; 2], [10; 2], Some(Item(i)))?;
    }
    tree.insert([10; 2], [10; 2], None)?;
    let mut output = vec![Some(Item(99))];
    let found = tree.search([10; 2], [10; 2], &mut |x| {
        output.push(x.cloned());
        output.len() < 4
    })?;
    assert_eq!(found, 3);
    assert_eq!(output.len(), 4);
    let range = tree.overlapping([10; 2], [10; 2])?;
    assert!(!range.is_empty()?);
    let mut it = tree.begin()?;
    let mut next = it.clone();
    next.advance()?;
    assert!(it == next);
    let mut n = 0;
    while !it.is_end() {
        n += 1;
        it.advance()?;
    }
    assert_eq!(n, 31);
    let last = it.current().clone();
    it.advance()?;
    assert_eq!(&last, it.current());
    let mut results = vec![(9, Some(Item(99)))];
    tree.nearest_neighbors([0; 2], 0, &mut results)?;
    assert!(results.is_empty());
    tree.nearest_neighbors([0; 2], 100, &mut results)?;
    assert_eq!(results.len(), 31);
    assert!(results.iter().all(|(d, _)| *d == 0 || *d == 200));
    Ok(())
}
#[test]
fn invalid_bounds_remain_defined_predicates_and_bulk_replaces() -> Result<(), Error> {
    let mut tree = Tree::new(100, NODES)?;
    tree.insert([8, 0], [2, 0], None)?;
    assert_eq!(tree.search([0; 2], [10; 2], &mut |_| true)?, 1);
    let batch = vec![
        BulkEntry {
            min: [4, 4],
            max: [4, 4],
            data: Some(Item(2))
        };
        70
    ];
    tree.bulk_load(&batch)?;
    assert_eq!(tree.len(), 70);
    assert_eq!(batch[0].data, Some(Item(2)));
    assert!(
        tree.node(&tree.root_handle().ok_or(Error::Invariant)?)?
            .level
            >= 2
    );
    tree.bulk_load(&[])?;
    assert!(tree.is_empty());
    assert!(tree.memory_usage()? > 0);
    Ok(())
}
#[test]
fn arithmetic_is_checked_at_the_evaluated_step_and_tree_can_be_reset() -> Result<(), Error> {
    let mut tree = Tree::new(100, NODES)?;
    // Merely storing a huge box evaluates no area/distance: valid reference input.
    tree.insert([i32::MIN; 2], [i32::MAX; 2], Some(Item(1)))?;
    assert_eq!(tree.search([0; 2], [0; 2], &mut |_| true)?, 1);
    let mut out = Vec::new();
    tree.nearest_neighbors([0; 2], 1, &mut out)?;
    assert_eq!(out[0].0, 0);
    tree.clear()?;
    tree.insert([i32::MIN; 2], [i32::MIN; 2], None)?;
    assert_eq!(
        tree.nearest_neighbors([i32::MAX; 2], 1, &mut out),
        Err(Error::Overflow)
    );
    assert_eq!(tree.len(), 1); // read-only refusal does not poison mutations
    tree.clear()?;
    for i in 0..4 {
        tree.insert([i32::MIN + i; 2], [i32::MIN + i; 2], Some(Item(i as u32)))?;
    }
    assert!(tree.insert([i32::MAX; 2], [i32::MAX; 2], None).is_err());
    assert!(matches!(tree.begin(), Err(Error::InterruptedMutation)));
    tree.clear()?;
    tree.insert([0; 2], [1; 2], None)?;
    assert_eq!(tree.len(), 1);
    Ok(())
}
#[test]
fn resource_budget_and_floating_bulk_refusals_preserve_previous_tree() -> Result<(), Error> {
    let mut tree = Tree::new(1, NODES)?;
    tree.insert([0; 2], [1; 2], None)?;
    assert_eq!(tree.insert([2; 2], [3; 2], None), Err(Error::ResourceLimit));
    assert_eq!(tree.len(), 1);
    // Inverted global bounds lead to an out-of-domain uint32 conversion.
    let batch = [BulkEntry {
        min: [i32::MAX; 2],
        max: [i32::MIN; 2],
        data: None,
    }];
    assert_eq!(tree.bulk_load(&batch), Err(Error::Conversion));
    assert_eq!(tree.len(), 1);
    Ok(())
}

/// Live nodes reachable from the root, walked with an explicit stack.
fn reachable<const N: usize, const M: usize>(
    tree: &DynamicRtree<Item, i32, N, M>,
) -> Result<usize, Error> {
    let mut stack: Vec<_> = tree.root_handle().into_iter().collect();
    let mut count = 0;
    while let Some(h) = stack.pop() {
        count += 1;
        let node = tree.node(&h)?;
        if node.is_internal() {
            for i in 0..node.count as usize {
                stack.push(node.child(i)?.cloned().ok_or(Error::Invariant)?);
            }
        }
    }
    Ok(count)
}
fn root_level<const N: usize, const M: usize>(
    tree: &DynamicRtree<Item, i32, N, M>,
) -> Result<i32, Error> {
    Ok(tree
        .node(&tree.root_handle().ok_or(Error::Invariant)?)?
        .level)
}
/// Insert, traverse every way, then remove everything from a tree whose
/// measured height exceeds the former fixed 64-level limit.
fn deep_lifecycle<const N: usize, const M: usize>(
    n: u32,
    boxed: impl Fn(u32) -> ([i32; N], [i32; N]),
    min_level: i32,
) -> Result<(), Error> {
    let mut tree = DynamicRtree::<Item, i32, N, M>::new(10_000, 1 << 20)?;
    for i in 0..n {
        let (lo, hi) = boxed(i);
        tree.insert(lo, hi, Some(Item(i)))?;
    }
    let level = root_level(&tree)?;
    println!(
        "deep N={N} M={M} n={n}: root level {level}, {} active nodes",
        tree.active_nodes()
    );
    assert!(level > min_level && level > 64);
    assert_eq!(reachable(&tree)?, tree.active_nodes());
    let mut seen = Vec::new();
    let found = tree.search([i32::MIN; N], [i32::MAX; N], &mut |x| {
        seen.push(x.cloned());
        true
    })?;
    assert_eq!(found as u32, n);
    let mut cursor = tree.begin()?;
    let mut walked = Vec::new();
    while !cursor.is_end() {
        walked.push(cursor.current().clone());
        cursor.advance()?;
    }
    assert_eq!(walked, seen); // both are the same depth-first slot order
    let (lo, hi) = boxed(n - 1);
    let range = tree.overlapping(lo, hi)?;
    let mut hit = range.begin()?;
    assert!(!hit.is_end());
    assert!(hit.current().is_some());
    hit.advance()?;
    let mut near = Vec::new();
    tree.nearest_neighbors(lo, 3, &mut near)?;
    assert_eq!(near.len(), 3);
    // Touching neighbours also lie at distance zero; the query box's own item
    // must be among the zero-distance results.
    assert_eq!(near[0].0, 0);
    assert!(near.iter().any(|(d, x)| *d == 0 && *x == Some(Item(n - 1))));
    // Remove from the deep end with a missing box (stored-bounds fallback).
    let before = tree.branches().clone();
    for i in (0..n).rev() {
        assert!(tree.remove([-90_000; N], [-89_999; N], Some(Item(i)))?);
        if i % 37 == 0 {
            assert_eq!(reachable(&tree)?, tree.active_nodes());
        }
    }
    assert!(tree.is_empty() && tree.root_handle().is_none());
    assert_eq!(tree.active_nodes(), 0);
    let after = tree.branches();
    println!("deep N={N} M={M} removal branch counts: {after:?}");
    assert!(after.fallback_removals >= before.fallback_removals + n as usize);
    assert!(after.root_condensations > before.root_condensations);
    Ok(())
}
#[test]
fn deep_fanout3_monotone_tree_has_no_depth_limit() -> Result<(), Error> {
    deep_lifecycle::<1, 3>(160, |i| ([2 * i as i32], [2 * i as i32 + 1]), 70)
}
#[test]
fn deep_fanout4_monotone_trees_have_no_depth_limit() -> Result<(), Error> {
    deep_lifecycle::<2, 4>(240, |i| ([i as i32, 0], [i as i32 + 1, 1]), 70)?;
    // The pattern the closed duplicate (#60) used; the fixed limit refused it.
    deep_lifecycle::<2, 4>(
        1200,
        |i| {
            let i = i as i32;
            ([i * 13, i % 11], [i * 13 + 7, i % 11 + 2])
        },
        70,
    )
}
#[test]
fn deep_moves_bulk_and_clear_keep_node_accounting() -> Result<(), Error> {
    type Deep = DynamicRtree<Item, i32, 1, 3>;
    let mut a = Deep::new(1000, 1 << 16)?;
    for i in 0..160 {
        a.insert([2 * i], [2 * i + 1], Some(Item(i as u32)))?;
    }
    let active = a.active_nodes();
    let mut b = Deep::move_construct(&mut a)?;
    assert_eq!((a.active_nodes(), b.active_nodes()), (0, active));
    assert_eq!((a.max_entries(), a.max_nodes()), (1000, 1 << 16));
    assert!(root_level(&b)? > 64);
    a.insert([0], [1], None)?;
    a.move_assign(&mut b)?;
    assert_eq!((a.active_nodes(), b.active_nodes()), (active, 0));
    let batch: Vec<_> = (0..50)
        .map(|i| BulkEntry {
            min: [i],
            max: [i + 1],
            data: Some(Item(i as u32)),
        })
        .collect();
    a.bulk_load(&batch)?;
    assert_eq!(a.active_nodes(), reachable(&a)?);
    assert_eq!(a.active_nodes(), 17 + 6 + 2 + 1);
    a.clear()?;
    assert_eq!(a.active_nodes(), 0);
    Ok(())
}
#[test]
fn node_budget_refusals_follow_the_interrupted_lifecycle() -> Result<(), Error> {
    type Deep = DynamicRtree<Item, i32, 1, 3>;
    // A zero budget refuses the first root before anything changes.
    let mut empty = Deep::new(10, 0)?;
    assert_eq!(empty.insert([0], [1], None), Err(Error::NodeBudget));
    assert!(empty.begin()?.is_end());
    assert_eq!(empty.search([0], [1], &mut |_| true)?, 0);
    // A budget reached by a split mid-insertion interrupts the tree.
    let mut tree = Deep::new(1000, 40)?;
    let mut inserted = 0;
    let refusal = loop {
        match tree.insert(
            [2 * inserted],
            [2 * inserted + 1],
            Some(Item(inserted as u32)),
        ) {
            Ok(()) => inserted += 1,
            Err(e) => break e,
        }
    };
    assert_eq!(refusal, Error::NodeBudget);
    assert!(tree.active_nodes() <= 40);
    assert_eq!(tree.len(), inserted as usize);
    assert!(matches!(tree.begin(), Err(Error::InterruptedMutation)));
    assert_eq!(tree.insert([0], [1], None), Err(Error::InterruptedMutation));
    assert_eq!(tree.remove([0], [1], None), Err(Error::InterruptedMutation));
    // Reset keeps both host budgets and allows reuse.
    tree.clear()?;
    assert_eq!(
        (tree.len(), tree.active_nodes(), tree.max_nodes()),
        (0, 0, 40)
    );
    tree.insert([0], [1], Some(Item(1)))?;
    assert!(tree.remove([0], [1], Some(Item(1)))?);
    // Bulk load knows its node count in advance and refuses before mutating.
    let mut small = Deep::new(1000, 5)?;
    small.insert([0], [1], Some(Item(7)))?;
    let batch: Vec<_> = (0..12)
        .map(|i| BulkEntry {
            min: [i],
            max: [i + 1],
            data: None,
        })
        .collect();
    assert_eq!(small.bulk_load(&batch), Err(Error::NodeBudget)); // needs 4+2+1 nodes
    assert_eq!((small.len(), small.active_nodes()), (1, 1));
    assert_eq!(small.search([0], [1], &mut |_| true)?, 1);
    small.bulk_load(&batch[..9])?; // 3 + 1 nodes
    assert_eq!(small.active_nodes(), 4);
    Ok(())
}
