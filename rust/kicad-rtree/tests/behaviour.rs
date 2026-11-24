// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_rtree::{BulkEntry, DynamicRtree, Error, NodeData};
#[derive(Clone, Debug, PartialEq)]
struct Item(i32);
impl NodeData for Item {}
type Tree = DynamicRtree<Item, i32, 2, 4>;
fn all(t: &Tree) -> Result<Vec<i32>, Error> {
    let mut it = t.begin()?;
    let mut out = Vec::new();
    while !it.at_end() {
        if let Some(v) = it.current() {
            out.push(v.0);
        }
        it.advance()?;
    }
    Ok(out)
}
#[test]
fn deep_insertions_fallback_removal_and_condensation() -> Result<(), Error> {
    let mut t = Tree::new()?;
    for i in 0..1200 {
        t.insert([i * 13, i % 11], [i * 13 + 7, i % 11 + 2], Item(i))?;
    }
    assert!(t.structure()?.height > 64);
    assert!(t.structure()?.nodes > 256);
    assert_eq!(t.size()?, 1200);
    let mut seen = all(&t)?;
    seen.sort();
    assert_eq!(seen, (0..1200).collect::<Vec<_>>());
    for i in 0..1200 {
        assert!(t.remove([-999; 2], [-998; 2], &Item(i))?);
        if i % 53 == 0 {
            assert_eq!(t.size()?, 1199 - i as usize);
        }
    }
    assert!(t.empty()?);
    assert_eq!(t.structure()?.nodes, 0);
    assert!(!t.remove([0; 2], [0; 2], &Item(1))?);
    assert!(t.memory_usage()? > 0);
    t.insert([0; 2], [1; 2], Item(7))?;
    assert_eq!(all(&t)?, vec![7]);
    Ok(())
}
#[test]
fn bulk_packing_is_mutable_and_preserves_input() -> Result<(), Error> {
    let mut t = Tree::new()?;
    let entries: Vec<_> = (0..257)
        .map(|i| BulkEntry {
            min: [i, 0],
            max: [i + 2, 2],
            data: Item(i),
        })
        .collect();
    t.bulk_load(&entries)?;
    assert_eq!(entries[0].data, Item(0));
    assert!(t.structure()?.height >= 5);
    for i in (0..257).rev() {
        assert!(t.remove([i, 0], [i + 2, 2], &Item(i))?);
    }
    assert!(t.empty()?);
    t.bulk_load(&entries)?;
    t.insert([900; 2], [901; 2], Item(900))?;
    assert_eq!(t.size()?, 258);
    t.bulk_load(&[])?;
    assert!(t.empty()?);
    Ok(())
}
#[test]
fn callback_stop_hit_is_counted_and_touching_boxes_match() -> Result<(), Error> {
    let mut t = Tree::new()?;
    for i in 0..70 {
        t.insert([i * 2, 0], [i * 2 + 2, 2], Item(i % 3))?;
    }
    let mut hits = Vec::new();
    let count = t.search([2, 2], [2, 2], &mut |v| {
        hits.push(v.0);
        false
    })?;
    assert_eq!(count, 1);
    assert_eq!(hits.len(), 1);
    let range = t.overlapping([2, 2], [2, 2])?;
    assert!(!range.empty()?);
    let mut it = range.begin()?;
    let mut n = 0;
    while !it.at_end() {
        n += 1;
        it.advance()?;
    }
    assert_eq!(n, 2);
    assert!(t.overlapping([900, 900], [901, 901])?.empty()?);
    Ok(())
}
#[test]
fn nearest_clears_output_and_equal_distances_are_not_data_sorted() -> Result<(), Error> {
    let mut t = DynamicRtree::<Item, i32, 2, 16>::new()?;
    for i in 0..6 {
        t.insert([0; 2], [0; 2], Item(i))?;
    }
    let mut out = vec![(99, Item(99))];
    t.nearest_neighbors([0; 2], 20, &mut out)?;
    assert_eq!(
        out,
        vec![
            (0, Item(0)),
            (0, Item(2)),
            (0, Item(5)),
            (0, Item(4)),
            (0, Item(1)),
            (0, Item(3))
        ]
    );
    t.nearest_neighbors([0; 2], 0, &mut out)?;
    assert!(out.is_empty());
    t.remove_all()?;
    out.push((99, Item(99)));
    t.nearest_neighbors([0; 2], 3, &mut out)?;
    assert!(out.is_empty());
    Ok(())
}
#[test]
fn moves_transfer_pages_and_leave_reusable_empty_source() -> Result<(), Error> {
    let mut t = Tree::new()?;
    for i in 0..30 {
        t.insert([i; 2], [i + 1; 2], Item(i))?;
    }
    let before = all(&t)?;
    let memory = t.memory_usage()?;
    let mut moved = t.move_out()?;
    assert!(t.empty()?);
    assert_eq!(t.memory_usage()?, 0);
    assert_eq!(all(&moved)?, before);
    assert_eq!(moved.memory_usage()?, memory);
    t.insert([0; 2], [0; 2], Item(99))?;
    t.move_assign(&mut moved)?;
    assert!(moved.empty()?);
    assert_eq!(all(&t)?, before);
    Ok(())
}
#[test]
fn cursors_compare_only_end_flags_and_keep_last_copy() -> Result<(), Error> {
    let mut t = Tree::new()?;
    t.insert([0; 2], [1; 2], Item(1))?;
    t.insert([2; 2], [3; 2], Item(2))?;
    let a = t.begin()?;
    let mut b = t.begin()?;
    b.advance()?;
    assert!(a == b);
    b.advance()?;
    assert!(b.at_end());
    assert_eq!(b.current(), Some(&Item(2)));
    assert!(b == t.end()?);
    b.advance()?;
    assert_eq!(b.current(), Some(&Item(2)));
    Ok(())
}
#[test]
fn exact_arithmetic_refusals_resource_and_failure_lifecycle() -> Result<(), Error> {
    let mut t = Tree::with_limit(2)?;
    t.insert([i32::MIN; 2], [i32::MIN; 2], Item(1))?;
    let mut out = vec![(9, Item(9))];
    assert_eq!(
        t.nearest_neighbors([i32::MAX; 2], 1, &mut out),
        Err(Error::Overflow)
    );
    assert!(out.is_empty());
    t.insert([0; 2], [0; 2], Item(2))?;
    assert_eq!(t.insert([1; 2], [1; 2], Item(3)), Err(Error::Resource));
    assert_eq!(t.size()?, 2);
    let mut overflow = Tree::new()?;
    for _ in 0..4 {
        overflow.insert([i32::MIN; 2], [i32::MAX; 2], Item(1))?;
    }
    assert!(overflow
        .insert([i32::MIN; 2], [i32::MAX; 2], Item(1))
        .is_err());
    assert_eq!(overflow.size(), Err(Error::FailedMutation));
    overflow.remove_all()?;
    assert!(overflow.empty()?);
    // No blanket coordinate bound: empty and negative-k paths do no distance math.
    overflow.nearest_neighbors([i32::MAX; 2], 1, &mut out)?;
    overflow.insert([i32::MIN; 2], [i32::MIN; 2], Item(0))?;
    overflow.nearest_neighbors([i32::MAX; 2], 0, &mut out)?;
    assert!(out.is_empty());
    Ok(())
}
#[test]
fn inverted_bulk_refuses_only_evaluated_cast_and_keeps_old_tree() -> Result<(), Error> {
    let mut t = Tree::new()?;
    t.insert([0; 2], [1; 2], Item(5))?;
    let input = [BulkEntry {
        min: [10; 2],
        max: [-10; 2],
        data: Item(7),
    }];
    assert_eq!(t.bulk_load(&input), Err(Error::FloatConversion));
    assert_eq!(all(&t)?, vec![5]);
    // Inverted insertion itself is defined while no overflowing area is evaluated.
    t.insert([10; 2], [-10; 2], Item(8))?;
    assert_eq!(t.size()?, 2);
    Ok(())
}

