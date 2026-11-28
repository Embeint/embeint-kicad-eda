// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/shape_index.h.
// Copyright (C) 2013 CERN, authors Jacobo Aragunde Pérez and Tomasz Wlostowski.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `SHAPE_INDEX` and its helper templates (libs/kimath/include/geometry/shape_index.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.ui.p004`: a spatial index of items over the
//! accepted copy-on-write R-tree (`KIRTREE::COW_RTREE<T, int, 2>`, [`kicad_rtree_cow_packed`]).
//!
//! The C++ item type `T` is a pointer; here it is `Option<P>` (`None` for `nullptr`) with `P` a
//! [`ShapeItem`] that the tree can store ([`NodeData`]). The tree lives in a [`CowForest`]
//! (the accepted crate's explicit shared storage), which each operation borrows; a
//! [`ShapeIndex`] holds its tree's identity and the shape layer. [`ShapeIndex::destroy`] is the
//! destructor.
//!
//! A failed `wxCHECK` in `shapeFunctor`, `boundingBox` or `collide` first calls the wx assertion
//! handler, a host effect no unit ports: the caller supplies it as `on_assert`, which receives
//! the arguments of that `wxOnAssert` call ([`WxCheck`]). An error stops the evaluation there; `Ok`
//! continues to the check's `return` as the reference does when the handler returns. The
//! production callee is [`deferred_assert`].
use core::marker::PhantomData;

use kicad_rtree_cow_packed::{
    BulkEntry, CowCursor, CowForest, Error as TreeError, NodeData, TreeId,
};

use crate::box2::Box2I;
use crate::seg::Seg;
use crate::shape::{Shape, ShapeError};
pub use crate::shape_index_list::ShapeItem;
use crate::shape_line_chain::UNIT;
use crate::vector::Vector2I;

/// `KIRTREE::COW_RTREE<T, int, 2>` storage: `int` coordinates, two dimensions, the default
/// 16 entries per node.
pub type IndexForest<P> = CowForest<P, i32, 2, 16>;

/// The upstream symbol of the host assertion effect of a failed `wxCHECK`.
pub const WX_ON_ASSERT: &str = "wxOnAssert";

/// The arguments one `wxCHECK( cond, rc )` passes to `wxOnAssert` when `wxTheAssertHandler` is
/// set (wx 3.2: `wxCHECK_MSG( cond, rc, (const char*)NULL )` -> `wxCHECK2_MSG` ->
/// `wxFAIL_COND_MSG( #cond, msg )` -> `wxFAIL_COND_MSG_AT`, which stringifies the already
/// stringified condition). `__FILE__` is the compiler's spelling of the header path and is left
/// to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WxCheck {
    /// `__LINE__` in the pinned shape_index.h.
    pub line: u32,
    /// `__WXFUNCTION__` (GCC's `__FUNCTION__`: the unqualified template name).
    pub function: &'static str,
    /// `#cond` of the stringified condition: the source text in double quotes.
    pub condition: &'static str,
}

/// `wxCHECK( aItem, nullptr )` in `shapeFunctor`.
pub const WX_CHECK_SHAPE_FUNCTOR: WxCheck = WxCheck {
    line: 46,
    function: "shapeFunctor",
    condition: "\"aItem\"",
};
/// `wxCHECK( shape, bBox )` in `boundingBox`.
pub const WX_CHECK_BOUNDING_BOX: WxCheck = WxCheck {
    line: 66,
    function: "boundingBox",
    condition: "\"shape\"",
};
/// `wxCHECK( shape, false )` in `collide`.
pub const WX_CHECK_COLLIDE: WxCheck = WxCheck {
    line: 104,
    function: "collide",
    condition: "\"shape\"",
};

/// Why a shape index operation produced no value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShapeIndexError {
    /// The pinned C++ evaluation is undefined for this input.
    Undefined,
    /// A dependency call (or the host assertion handler) that is not ported.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
    /// A tree failure that is not a reference outcome (a host budget, an interrupted mutation).
    Tree(TreeError),
}

pub type ShapeIndexResult<T> = Result<T, ShapeIndexError>;

impl From<ShapeError> for ShapeIndexError {
    fn from(error: ShapeError) -> Self {
        match error {
            ShapeError::Undefined => Self::Undefined,
            ShapeError::Unported { upstream, unit } => Self::Unported { upstream, unit },
        }
    }
}

impl From<crate::vector::VectorDomainError> for ShapeIndexError {
    fn from(error: crate::vector::VectorDomainError) -> Self {
        ShapeError::from(error).into()
    }
}

