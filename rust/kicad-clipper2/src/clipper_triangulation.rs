// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 clipper.triangulation.h and clipper.version.h (Copyright Angus
// Johnson 2010-2025), Boost Software License 1.0; see LICENSE-BSL-1.0. Port: Embeint,
// GPL-3.0-or-later.

//! `clipper.triangulation.h` and `clipper.version.h`, part of unit [`crate::clipper::UNIT`].
//!
//! `clipper.triangulation.h` declares `TriangulateResult` and two `Triangulate` overloads; their
//! bodies are in `clipper.triangulation.cpp` (unit [`TRIANGULATION_SRC`]), which KiCad's
//! `thirdparty/clipper2/CMakeLists.txt` does not compile at all. The declarations are ported
//! here and both calls return [`ClipperError::Unported`] naming that unit.

use crate::clipper_core::{Paths64, PathsD};
use crate::{ClipperError, Result};

/// `constexpr auto CLIPPER2_VERSION = "2.0.1";` (`clipper.version.h`).
pub const CLIPPER2_VERSION: &str = "2.0.1";

/// Owner of `clipper.triangulation.cpp` (blocked: not part of KiCad's Clipper2 build).
pub const TRIANGULATION_SRC: &str = "thirdparty.clipper2.clipper2lib.src.thirdparty.ui.p001";

/// `enum class TriangulateResult { success, fail, no_polygons, paths_intersect };`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriangulateResult {
    Success,
    Fail,
    NoPolygons,
    PathsIntersect,
}

/// `TriangulateResult Triangulate(const Paths64& pp, Paths64& solution,
/// bool useDelaunay = true)`: "this function will not accept intesecting paths".
pub fn triangulate_64(
    _pp: &Paths64,
    _solution: &mut Paths64,
    _use_delaunay: bool,
) -> Result<TriangulateResult> {
    // UNPORTED(thirdparty.clipper2.clipper2lib.src.thirdparty.ui.p001): Clipper2Lib::Triangulate(const Paths64&, Paths64&, bool)
    Err(ClipperError::Unported {
        upstream: "Clipper2Lib::Triangulate(const Paths64&, Paths64&, bool)",
        unit: TRIANGULATION_SRC,
    })
}

/// `TriangulateResult Triangulate(const PathsD& pp, int decPlaces, PathsD& solution,
/// bool useDelaunay = true)`.
pub fn triangulate_d(
    _pp: &PathsD,
    _dec_places: i32,
    _solution: &mut PathsD,
    _use_delaunay: bool,
) -> Result<TriangulateResult> {
    // UNPORTED(thirdparty.clipper2.clipper2lib.src.thirdparty.ui.p001): Clipper2Lib::Triangulate(const PathsD&, int, PathsD&, bool)
    Err(ClipperError::Unported {
        upstream: "Clipper2Lib::Triangulate(const PathsD&, int, PathsD&, bool)",
        unit: TRIANGULATION_SRC,
    })
}
