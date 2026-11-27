# IsA and dyn_cast: template evidence and production ClassOf owners

Unit: `include.core.application.pure.p001`. Location: `include/core/typeinfo.h`, `IsA<T>( const
I* )`, `IsA<T>( const I& )` and `dyn_cast<Casted>( From )`.

## State

The three templates are ported (`kicad_core::typeinfo::{is_a, is_a_ref, dyn_cast,
dyn_cast_mut}`) against a `ClassOf<I>` trait standing for the static `T::ClassOf( const I* )`
they call, with `static_cast` and `static_cast_mut` for the const and non-const casts.

The templates are generic, so their branches are qualified with a generic fixture hierarchy
compiled into both adapters (`FIX_BASE`, `FIX_DERIVED`, `FIX_LEAF`, `FIX_OTHER`,
`FIX_NULLABLE`, `FIX_MULTI : FIX_SIDE, FIX_BASE`), each with its own `ClassOf` predicate. The
oracle instantiates the unmodified pinned templates with them; the `isa-*` cases cover the
null short-circuit of `IsA( const I* )`, the reference form, the pointer template argument,
`dyn_cast` passing a null pointer to `ClassOf` (and a `ClassOf` that accepts it), accepted and
rejected casts, up, identity and down casts from a `FIX_DERIVED*`, a `static_cast` that
adjusts the pointer, const and non-const casts, the identity of the result with the original
object and a write through it.

The fixture is not a stand-in for KiCad's classes and makes no claim about them. The
production `ClassOf` implementations belong to the units that own those classes:

- the `EDA_ITEM` subclasses in `eeschema/` and `pcbnew/` (`SCH_JUNCTION`, `PCB_VIA`, `PAD`,
  `ZONE`, ...), whose headers pull `eda_item.h` and wxWidgets;
- the `PNS::ITEM` subclasses (`PNS::SEGMENT::ClassOf` in `pcbnew.router.application.ui.p021`,
  `LINE`, `ARC`, `VIA`, `SOLID`, `DIFF_PAIR`), whose headers pull `padstack.h` and board types.

## Next step

When an owning unit ports one of those classes, it implements `ClassOf` for it and adds
cases through this unit's templates for its own predicate.
