# SPDX-License-Identifier: GPL-3.0-or-later
cmake_minimum_required( VERSION 3.22 )
project( kicad_oracle LANGUAGES C CXX )

if( NOT CMAKE_SYSTEM_NAME STREQUAL "Linux" OR NOT CMAKE_GENERATOR STREQUAL "Ninja" )
    message( FATAL_ERROR "The phase 0 oracle supports Linux with Ninja only" )
endif()

set( CMAKE_CXX_STANDARD 20 )
set( CMAKE_CXX_STANDARD_REQUIRED ON )
# No C++ modules are used. Keep Ninja's generated module-scan response files out
# of this audited source/link closure; hidden response files remain forbidden.
set( CMAKE_CXX_SCAN_FOR_MODULES OFF )
set( CMAKE_EXPORT_COMPILE_COMMANDS ON )
set( CMAKE_INTERPROCEDURAL_OPTIMIZATION OFF )
find_package( Python3 REQUIRED COMPONENTS Interpreter )
find_package( Boost 1.74 REQUIRED CONFIG )
find_package( wxWidgets 3.2 REQUIRED COMPONENTS base core )
include( ${wxWidgets_USE_FILE} )

if( NOT CMAKE_CXX_COMPILER_ID MATCHES "^(Clang|GNU)$" )
    message( FATAL_ERROR "Use Clang or GCC for the reference build" )
endif()

add_compile_options( -fno-fast-math -ffp-contract=off )
add_link_options( -fno-fast-math -ffp-contract=off )

get_filename_component( ORACLE_OVERLAY "${CMAKE_CURRENT_SOURCE_DIR}" ABSOLUTE )
get_filename_component( KICAD_REFERENCE_SOURCE "${KICAD_REFERENCE_SOURCE}" ABSOLUTE )
if( NOT EXISTS "${KICAD_REFERENCE_SOURCE}/source-manifest.json" )
    message( FATAL_ERROR "Run prepare.py and build.sh to provide the verified pin" )
endif()

# Supply the application's global include directory to the unmodified libraries.
include_directories( "${KICAD_REFERENCE_SOURCE}/include" )
include( "${KICAD_REFERENCE_SOURCE}/cmake/PerformFeatureChecks.cmake" )
perform_feature_checks()
set( PYTHON_VERSION_MAJOR 0 )
set( PYTHON_VERSION_MINOR 0 )
configure_file( "${KICAD_REFERENCE_SOURCE}/cmake/config.h.cmake"
                "${CMAKE_BINARY_DIR}/config.h" )

add_subdirectory( "${KICAD_REFERENCE_SOURCE}/libs/core" reference/core )
add_subdirectory( "${KICAD_REFERENCE_SOURCE}/thirdparty/clipper2" reference/clipper2 )
add_subdirectory( "${KICAD_REFERENCE_SOURCE}/thirdparty/rtree" reference/rtree )
add_subdirectory( "${KICAD_REFERENCE_SOURCE}/thirdparty/nlohmann_json" reference/json )
add_subdirectory( "${KICAD_REFERENCE_SOURCE}/libs/kimath" reference/kimath )
set( FMT_DOC OFF CACHE BOOL "" FORCE )
set( FMT_TEST OFF CACHE BOOL "" FORCE )
set( FMT_INSTALL OFF CACHE BOOL "" FORCE )
add_subdirectory( "${KICAD_REFERENCE_SOURCE}/thirdparty/fmt" reference/fmt )

add_library( resistor-reference STATIC
    "${KICAD_REFERENCE_SOURCE}/pcb_calculator/eseries.cpp"
    "${ORACLE_OVERLAY}/reference/resistors_access.cpp" )
target_include_directories( resistor-reference PUBLIC "${KICAD_REFERENCE_SOURCE}/pcb_calculator"
                           "${ORACLE_OVERLAY}/reference" )
target_link_libraries( resistor-reference PUBLIC fmt::fmt )