#[test]
fn node_storage_budget_is_explicit_and_clear_recovers() -> Result<(), Error> {
    use kicad_rtree::ResourceBudget;
    let mut t = Tree::with_resources(ResourceBudget {
        max_items: 100,
        max_nodes: 1,
    })?;
    for i in 0..4 {
        t.insert([i; 2], [i + 1; 2], Item(i))?;
    }
    assert_eq!(t.insert([5; 2], [6; 2], Item(5)), Err(Error::Resource));
    assert_eq!(t.size(), Err(Error::FailedMutation));
    t.remove_all()?;
    assert!(t.empty()?);
    let mut zero = Tree::with_resources(ResourceBudget {
        max_items: 10,
        max_nodes: 0,
    })?;
    assert_eq!(zero.insert([0; 2], [1; 2], Item(0)), Err(Error::Resource));
    Ok(())
}

#[test]
fn standalone_node_cursors_default_clone_and_null_payload() -> Result<(), Error> {
    use kicad_rtree::{SearchRange, TreeIterator};
    // These are the accepted node/arena types, not a tree-specific substitute.
    use kicad_rtree_node::{RtreeNode, SlabArena};
    type Node = RtreeNode<Item, i32, 2, 4>;
    let mut arena = SlabArena::<Node>::new();
    let root = arena.new_external()?;
    let n = arena.node_mut(&root)?;
    n.count = 2;
    n.set_child_bounds(0, &[0; 2], &[1; 2])?;
    n.set_child_bounds(1, &[4; 2], &[5; 2])?;
    n.set_data(1, Some(Item(7)))?;
    let mut end = TreeIterator::<Item, i32, 2, 4>::default();
    assert!(end.at_end());
    assert_eq!(end.current(), None);
    end.advance()?;
    let mut it = TreeIterator::from_root(&arena, Some(&root), None)?;
    assert!(!it.at_end());
    assert_eq!(it.current(), None, "null DATATYPE is a valid active entry");
    let saved = it.clone();
    it.advance()?;
    assert!(it == saved);
    assert_eq!(it.current(), Some(&Item(7)));
    assert_eq!(saved.current(), None);
    let range = SearchRange::from_root(&arena, Some(&root), [4; 2], [5; 2]);
    let copy = range.clone();
    assert_eq!(copy.begin()?.current(), Some(&Item(7)));
    assert!(!copy.empty()?);
    assert!(SearchRange::from_root(&arena, None, [0; 2], [9; 2]).empty()?);
    Ok(())
}

