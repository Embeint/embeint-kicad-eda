# SPDX-License-Identifier: GPL-3.0-or-later
cmake_minimum_required( VERSION 3.22 )
project( kicad_oracle LANGUAGES C CXX )

if( NOT CMAKE_SYSTEM_NAME STREQUAL "Linux" OR NOT CMAKE_GENERATOR STREQUAL "Ninja" )
    message( FATAL_ERROR "The phase 0 oracle supports Linux with Ninja only" )
endif()

set( CMAKE_CXX_STANDARD 20 )
set( CMAKE_CXX_STANDARD_REQUIRED ON )
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

# Compile the unchanged pinned calculator without any GUI panel implementation.
add_library( iec60664-reference STATIC
    "${KICAD_REFERENCE_SOURCE}/pcb_calculator/calculator_panels/iec60664.cpp" )
target_include_directories( iec60664-reference PUBLIC
    "${KICAD_REFERENCE_SOURCE}/pcb_calculator/calculator_panels" )

file( GLOB ORACLE_SHIMS CONFIGURE_DEPENDS "${ORACLE_OVERLAY}/shims/*.cpp" )
add_executable( kicad-oracle main.cpp protocol.cpp registry.cpp ${ORACLE_SHIMS} )
target_include_directories( kicad-oracle PRIVATE "${ORACLE_OVERLAY}" "${CMAKE_BINARY_DIR}" )
target_link_libraries( kicad-oracle PRIVATE kimath nlohmann_json iec60664-reference )

# Audit every build, including direct cmake --build calls and warm cache builds.
add_custom_target( oracle-audit
    BYPRODUCTS "${CMAKE_BINARY_DIR}/oracle_provenance.h"
               "${CMAKE_BINARY_DIR}/oracle-provenance.json"
    COMMAND "${Python3_EXECUTABLE}" "${ORACLE_OVERLAY}/audit.py"
            --build "${CMAKE_BINARY_DIR}" --source "${KICAD_REFERENCE_SOURCE}"
            --overlay "${ORACLE_OVERLAY}"
    VERBATIM )
foreach( target core clipper2 kimath iec60664-reference kicad-oracle )
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
                ${ORACLE_SHIMS} )
target_include_directories( oracle-dispatch-test PRIVATE "${ORACLE_OVERLAY}" )
target_link_libraries( oracle-dispatch-test PRIVATE kimath nlohmann_json iec60664-reference )
add_dependencies( oracle-dispatch-test oracle-audit )
add_test( NAME oracle-dispatch COMMAND oracle-dispatch-test )
set_tests_properties( oracle-protocol oracle-build-guards oracle-iec60664-build-guards
                      oracle-dispatch PROPERTIES TIMEOUT 60 )
