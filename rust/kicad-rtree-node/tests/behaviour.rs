// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour checks that need no oracle.  Exact agreement with the pinned C++ is
//! established by the difftest corpus (`port/cases/<unit>.jsonl`).

use kicad_rtree_node::{
    hilbert_nd, hilbert_nd2d, hilbert_xy2d, overlap_mask_2d, NodeData, Payload, RtreeError,
    RtreeNode, SlabArena, SlabNode,
};

#[derive(Clone, Debug, PartialEq)]
struct Item(u8);
impl NodeData for Item {}

type Node = RtreeNode<Item, i32, 2, 16>;
type Node3 = RtreeNode<Item, i32, 3, 16>;

#[test]
fn hilbert_order_one_visits_the_unit_square_in_curve_order() {
    let visits: Vec<u64> = [(0, 0), (0, 1), (1, 1), (1, 0)]
        .iter()
        .map(|&(x, y)| hilbert_xy2d(1, x, y).unwrap())
        .collect();
    assert_eq!(visits, [0, 1, 2, 3]);
}

#[test]
fn hilbert_refuses_undefined_shifts() {
    assert_eq!(hilbert_xy2d(0, 0, 0), Err(RtreeError::OrderOutOfDomain));
    assert_eq!(hilbert_xy2d(33, 0, 0), Err(RtreeError::OrderOutOfDomain));
    assert_eq!(
        hilbert_nd2d(33, &[0, 0, 0]),
        Err(RtreeError::OrderOutOfDomain)
    );
    assert_eq!(hilbert_nd2d(4, &[]), Err(RtreeError::DimensionsOutOfDomain));
    // Non-positive orders select no bits in the Morton fallback.
    assert_eq!(hilbert_nd(-3, &[u32::MAX; 3]), Ok(0));
    // 65 dimensions leave no bits per axis.
    assert_eq!(hilbert_nd(32, &[u32::MAX; 65]), Ok(0));
    assert_eq!(hilbert_nd(32, &[u32::MAX; 64]), Ok(u64::MAX));
}

#[test]
fn morton_refuses_only_the_undefined_intermediates() {
    // Above 64 dimensions no bit is selected and nothing is shifted.
    for order in [33, 64, i32::MAX, 0, -1, i32::MIN + 1] {
        assert_eq!(hilbert_nd(order, &[u32::MAX; 65]), Ok(0), "order {order}");
    }
    // bits - 1 overflows in the loop initializer.
    assert_eq!(
        hilbert_nd(i32::MIN, &[0; 65]),
        Err(RtreeError::OrderOutOfDomain)
    );
    assert_eq!(
        hilbert_nd(i32::MIN, &[0; 3]),
        Err(RtreeError::OrderOutOfDomain)
    );
    assert_eq!(hilbert_nd(i32::MIN + 1, &[u32::MAX; 3]), Ok(0));
    // Whenever the loop runs, the first shift is aOrder - 1.
    assert_eq!(hilbert_nd(33, &[0; 64]), Err(RtreeError::OrderOutOfDomain));
    assert_eq!(hilbert_nd(33, &[0; 1]), Err(RtreeError::OrderOutOfDomain));
    assert_eq!(hilbert_nd(32, &[u32::MAX; 1]), Ok(u64::from(u32::MAX)));
}

#[test]
fn overlap_mask_masks_by_count_and_checks_lanes() {
    let lo = [0, 10, 20, 30];
    let hi = [5, 15, 25, 35];
    let zero = [0; 4];
    let mask = overlap_mask_2d::<i32, 4>(&lo, &hi, &zero, &zero, 4, &[5, 0], &[20, 0]);
    assert_eq!(mask, Ok(0b0111));
    let mask = overlap_mask_2d::<i32, 4>(&lo, &hi, &zero, &zero, 2, &[5, 0], &[20, 0]);
    assert_eq!(mask, Ok(0b0011));
    let short = overlap_mask_2d::<i32, 4>(&lo[..3], &hi, &zero, &zero, 4, &[0, 0], &[0, 0]);
    assert_eq!(short, Err(RtreeError::SliceTooShort));
    let negative = overlap_mask_2d::<i32, 4>(&lo, &hi, &zero, &zero, -1, &[0, 0], &[0, 0]);
    assert_eq!(negative, Err(RtreeError::CountOutOfDomain));
}

