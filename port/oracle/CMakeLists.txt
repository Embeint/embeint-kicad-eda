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
# Keep previously qualified adapters first when selecting weak template bodies.
set( GEOMETRY_SHIM "${ORACLE_OVERLAY}/shims/geometry_headers.cpp" )
list( REMOVE_ITEM ORACLE_SHIMS "${GEOMETRY_SHIM}" )
list( APPEND ORACLE_SHIMS "${GEOMETRY_SHIM}" )
add_executable( kicad-oracle main.cpp protocol.cpp registry.cpp ${ORACLE_SHIMS}
                reference/geometry_unreachable_config.cpp )
target_include_directories( kicad-oracle PRIVATE "${ORACLE_OVERLAY}" "${CMAKE_BINARY_DIR}" )
target_link_libraries( kicad-oracle PRIVATE kimath nlohmann_json iec60664-reference resistor-reference
                       calculator-reader-reference )
target_link_options( kicad-oracle PRIVATE -Wl,--gc-sections )

# Audit every build, including direct cmake --build calls and warm cache builds.
add_custom_target( oracle-audit
    BYPRODUCTS "${CMAKE_BINARY_DIR}/oracle_provenance.h"
               "${CMAKE_BINARY_DIR}/oracle-provenance.json"
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/audit.py"
            --build "${CMAKE_BINARY_DIR}" --source "${KICAD_REFERENCE_SOURCE}"
            --overlay "${ORACLE_OVERLAY}"
    VERBATIM )
foreach( target core clipper2 kimath fmt iec60664-reference resistor-reference
         calculator-reader-reference kicad-oracle )
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
                ${ORACLE_SHIMS} reference/geometry_unreachable_config.cpp )
target_include_directories( oracle-dispatch-test PRIVATE "${ORACLE_OVERLAY}" )
target_link_libraries( oracle-dispatch-test PRIVATE kimath nlohmann_json iec60664-reference
                       resistor-reference calculator-reader-reference )
target_link_options( oracle-dispatch-test PRIVATE -Wl,--gc-sections )
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
