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
    for i in 0..1000 {
        t.insert([i * 13, i % 11], [i * 13 + 7, i % 11 + 2], Item(i))?;
    }
    assert!(t.structure()?.height >= 5);
    assert!(t.structure()?.nodes > 256);
    assert_eq!(t.size()?, 1000);
    let mut seen = all(&t)?;
    seen.sort();
    assert_eq!(seen, (0..1000).collect::<Vec<_>>());
    for i in 0..1000 {
        assert!(t.remove([-999; 2], [-998; 2], &Item(i))?);
        if i % 53 == 0 {
            assert_eq!(t.size()?, 999 - i as usize);
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