# Compile the actual parser constructor and real in-memory line reader so the
# owned GetReader inline method is tested on a live defined-domain parser object.
# Unused GUI/parser bodies in the original TU are discarded by the final linker.
set( READER_GENERATED "${CMAKE_BINARY_DIR}/reader-generated" )
file( MAKE_DIRECTORY "${READER_GENERATED}" )
execute_process( COMMAND "${CMAKE_COMMAND}"
    -Denum=PCBCALC_DATA_T
    "-DinputFile=${KICAD_REFERENCE_SOURCE}/pcb_calculator/pcb_calculator_datafile.keywords"
    -DoutHeaderFile=pcb_calculator_datafile_lexer.h
    -DoutCppFile=pcb_calculator_datafile_keywords.cpp
    -P "${KICAD_REFERENCE_SOURCE}/cmake/BuildSteps/TokenList2DsnLexer.cmake"
    WORKING_DIRECTORY "${READER_GENERATED}" COMMAND_ERROR_IS_FATAL ANY )
add_library( calculator-reader-reference STATIC
    "${KICAD_REFERENCE_SOURCE}/pcb_calculator/datafile_read_write.cpp"
    "${KICAD_REFERENCE_SOURCE}/common/dsnlexer.cpp"
    "${KICAD_REFERENCE_SOURCE}/common/richio.cpp"
    "${KICAD_REFERENCE_SOURCE}/common/exceptions.cpp"
    "${READER_GENERATED}/pcb_calculator_datafile_keywords.cpp" )
target_include_directories( calculator-reader-reference PUBLIC
    "${READER_GENERATED}" "${KICAD_REFERENCE_SOURCE}/pcb_calculator"
    "${KICAD_REFERENCE_SOURCE}/libs/kiplatform/include"
    "${KICAD_REFERENCE_SOURCE}/thirdparty/fast_float/include" "${CMAKE_BINARY_DIR}" )
target_compile_definitions( calculator-reader-reference PRIVATE PCB_CALCULATOR_BUILD )
target_compile_options( calculator-reader-reference PRIVATE -ffunction-sections -fdata-sections )
target_link_libraries( calculator-reader-reference PUBLIC fmt::fmt nlohmann_json kimath )

# Compile the unchanged pinned calculator without any GUI panel implementation.
add_library( iec60664-reference STATIC
    "${KICAD_REFERENCE_SOURCE}/pcb_calculator/calculator_panels/iec60664.cpp" )
target_include_directories( iec60664-reference PUBLIC
    "${KICAD_REFERENCE_SOURCE}/pcb_calculator/calculator_panels" )

# The genuine pinned Unix kiplatform drivers.cpp and sysinfo.cpp, unchanged.
add_library( kiplatform-unix-reference STATIC
    "${KICAD_REFERENCE_SOURCE}/libs/kiplatform/os/unix/drivers.cpp"
    "${KICAD_REFERENCE_SOURCE}/libs/kiplatform/os/unix/sysinfo.cpp" )
target_include_directories( kiplatform-unix-reference PUBLIC
    "${KICAD_REFERENCE_SOURCE}/libs/kiplatform/include" )

file( GLOB ORACLE_SHIMS CONFIGURE_DEPENDS "${ORACLE_OVERLAY}/shims/*.cpp" )
# GNU ld keeps the first weak/COMDAT template definition. Preserve the measured
# matrix adapter's selection when the later hash adapter is present: alphabetical
# glob order would otherwise put kimath_header_hash before kimath_math and select
# differently compiled JSON/string helpers. Both executables use the same order.
set( MATRIX_SHIM "${ORACLE_OVERLAY}/shims/kimath_math.cpp" )
set( HASH_SHIM "${ORACLE_OVERLAY}/shims/kimath_header_hash.cpp" )
if( MATRIX_SHIM IN_LIST ORACLE_SHIMS AND HASH_SHIM IN_LIST ORACLE_SHIMS )
    list( REMOVE_ITEM ORACLE_SHIMS "${HASH_SHIM}" )
    list( FIND ORACLE_SHIMS "${MATRIX_SHIM}" MATRIX_SHIM_INDEX )
    math( EXPR HASH_SHIM_INDEX "${MATRIX_SHIM_INDEX} + 1" )
    list( INSERT ORACLE_SHIMS ${HASH_SHIM_INDEX} "${HASH_SHIM}" )