#[test]
fn node_construction_and_predicates() {
    let mut node = Node::new();
    assert_eq!(Node::MIN_NODES, 6);
    assert!(node.is_leaf() && !node.is_internal() && node.is_underflow() && !node.is_full());
    assert_eq!(node.refcount_load(), 1);
    assert_eq!(node.payload(15), Ok(&Payload::Null));
    assert_eq!(
        node.compute_enclosing_bounds(),
        Ok(([i32::MAX; 2], [i32::MIN; 2]))
    );
    node.count = 16;
    node.level = -1;
    assert!(node.is_full() && !node.is_leaf() && !node.is_internal());
    node.count = 17;
    assert_eq!(
        node.compute_enclosing_bounds(),
        Err(RtreeError::CountOutOfDomain)
    );
}

#[test]
fn signed_overflow_is_refused_not_wrapped() {
    let mut node = Node::new();
    node.set_child_bounds(0, &[i32::MIN; 2], &[i32::MAX; 2])
        .unwrap();
    assert_eq!(node.child_area(0), Err(RtreeError::Overflow));
    assert_eq!(node.child_perimeter(0), Ok(4 * (i64::from(u32::MAX))));
    node.set_child_bounds(1, &[i32::MIN, 0], &[i32::MAX, 1])
        .unwrap();
    assert_eq!(node.child_area(1), Ok(i64::from(u32::MAX)));
    let mut wide = RtreeNode::<Item, i64, 2, 16>::new();
    wide.set_child_bounds(0, &[i64::MIN, 0], &[0, 1]).unwrap();
    assert_eq!(wide.child_area(0), Err(RtreeError::Overflow));
    let mut cube = Node3::new();
    cube.set_child_bounds(0, &[-(1 << 21); 3], &[1 << 21; 3])
        .unwrap();
    assert_eq!(cube.child_area(0), Err(RtreeError::Overflow));
}

#[test]
fn remove_child_moves_last_slot_and_keeps_stale_tail() {
    let mut node = Node::new();
    for slot in 0..3_u8 {
        let v = i32::from(slot);
        node.set_child_bounds(usize::from(slot), &[v; 2], &[v + 1; 2])
            .unwrap();
        node.set_insert_bounds(usize::from(slot), &[-v; 2], &[v; 2])
            .unwrap();
        node.set_data(usize::from(slot), Some(Item(slot))).unwrap();
    }
    node.count = 3;
    assert_eq!(node.remove_child(3), Err(RtreeError::SlotOutOfRange));
    node.remove_child(0).unwrap();
    assert_eq!(node.count, 2);
    assert_eq!(node.get_child_bounds(0), Ok(([2; 2], [3; 2])));
    assert_eq!(node.get_insert_bounds(0), Ok(([-2; 2], [2; 2])));
    assert_eq!(node.data(0), Ok(Some(&Item(2))));
    assert_eq!(
        node.data(2),
        Ok(Some(&Item(2))),
        "the vacated slot is not cleared"
    );
    assert_eq!(node.child(0), Err(RtreeError::InactiveUnionMember));
}

#[test]
fn n_dimensional_mask_shifts_only_overlapping_slots() {
    let mut node = RtreeNode::<Item, i32, 3, 33>::new();
    node.count = 33;
    let point = [1; 3];
    assert_eq!(node.child_overlap_mask(&point, &point), Ok(0));
    node.set_child_bounds(0, &point, &point).unwrap();
    assert_eq!(node.child_overlap_mask(&point, &point), Ok(1));
    // Every zero box overlaps the origin, including slot 32: 1U << 32 is undefined.
    assert_eq!(
        node.child_overlap_mask(&[0; 3], &[0; 3]),
        Err(RtreeError::CountOutOfDomain)
    );
    node.count = 34;
    assert_eq!(
        node.child_overlap_mask(&[9; 3], &[9; 3]),
        Err(RtreeError::CountOutOfDomain)
    );
}

