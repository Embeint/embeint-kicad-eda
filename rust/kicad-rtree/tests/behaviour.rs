// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_rtree::{BulkEntry, DynamicRtree, Error, NodeData};
#[derive(Clone, Debug, PartialEq, Eq)]
struct Item(u32);
impl NodeData for Item {}
type Tree = DynamicRtree<Item, i32, 2, 4>;
fn point(i: u32) -> ([i32; 2], [i32; 2]) {
    let x = ((i * 97) % 997) as i32 - 500;
    let y = ((i * 43) % 787) as i32 - 400;
    ([x, y], [x + 9, y + 13])
}
#[test]
fn structural_mutations_and_delete_cascade() -> Result<(), Error> {
    let mut tree = DynamicRtree::<Item, i32, 2, 8>::new(2000)?;
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
    let mut source = Tree::new(100)?;
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
    let mut tree = Tree::new(100)?;
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
    let mut tree = Tree::new(100)?;
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
    let mut tree = Tree::new(100)?;
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
    let mut tree = Tree::new(1)?;
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