endif()
# Add the utility/box adapter after already qualified weak template definitions.
set( UTIL_BOX_SHIM "${ORACLE_OVERLAY}/shims/util_box.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${UTIL_BOX_SHIM}" )
list( APPEND ORACLE_SHIMS "${UTIL_BOX_SHIM}" )
# Adapters for out-of-line kimath sources (shims/kimath_src_*.cpp) follow every
# previously qualified adapter, in name order, so their weak template copies are
# never selected ahead of an earlier adapter's.
file( GLOB KIMATH_SRC_SHIMS CONFIGURE_DEPENDS "${ORACLE_OVERLAY}/shims/kimath_src_*.cpp" )
if( KIMATH_SRC_SHIMS )
    list( SORT KIMATH_SRC_SHIMS )
    list( REMOVE_ITEM ORACLE_SHIMS ${KIMATH_SRC_SHIMS} )
    list( APPEND ORACLE_SHIMS ${KIMATH_SRC_SHIMS} )
endif()
# Keep previously qualified adapters first when selecting weak template bodies: the
# Clipper2 header adapter follows them, and the geometry adapter stays last.
set( CLIPPER2_SHIM "${ORACLE_OVERLAY}/shims/clipper2_core_engine.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${CLIPPER2_SHIM}" )
list( APPEND ORACLE_SHIMS "${CLIPPER2_SHIM}" )
# The libs/core header adapter follows every previously qualified adapter too.
set( CORE_INCLUDE_SHIM "${ORACLE_OVERLAY}/shims/core_include_p001.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${CORE_INCLUDE_SHIM}" )
list( APPEND ORACLE_SHIMS "${CORE_INCLUDE_SHIM}" )
# The engine source adapter compiles the pinned clipper.engine.cpp itself into its translation
# unit (in a renamed namespace) to reach its private and file-local bodies; it follows the
# header adapter so no earlier weak definition changes.
set( CLIPPER2_ENGINE_P001_SHIM "${ORACLE_OVERLAY}/shims/clipper2_engine_p001.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${CLIPPER2_ENGINE_P001_SHIM}" )
list( APPEND ORACLE_SHIMS "${CLIPPER2_ENGINE_P001_SHIM}" )
set_source_files_properties( "${CLIPPER2_ENGINE_P001_SHIM}" PROPERTIES INCLUDE_DIRECTORIES
                             "${KICAD_REFERENCE_SOURCE}/thirdparty/clipper2/Clipper2Lib/src" )
# The vector2wx adapter follows every previously qualified adapter, so none of its weak
# template copies (VECTOR2, JSON helpers) is selected ahead of an earlier adapter's.
set( VECTOR2WX_SHIM "${ORACLE_OVERLAY}/shims/vector2wx.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${VECTOR2WX_SHIM}" )
list( APPEND ORACLE_SHIMS "${VECTOR2WX_SHIM}" )
# The kiplatform Unix adapter follows every previously qualified adapter, vector2wx included.
set( KIPLATFORM_UNIX_SHIM "${ORACLE_OVERLAY}/shims/kiplatform_unix_p001.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${KIPLATFORM_UNIX_SHIM}" )
list( APPEND ORACLE_SHIMS "${KIPLATFORM_UNIX_SHIM}" )
set( GEOMETRY_SHIM "${ORACLE_OVERLAY}/shims/geometry_headers.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${GEOMETRY_SHIM}" )
list( APPEND ORACLE_SHIMS "${GEOMETRY_SHIM}" )
# The rectangle clipping adapter follows every earlier adapter, so its weak template copies
# are never selected ahead of theirs.
set( RECTCLIP_SHIM "${ORACLE_OVERLAY}/shims/clipper2_rectclip.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${RECTCLIP_SHIM}" )
list( APPEND ORACLE_SHIMS "${RECTCLIP_SHIM}" )
# Two extra compilations of the pinned, unmodified clipper.rectclip.cpp for that adapter
# (reference/clipper2_rectclip_replay.h): an uninstrumented copy for the file's inline helpers
# and a UB replay with UBSan in trap mode and _GLIBCXX_ASSERTIONS. Each renames namespace
# Clipper2Lib in its own translation unit; neither replaces the genuine archive's code.
add_library( clipper2-rectclip-replay STATIC
    "${ORACLE_OVERLAY}/reference/clipper2_rectclip_copy.cpp"
    "${ORACLE_OVERLAY}/reference/clipper2_rectclip_guard.cpp" )
