// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/shape_compound.h.
// Copyright (C) 2020 CERN, author Tomasz Wlostowski.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `SHAPE_COMPOUND` inline members (libs/kimath/include/geometry/shape_compound.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.ui.p003`.
//!
//! Inline: the default constructor, both `AddShape` overloads, `Shapes`, `Empty`, `Size`,
//! `UniqueSubshape`, the indexable-subshape overrides and the two `Collide( const SHAPE* ... )`
//! forwarders to `SHAPE::Collide`. The vector constructor, copy constructor, destructor,
//! `Clone`, `Format`, `Collide( SEG )`, `BBox`, `Distance( SEG )`, `Move`, `Rotate`, `IsSolid`
//! and `TransformToPolygon` are in shape_compound.cpp
//! (`libs.kimath.src.geometry.shape_compound.application.ui.p001`, not ported) and return
//! [`crate::shape::ShapeError::Unported`]. Rust ownership replaces the destructor's `delete`
//! of each held shape.
//!
//! `m_shapes` holds owning `SHAPE*` that may be null (a base `SHAPE::Clone` returns `nullptr`,
//! which `AddShape` stores unchanged); a null element is `None`.
use core::any::Any;
use std::rc::Rc;

use crate::box2::Box2I;
use crate::eda_angle::EdaAngle;
use crate::seg::Seg;
use crate::shape::{
    collide_shape_base, collide_shape_mtv_base, unported, Shape, ShapeResult, ShapeType,
};
use crate::vector::Vector2I;

/// Owner of shape_compound.cpp.
pub const SHAPE_COMPOUND_UNIT: &str = "libs.kimath.src.geometry.shape_compound.application.ui.p001";

/// `SHAPE_COMPOUND`: a compound shape, consisting of multiple simple shapes.
#[derive(Debug)]
pub struct ShapeCompound {
    /// `m_cachedBBox`, read and maintained only by the unported shape_compound.cpp `BBox`.
    #[allow(dead_code)]
    cached_bbox: Box2I,
    /// `m_dirty`.
    dirty: bool,
    /// `m_shapes`.
    shapes: Vec<Option<Box<dyn Shape>>>,
}

impl Default for ShapeCompound {
    fn default() -> Self {
        Self::new()
    }
}

impl ShapeCompound {
    /// `SHAPE_COMPOUND()`: `SHAPE( SH_COMPOUND )`, `m_dirty( true )`, an empty list and a
    /// default `BOX2I`.
    pub fn new() -> Self {
        Self {
            cached_bbox: Box2I::default(),
            dirty: true,
            shapes: Vec::new(),
        }
    }

    /// `SHAPE_COMPOUND( const std::vector<SHAPE*>& aShapes )` (shape_compound.cpp).
    pub fn from_shapes(_shapes: Vec<Option<Box<dyn Shape>>>) -> ShapeResult<Self> {
        // UNPORTED(libs.kimath.src.geometry.shape_compound.application.ui.p001): SHAPE_COMPOUND::SHAPE_COMPOUND(const std::vector<SHAPE*>&)
        unported(
            "SHAPE_COMPOUND::SHAPE_COMPOUND(const std::vector<SHAPE*>&)",
            SHAPE_COMPOUND_UNIT,
        )
    }

    /// `SHAPE_COMPOUND( const SHAPE_COMPOUND& aOther )` (shape_compound.cpp).
    pub fn copy_of(_other: &ShapeCompound) -> ShapeResult<Self> {
        // UNPORTED(libs.kimath.src.geometry.shape_compound.application.ui.p001): SHAPE_COMPOUND::SHAPE_COMPOUND(const SHAPE_COMPOUND&)
        unported(
            "SHAPE_COMPOUND::SHAPE_COMPOUND(const SHAPE_COMPOUND&)",
            SHAPE_COMPOUND_UNIT,
        )
    }

    /// The subshapes of `shape` when it is a `SHAPE_COMPOUND` (`dynamic_cast`), cloned in
    /// order and appended to `m_shapes`. A failing `Clone` stops the loop with the earlier
    /// clones already appended, as the C++ has done at that point.
    fn append_subshape_clones(&mut self, shape: &dyn Shape) -> ShapeResult<bool> {
        if shape.as_any().downcast_ref::<ShapeCompound>().is_none() {
            return Ok(false);
        }
        let mut subshapes: Vec<Option<&dyn Shape>> = Vec::new();
        shape.get_indexable_subshapes(&mut subshapes);
        for subshape in subshapes {
            // A null element would be dereferenced by `subshape->Clone()`; AddShape never
            // stores one from a flattened compound, but the C++ behaviour is undefined.
            let subshape = subshape.ok_or(crate::shape::ShapeError::Undefined)?;
            self.shapes.push(subshape.clone_shape()?);
        }
        Ok(true)
    }

    /// `AddShape( SHAPE* aShape )`: takes ownership. A compound is flattened (its subshapes are
    /// cloned and it is deleted); any other shape is stored as given. `m_dirty` is set after
    /// either, so it stays unchanged when a clone defers.
    pub fn add_shape(&mut self, shape: Box<dyn Shape>) -> ShapeResult<()> {
        // Don't make clients deal with nested SHAPE_COMPOUNDs
        if !self.append_subshape_clones(shape.as_ref())? {
            self.shapes.push(Some(shape));
        }
        // A flattened compound is deleted when `shape` drops here.
        self.dirty = true;
        Ok(())
    }