impl From<TreeError> for ShapeIndexError {
    fn from(error: TreeError) -> Self {
        match error {
            // The reference evaluation is undefined (a signed overflow, a floating conversion).
            TreeError::Overflow | TreeError::Conversion => Self::Undefined,
            TreeError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            other => Self::Tree(other),
        }
    }
}

/// The `wxOnAssert` callee: `Ok` when the host's handler returned (and did not request
/// `wxTrap()`), an error to stop the evaluation there.
pub type OnAssert<'c> = dyn FnMut(&WxCheck) -> ShapeIndexResult<()> + 'c;

/// The production `wxOnAssert`: deferred. No unit ports the wx assertion handler; the call is
/// reached from this unit's own bodies, so the deferral names this unit.
pub fn deferred_assert(_check: &WxCheck) -> ShapeIndexResult<()> {
    // UNPORTED(libs.kimath.include.geometry.application.ui.p004): wxOnAssert
    Err(ShapeIndexError::Unported {
        upstream: WX_ON_ASSERT,
        unit: UNIT,
    })
}

/// `shapeFunctor( T aItem, int aLayer )`: `aItem->Shape( aLayer )`, or `nullptr` after a failed
/// `wxCHECK( aItem, nullptr )` for a null item.
pub fn shape_functor<'a, P: ShapeItem<'a>>(
    item: Option<P>,
    layer: i32,
    on_assert: &mut OnAssert<'_>,
) -> ShapeIndexResult<Option<&'a dyn Shape>> {
    let Some(item) = item else {
        on_assert(&WX_CHECK_SHAPE_FUNCTOR)?;
        return Ok(None);
    };
    Ok(item.shape(layer))
}

/// `boundingBox( T aObject, int aLayer )`: the shape's `BBox()`, or a default `BOX2I` after a
/// failed `wxCHECK( shape, bBox )`.
pub fn bounding_box<'a, P: ShapeItem<'a>>(
    object: Option<P>,
    layer: i32,
    on_assert: &mut OnAssert<'_>,
) -> ShapeIndexResult<Box2I> {
    let shape = shape_functor(object, layer, on_assert)?;

    let mut b_box = Box2I::default();
    let Some(shape) = shape else {
        on_assert(&WX_CHECK_BOUNDING_BOX)?;
        return Ok(b_box);
    };

    b_box = shape.bbox(0)?;
    Ok(b_box)
}

/// `acceptVisitor( T aObject, V aVisitor )`: `aVisitor( aObject )`, on the visitor copy passed
/// by value (its own state changes are lost with it).
pub fn accept_visitor<T, V: FnMut(T)>(object: T, mut visitor: V) {
    visitor(object);
}

/// The overloads `collide` selects for `shape->Collide( aAnotherObject, aMinDistance )`.
pub trait CollideTarget {
    /// The call with the remaining arguments at their defaults.
    fn collide_with(&self, shape: &dyn Shape, min_distance: i32) -> ShapeIndexResult<bool>;
}

impl CollideTarget for Vector2I {
    /// `Collide( const VECTOR2I&, int, int* = nullptr, VECTOR2I* = nullptr )`.
    fn collide_with(&self, shape: &dyn Shape, min_distance: i32) -> ShapeIndexResult<bool> {
        Ok(shape.collide_point(*self, min_distance, None, None)?)
    }
}

impl CollideTarget for Seg {
    /// `Collide( const SEG&, int, int* = nullptr, VECTOR2I* = nullptr )`.
    fn collide_with(&self, shape: &dyn Shape, min_distance: i32) -> ShapeIndexResult<bool> {
        Ok(shape.collide_seg(self, min_distance, None, None)?)
    }
}

impl CollideTarget for &dyn Shape {
    /// `Collide( const SHAPE*, int, int* = nullptr, VECTOR2I* = nullptr )`.
    fn collide_with(&self, shape: &dyn Shape, min_distance: i32) -> ShapeIndexResult<bool> {
        Ok(shape.collide_shape(*self, min_distance, None, None)?)
    }
}

/// `collide( T aObject, U aAnotherObject, int aLayer, int aMinDistance )`: the object's shape
/// collides with `aAnotherObject`, or `false` after a failed `wxCHECK( shape, false )`.
pub fn collide<'a, P: ShapeItem<'a>, U: CollideTarget>(
    object: Option<P>,
    another_object: U,
    layer: i32,
    min_distance: i32,
    on_assert: &mut OnAssert<'_>,
) -> ShapeIndexResult<bool> {
    let shape = shape_functor(object, layer, on_assert)?;

    let Some(shape) = shape else {
        on_assert(&WX_CHECK_COLLIDE)?;
        return Ok(false);
    };

    another_object.collide_with(shape, min_distance)
}

