// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad box2.h/box2_minmax.h, copyright SoftPLC, CERN, Alex Shvartzkop and KiCad Developers.
//! Canonical BOX2 port. Linux's int64_t=long generic KiCheckedCast semantics are explicit.
use crate::eda_angle::EdaAngle;
use crate::util::{self, clamp, max, min, Result};
use crate::vector::{VectorComponent, VectorDomainError};
use crate::{Vector2, Vector2D, Vector2I, Vector2L};

pub trait BoxCoord: VectorComponent {
    const LOWEST: Self;
    fn narrow(value: Self::Extended) -> Result<Self>;
    fn vector_cast(value: Self::Extended) -> Result<Self>;
    fn maximum() -> (Self, Self::Extended);
    fn distance(value: f64) -> Result<Self::Extended>;
    fn int(self) -> Result<i32>;
    fn implicit_int(self) -> Result<i32>;
    fn size_int(v: Self::Extended) -> Result<i32>;
}
impl BoxCoord for i32 {
    fn implicit_int(self) -> Result<i32> {
        Ok(self)
    }
    fn size_int(v: Self::Extended) -> Result<i32> {
        Ok(v as i32)
    }
    const LOWEST: Self = i32::MIN;
    fn narrow(v: i64) -> Result<Self> {
        Ok(v as i32)
    }
    fn vector_cast(v: i64) -> Result<Self> {
        Ok(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
    }
    fn maximum() -> (Self, i64) {
        (-i32::MAX, 2 * i64::from(i32::MAX))
    }
    fn distance(v: f64) -> Result<i64> {
        util::ki_round(v, false)
    }
    fn int(self) -> Result<i32> {
        Ok(self)
    }
}
impl BoxCoord for i64 {
    fn implicit_int(self) -> Result<i32> {
        Ok(self as i32)
    }
    fn size_int(v: Self::Extended) -> Result<i32> {
        Ok(v as i32)
    }
    const LOWEST: Self = i64::MIN;
    fn narrow(v: i64) -> Result<Self> {
        Ok(v)
    }
    fn vector_cast(v: i64) -> Result<Self> {
        Ok(v)
    }
    fn maximum() -> (Self, i64) {
        (-(i64::MAX / 2), i64::MAX)
    }
    fn distance(v: f64) -> Result<i64> {
        util::ki_round(v, false)
    }
    fn int(self) -> Result<i32> {
        Ok(self.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
    }
}
impl BoxCoord for f64 {
    fn implicit_int(self) -> Result<i32> {
        <i32 as VectorComponent>::from_double(self)
    }
    fn size_int(v: Self::Extended) -> Result<i32> {
        <i32 as VectorComponent>::from_double(v)
    }
    const LOWEST: Self = f64::MIN;
    fn narrow(v: f64) -> Result<Self> {
        Ok(v)
    }
    fn vector_cast(v: f64) -> Result<Self> {
        Ok(v)
    }
    fn maximum() -> (Self, f64) {
        (f64::MIN / 2.0, f64::MAX)
    }
    fn distance(v: f64) -> Result<f64> {
        Ok(v)
    }
    fn int(self) -> Result<i32> {
        Ok(Vector2I::try_from(Vector2D::new(self, 0.0))?.x)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Box2<T: BoxCoord = i32> {
    pos: Vector2<T>,
    size: Vector2<T::Extended>,
    init: bool,
}
pub type Box2I = Box2<i32>;
pub type Box2L = Box2<i64>;
pub type Box2D = Box2<f64>;
impl<T: BoxCoord> Default for Box2<T> {
    fn default() -> Self {
        Self {
            pos: Vector2::default(),
            size: Vector2::default(),
            init: false,
        }
    }
}
fn e<T: VectorComponent>(v: f64) -> Result<T> {
    T::from_double(v)
}
impl<T: BoxCoord> Box2<T> {
    pub fn new(pos: Vector2<T>, size: Vector2<T::Extended>) -> Result<Self> {
        let mut b = Self {
            pos,
            size,
            init: true,
        };
        // Both range checks execute even though the generic Linux cast has no side effects.
        T::narrow(pos.x.wide().c_add(size.x)?)?;
        T::narrow(pos.y.wide().c_add(size.y)?)?;
        b.normalize()?;
        Ok(b)
    }
    pub fn at(pos: Vector2<T>) -> Result<Self> {
        Self::new(pos, Vector2::default())
    }
    pub fn by_corners(a: Vector2<T>, b: Vector2<T>) -> Result<Self> {
        // The subtraction is in coord_type, before the converting SizeVec constructor.
        Self::new(
            a,
            Vector2::new(b.x.c_sub(a.x)?.wide(), b.y.c_sub(a.y)?.wide()),
        )
    }
    pub fn by_center(c: Vector2<T>, s: Vector2<T::Extended>) -> Result<Self> {
        // SizeVec / 2 selects VECTOR2::operator/(double), which rounds integral
        // components through KiROUND<double,int>; scalar integer /2 would be wrong.
        let half = s.divide(2.0)?;
        Self::new(
            Vector2::new(
                T::vector_cast(c.x.wide().c_sub(half.x)?)?,
                T::vector_cast(c.y.wide().c_sub(half.y)?)?,
            ),
            s,
        )
    }

    pub fn set_maximum(&mut self) {
        let (p, s) = T::maximum();
        self.pos = Vector2::new(p, p);
        self.size = Vector2::new(s, s);
        self.init = true;
    }
    pub fn centre(&self) -> Result<Vector2<T>> {
        let two = e::<T::Extended>(2.0)?;
        Ok(Vector2::new(
            T::narrow(self.pos.x.wide().c_add(self.size.x.c_div(two)?)?)?,
            T::narrow(self.pos.y.wide().c_add(self.size.y.c_div(two)?)?)?,
        ))
    }
    pub fn center(&self) -> Result<Vector2<T>> {
        self.centre()
    }
    pub fn compute(&mut self, points: &[Vector2<T>]) -> Result<()> {
        let Some(first) = points.first() else {
            return Ok(());
        };
        let mut lo = *first;
        let mut hi = *first;
        for p in points {
            lo.x = min(lo.x, p.x);
            lo.y = min(lo.y, p.y);
            hi.x = max(hi.x, p.x);
            hi.y = max(hi.y, p.y);
        }
        self.set_origin(lo);
        self.set_size(Vector2::new(
            hi.x.c_sub(lo.x)?.wide(),
            hi.y.c_sub(lo.y)?.wide(),
        ));
        Ok(())
    }
    pub fn normalize(&mut self) -> Result<&mut Self> {
        if self.size.y < T::Extended::default() {
            self.size.y = self.size.y.c_neg()?;
            self.pos.y = T::narrow(self.pos.y.wide().c_sub(self.size.y)?)?;
        }
        if self.size.x < T::Extended::default() {
            self.size.x = self.size.x.c_neg()?;
            self.pos.x = T::narrow(self.pos.x.wide().c_sub(self.size.x)?)?;
        }
        Ok(self)
    }
    pub fn contains(&self, p: Vector2<T>) -> Result<bool> {
        let mut rx = p.x.wide().c_sub(self.pos.x.wide())?;
        let mut ry = p.y.wide().c_sub(self.pos.y.wide())?;
        let (mut sx, mut sy) = (self.size.x, self.size.y);
        let z = T::Extended::default();
        if sx < z {
            sx = sx.c_neg()?;
            rx = rx.c_add(sx)?;
        }
        if sy < z {
            sy = sy.c_neg()?;
            ry = ry.c_add(sy)?;
        }
        Ok(rx >= z && ry >= z && ry <= sy && rx <= sx)
    }
    pub fn contains_xy(&self, x: T, y: T) -> Result<bool> {
        self.contains(Vector2::new(x, y))
    }
    pub fn contains_box(&self, b: &Self) -> Result<bool> {
        Ok(self.contains(b.origin())? && self.contains(b.end()?)?)
    }
    pub fn size(&self) -> Vector2<T::Extended> {
        self.size
    }
    pub fn x(&self) -> T {
        self.pos.x
    }
    pub fn y(&self) -> T {
        self.pos.y
    }
    pub fn origin(&self) -> Vector2<T> {
        self.pos
    }
    pub fn position(&self) -> Vector2<T> {
        self.pos
    }
    pub fn end(&self) -> Result<Vector2<T>> {
        Ok(Vector2::new(self.right()?, self.bottom()?))
    }
    pub fn width(&self) -> T::Extended {
        self.size.x
    }
    pub fn height(&self) -> T::Extended {
        self.size.y
    }
    pub fn right(&self) -> Result<T> {
        T::narrow(self.pos.x.wide().c_add(self.size.x)?)
    }
    pub fn bottom(&self) -> Result<T> {
        T::narrow(self.pos.y.wide().c_add(self.size.y)?)
    }
    pub fn left(&self) -> T {
        self.x()
    }
    pub fn top(&self) -> T {
        self.y()
    }
    pub fn size_max(&self) -> Result<i32> {
        let v = if self.size.x > self.size.y {
            self.size.x
        } else {
            self.size.y
        };
        T::size_int(v)
    }
    pub fn set_origin(&mut self, p: Vector2<T>) {
        self.pos = p;
        self.init = true;
    }
    pub fn set_origin_xy(&mut self, x: T, y: T) {
        self.set_origin(Vector2::new(x, y));
    }
    pub fn set_size(&mut self, s: Vector2<T::Extended>) {
        self.size = s;
        self.init = true;
    }
    pub fn set_size_xy(&mut self, w: T::Extended, h: T::Extended) {
        self.set_size(Vector2::new(w, h));
    }
    pub fn set_x(&mut self, x: T) {
        self.set_origin_xy(x, self.pos.y);
    }
    pub fn set_y(&mut self, y: T) {
        self.set_origin_xy(self.pos.x, y);
    }
    pub fn set_width(&mut self, w: T::Extended) {
        self.set_size_xy(w, self.size.y);
    }
    pub fn set_height(&mut self, h: T::Extended) {
        self.set_size_xy(self.size.x, h);
    }
    pub fn set_end(&mut self, p: Vector2<T>) -> Result<()> {
        self.set_size(Vector2::new(
            p.x.wide().c_sub(self.pos.x.wide())?,
            p.y.wide().c_sub(self.pos.y.wide())?,
        ));
        Ok(())
    }
    pub fn set_end_xy(&mut self, x: T, y: T) -> Result<()> {
        self.set_end(Vector2::new(x, y))
    }
    pub fn offset_xy(&mut self, x: T, y: T) -> Result<()> {
        self.pos.x = self.pos.x.c_add(x)?;
        self.pos.y = self.pos.y.c_add(y)?;
        Ok(())
    }
    pub fn offset(&mut self, p: Vector2<T>) -> Result<()> {
        self.offset_xy(p.x, p.y)
    }
    pub fn move_by(&mut self, p: Vector2<T>) -> Result<()> {
        self.offset(p)
    }
    pub fn with_offset(&self, p: Vector2<T>) -> Result<Self> {
        let mut b = *self;
        b.move_by(p)?;
        Ok(b)
    }
    pub fn intersects(&self, b: &Self) -> Result<bool> {
        let mut a = *self;
        let mut b = *b;
        a.normalize()?;
        b.normalize()?;
        let left = max(a.pos.x, b.pos.x).wide();
        let right = min(
            a.pos.x.wide().c_add(a.size.x)?,
            b.pos.x.wide().c_add(b.size.x)?,
        );
        let top = max(a.pos.y, b.pos.y).wide();
        let bottom = min(
            a.pos.y.wide().c_add(a.size.y)?,
            b.pos.y.wide().c_add(b.size.y)?,
        );
        Ok(left <= right && top <= bottom)
    }
    pub fn intersect(&self, b: &Self) -> Result<Self> {
        let mut a = *self;
        let mut b = *b;
        a.normalize()?;
        b.normalize()?;
        let lo = Vector2::new(max(a.pos.x, b.pos.x), max(a.pos.y, b.pos.y));
        let hi = Vector2::new(min(a.right()?, b.right()?), min(a.bottom()?, b.bottom()?));
        if lo.x < hi.x && lo.y < hi.y {
            Self::new(
                lo,
                Vector2::new(
                    hi.x.wide().c_sub(lo.x.wide())?,
                    hi.y.wide().c_sub(lo.y.wide())?,
                ),
            )
        } else {
            Self::at(Vector2::default())
        }
    }
    pub fn intersects_segment(&self, a: Vector2<T>, b: Vector2<T>) -> Result<bool> {
        if self.contains(a)? || self.contains(b)? {
            return Ok(true);
        }
        let p2 = Vector2::new(self.end()?.x, self.origin().y);
        let p4 = Vector2::new(self.origin().x, self.end()?.y);
        if segment_intersects(a, b, self.origin(), p2)? {
            return Ok(true);
        }
        if segment_intersects(a, b, p2, self.end()?)? {
            return Ok(true);
        }
        if segment_intersects(a, b, self.end()?, p4)? {
            return Ok(true);
        }
        Ok(false)
    }
    pub fn intersects_rotated(&self, b: &Self, angle: EdaAngle) -> Result<bool> {
        if !self.init {
            return Ok(false);
        }
        let rotation = angle.normalized();
        let r = rotation.as_degrees();
        for a in [0.0, 180.0, 360.0] {
            if (r - a).abs() < 1e-9 {
                return self.intersects(b);
            }
        }
        for a in [90.0, 270.0] {
            if (r - a).abs() < 1e-9 {
                let mut rect = Self::default();
                rect.set_origin(b.centre()?);
                let two = e::<T::Extended>(2.0)?;
                let one = e::<T::Extended>(1.0)?;
                rect.inflate(
                    T::narrow(b.height().c_div(two)?.c_add(one)?)?,
                    T::narrow(b.width().c_div(two)?.c_add(one)?)?,
                )?;
                return self.intersects(&rect);
            }
        }
        let mut corners = [
            Vector2::new(self.left().implicit_int()?, self.top().implicit_int()?),
            Vector2::new(self.right()?.implicit_int()?, self.top().implicit_int()?),
            Vector2::new(
                self.right()?.implicit_int()?,
                self.bottom()?.implicit_int()?,
            ),
            Vector2::new(self.left().implicit_int()?, self.bottom()?.implicit_int()?),
        ];
        let c = b.centre()?;
        let center = Vector2::new(c.x.int()?, c.y.int()?);
        for p in corners {
            let mut delta = p.checked_sub(center)?;
            rotate(&mut delta, -rotation)?;
            delta.add_assign(center)?;
            if b.contains(Vector2::new(
                T::from_double(f64::from(delta.x))?,
                T::from_double(f64::from(delta.y))?,
            ))? {
                return Ok(true);
            }
        }
        let two = e::<T::Extended>(2.0)?;
        let w = T::narrow(b.width().c_div(two)?)?.implicit_int()?;
        let h = T::narrow(b.height().c_div(two)?)?.implicit_int()?;
        let nw = w.c_neg()?;
        let nh = h.c_neg()?;
        corners = [
            Vector2::new(nw, nh),
            Vector2::new(w, nh),
            Vector2::new(w, h),
            Vector2::new(nw, h),
        ];
        for p in &mut corners {
            rotate(p, rotation)?;
            p.add_assign(center)?;
            if self.contains(Vector2::new(
                T::from_double(f64::from(p.x))?,
                T::from_double(f64::from(p.y))?,
            ))? {
                return Ok(true);
            }
        }
        for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
            let convert = |p: Vector2I| -> Result<Vector2<T>> {
                Ok(Vector2::new(
                    T::from_double(f64::from(p.x))?,
                    T::from_double(f64::from(p.y))?,
                ))
            };
            if self.intersects_segment(convert(corners[a])?, convert(corners[b])?)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn intersects_circle(&self, c: Vector2<T>, radius: i32) -> Result<bool> {
        if !self.init {
            return Ok(false);
        }
        let p = self.nearest_point(c)?;
        let dx = c.x.double().c_sub(p.x.double())?;
        let dy = c.y.double().c_sub(p.y.double())?;
        let r = f64::from(radius);
        Ok(dx.c_mul(dx)?.c_add(dy.c_mul(dy)?)? <= r.c_mul(r)?)
    }
    pub fn intersects_circle_edge(&self, c: Vector2<T>, radius: i32, width: i32) -> Result<bool> {
        if !self.init {
            return Ok(false);
        }
        let mut me = *self;
        me.normalize()?;
        if !self.intersects_circle(c, radius.c_add(width / 2)?.c_add(1)?)? {
            return Ok(false);
        }
        let p = self.farthest_point(c)?;
        let fx = p.x.double().c_sub(c.x.double())?;
        let fy = p.y.double().c_sub(c.y.double())?;
        let r = f64::from(radius).c_sub(f64::from(width) / 2.0)?;
        Ok(fx.c_mul(fx)?.c_add(fy.c_mul(fy)?)? > r.c_mul(r)?)
    }
    pub fn inflate(&mut self, dx: T, dy: T) -> Result<&mut Self> {
        inflate_axis::<T>(&mut self.pos.x, &mut self.size.x, dx)?;
        inflate_axis::<T>(&mut self.pos.y, &mut self.size.y, dy)?;
        Ok(self)
    }
    pub fn inflate_uniform(&mut self, d: T) -> Result<&mut Self> {
        self.inflate(d, d)
    }
    pub fn inflated(&self, dx: T, dy: T) -> Result<Self> {
        let mut b = *self;
        b.inflate(dx, dy)?;
        Ok(b)
    }
    pub fn inflated_uniform(&self, d: T) -> Result<Self> {
        self.inflated(d, d)
    }
    pub fn merge(&mut self, b: &Self) -> Result<&mut Self> {
        if !self.init {
            if b.init {
                self.pos = b.pos;
                self.size = b.size;
                self.init = true;
            }
            return Ok(self);
        }
        self.normalize()?;
        let mut b = *b;
        b.normalize()?;
        let mut end = self.end()?;
        let be = b.end()?;
        self.pos.x = min(self.pos.x, b.pos.x);
        self.pos.y = min(self.pos.y, b.pos.y);
        end.x = max(end.x, be.x);
        end.y = max(end.y, be.y);
        self.set_end(end)?;
        Ok(self)
    }
    pub fn merge_point(&mut self, p: Vector2<T>) -> Result<&mut Self> {
        if !self.init {
            self.pos = p;
            self.size = Vector2::default();
            self.init = true;
            return Ok(self);
        }
        self.normalize()?;
        let mut end = self.end()?;
        self.pos.x = min(self.pos.x, p.x);
        self.pos.y = min(self.pos.y, p.y);
        end.x = max(end.x, p.x);
        end.y = max(end.y, p.y);
        self.set_end(end)?;
        Ok(self)
    }
    pub fn bounding_box_rotated(&self, center: Vector2I, angle: EdaAngle) -> Result<Self> {
        let p = self.origin();
        let end = self.end()?;
        let mut corners = [
            Vector2::new(p.x.int()?, p.y.int()?),
            Vector2::new(p.x.int()?, end.y.int()?),
            Vector2::new(end.x.int()?, end.y.int()?),
            Vector2::new(end.x.int()?, p.y.int()?),
        ];
        for p in &mut corners {
            rotate_center(p, center, angle)?;
        }
        let mut lo = corners[0];
        let mut hi = lo;
        for p in corners.iter().skip(1) {
            lo.x = min(lo.x, p.x);
            lo.y = min(lo.y, p.y);
            hi.x = max(hi.x, p.x);
            hi.y = max(hi.y, p.y);
        }
        let cv = |p: Vector2I| -> Result<Vector2<T>> {
            Ok(Vector2::new(
                T::from_double(f64::from(p.x))?,
                T::from_double(f64::from(p.y))?,
            ))
        };
        let mut b = Self::default();
        b.set_origin(cv(lo)?);
        b.set_end(cv(hi)?)?;
        Ok(b)
    }
    pub fn area(&self) -> Result<T::Extended> {
        self.width().c_mul(self.height())
    }
    pub fn diagonal(&self) -> Result<T::Extended> {
        T::Extended::norm2(self.size.x, self.size.y)
    }
    pub fn squared_diagonal(&self) -> Result<<T::Extended as VectorComponent>::Extended> {
        T::Extended::squared_norm2(self.size.x, self.size.y)
    }
    pub fn squared_distance(&self, p: Vector2<T>) -> Result<T::Extended> {
        let x = self.pos.x.wide();
        let y = self.pos.y.wide();
        let x1 = min(x, x.c_add(self.size.x)?);
        let x2 = max(x, x.c_add(self.size.x)?);
        let y1 = min(y, y.c_add(self.size.y)?);
        let y2 = max(y, y.c_add(self.size.y)?);
        let dx = max(
            if p.x.wide() < x1 {
                x1.c_sub(p.x.wide())?
            } else {
                p.x.wide().c_sub(x2)?
            },
            T::Extended::default(),
        );
        let dy = max(
            if p.y.wide() < y1 {
                y1.c_sub(p.y.wide())?
            } else {
                p.y.wide().c_sub(y2)?
            },
            T::Extended::default(),
        );
        dx.c_mul(dx)?.c_add(dy.c_mul(dy)?)
    }
    pub fn distance(&self, p: Vector2<T>) -> Result<T::Extended> {
        T::distance(crate::vector::vector_sqrt(
            self.squared_distance(p)?.double(),
        )?)
    }
    pub fn squared_distance_box(&self, b: &Self) -> Result<T::Extended> {
        let mut s = T::Extended::default();
        for (p, z, bp, bz) in [
            (self.pos.x, self.size.x, b.pos.x, b.size.x),
            (self.pos.y, self.size.y, b.pos.y, b.size.y),
        ] {
            if bp.wide().c_add(bz)? < p.wide() {
                let d = bp.wide().c_add(bz)?.c_sub(p.wide())?;
                s = s.c_add(d.c_mul(d)?)?;
            } else if bp.wide() > p.wide().c_add(z)? {
                let d = bp.wide().c_sub(z)?.c_sub(p.wide())?;
                s = s.c_add(d.c_mul(d)?)?;
            }
        }
        Ok(s)
    }
    pub fn distance_box(&self, b: &Self) -> Result<T::Extended> {
        T::distance(crate::vector::vector_sqrt(
            self.squared_distance_box(b)?.double(),
        )?)
    }
    pub fn nearest_point(&self, p: Vector2<T>) -> Result<Vector2<T>> {
        let mut b = *self;
        b.normalize()?;
        Ok(Vector2::new(
            clamp(p.x, b.left(), b.right()?)?,
            clamp(p.y, b.top(), b.bottom()?)?,
        ))
    }
    pub fn farthest_point(&self, p: Vector2<T>) -> Result<Vector2<T>> {
        let mut b = *self;
        b.normalize()?;
        let c = b.center()?;
        Ok(Vector2::new(
            if p.x < c.x { b.right()? } else { b.left() },
            if p.y < c.y { b.bottom()? } else { b.top() },
        ))
    }
    pub fn equals(&self, b: &Self) -> Result<bool> {
        let mut a = *self;
        let mut b = *b;
        a.normalize()?;
        b.normalize()?;
        Ok(a.pos == b.pos && a.size == b.size)
    }
    pub fn not_equals(&self, b: &Self) -> Result<bool> {
        let mut a = *self;
        let mut b = *b;
        a.normalize()?;
        b.normalize()?;
        Ok(a.pos != b.pos || a.size != b.size)
    }
    pub fn is_valid(&self) -> bool {
        self.init
    }
    pub fn format(&self) -> Result<String> {
        Ok(format!(
            "( box corner {} w {} h {} )",
            self.pos.format()?,
            self.size.x.stream()?,
            self.size.y.stream()?
        ))
    }
    pub fn is_safe(&self) -> Result<bool> {
        Ok(self.left().double() >= -2_147_483_647.0
            && self.top().double() >= -2_147_483_647.0
            && self.right()?.double() <= 2_147_483_647.0
            && self.bottom()?.double() <= 2_147_483_647.0)
    }
}
fn inflate_axis<T: BoxCoord>(p: &mut T, s: &mut T::Extended, d: T) -> Result<()> {
    let pos = p.wide();
    let delta = d.wide();
    let lo = T::LOWEST.wide();
    let hi = T::MAX.wide();
    let two = e::<T::Extended>(2.0)?;
    if *s >= T::Extended::default() {
        if *s < two.c_neg()?.c_mul(delta)? {
            *p = T::narrow(pos.c_add(s.c_div(two)?)?)?;
            *s = T::Extended::default();
        } else {
            let low = clamp(pos.c_sub(delta)?, lo, hi)?;
            let high = clamp(pos.c_add(*s)?.c_add(delta)?, lo, hi)?;
            *p = T::narrow(low)?;
            *s = high.c_sub(low)?;
        }
    } else if *s > two.c_mul(delta)? {
        *p = T::narrow(pos.c_sub(s.c_div(two)?)?)?;
        *s = T::Extended::default();
    } else {
        let high = clamp(pos.c_add(delta)?, lo, hi)?;
        let low = clamp(pos.c_add(*s)?.c_sub(delta)?, lo, hi)?;
        *p = T::narrow(high)?;
        *s = low.c_sub(high)?;
    }
    Ok(())
}
fn segment_intersects<T: BoxCoord>(
    a: Vector2<T>,
    b: Vector2<T>,
    c: Vector2<T>,
    d: Vector2<T>,
) -> Result<bool> {
    // The actual callee takes VECTOR2I, so converting vector constructors execute first.
    for p in [a, b, c, d] {
        p.x.int()?;
        p.y.int()?;
    }
    // UNPORTED(libs.kimath.src.trigo.application.ui.p001): SegmentIntersectsSegment
    Err(VectorDomainError::Unported {
        upstream: "SegmentIntersectsSegment",
        unit: "libs.kimath.src.trigo.application.ui.p001",
    })
}
fn rotate(_p: &mut Vector2I, _angle: EdaAngle) -> Result<()> {
    // UNPORTED(libs.kimath.include.application.ui.p001): RotatePoint(VECTOR2I&, EDA_ANGLE)
    Err(VectorDomainError::Unported {
        upstream: "RotatePoint(VECTOR2I&, EDA_ANGLE)",
        unit: "libs.kimath.include.application.ui.p001",
    })
}
fn rotate_center(_p: &mut Vector2I, _c: Vector2I, _angle: EdaAngle) -> Result<()> {
    // UNPORTED(libs.kimath.include.application.ui.p001): RotatePoint(VECTOR2I&, VECTOR2I, EDA_ANGLE)
    Err(VectorDomainError::Unported {
        upstream: "RotatePoint(VECTOR2I&, VECTOR2I, EDA_ANGLE)",
        unit: "libs.kimath.include.application.ui.p001",
    })
}
pub fn box2i_safe_parts(pos: Vector2D, size: Vector2D) -> Result<Box2I> {
    let lo = -2_147_483_647.0;
    let hi = 2_147_483_647.0;
    let cast = |v: f64| -> Result<i64> { <i64 as VectorComponent>::from_double(clamp(v, lo, hi)?) };
    let left = cast(pos.x)? as i32;
    let top = cast(pos.y)? as i32;
    let right = cast(pos.x.c_add(size.x)?)?;
    let bottom = cast(pos.y.c_add(size.y)?)?;
    Box2I::new(
        Vector2::new(left, top),
        Vector2::new(right - i64::from(left), bottom - i64::from(top)),
    )
}
pub fn box2i_safe(b: &Box2D) -> Result<Box2I> {
    let lo = -2_147_483_647.0;
    let hi = 2_147_483_647.0;
    let left = <i32 as VectorComponent>::from_double(clamp(b.left(), lo, hi)?)?;
    let top = <i32 as VectorComponent>::from_double(clamp(b.top(), lo, hi)?)?;
    let right = <i64 as VectorComponent>::from_double(clamp(b.right()?, lo, hi)?)?;
    let bottom = <i64 as VectorComponent>::from_double(clamp(b.bottom()?, lo, hi)?)?;
    Box2I::new(
        Vector2::new(left, top),
        Vector2::new(right - i64::from(left), bottom - i64::from(top)),
    )
}
pub fn box2i_safe_integral<S: Copy + Into<i64>>(pos: Vector2I, size: Vector2<S>) -> Result<Box2I> {
    let right = clamp(
        i64::from(pos.x).c_add(size.x.into())?,
        -i64::from(i32::MAX),
        i64::from(i32::MAX),
    )?;
    let bottom = clamp(
        i64::from(pos.y).c_add(size.y.into())?,
        -i64::from(i32::MAX),
        i64::from(i32::MAX),
    )?;
    Box2I::new(
        pos,
        Vector2::new(right - i64::from(pos.x), bottom - i64::from(pos.y)),
    )
}
pub fn ki_round_box(b: &Box2D) -> Result<Box2I> {
    Box2I::new(b.origin().ki_round()?, {
        let s = b.size().ki_round()?;
        Vector2::new(i64::from(s.x), i64::from(s.y))
    })
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Box2IMinMax {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl Box2IMinMax {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
    pub const fn point(p: Vector2I) -> Self {
        Self::new(p.x, p.y, p.x, p.y)
    }
    pub const fn xy(x: i32, y: i32) -> Self {
        Self::point(Vector2::new(x, y))
    }
    pub fn from_box(b: &Box2I) -> Result<Self> {
        Ok(Self::new(b.left(), b.top(), b.right()?, b.bottom()?))
    }
    pub fn corners(a: Vector2I, b: Vector2I) -> Self {
        Self::new(min(a.x, b.x), min(a.y, b.y), max(a.x, b.x), max(a.y, b.y))
    }
    /// SEG's A/B public fields require no unavailable method call.
    pub fn segment(a: Vector2I, b: Vector2I) -> Self {
        Self::corners(a, b)
    }
    pub fn from_arc<A: ArcBoundingBox>(arc: &A) -> Result<Self> {
        let b = arc.bbox()?;
        Self::from_box(&b)
    }
    pub fn to_box(self) -> Result<Box2I> {
        box2i_safe_integral(
            Vector2::new(self.left, self.top),
            Vector2L::new(
                i64::from(self.right) - i64::from(self.left),
                i64::from(self.bottom) - i64::from(self.top),
            ),
        )
    }
    pub fn intersects(&self, b: &Self) -> bool {
        let left = max(self.left, b.left);
        let right = min(self.right, b.right);
        let top = max(self.top, b.top);
        let bottom = min(self.bottom, b.bottom);
        left <= right && top <= bottom
    }
    pub fn merge(&mut self, p: Vector2I) {
        self.left = min(self.left, p.x);
        self.right = max(self.right, p.x);
        self.top = min(self.top, p.y);
        self.bottom = max(self.bottom, p.y);
    }
    pub fn center(&self) -> Vector2I {
        Vector2::new(
            ((i64::from(self.left) + i64::from(self.right)) / 2) as i32,
            ((i64::from(self.top) + i64::from(self.bottom)) / 2) as i32,
        )
    }
    pub fn diameter(&self) -> Result<f64> {
        let d = Vector2L::new(
            i64::from(self.right) - i64::from(self.left),
            i64::from(self.bottom) - i64::from(self.top),
        );
        Ok(crate::hypot::finite(d.x as f64, d.y as f64))
    }
}

/// Dependency boundary for SHAPE_ARC::BBox; implemented by the arc owner when ported.
pub trait ArcBoundingBox {
    fn bbox(&self) -> Result<Box2I>;
}
