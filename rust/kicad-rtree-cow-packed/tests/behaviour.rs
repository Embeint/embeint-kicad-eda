// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_rtree_cow_packed::{Budget, Builder, BulkEntry, CowForest, Error, NodeData};
#[derive(Clone, Debug, PartialEq)]
struct Item(u64);
impl NodeData for Item {}
fn budget() -> Budget {
    Budget {
        entries: 10000,
        trees: 10000,
        nodes: 100000,
    }
}
#[test]
fn sharing_divergence_and_lifetime() -> Result<(), Error> {
    let mut f = CowForest::<Item, i32>::new(budget())?;
    let a = f.new_tree()?;
    let mut es = (0..300)
        .map(|i| BulkEntry {
            min: [i, i],
            max: [i + 2, i + 2],
            data: Some(Item(i as u64)),
        })
        .collect();
    f.bulk_load(&a, &mut es)?;
    let b = f.clone_tree(&a)?;
    let root = f.root_handle(&a)?.ok_or(Error::Invariant)?;
    assert_eq!(f.root_handle(&b)?, Some(root.clone()));
    assert_eq!(f.node(&root)?.refcount_load(), 2);
    assert!(!f.remove(&b, [0, 0], [0, 0], Some(Item(999)))?);
    assert_ne!(f.root_handle(&b)?, Some(root.clone())); // even failed removal writes path
    assert_eq!(f.node(&root)?.refcount_load(), 1);
    f.insert(&b, [900, 900], [901, 901], None)?;
    assert_eq!(f.len(&a)?, 300);
    assert_eq!(f.len(&b)?, 301);
    f.destroy(&a)?;
    assert_eq!(f.search(&b, [-1000, -1000], [1000, 1000], |_| true)?, 301);
    f.clear(&b)?;
    f.destroy(&b)?;
    assert!(f.node(&root).is_err()); // last chain lease destroyed ancestor's slab
    Ok(())
}
#[test]
fn distinct_stop_contracts() -> Result<(), Error> {
    let mut f = CowForest::<Item, i32, 2, 4>::new(budget())?;
    let a = f.new_tree()?;
    let mut es = (0..20)
        .map(|i| BulkEntry {
            min: [0, 0],
            max: [0, 0],
            data: Some(Item(i)),
        })
        .collect();
    f.bulk_load(&a, &mut es)?;
    assert_eq!(f.search(&a, [0, 0], [0, 0], |_| false)?, 5); // one per leaf, not global stop
    let mut b = Builder::<Item, i32, 2, 4>::new(100)?;
    for e in es {
        b.add(e.min, e.max, e.data)?;
    }
    let p = b.build()?;
    assert_eq!(p.search([0, 0], [0, 0], |_| false)?, 1);
    assert!(b.build()?.is_empty());
    Ok(())
}
#[test]
fn no_fixed_depth_limit_and_iterative_chain_destruction() -> Result<(), Error> {
    let mut f = CowForest::<Item, i32, 1, 3>::new(budget())?;
    let a = f.new_tree()?;
    for i in 0..400 {
        f.insert(&a, [i], [i], Some(Item(i as u64)))?;
    }
    let root = f.root_handle(&a)?.ok_or(Error::Invariant)?;
    assert!(f.node(&root)?.level > 64);
    let mut a = a;
    for _ in 0..2000 {
        let b = f.clone_tree(&a)?;
        f.destroy(&a)?;
        a = b;
    }
    assert_eq!(f.search(&a, [-1], [500], |_| true)?, 400);
    f.destroy(&a)?;
    Ok(())
}
#[test]
fn defined_extremes_and_exact_overflow_refusals() -> Result<(), Error> {
    let mut f = CowForest::<Item, i32, 2, 2>::new(budget())?;
    let a = f.new_tree()?;
    f.insert(&a, [i32::MIN; 2], [i32::MAX; 2], None)?;
    assert_eq!(f.search(&a, [0; 2], [0; 2], |_| true)?, 1);
    f.insert(&a, [0; 2], [0; 2], None)?;
    assert_eq!(f.insert(&a, [1; 2], [1; 2], None), Err(Error::Overflow)); // ELEMTYPE extent
    assert_eq!(f.len(&a), Err(Error::InterruptedMutation));
    f.clear(&a)?;
    assert!(f.is_empty(&a)?);
    let mut f = CowForest::<Item, i64, 2, 2>::new(budget())?;
    let a = f.new_tree()?;
    f.insert(&a, [i64::MAX; 2], [i64::MAX; 2], None)?;
    f.insert(&a, [i64::MAX; 2], [i64::MAX; 2], None)?;
    assert_eq!(
        f.insert(&a, [i64::MAX; 2], [i64::MAX; 2], None),
        Err(Error::Overflow)
    ); // center key
    let mut b = Builder::<Item, i64>::new(10)?;
    b.add([i64::MIN; 2], [i64::MAX; 2], None)?;
    b.add([i64::MAX; 2], [i64::MAX; 2], None)?;
    assert_eq!(b.build()?.len(), 2); // separate halves never overflow
    Ok(())
}
#[test]
fn budgets_moves_and_cursor_contracts() -> Result<(), Error> {
    let mut f = CowForest::<Item, i32>::new(Budget {
        entries: 1,
        ..budget()
    })?;
    let a = f.new_tree()?;
    f.insert(&a, [0; 2], [1; 2], Some(Item(8)))?;
    assert_eq!(
        f.insert(&a, [0; 2], [1; 2], None),
        Err(Error::ResourceLimit)
    );
    let mut it = f.begin(&a)?;
    let mut same = it.clone();
    same.advance()?;
    assert!(it != same);
    it.advance()?;
    it.advance()?;
    assert_eq!(it.current(), &Some(Item(8)));
    let b = f.move_construct(&a)?;
    assert!(f.is_empty(&a)?);
    assert_eq!(f.len(&b)?, 1);
    assert!(f.insert(&a, [0; 2], [1; 2], None).is_err()); // moved C++ allocator is null
    let mut b = Builder::<Item, i32>::new(1)?;
    b.add([0; 2], [1; 2], None)?;
    assert_eq!(b.add([0; 2], [1; 2], None), Err(Error::ResourceLimit));
    let mut p = b.build()?;
    let moved = kicad_rtree_cow_packed::PackedRtree::move_construct(&mut p)?;
    assert!(p.is_empty());
    let mut it = moved.begin();
    let old = it.post_advance()?;
    assert_eq!(old.current()?, &None);
    assert!(it.is_end());
    assert!(it.advance().is_err());
    Ok(())
}