#[test]
fn remove_child_last_slot_touches_no_array() {
    let mut node = Node::new();
    node.set_data(15, Some(Item(7))).unwrap();
    // i == count - 1 above MAXNODES: only count changes.
    node.count = 17;
    node.remove_child(16).unwrap();
    assert_eq!(node.count, 16);
    assert_eq!(node.data(15), Ok(Some(&Item(7))));
    node.count = 0;
    node.remove_child(-1).unwrap();
    assert_eq!(node.count, -1);
    node.count = i32::MAX;
    node.remove_child(i32::MAX - 1).unwrap();
    assert_eq!(node.count, i32::MAX - 1);
    // The explicit precondition assert( i < count ).
    node.count = i32::MIN;
    assert_eq!(node.remove_child(0), Err(RtreeError::SlotOutOfRange));
    // A copy needs both slots inside the arrays.
    node.count = 3;
    assert_eq!(node.remove_child(-1), Err(RtreeError::SlotOutOfRange));
    node.count = 18;
    assert_eq!(node.remove_child(16), Err(RtreeError::CountOutOfDomain));
    assert_eq!(node.count, 18);
}

#[test]
fn reference_layout_matches_the_pinned_linux_abi() {
    let node = Node::reference_layout().unwrap();
    assert_eq!((node.size, node.align), (656, 8));
    assert_eq!(SlabArena::<Node>::reference_page_size(), Ok(168_000));
}

#[test]
fn allocator_reuses_freed_nodes_last_in_first_out() {
    let mut arena = SlabArena::<Node>::new();
    let slab = arena.new_allocator().unwrap();
    assert_eq!(arena.memory_usage(&slab), Ok(0));
    let nodes: Vec<_> = (0..3).map(|_| arena.allocate(&slab).unwrap()).collect();
    assert_eq!(arena.find_owning_page(&slab, &nodes[2]), Ok(Some((0, 2))));
    arena.node_mut(&nodes[1]).unwrap().count = 9;
    arena.free(&slab, Some(&nodes[1])).unwrap();
    arena.free(&slab, Some(&nodes[0])).unwrap();
    assert_eq!(
        arena.free(&slab, Some(&nodes[0])),
        Err(RtreeError::NodeNotLive)
    );
    assert_eq!(arena.node(&nodes[1]).err(), Some(RtreeError::NodeNotLive));
    assert_eq!(
        arena.owns(&slab, &nodes[1]),
        Ok(true),
        "freed slots stay owned"
    );
    assert_eq!(arena.free(&slab, None), Ok(()));
    assert_eq!(arena.free_list_capacity(&slab), Ok(2));
    assert_eq!(arena.memory_usage(&slab), Ok(168_000 + 16));
    assert_eq!(arena.allocate(&slab), Ok(nodes[0].clone()));
    assert_eq!(arena.allocate(&slab), Ok(nodes[1].clone()));
    assert_eq!(arena.node(&nodes[1]).unwrap().count, 0, "re-constructed");
    assert_eq!(
        arena.free_list_capacity(&slab),
        Ok(2),
        "pop_back keeps capacity"
    );
}

#[test]
fn allocator_fills_pages_then_adds_one() {
    let mut arena = SlabArena::<Node>::new();
    let slab = arena.new_allocator().unwrap();
    for _ in 0..256 {
        arena.allocate(&slab).unwrap();
    }
    assert_eq!(arena.page_count(&slab), Ok(1));
    let next = arena.allocate(&slab).unwrap();
    assert_eq!(arena.find_owning_page(&slab, &next), Ok(Some((1, 0))));
    assert_eq!(arena.page_used_words(&slab, 1), Ok(Some([1, 0, 0, 0])));
}

