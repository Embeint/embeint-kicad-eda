# kiplatform public headers: audit, co-port adoption and oracle domain

Unit: `libs.kiplatform.include.kiplatform.application.pure.p001`, the ten headers
`libs/kiplatform/include/kiplatform/{app,drivers,environment,io,secrets,sysinfo,sysinfo_gtk,
touchpad,ui,webview}.h` at KiCad pin `4cf939778df04d2b747bb2c4714fc5014a7565c7` (each byte
identical to the fork's tree, checked with `git diff 4cf939778df0 HEAD -- <header>`).

## What the headers define

Almost all of the 1,402 lines are declarations of platform functions whose bodies live in
`os/unix`, `os/common` and `port/wxgtk` (and the Windows/macOS equivalents). The headers
themselves define:

| Header | Definition | Rust |
| --- | --- | --- |
| `io.h` | `CLOUD_SYNC_BUFFER_SIZE = 512 * 1024` | `io::CLOUD_SYNC_BUFFER_SIZE` |
| `io.h` | `TARGET_ATTRS` (`value = 0`, `captured = false`) | `io::TargetAttrs` (`Default`) |
| `io.h` | `FILE_LOCK::STATE` | `io::FileLockState` |
| `io.h` | `FILE_LOCK() = default` with POSIX `m_fd = -1`, `m_state = NONE` | `io::FileLock::new` |
| `io.h` | inline `MAPPED_FILE::Data()`, `Size()` over the POSIX members | `io::MappedFile::{data,size}` |
| `io.h` | `aError = nullptr` defaults of four functions | `IoPlatform::*_without_error` |
| `environment.h` | `PROXY_CONFIG` | `environment::ProxyConfig` |
| `secrets.h` | `aDone = nullptr` of `StoreSecretAsync` | `SecretsPlatform::store_secret_async_without_done` |
| `touchpad.h` | `TOUCHPAD_GESTURE` | `touchpad::TouchpadGesture` |
| `touchpad.h` | `TOUCHPAD_GESTURE_HANDLER`, `virtual ~...() = default` | `touchpad::TouchpadGestureHandler` (trait object) |
| `touchpad.h` | `TOUCHPAD_GESTURE_CALLBACK` (`std::function`) | `touchpad::TouchpadGestureCallback` |
| `ui.h` | `NATIVE_DISPLAY_TYPE` | `ui::NativeDisplayType` |
| `ui.h` | `aDisplayType = nullptr` of `GetNativeWindowHandles` | `UiPlatform::get_native_window_handles_without_display_type` |
| `sysinfo.h` | `GPU_INFO`, `CPU_INFO`, `MEMORY_INFO`, `SYSINFO_BASE` | `kicad_kiplatform_sysinfo::sysinfo::{GpuInfo, CpuInfo, MemoryInfo, SysinfoBase}` |
| `sysinfo_gtk.h` | `class SYSINFO : SYSINFO_BASE` | `kicad_kiplatform_sysinfo::sysinfo::Sysinfo` |
| `drivers.h` | `Valid3DConnexionDriverVersion` declaration | `kicad_kiplatform_sysinfo::drivers::valid_3dconnexion_driver_version` |

Every other declaration is a typed host boundary: one trait per header (`AppPlatform`,
`EnvPlatform`, `IoPlatform`, `SecretsPlatform`, `TouchpadPlatform`, `UiPlatform`,
`WebviewPlatform`), with wx and OS objects as associated handle types and pointer parameters
as `Option`s. Portable code performs no OS call, I/O or GUI work; `kicad_kiplatform::owners`
names the unit that implements each Linux body (`os.unix.ui.p001`, `os.common.ui.p001`,
`port.wxgtk.ui.p001`, all `todo`). The `_WIN32` members (`MAPPED_FILE::m_fileHandle`,
`m_mapHandle`, `FILE_LOCK::m_handle`) and `sysinfo_windows.h`/`sysinfo_osx.h` are not part of
the Linux reference; they belong to the blocked Windows/macOS units and are not modelled.

## Co-ports

`libs.kiplatform.os.unix.application.pure.p001` co-ported `GPU_INFO`, `CPU_INFO`,
`MEMORY_INFO` and `SYSINFO` in their minimum form, without `SYSINFO_BASE` and without
`Default`. Audit against the pinned `sysinfo.h`/`sysinfo_gtk.h`: member names, types
(`long long` as `i64`, `uint32_t`, `uint64_t`, `std::string` as bytes) and order match. The
header unit adopts them as its port and adds `SysinfoBase` (the Unix `SYSINFO` inherits it
privately, so outside the class it is not usable as a base and keeps inherent methods) and
`Default` as value-initialization (`T{}`: empty strings, zero scalars). Default
initialization (`T t;`) leaves the scalars indeterminate, which Rust cannot represent and
the oracle never reads. `drivers.h` declares only the function that unit implements; its
declaration is adopted as is. The policy unit's `PBool` belongs to `policy.h` (another
unit); `WxString`, `WxPoint` and `WxSize` are accepted dependency co-ports, not part of these
headers.

## Oracle domain

The adapter includes the ten headers unchanged. To run the inline `MAPPED_FILE` accessors
and observe the default arguments, bodies are needed for out-of-line declarations; linking
recording bodies under the genuine names would replace symbols another adapter's reference
code (`common/richio.cpp`) uses. `reference/kiplatform_include_probe.cpp` therefore compiles
the pinned `io.h`, `secrets.h` and `ui.h` text with `#define KIPLATFORM
KIPLATFORM_INCLUDE_P001_PROBE`, so the header text, defaults and inline bodies are exactly the
pinned bytes in a renamed namespace. The live workflow checks with `nm` that the probe object
defines renamed symbols only.

Refused as undefined or outside the reference's states: reading default-initialized scalar
members; a `mapped` state with no bytes (the constructor returns before `mmap` for a
zero-length file) and an `empty_file` state with bytes. Accepted: a `fallback` state with no
bytes, reachable when the file shrinks to zero between `fstat` and `readIntoBuffer`; libstdc++
then leaves `m_fallbackBuffer.data()` null (`resize(0)` on a never-allocated vector), so
`Data()` is null, which the port reproduces.

## Evidence (local, diagnostic)

Own WSL tree, extracted Ubuntu `g++-13` 13.3.0 with local wx 3.2.9 (the hosted target is
GCC 13.3 with wx 3.2.4; hosted CI is the reference). Oracle sha256
`38fee282d420c219a7ad751ccde45a745a7db9dea3f20cd95443694ce0f72d1a`: 235 public cases, 235
compared exactly, 0 deferred, 0 failures; reply streams byte-identical (sha256
`a2105bc0e68b243eb48ce8507cafd7fa4ef81ea900ad58a06cb10d4bf8937a40`); 219 controls refused
identically by both programs, each followed by a successful request; import-free wasm32
agrees with every native reply.

## Remaining gaps

- The unit is `partial`: no case defers, but nearly every declaration only fixes an
  interface whose behaviour belongs to the `todo` Linux implementation units, so a green
  status here would claim more than the headers define. No golden is supplied.
- The host traits are not yet exercised by a real host; their signatures are checked when
  the implementation units wire them.
