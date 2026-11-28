// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/shape_index_list.h.
// Copyright (C) 2013 CERN, author Tomasz Wlostowski.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `SHAPE_INDEX_LIST` (libs/kimath/include/geometry/shape_index_list.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.ui.p004`: a flat list of items with their shapes and
//! bounding boxes, queried by box distance and, optionally, an exact collision test.
//!
//! The C++ template parameter `T` is the item type (a pointer in every pinned use) and
//! `ShapeFunctor` the function giving an item's shape; here they are `T` and `F`. A pointer item
//! is `Option<P>`, `None` being `nullptr`, with [`ShapeItem`] giving `aItem->Shape( aLayer )`.
//!
//! The two iterator classes keep the C++ positions (`std::vector` iterators, which libstdc++
//! represents and compares as element pointers): a position is the list's allocation together
//! with an offset into it. `None` is a null pointer, which is what a default-constructed iterator
//! holds and what `begin()`/`end()` of a list that never allocated return (libstdc++ and Rust both
//! allocate on the first insertion and keep the allocation through `erase`/`clear`). Iterators of
//! two lists that are both allocated therefore never compare equal, even at the same offset.
//! Dereferencing the end, or incrementing past it, is undefined and refused. Iterators borrow the
//! list, so the C++ invalidation of iterators by `Add`/`Remove`/`Clear` cannot occur.
use crate::box2::Box2I;
use crate::shape::{Shape, ShapeError, ShapeResult};

/// An item that can give its shape on a layer: `aItem->Shape( aLayer )` for a non-null pointer
/// item.
pub trait ShapeItem<'a>: Copy + PartialEq {
    /// The item's shape on `layer`, `None` for `nullptr`.
    fn shape(&self, layer: i32) -> Option<&'a dyn Shape>;
}

/// `defaultShapeFunctor( const T aItem )`: `aItem->Shape( -1 )`, undefined for a null item.
pub fn default_shape_functor<'a, P: ShapeItem<'a>>(
    item: Option<P>,
) -> ShapeResult<Option<&'a dyn Shape>> {
    match item {
        Some(p) => Ok(p.shape(-1)),
        None => Err(ShapeError::Undefined),
    }
}

/// `SHAPE_INDEX_LIST::SHAPE_ENTRY`: an item, its shape and the shape's `BBox( 0 )`.
#[derive(Clone, Copy, Debug)]
pub struct ShapeEntry<'a, T> {
    /// `parent`.
    pub parent: T,
    /// `shape`.
    pub shape: &'a dyn Shape,
    /// `bbox`.
    pub bbox: Box2I,
}

impl<'a, T: Copy> ShapeEntry<'a, T> {
    /// `SHAPE_ENTRY( T aParent )`: `shape = ShapeFunctor( aParent )`, `bbox = shape->BBox( 0 )`
    /// (a null shape is undefined), `parent = aParent`.
    pub fn new<F>(parent: T, functor: &F) -> ShapeResult<Self>
    where
        F: Fn(T) -> ShapeResult<Option<&'a dyn Shape>>,
    {
        let shape = functor(parent)?.ok_or(ShapeError::Undefined)?;
        let bbox = shape.bbox(0)?;
        Ok(Self {
            parent,
            shape,
            bbox,
        })
    }
}

/// `SHAPE_INDEX_LIST<T, ShapeFunctor>`.
pub struct ShapeIndexList<'a, T, F> {
    /// `m_shapes`.
    shapes: Vec<ShapeEntry<'a, T>>,
    /// The `ShapeFunctor` template argument.
    functor: F,
}

/// The default `ShapeFunctor` argument for a pointer item `Option<P>`.
pub type DefaultFunctor<'a, P> = fn(Option<P>) -> ShapeResult<Option<&'a dyn Shape>>;

impl<'a, P: ShapeItem<'a>> ShapeIndexList<'a, Option<P>, DefaultFunctor<'a, P>> {
    /// An empty list with `defaultShapeFunctor<T>`.
    pub fn new() -> Self {
        Self::with_functor(default_shape_functor::<P>)
    }
}