target_include_directories( clipper2-rectclip-replay PRIVATE "${ORACLE_OVERLAY}/reference"
    "${KICAD_REFERENCE_SOURCE}/thirdparty/clipper2/Clipper2Lib/src" )
target_link_libraries( clipper2-rectclip-replay PRIVATE clipper2 )
if( CMAKE_CXX_COMPILER_ID STREQUAL "GNU" )
    set( RECTCLIP_TRAP -fsanitize-undefined-trap-on-error )
else()
    set( RECTCLIP_TRAP -fsanitize-trap=all )
endif()
set_source_files_properties( "${ORACLE_OVERLAY}/reference/clipper2_rectclip_guard.cpp"
    PROPERTIES COMPILE_OPTIONS
    "-fsanitize=undefined;-fsanitize=float-cast-overflow;-fno-sanitize=vptr;${RECTCLIP_TRAP};-D_GLIBCXX_ASSERTIONS" )
# The map_helpers adapter follows every previously qualified adapter, so its weak
# wxString/std::map/JSON template copies are never selected ahead of theirs.
set( MAP_HELPERS_SHIM "${ORACLE_OVERLAY}/shims/core_map_helpers.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${MAP_HELPERS_SHIM}" )
list( APPEND ORACLE_SHIMS "${MAP_HELPERS_SHIM}" )
# The application include/core adapter follows every previously qualified adapter, so none
# of its weak template copies (VECTOR2, EDA_ANGLE, JSON helpers) is selected ahead of theirs.
set( CORE_APP_INCLUDE_SHIM "${ORACLE_OVERLAY}/shims/core_app_include_p001.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${CORE_APP_INCLUDE_SHIM}" )
list( APPEND ORACLE_SHIMS "${CORE_APP_INCLUDE_SHIM}" )
# The policy adapter and the unchanged pinned Unix policy source it calls across
# the TU boundary follow every previously qualified object, the map_helpers adapter,
# geometry adapter and reference config included, so they can supply only weak
# definitions none of those provide and every earlier template selection is unchanged.
set( KIPLATFORM_POLICY_SHIM "${ORACLE_OVERLAY}/shims/kiplatform_policy.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${KIPLATFORM_POLICY_SHIM}" )
set( KIPLATFORM_POLICY_SOURCE "${KICAD_REFERENCE_SOURCE}/libs/kiplatform/os/unix/policy.cpp" )
# The engine source p002 adapter compiles the pinned clipper.engine.cpp and clipper.offset.cpp
# into its own translation unit (in a renamed namespace). It follows every previously
# qualified object, the policy adapter and source included, so every earlier weak template
# selection is unchanged.
set( CLIPPER2_ENGINE_P002_SHIM "${ORACLE_OVERLAY}/shims/clipper2_engine_p002.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${CLIPPER2_ENGINE_P002_SHIM}" )
set_source_files_properties( "${CLIPPER2_ENGINE_P002_SHIM}" PROPERTIES INCLUDE_DIRECTORIES
                             "${KICAD_REFERENCE_SOURCE}/thirdparty/clipper2/Clipper2Lib/src" )
