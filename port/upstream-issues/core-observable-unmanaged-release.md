<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# OBSERVABLE notifications drop unmanaged subscribers and NotifyIgnore never leaves its iteration

- **Location:** `libs/core/observable.cpp:237-246` (`OBSERVABLE_BASE::leave_iteration`)
  and `libs/core/include/core/observable.h:213-241` (`OBSERVABLE<T>::NotifyIgnore`),
  KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
- **Trigger 1:** `SubscribeUnmanaged( a )` on an observable that is not shared (never
  copied) and has no outstanding `LINK`, then `Notify`. After the loop,
  `leave_iteration` releases `impl_` because it is not iterating, not shared and
  `use_count() == 1`; the condition does not check that the list is empty.
- **Effect 1:** the unmanaged subscription is silently lost after its first
  notification (`size()` becomes 0). A later `Unsubscribe( a )` reaches
  `OBSERVABLE_BASE::remove_observer` with a null `impl_`: `assert( impl_ )` is a
  release no-op and the null pointer is dereferenced.
- **Trigger 2:** any `NotifyIgnore` on an observable with a list.
- **Effect 2:** `NotifyIgnore` calls `enter_iteration` but, on success, never
  `leave_iteration` (only its exception path does). The iteration count stays
  nonzero, so later removals only null their entries, which are never collected;
  `size()` keeps counting them, a later `Notify` cannot release or collect the list,
  and `add_observer`'s `assert( !is_iterating() )` would fire in a debug build.
- **Reproduction:** corpus scenarios `observable-00002` (unmanaged release),
  `observable-00006` (NotifyIgnore) and the `unsubscribe without impl` controls in
  `port/oracle/fixtures/core-p001-controls.jsonl`. Both behaviours are the pinned
  release semantics and are compared exactly; the null dereference is refused.
- **Confirmation:** source-confirmed in release (`NDEBUG`) builds, which the oracle and
  KiCad releases use. No application-level symptom is claimed. NotifyIgnore is a
  header body owned by `libs.core.include.core.application.pure.p001`; it is recorded
  here because its effect is observed through this unit's base. Not a comparison
  exemption.
