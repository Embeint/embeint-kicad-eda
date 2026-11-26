# IsA and dyn_cast wait for a real ClassOf

Unit: `include.core.application.pure.p001`. Location: `include/core/typeinfo.h`, `IsA<T>( const
I* )`, `IsA<T>( const I& )` and `dyn_cast<Casted>( From )`.

## State

The three template bodies are ported (`kicad_core::typeinfo::{is_a, is_a_ref, dyn_cast}`)
against a `ClassOf<I>` trait that stands for the static `T::ClassOf( const I* )` they call.
No case exercises them: every real `ClassOf` is a member of a class owned by a `UI` unit that
is still `todo`:

- the `EDA_ITEM` subclasses in `eeschema/` and `pcbnew/` (`SCH_JUNCTION`, `PCB_VIA`, `PAD`,
  `ZONE`, ...), whose headers pull `eda_item.h` and wxWidgets;
- the `PNS::ITEM` subclasses (`PNS::SEGMENT::ClassOf` in `pcbnew.router.application.ui.p021`,
  `LINE`, `ARC`, `VIA`, `SOLID`, `DIFF_PAIR`), whose headers pull `padstack.h` and board
  types.

None is in the oracle's source closure (`port/oracle/pin.py`), and a stand-in class hierarchy
would prove nothing about the real ones, so none is used.

## Next step

When an owning unit ports one of those classes, it implements `ClassOf` for it, extends the
oracle closure with the real class, and adds cases that call `IsA` (null and non-null
pointers, references) and `dyn_cast` (accepted and rejected objects, a null pointer) through
this unit's bodies. Until then this unit stays `partial`.