# The wx_stl_compat adapter follows every previously qualified object, the policy adapter
# and its pinned source included, so none of its weak wxString/std::map/JSON template
# copies is selected ahead of an earlier adapter's and every accepted selection is unchanged.
set( WX_STL_COMPAT_SHIM "${ORACLE_OVERLAY}/shims/core_wx_stl_compat.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${WX_STL_COMPAT_SHIM}" )
# The seg.h / shape header adapter follows every previously qualified object, the wx_stl_compat
# adapter included, so none of its weak template copies (VECTOR2, SEG, EDA_ANGLE, BOX2, JSON
# helpers) is selected ahead of an earlier adapter's and every accepted selection is unchanged.
set( SHAPE_HEADERS_P003_SHIM "${ORACLE_OVERLAY}/shims/kimath_include_geometry_p003.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${SHAPE_HEADERS_P003_SHIM}" )
add_executable( kicad-oracle main.cpp protocol.cpp registry.cpp ${ORACLE_SHIMS}
                reference/geometry_unreachable_config.cpp "${KIPLATFORM_POLICY_SHIM}"
                "${KIPLATFORM_POLICY_SOURCE}" "${CLIPPER2_ENGINE_P002_SHIM}"
                "${WX_STL_COMPAT_SHIM}" "${SHAPE_HEADERS_P003_SHIM}" )
target_include_directories( kicad-oracle PRIVATE "${ORACLE_OVERLAY}" "${CMAKE_BINARY_DIR}"
                            "${KICAD_REFERENCE_SOURCE}/libs/kiplatform/include" )
target_link_libraries( kicad-oracle PRIVATE kimath nlohmann_json iec60664-reference resistor-reference
                       calculator-reader-reference clipper2-rectclip-replay
                       kiplatform-unix-reference )
# shims/core_p001.cpp scripts the clock that the reference GetRunningMicroSecs reads,
# and shims/core_include_p001.cpp the std::chrono::system_clock::now() that the
# profile.h timers read (libstdc++'s high_resolution_clock), and
# shims/core_app_include_p001.cpp the std::chrono::steady_clock::now() that THROTTLE
# reads. --wrap redirects only
# static objects' references (unarmed calls forward to the real functions); shared
# libraries keep the system clocks.
set( ORACLE_CLOCK_WRAPS -Wl,--wrap=clock_gettime
     -Wl,--wrap=_ZNSt6chrono3_V212system_clock3nowEv
     -Wl,--wrap=_ZNSt6chrono3_V212steady_clock3nowEv )
target_link_options( kicad-oracle PRIVATE -Wl,--gc-sections ${ORACLE_CLOCK_WRAPS} )

# Audit every build, including direct cmake --build calls and warm cache builds.
add_custom_target( oracle-audit
    BYPRODUCTS "${CMAKE_BINARY_DIR}/oracle_provenance.h"
               "${CMAKE_BINARY_DIR}/oracle-provenance.json"
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/audit.py"
            --build "${CMAKE_BINARY_DIR}" --source "${KICAD_REFERENCE_SOURCE}"
            --overlay "${ORACLE_OVERLAY}"
    VERBATIM )
foreach( target core clipper2 kimath fmt iec60664-reference resistor-reference
         calculator-reader-reference clipper2-rectclip-replay
         kiplatform-unix-reference kicad-oracle )
    add_dependencies( ${target} oracle-audit )
endforeach()

enable_testing()
add_test( NAME oracle-protocol
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_protocol.py"
            "$<TARGET_FILE:kicad-oracle>" )
