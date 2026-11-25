<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# ARC_CHORD_PARAMS::GetArcAngle calls std::clamp with NaN before a successful Compute

Reference: KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`,
`libs/kimath/src/geometry/arc_chord_params.cpp:104-113` (`GetArcAngle`), with the
default member initializers in `libs/kimath/include/geometry/arc_chord_params.h`.

Trigger: `GetArcAngle` evaluates `std::clamp( m_halfChord / m_radius, 0.0, 1.0 )`.
Both members default to `0.0`, and `Compute` assigns them only after its zero-chord
(`m_chordLen <= 0.0`) and collinear (`cross == 0.0`) early returns. On a
default-constructed object, or one whose every `Compute` so far has returned false,
the quotient is `0.0 / 0.0`. That floating division is defined on the IEC 60559
reference (quiet NaN). C++20 [alg.clamp] requires `T` to meet
`Cpp17LessThanComparable`, whose `<` must be a strict weak ordering; NaN is
unordered with every value, so `std::clamp` called with that value is outside
its defined domain. libstdc++ happens to return the NaN unchanged (`min( max( v,
lo ), hi )` with two false comparisons), but that observation does not make the
call defined. Once one `Compute` has succeeded, a later refusal leaves the earlier
positive radius and finite half chord in place, so the quotient stays defined.

Reproduction (source analysis): `ARC_CHORD_PARAMS p; p.GetArcAngle();`, or
`p.Compute( { 3, 3 }, { 1, 1 }, { 3, 3 } ); p.GetArcAngle();` (zero chord), or
`p.Compute( { 0, 0 }, { 2, 2 }, { 4, 4 } ); p.GetArcAngle();` (collinear).

Evidence: the fixed-seed generator for unit
`libs.kimath.src.geometry.arc_chord_params.application.pure.p001`
(`rust/kicad-arc-chord-shim/generate_cases.py --branches`) found 79 requests of
the earlier public corpus that observe such a state (96 observed states): the
default state, initial zero-chord and collinear refusals, the refusal-first
stale-state sequences and random sequences starting with a refusal. Those requests
are now the undefined-domain controls `port/oracle/fixtures/arc-chord-controls.jsonl`.
The oracle adapter (`port/oracle/shims/kimath_src_arc_chord_params.cpp`) evaluates
the same IEEE quotient from the header accessors and, when it is NaN, reports the
arc angle as `{"error":{"kind":"invalid_input","message":"undefined arc chord
operation"}}` without calling `GetArcAngle`; all other observations of the state
are still reported. `kicad_math::arc_chord_params::ArcChordParams::get_arc_angle`
returns `Err(ArcChordError::UndefinedClamp)` at the same point, and
`rust/kicad-arc-chord-shim/compare_controls.py` requires both programs to refuse
exactly those observations and to agree on every other value. The public corpus
keeps each of these Compute sequences after one valid Compute (`primed-` cases),
so its observations are all defined. No other owned body makes an
ordering-dependent library call on a NaN: `Compute` compares with built-in
operators and takes `std::sqrt` of a finite non-negative sum; `GetStartAngle` and
`GetEndAngle` reach `EDA_ANGLE( VECTOR2D )`, whose comparisons are built-in and
whose `atan2` of NaN is IEC 60559 defined; `GetCenterPoint` is arithmetic only.

Confirmation state: source-confirmed against C++20 [alg.clamp] and
[utility.arg.requirements]; not demonstrated with a sanitizer (none diagnoses
this precondition). No KiCad application reachability is claimed. At the pin
both callers (`libs/kimath/src/convert_basic_shapes_to_polygon.cpp:526-538` and
`:712-724`) construct a fresh object and call its getters only after `Compute`
returns true. This is a fork-local record, not permission to change KiCad's
behaviour or to send anything upstream.
