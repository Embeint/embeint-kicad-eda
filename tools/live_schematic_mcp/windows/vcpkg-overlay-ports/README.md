# vcpkg overlay ports for the Windows build

`build_windows.ps1` passes this directory as `VCPKG_OVERLAY_PORTS`.

- `vcpkg-boost`: the port from the vcpkg baseline in `vcpkg-configuration.json`
  (MIT licensed, version 2026-08-21) with one change in `boost-install.cmake`: the
  two `file(RENAME)` calls no longer end the directory paths with `/`. On Windows 11
  build 26320, renaming a directory through a path with a trailing slash fails with
  "The parameter is incorrect", which stops every Boost port from building.
  Remove the overlay once upstream vcpkg drops the trailing slashes or Windows
  accepts them again; refresh it if the baseline's `vcpkg-boost` changes.