/// `queryCallback( T aShape, void* aContext )`: `acceptVisitor` on a copy of the visitor the
/// context points to, then `true`.
pub fn query_callback<T, V: FnMut(T) + Clone>(shape: T, context: &V) -> bool {
    let visitor = context;
    accept_visitor(shape, visitor.clone());
    true
}

/// `SHAPE_INDEX<T>`.
#[derive(Debug)]
pub struct ShapeIndex<P: NodeData> {
    /// `m_tree`.
    tree: TreeId,
    /// `m_shapeLayer`.
    shape_layer: i32,
    _item: PhantomData<P>,
}

/// `SHAPE_INDEX::Iterator`: a pair of tree iterators, the current and the end.
#[derive(Clone)]
pub struct ShapeIndexIterator<'f, P: NodeData> {
    /// `m_current`.
    current: CowCursor<'f, P, i32, 2, 16>,
    /// `m_end`.
    end: CowCursor<'f, P, i32, 2, 16>,
}

impl<P: NodeData> ShapeIndexIterator<'_, P> {
    /// `operator*`: `*m_current`, the last item reached (a value-initialised `nullptr` before
    /// any).
    pub fn deref(&self) -> Option<P> {
        self.current.current().clone()
    }

    /// `operator++()` and `operator++( int )`: advance, then `m_current != m_end` (tree iterators
    /// compare only whether they are at the end).
    pub fn increment(&mut self) -> ShapeIndexResult<bool> {
        self.current.advance()?;
        Ok(self.current != self.end)
    }

    /// `IsNull`: at the end.
    pub fn is_null(&self) -> bool {
        self.current == self.end
    }

    /// `IsNotNull`.
    pub fn is_not_null(&self) -> bool {
        self.current != self.end
    }

    /// `Next`: the current item, then advance.
    pub fn next_item(&mut self) -> ShapeIndexResult<Option<P>> {
        let object = self.current.current().clone();
        self.current.advance()?;
        Ok(object)
    }
}

impl<'a, P: NodeData + ShapeItem<'a>> ShapeIndex<P> {
    /// `explicit SHAPE_INDEX( int aLayer )`: an empty tree.
    pub fn new(forest: &mut IndexForest<P>, layer: i32) -> ShapeIndexResult<Self> {
        Ok(Self {
            tree: forest.new_tree()?,
            shape_layer: layer,
            _item: PhantomData,
        })
    }

    /// `~SHAPE_INDEX()`: the tree's destructor.
    pub fn destroy(self, forest: &mut IndexForest<P>) -> ShapeIndexResult<()> {
        forest.destroy(&self.tree)?;
        Ok(())
    }

    /// The layer the index asks its items' shapes for.
    pub const fn shape_layer(&self) -> i32 {
        self.shape_layer
    }

    /// The tree's identity in the forest, for observation.
    pub fn tree(&self) -> &TreeId {
        &self.tree
    }

    /// `Clone`: `SHAPE_INDEX clone( m_shapeLayer )`, then `clone.m_tree = m_tree.Clone()` (a
    /// move assignment from the temporary clone, which is then destroyed).
    pub fn clone_index(&self, forest: &mut IndexForest<P>) -> ShapeIndexResult<Self> {
        let clone = Self::new(forest, self.shape_layer)?;
        let temporary = forest.clone_tree(&self.tree)?;
        forest.move_assign(&clone.tree, &temporary)?;
        forest.destroy(&temporary)?;
        Ok(clone)
    }

    /// `int min[2] = { box.GetX(), box.GetY() }; int max[2] = { box.GetRight(), box.GetBottom() }`.
    fn corners(b: &Box2I) -> ShapeIndexResult<([i32; 2], [i32; 2])> {
        Ok(([b.x(), b.y()], [b.right()?, b.bottom()?]))
    }

    /// `Add( T aShape )`: insert under the item's bounding box on the index layer.
    pub fn add(
        &mut self,
        forest: &mut IndexForest<P>,
        shape: Option<P>,
        on_assert: &mut OnAssert<'_>,
    ) -> ShapeIndexResult<()> {
        let b = bounding_box(shape, self.shape_layer, on_assert)?;
        let (min, max) = Self::corners(&b)?;

        forest.insert(&self.tree, min, max, shape)?;
        Ok(())
    }

    /// `Add( T aShape, const BOX2I& aBbox )`: insert under an alternate box.
    pub fn add_with_bbox(
        &mut self,
        forest: &mut IndexForest<P>,
        shape: Option<P>,
        bbox: &Box2I,
    ) -> ShapeIndexResult<()> {
        let (min, max) = Self::corners(bbox)?;

        forest.insert(&self.tree, min, max, shape)?;
        Ok(())
    }