    /// `AddShape( std::shared_ptr<SHAPE> aShape )`: the shape stays shared; a compound is
    /// flattened as above, anything else is stored as `aShape->Clone()`.
    pub fn add_shape_shared(&mut self, shape: Rc<dyn Shape>) -> ShapeResult<()> {
        // Don't make clients deal with nested SHAPE_COMPOUNDs
        if !self.append_subshape_clones(shape.as_ref())? {
            self.shapes.push(shape.clone_shape()?);
        }
        self.dirty = true;
        Ok(())
    }

    /// `Shapes`.
    pub fn shapes(&self) -> &[Option<Box<dyn Shape>>] {
        &self.shapes
    }

    /// `Empty`.
    pub fn empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// `Size`: `(int) m_shapes.size()`, the C++20 modular conversion.
    pub fn size(&self) -> i32 {
        self.shapes.len() as i32
    }

    /// `UniqueSubshape`: the only shape, or `nullptr` unless there is exactly one.
    pub fn unique_subshape(&self) -> Option<&dyn Shape> {
        if self.shapes.len() != 1 {
            return None;
        }
        self.shapes.first().and_then(|s| s.as_deref())
    }

    /// `m_dirty`, which shape_compound.cpp's `BBox` reads; for tests of this unit.
    #[cfg(test)]
    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// `Distance( const SEG& aSeg )` (shape_compound.cpp).
    pub fn distance_seg(&self, _seg: &Seg) -> ShapeResult<i32> {
        // UNPORTED(libs.kimath.src.geometry.shape_compound.application.ui.p001): SHAPE_COMPOUND::Distance(const SEG&)
        unported("SHAPE_COMPOUND::Distance(const SEG&)", SHAPE_COMPOUND_UNIT)
    }
}

impl Shape for ShapeCompound {
    fn shape_type(&self) -> ShapeType {
        ShapeType::Compound
    }

    /// `HasIndexableSubshapes`: `true`.
    fn has_indexable_subshapes(&self) -> bool {
        true
    }

    /// `GetIndexableSubshapeCount`: `m_shapes.size()`.
    fn get_indexable_subshape_count(&self) -> usize {
        self.shapes.len()
    }

    /// `GetIndexableSubshapes`: clears `aSubshapes`, then copies every pointer in order.
    fn get_indexable_subshapes<'a>(&'a self, subshapes: &mut Vec<Option<&'a dyn Shape>>) {
        subshapes.clear();
        subshapes.reserve(self.shapes.len());
        subshapes.extend(self.shapes.iter().map(|s| s.as_deref()));
    }

    /// `Clone` (shape_compound.cpp).
    fn clone_shape(&self) -> ShapeResult<Option<Box<dyn Shape>>> {
        // UNPORTED(libs.kimath.src.geometry.shape_compound.application.ui.p001): SHAPE_COMPOUND::Clone
        unported("SHAPE_COMPOUND::Clone", SHAPE_COMPOUND_UNIT)
    }

    /// `Collide( const SHAPE*, int, VECTOR2I* )`: `SHAPE::Collide( aShape, aClearance, aMTV )`.
    fn collide_shape_mtv(
        &self,
        shape: &dyn Shape,
        clearance: i32,
        mtv: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        collide_shape_mtv_base(self, shape, clearance, mtv)
    }

    /// `Collide( const SHAPE*, int, int*, VECTOR2I* )`: `SHAPE::Collide( ... )`.
    fn collide_shape(
        &self,
        shape: &dyn Shape,
        clearance: i32,
        actual: Option<&mut i32>,
        location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        collide_shape_base(self, shape, clearance, actual, location)
    }

    /// `Collide( const SEG& ... )` (shape_compound.cpp).
    fn collide_seg(
        &self,
        _seg: &Seg,
        _clearance: i32,
        _actual: Option<&mut i32>,
        _location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        // UNPORTED(libs.kimath.src.geometry.shape_compound.application.ui.p001): SHAPE_COMPOUND::Collide(const SEG&, int, int*, VECTOR2I*)
        unported(
            "SHAPE_COMPOUND::Collide(const SEG&, int, int*, VECTOR2I*)",
            SHAPE_COMPOUND_UNIT,
        )
    }

    /// `BBox` (shape_compound.cpp).
    fn bbox(&self, _clearance: i32) -> ShapeResult<Box2I> {
        // UNPORTED(libs.kimath.src.geometry.shape_compound.application.ui.p001): SHAPE_COMPOUND::BBox
        unported("SHAPE_COMPOUND::BBox", SHAPE_COMPOUND_UNIT)
    }

    /// `Rotate` (shape_compound.cpp).
    fn rotate(&mut self, _angle: EdaAngle, _center: Vector2I) -> ShapeResult<()> {
        // UNPORTED(libs.kimath.src.geometry.shape_compound.application.ui.p001): SHAPE_COMPOUND::Rotate
        unported("SHAPE_COMPOUND::Rotate", SHAPE_COMPOUND_UNIT)
    }

    /// `Move` (shape_compound.cpp).
    fn move_by(&mut self, _vector: Vector2I) -> ShapeResult<()> {
        // UNPORTED(libs.kimath.src.geometry.shape_compound.application.ui.p001): SHAPE_COMPOUND::Move
        unported("SHAPE_COMPOUND::Move", SHAPE_COMPOUND_UNIT)
    }

    /// `IsSolid` (shape_compound.cpp).
    fn is_solid(&self) -> ShapeResult<bool> {
        // UNPORTED(libs.kimath.src.geometry.shape_compound.application.ui.p001): SHAPE_COMPOUND::IsSolid
        unported("SHAPE_COMPOUND::IsSolid", SHAPE_COMPOUND_UNIT)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