impl<'a, P: ShapeItem<'a>> Default for ShapeIndexList<'a, Option<P>, DefaultFunctor<'a, P>> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T, F> ShapeIndexList<'a, T, F>
where
    T: Copy + PartialEq,
    F: Fn(T) -> ShapeResult<Option<&'a dyn Shape>>,
{
    /// An empty list with the given `ShapeFunctor`.
    pub fn with_functor(functor: F) -> Self {
        Self {
            shapes: Vec::new(),
            functor,
        }
    }

    /// The entries (`m_shapes`), for observation.
    pub fn entries(&self) -> &[ShapeEntry<'a, T>] {
        &self.shapes
    }

    /// `Add( T aItem )`: a new entry, appended.
    pub fn add(&mut self, item: T) -> ShapeResult<()> {
        let s = ShapeEntry::new(item, &self.functor)?;
        self.shapes.push(s);
        Ok(())
    }

    /// `Remove( const T aItem )`: erase the first entry whose parent equals `aItem`, if any.
    pub fn remove(&mut self, item: T) {
        if let Some(i) = self.shapes.iter().position(|s| s.parent == item) {
            self.shapes.remove(i);
        }
    }

    /// `Size`: `m_shapes.size()` converted to `int`.
    pub fn size(&self) -> i32 {
        self.shapes.len() as i32
    }

    /// `Query( const SHAPE* aShape, int aMinDistance, Visitor& aV, bool aExact = true )`: for each
    /// entry, in order, whose box is within `aMinDistance` of `aShape->BBox()` (squared, as
    /// `int64_t`) and, when exact, whose shape collides with `aShape`, count it and call the
    /// visitor; stop after the visitor returns `false`. Returns the count.
    pub fn query<V>(
        &self,
        shape: &dyn Shape,
        min_distance: i32,
        visitor: &mut V,
        exact: bool,
    ) -> ShapeResult<i32>
    where
        V: FnMut(T) -> bool,
    {
        let mut n: i32 = 0;
        let min_dist_sq = i64::from(min_distance) * i64::from(min_distance);

        let ref_bbox = shape.bbox(0)?;

        for i in &self.shapes {
            if ref_bbox.squared_distance_box(&i.bbox)? <= min_dist_sq
                && (!exact || i.shape.collide_shape(shape, min_distance, None, None)?)
            {
                n = n.checked_add(1).ok_or(ShapeError::Undefined)?;

                if !visitor(i.parent) {
                    return Ok(n);
                }
            }
        }

        Ok(n)
    }

    /// `Clear`.
    pub fn clear(&mut self) {
        self.shapes.clear();
    }

    /// The position of `begin()`: a null pointer for a list that never allocated.
    fn begin_pos(&self) -> Option<usize> {
        (self.shapes.capacity() != 0).then_some(0)
    }

    /// The position of `end()`.
    fn end_pos(&self) -> Option<usize> {
        (self.shapes.capacity() != 0).then_some(self.shapes.len())
    }

    /// `qbegin( SHAPE* aShape, int aMinDistance, bool aExact )`.
    pub fn qbegin<'l>(
        &'l self,
        shape: Option<&'l dyn Shape>,
        min_distance: i32,
        exact: bool,
    ) -> ShapeResult<QueryIterator<'l, 'a, T>> {
        QueryIterator::new(
            &self.shapes,
            self.begin_pos(),
            self.end_pos(),
            shape,
            min_distance,
            exact,
        )
    }

    /// `qend()`: `query_iterator( m_shapes.end(), m_shapes.end(), nullptr, 0, false )`.
    pub fn qend<'l>(&'l self) -> ShapeResult<QueryIterator<'l, 'a, T>> {
        QueryIterator::new(&self.shapes, self.end_pos(), self.end_pos(), None, 0, false)
    }

    /// `begin()`.
    pub fn begin<'l>(&'l self) -> ListIterator<'l, 'a, T> {
        ListIterator {
            entries: &self.shapes,
            current: self.begin_pos(),
        }
    }

    /// `end()`.
    pub fn end<'l>(&'l self) -> ListIterator<'l, 'a, T> {
        ListIterator {
            entries: &self.shapes,
            current: self.end_pos(),
        }
    }
}