    /// `Remove( T aShape )`: remove the item found under its current bounding box (the tree's
    /// result is discarded).
    pub fn remove(
        &mut self,
        forest: &mut IndexForest<P>,
        shape: Option<P>,
        on_assert: &mut OnAssert<'_>,
    ) -> ShapeIndexResult<()> {
        let b = bounding_box(shape, self.shape_layer, on_assert)?;
        let (min, max) = Self::corners(&b)?;

        forest.remove(&self.tree, min, max, shape)?;
        Ok(())
    }

    /// `RemoveAll`.
    pub fn remove_all(&mut self, forest: &mut IndexForest<P>) -> ShapeIndexResult<()> {
        forest.clear(&self.tree)?;
        Ok(())
    }

    /// `Accept( V aVisitor )`: `acceptVisitor( item, aVisitor )` for every item in tree order,
    /// each on its own copy of this call's visitor.
    pub fn accept<V: FnMut(Option<P>) + Clone>(
        &self,
        forest: &IndexForest<P>,
        visitor: V,
    ) -> ShapeIndexResult<()> {
        let mut it = forest.begin(&self.tree)?;
        let end = forest.end();
        while it != end {
            accept_visitor(*it.current(), visitor.clone());
            it.advance()?;
        }
        Ok(())
    }

    /// `BulkLoad( std::vector<std::pair<T, BOX2I>>& aItems )`: replace the content by a
    /// Hilbert-packed tree of the items under the given boxes.
    pub fn bulk_load(
        &mut self,
        forest: &mut IndexForest<P>,
        items: &[(Option<P>, Box2I)],
    ) -> ShapeIndexResult<()> {
        let mut entries = Vec::new();
        entries
            .try_reserve(items.len())
            .map_err(|_| ShapeIndexError::Tree(TreeError::ResourceLimit))?;

        for (item, b) in items {
            let (min, max) = Self::corners(b)?;
            entries.push(BulkEntry {
                min,
                max,
                data: *item,
            });
        }

        forest.bulk_load(&self.tree, &mut entries)?;
        Ok(())
    }

    /// `Reindex`: collect the items in tree order, empty the tree and `Add` each again.
    pub fn reindex(
        &mut self,
        forest: &mut IndexForest<P>,
        on_assert: &mut OnAssert<'_>,
    ) -> ShapeIndexResult<()> {
        let mut items = Vec::new();
        items
            .try_reserve(forest.len(&self.tree)?)
            .map_err(|_| ShapeIndexError::Tree(TreeError::ResourceLimit))?;

        {
            let mut it = forest.begin(&self.tree)?;
            let end = forest.end();
            while it != end {
                items.push(*it.current());
                it.advance()?;
            }
        }

        forest.clear(&self.tree)?;

        for item in items {
            self.add(forest, item, on_assert)?;
        }
        Ok(())
    }

    /// `Query( const SHAPE* aShape, int aMinDistance, V& aVisitor )`: search the tree under
    /// `aShape->BBox()` inflated by `aMinDistance`; the tree's visitor protocol applies (a
    /// `false` return stops the current leaf). Returns the tree's count.
    pub fn query<V: FnMut(&Option<P>) -> bool>(
        &self,
        forest: &IndexForest<P>,
        shape: &dyn Shape,
        min_distance: i32,
        visitor: &mut V,
    ) -> ShapeIndexResult<i32> {
        let mut b = shape.bbox(0)?;
        b.inflate_uniform(min_distance)?;

        let (min, max) = Self::corners(&b)?;

        Ok(forest.search(&self.tree, min, max, visitor)?)
    }

    /// `Begin`: an iterator from the first item.
    pub fn begin_iterator<'f>(
        &self,
        forest: &'f IndexForest<P>,
    ) -> ShapeIndexResult<ShapeIndexIterator<'f, P>> {
        Ok(ShapeIndexIterator {
            current: forest.begin(&self.tree)?,
            end: forest.end(),
        })
    }

    /// `begin()`: the tree's iterator.
    pub fn begin<'f>(
        &self,
        forest: &'f IndexForest<P>,
    ) -> ShapeIndexResult<CowCursor<'f, P, i32, 2, 16>> {
        Ok(forest.begin(&self.tree)?)
    }

    /// `end()`.
    pub fn end<'f>(&self, forest: &'f IndexForest<P>) -> CowCursor<'f, P, i32, 2, 16> {
        forest.end()
    }

    /// `Size`: the tree's count.
    pub fn size(&self, forest: &IndexForest<P>) -> ShapeIndexResult<usize> {
        Ok(forest.len(&self.tree)?)
    }
}