add_test( NAME oracle-build-guards
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_guards.py" )
add_test( NAME oracle-iec60664-build-guards
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_iec60664_guards.py" )
add_executable( oracle-dispatch-test EXCLUDE_FROM_ALL test_dispatch.cpp protocol.cpp registry.cpp
                ${ORACLE_SHIMS} reference/geometry_unreachable_config.cpp
                "${KIPLATFORM_POLICY_SHIM}" "${KIPLATFORM_POLICY_SOURCE}"
                "${CLIPPER2_ENGINE_P002_SHIM}" "${WX_STL_COMPAT_SHIM}"
                "${SHAPE_HEADERS_P003_SHIM}" )
target_include_directories( oracle-dispatch-test PRIVATE "${ORACLE_OVERLAY}"
                            "${KICAD_REFERENCE_SOURCE}/libs/kiplatform/include" )
target_link_libraries( oracle-dispatch-test PRIVATE kimath nlohmann_json iec60664-reference
                       resistor-reference calculator-reader-reference clipper2-rectclip-replay
                       kiplatform-unix-reference )
target_link_options( oracle-dispatch-test PRIVATE -Wl,--gc-sections ${ORACLE_CLOCK_WRAPS} )
add_dependencies( oracle-dispatch-test oracle-audit )
add_test( NAME oracle-dispatch COMMAND oracle-dispatch-test )
add_test( NAME oracle-md5-inputs
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_md5_inputs.py"
            "$<TARGET_FILE:kicad-oracle>" )
set_tests_properties( oracle-md5-inputs PROPERTIES TIMEOUT 60 )
add_test( NAME oracle-iec60664-inputs
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_iec60664_inputs.py"
            "$<TARGET_FILE:kicad-oracle>" )
set_tests_properties( oracle-iec60664-inputs PROPERTIES TIMEOUT 60 )
add_test( NAME oracle-resistor-inputs
    COMMAND "${Python3_EXECUTABLE}"
            "${ORACLE_OVERLAY}/test_resistor_collector_inputs.py"
            "$<TARGET_FILE:kicad-oracle>" )
add_test( NAME oracle-resistor-private-keys
    COMMAND "${Python3_EXECUTABLE}"
            "${ORACLE_OVERLAY}/test_resistor_private_keys.py" --byte-identical
            "$<TARGET_FILE:kicad-oracle>" )
set_tests_properties( oracle-protocol oracle-build-guards oracle-iec60664-build-guards
                      oracle-dispatch oracle-resistor-inputs oracle-resistor-private-keys
                      PROPERTIES TIMEOUT 60 )

add_test( NAME oracle-vector-inputs
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_vector_inputs.py"
            "$<TARGET_FILE:kicad-oracle>" )
set_tests_properties( oracle-vector-inputs PROPERTIES TIMEOUT 60 )

add_test( NAME oracle-math-util-inputs
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_math_util_inputs.py"
            "$<TARGET_FILE:kicad-oracle>" )
set_tests_properties( oracle-math-util-inputs PROPERTIES TIMEOUT 60 )

add_test( NAME oracle-core-p001-inputs
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_core_p001_inputs.py"
            "$<TARGET_FILE:kicad-oracle>" )
set_tests_properties( oracle-core-p001-inputs PROPERTIES TIMEOUT 120 )

add_test( NAME oracle-core-include-p001-inputs
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_core_include_p001_inputs.py"
            "$<TARGET_FILE:kicad-oracle>" )
set_tests_properties( oracle-core-include-p001-inputs PROPERTIES TIMEOUT 120 )
add_test( NAME oracle-vector2wx-inputs
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_vector2wx_inputs.py"
            "$<TARGET_FILE:kicad-oracle>" )
set_tests_properties( oracle-vector2wx-inputs PROPERTIES TIMEOUT 60 )
add_test( NAME oracle-kiplatform-policy-inputs
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_kiplatform_policy_inputs.py"
            "$<TARGET_FILE:kicad-oracle>" )
set_tests_properties( oracle-kiplatform-policy-inputs PROPERTIES TIMEOUT 60 )
add_test( NAME oracle-kiplatform-unix-p001-inputs
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_kiplatform_unix_p001_inputs.py"
            "$<TARGET_FILE:kicad-oracle>" )
set_tests_properties( oracle-kiplatform-unix-p001-inputs PROPERTIES TIMEOUT 60 )
add_test( NAME oracle-wx-stl-compat-inputs
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/test_wx_stl_compat_inputs.py"
            "$<TARGET_FILE:kicad-oracle>" )
set_tests_properties( oracle-wx-stl-compat-inputs PROPERTIES TIMEOUT 60 )