#[test]
fn foreign_free_and_allocate_reconstruct_the_owners_slot() {
    // B.Free( A.Allocate() ); B.Allocate() in the pinned reference.
    let mut arena = SlabArena::<Node>::new();
    let (owner, borrower) = (
        arena.new_allocator().unwrap(),
        arena.new_allocator().unwrap(),
    );
    let node = arena.allocate(&owner).unwrap();
    arena.node_mut(&node).unwrap().count = 7;
    arena.free(&borrower, Some(&node)).unwrap();
    // markUnused found no page in the borrower: the owner's bit stays set.
    assert_eq!(arena.page_used_words(&owner, 0), Ok(Some([1, 0, 0, 0])));
    assert_eq!(arena.node(&node).err(), Some(RtreeError::NodeNotLive));
    assert_eq!(
        arena.free(&owner, Some(&node)),
        Err(RtreeError::NodeNotLive)
    );
    assert_eq!(arena.memory_usage(&borrower), Ok(8));
    let revived = arena.allocate(&borrower).unwrap();
    assert_eq!(revived, node);
    assert_eq!(arena.node(&node).unwrap().count, 0);
    assert_eq!(arena.owns(&owner, &node), Ok(true));
    assert_eq!(arena.owns(&borrower, &node), Ok(false));
    assert_eq!(arena.memory_usage(&owner), Ok(168_000));
    assert_eq!(arena.memory_usage(&borrower), Ok(8));
    // The owner can free it again normally, clearing its own bit.
    arena.free(&owner, Some(&node)).unwrap();
    assert_eq!(arena.page_used_words(&owner, 0), Ok(Some([0, 0, 0, 0])));
    assert_eq!(arena.allocate(&owner), Ok(node));
}

#[test]
fn moves_and_destruction_carry_pages_and_identity() {
    let mut arena = SlabArena::<Node>::new();
    let first = arena.new_allocator().unwrap();
    let node = arena.allocate(&first).unwrap();
    let second = arena.move_construct(&first).unwrap();
    assert_eq!(arena.owns(&first, &node), Ok(false));
    assert_eq!(arena.owns(&second, &node), Ok(true));
    assert_eq!(arena.memory_usage(&first), Ok(0));
    let third = arena.new_allocator().unwrap();
    let old = arena.allocate(&third).unwrap();
    arena.move_assign(&third, &third).unwrap();
    assert_eq!(arena.owns(&third, &old), Ok(true), "self move is a no-op");
    arena.move_assign(&third, &second).unwrap();
    assert_eq!(arena.owns(&third, &node), Ok(true));
    assert_eq!(arena.owns(&second, &node), Ok(false));
    assert_eq!(arena.node(&old).err(), Some(RtreeError::NodeDestroyed));
    // A foreign free-list entry into a page that is then destroyed cannot be reused.
    let borrower = arena.new_allocator().unwrap();
    arena.free(&borrower, Some(&node)).unwrap();
    arena.destroy_allocator(&third).unwrap();
    assert_eq!(arena.is_destroyed(&node), Ok(true));
    assert_eq!(arena.allocate(&borrower), Err(RtreeError::NodeDestroyed));
    assert_eq!(
        arena.free_list(&borrower).map(<[_]>::len),
        Ok(1),
        "refusal keeps state"
    );
    assert_eq!(
        arena.memory_usage(&third),
        Err(RtreeError::AllocatorDestroyed)
    );
    let fresh = arena.allocate(&first).unwrap();
    assert_ne!(fresh, node, "a destroyed page never matches a later one");
    let other = SlabArena::<Node>::new();
    assert_eq!(other.node(&fresh).err(), Some(RtreeError::ForeignArena));
}

#[test]
fn node_pairs_are_distinct_and_ordered() {
    let mut arena = SlabArena::<Node>::new();
    let slab = arena.new_allocator().unwrap();
    let nodes: Vec<_> = (0..258).map(|_| arena.allocate(&slab).unwrap()).collect();
    for (a, b) in [(3, 1), (1, 3), (257, 2), (2, 257)] {
        let (x, y) = arena.node_pair_mut(&nodes[a], &nodes[b]).unwrap();
        x.count = 100 + a as i32;
        y.count = 100 + b as i32;
        assert_eq!(arena.node(&nodes[a]).unwrap().count, 100 + a as i32);
        assert_eq!(arena.node(&nodes[b]).unwrap().count, 100 + b as i32);
    }
    assert_eq!(
        arena.node_pair_mut(&nodes[4], &nodes[4]).err(),
        Some(RtreeError::AliasedNodes)
    );
}

#[test]
fn refcount_wraps_like_cxx20_atomics() {
    let mut node = Node::new();
    node.refcount_store(i32::MAX);
    assert_eq!(node.refcount_fetch_add(1), i32::MAX);
    assert_eq!(node.refcount_load(), i32::MIN);
    assert_eq!(node.refcount_fetch_sub(1), i32::MIN);
    assert_eq!(node.refcount_load(), i32::MAX);
}
