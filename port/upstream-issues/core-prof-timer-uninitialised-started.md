<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# PROF_TIMER::to_string reads an uninitialised m_started

- **Location:** `libs/core/include/core/profile.h:54-59` (`PROF_TIMER( const std::string&
  aName, bool aAutostart = true )`) and `:154-185` (`PROF_TIMER::to_string`), KiCad
  `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
- **Trigger:** a timer built with the named constructor and `aAutostart == false`, then
  `to_string()` before any `Start()`. The constructor initialises `m_name` and `m_running`
  only; `m_started` is written solely by `Start()` (always `true`).
- **Effect:** `to_string()` evaluates `!m_started` on an indeterminate `bool`, which is
  undefined behaviour. The only defined value `m_started` can hold is `true`, so the
  `"none"` result the branch exists for is never produced by defined code.
- **Reproduction:** the `refuse-profile-00000` and `refuse-profile-00001` controls in
  `port/oracle/fixtures/core-include-p001-controls.jsonl` (refused before the reference is
  called on the C++ side). The Rust port models the member as `Option<bool>` and returns
  `ProfileError::Indeterminate`.
- **Evidence:** source reading of the two constructors, `Start()` and `to_string()`. Show,
  `msecs` and `SinceStart` of such a timer read only the zero-initialised time points and
  `m_running`, and are compared exactly (`profile` cases with `autostart: false`).
- **Confirmation:** source-confirmed. Not a comparison exemption: every defined use of
  the timer, including those of a never-started one, is compared.