/// The entry at a position: undefined for a null pointer or the end.
fn entry<'l, 'a, T>(
    entries: &'l [ShapeEntry<'a, T>],
    current: Option<usize>,
) -> ShapeResult<&'l ShapeEntry<'a, T>> {
    current
        .and_then(|i| entries.get(i))
        .ok_or(ShapeError::Undefined)
}

/// `m_current == aRhs.m_current` for two `std::vector` iterators: libstdc++'s `__normal_iterator`
/// compares the element pointers. Two null pointers are equal and a null pointer equals no element
/// pointer. Element pointers are equal when they address the same allocation at the same offset;
/// the allocation is identified by its address, which is only compared, never observed, and is
/// distinct for every simultaneously live list. (A one-past-the-end pointer of one allocation
/// never equals an element pointer of another with glibc's allocator, whose chunk header lies
/// between them.)
fn same_position<T>(
    entries: &[ShapeEntry<'_, T>],
    current: Option<usize>,
    rhs_entries: &[ShapeEntry<'_, T>],
    rhs_current: Option<usize>,
) -> bool {
    match (current, rhs_current) {
        (None, None) => true,
        (Some(i), Some(j)) => i == j && core::ptr::eq(entries.as_ptr(), rhs_entries.as_ptr()),
        _ => false,
    }
}

/// `++m_current`: undefined for a null pointer or the end.
fn increment<T>(entries: &[ShapeEntry<'_, T>], current: &mut Option<usize>) -> ShapeResult<()> {
    match *current {
        Some(i) if i < entries.len() => {
            *current = Some(i + 1);
            Ok(())
        }
        _ => Err(ShapeError::Undefined),
    }
}

/// `SHAPE_INDEX_LIST::iterator`: the "normal" iterator over every entry.
#[derive(Clone, Copy, Debug)]
pub struct ListIterator<'l, 'a, T> {
    entries: &'l [ShapeEntry<'a, T>],
    /// `m_current`.
    current: Option<usize>,
}

impl<'l, 'a, T: Copy> ListIterator<'l, 'a, T> {
    /// `iterator()`: a value-initialised (null) vector iterator.
    pub fn null() -> Self {
        Self {
            entries: &[],
            current: None,
        }
    }

    /// `iterator( const iterator& aB )` and `operator=`: the position (the C++ assignment
    /// returns `const iterator&`, which is `*self` here).
    pub fn assign_from(&mut self, rhs: &Self) -> &Self {
        *self = *rhs;
        self
    }

    /// `operator*`: the entry's parent.
    pub fn deref(&self) -> ShapeResult<T> {
        Ok(entry(self.entries, self.current)?.parent)
    }

    /// `operator++()` (and `operator++( int )`, which returns `*this` after the increment).
    pub fn increment(&mut self) -> ShapeResult<&mut Self> {
        increment(self.entries, &mut self.current)?;
        Ok(self)
    }
}

impl<T> PartialEq for ListIterator<'_, '_, T> {
    /// `operator==` (and `operator!=`): the same `std::vector` iterator (allocation and offset).
    fn eq(&self, rhs: &Self) -> bool {
        same_position(self.entries, self.current, rhs.entries, rhs.current)
    }
}

/// `SHAPE_INDEX_LIST::query_iterator`: iterates the entries matching a query shape.
///
/// A default-constructed iterator leaves `m_exact`, `m_shape` and `m_minDistance`
/// uninitialised, so copying it is undefined; those members are `None` here until set.
#[derive(Clone, Copy, Debug)]
pub struct QueryIterator<'l, 'a, T> {
    entries: &'l [ShapeEntry<'a, T>],
    /// `m_end`.
    end: Option<usize>,
    /// `m_current`.
    current: Option<usize>,
    /// `m_refBBox`.
    ref_bbox: Box2I,
    /// `m_exact` (indeterminate when `None`).
    exact: Option<bool>,
    /// `m_shape` (indeterminate when `None`; `Some( None )` is `nullptr`).
    shape: Option<Option<&'l dyn Shape>>,
    /// `m_minDistance` (indeterminate when `None`).
    min_distance: Option<i32>,
}

impl<'l, 'a, T: Copy> QueryIterator<'l, 'a, T> {
    /// `query_iterator()`: null positions, a default `BOX2I`, the rest uninitialised.
    pub fn null() -> Self {
        Self {
            entries: &[],
            end: None,
            current: None,
            ref_bbox: Box2I::default(),
            exact: None,
            shape: None,
            min_distance: None,
        }
    }

    /// `query_iterator( SHAPE_VEC_ITER aCurrent, SHAPE_VEC_ITER aEnd, SHAPE* aShape,
    /// int aMinDistance, bool aExact )`: with a shape, `m_refBBox = aShape->BBox()` and the first
    /// match is found.
    fn new(
        entries: &'l [ShapeEntry<'a, T>],
        current: Option<usize>,
        end: Option<usize>,
        shape: Option<&'l dyn Shape>,
        min_distance: i32,
        exact: bool,
    ) -> ShapeResult<Self> {
        let mut it = Self {
            entries,
            end,
            current,
            ref_bbox: Box2I::default(),
            exact: Some(exact),
            shape: Some(shape),
            min_distance: Some(min_distance),
        };
        if let Some(s) = shape {
            it.ref_bbox = s.bbox(0)?;
            it.next()?;
        }
        Ok(it)
    }

    /// The copy constructor: every member is read, so a default-constructed source is
    /// undefined.
    pub fn try_copy(&self) -> ShapeResult<Self> {
        if self.exact.is_none() || self.shape.is_none() || self.min_distance.is_none() {
            return Err(ShapeError::Undefined);
        }
        Ok(*self)
    }

    /// `operator=( const query_iterator& aRhs )`: every member of `aRhs` is read.
    pub fn assign_from(&mut self, rhs: &Self) -> ShapeResult<&Self> {
        *self = rhs.try_copy()?;
        Ok(self)
    }

    /// `operator*`: the entry's parent.
    pub fn deref(&self) -> ShapeResult<T> {
        Ok(entry(self.entries, self.current)?.parent)
    }

    /// `operator++()` (and `operator++( int )`): step, then find the next match.
    pub fn increment(&mut self) -> ShapeResult<&mut Self> {
        increment(self.entries, &mut self.current)?;
        self.next()?;
        Ok(self)
    }

    /// `next()`: advance to the first entry from `m_current` whose box `Distance` (rounded, as
    /// `int64_t`) is at most `m_minDistance` and, when exact, whose shape collides with
    /// `m_shape`.
    fn next(&mut self) -> ShapeResult<()> {
        let min_distance = self.min_distance.ok_or(ShapeError::Undefined)?;
        let exact = self.exact.ok_or(ShapeError::Undefined)?;
        let shape = self.shape.ok_or(ShapeError::Undefined)?;

        while self.current != self.end {
            let e = entry(self.entries, self.current)?;

            if self.ref_bbox.distance_box(&e.bbox)? <= i64::from(min_distance) {
                if !exact {
                    return Ok(());
                }
                // A null m_shape reaches SHAPE::Collide( nullptr, ... ), which dereferences it
                // (shape_collisions.cpp switches on aShape->Type()).
                let shape = shape.ok_or(ShapeError::Undefined)?;
                if e.shape.collide_shape(shape, min_distance, None, None)? {
                    return Ok(());
                }
            }

            increment(self.entries, &mut self.current)?;
        }
        Ok(())
    }
}

impl<T> PartialEq for QueryIterator<'_, '_, T> {
    /// `operator==` (and `operator!=`): the same `m_current` (allocation and offset). Only
    /// `m_current` is read, which a default-constructed iterator value-initialises, so comparing
    /// one is defined.
    fn eq(&self, rhs: &Self) -> bool {
        same_position(self.entries, self.current, rhs.entries, rhs.current)
    }
}