#[test]
fn detached_cursors_follow_defined_mutation_and_refuse_freed_nodes() -> Result<(), Error> {
    let mut t = Tree::new()?;
    t.insert([0; 2], [1; 2], Item(0))?;
    t.insert([2; 2], [3; 2], Item(1))?;
    let mut cursor = t.cursor()?;
    assert_eq!(cursor.current(), Some(&Item(0)));
    t.insert([4; 2], [5; 2], Item(2))?;
    t.advance_cursor(&mut cursor)?;
    assert_eq!(cursor.current(), Some(&Item(1)));
    assert!(t.remove([0; 2], [1; 2], &Item(0))?);
    assert_eq!(
        cursor.current(),
        Some(&Item(1)),
        "current is copied, not a slot reference"
    );
    t.advance_cursor(&mut cursor)?;
    assert!(cursor.at_end());
    assert_eq!(cursor.current(), Some(&Item(1)));
    let mut cursor = t.cursor()?;
    let mut query = t.overlapping_cursor([0; 2], [9; 2])?;
    // A split retains the old root node: advancing a cursor into it is defined,
    // and follows that old node's slots rather than silently restarting at root.
    for i in 3..8 {
        t.insert([i * 2; 2], [i * 2 + 1; 2], Item(i))?;
    }
    t.advance_cursor(&mut cursor)?;
    t.advance_cursor(&mut query)?;
    let mut freed = t.cursor()?;
    t.remove_all()?;
    assert_eq!(
        t.advance_cursor(&mut freed),
        Err(Error::Node(kicad_rtree::RtreeError::NodeNotLive))
    );
    let mut source = Tree::new()?;
    source.insert([0; 2], [1; 2], Item(5))?;
    source.insert([2; 2], [3; 2], Item(6))?;
    let mut moved_cursor = source.cursor()?;
    let dest = source.move_out()?;
    dest.advance_cursor(&mut moved_cursor)?;
    assert_eq!(moved_cursor.current(), Some(&Item(6)));
    Ok(())
}

#[test]
fn external_cursor_cycles_have_a_resource_bound() -> Result<(), Error> {
    use kicad_rtree::{RtreeError, TreeCursor};
    use kicad_rtree_node::{RtreeNode, SlabArena};
    let mut arena = SlabArena::<RtreeNode<Item, i32, 2, 4>>::new();
    let h = arena.new_external()?;
    let n = arena.node_mut(&h)?;
    n.count = 1;
    n.level = 1;
    n.set_child(0, Some(h.clone()))?;
    assert!(matches!(
        TreeCursor::from_root(&arena, Some(&h), None, 2),
        Err(Error::Resource)
    ));
    // A null pointer is different from a null payload; descending through it
    // would dereference null in C++, and is refused without inventing an item.
    arena.node_mut(&h)?.set_child(0, None)?;
    assert!(matches!(
        TreeCursor::from_root(&arena, Some(&h), None, 2),
        Err(Error::Invariant)
    ));
    arena.destroy_external(&h)?;
    assert!(matches!(
        TreeCursor::from_root(&arena, Some(&h), None, 2),
        Err(Error::Node(RtreeError::NodeDestroyed))
    ));
    Ok(())
}

#[test]
fn saved_ranges_begin_at_the_saved_root_after_mutations() -> Result<(), Error> {
    let mut t = Tree::new()?;
    let empty = t.overlapping([-100; 2], [100; 2])?.detach();
    for i in 0..4 {
        t.insert([i; 2], [i + 1; 2], Item(i))?;
    }
    assert!(t.begin_range(&empty)?.at_end());
    let saved = t.overlapping([-100; 2], [100; 2])?.detach();
    t.insert([8; 2], [9; 2], Item(8))?;
    let mut it = t.begin_range(&saved)?;
    let mut n = 0;
    while !it.at_end() {
        n += 1;
        t.advance_cursor(&mut it)?;
    }
    assert!(
        n > 0 && n < 5,
        "saved range follows the former leaf root after splitting"
    );
    assert_eq!(t.size()?, 5);
    Ok(())
}
