// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for unit include.core.application.pure.p001: the pinned application
// include/core headers inplace_function.h, leak_at_exit.h, mirror.h, throttle.h and
// typeinfo.h, compiled here unmodified. Only validation, conversion, scripting and
// serialization live here; every reply comes from the pinned bodies.
//
// THROTTLE reads std::chrono::steady_clock::now(), an out-of-line libstdc++ function. The
// executables link with -Wl,--wrap=_ZNSt6chrono3_V212steady_clock3nowEv, so the calls
// from this translation unit reach the wrapper below. While a throttle case has armed the
// script each call returns the next scripted time point; otherwise it forwards to the
// real clock.
//
// The THROTTLE and LEAK_AT_EXIT private members are read through the standard rule that
// explicit instantiation names may refer to private members ([temp.spec.general]/6): the
// guards refuse exactly the undefined chrono arithmetic, and the leak cases report whether
// the object exists. No member is written.
//
// Three harness types exercise the headers' generic code paths: FLAKY, whose default
// constructor throws on demand (LEAK_AT_EXIT's retry after a failed initialization); PROBE,
// a callable recording its special members (INPLACE_FUNCTION's moves, copies, failed copies
// and destructions); and the FIX_* hierarchy with its own ClassOf predicates (IsA and
// dyn_cast). None stands for a KiCad class.
#include "registry.h"
#include <core/inplace_function.h>
#include <core/leak_at_exit.h>
#include <core/mirror.h>
#include <core/throttle.h>
#include <core/typeinfo.h>
#include <array>
#include <bit>
#include <chrono>
#include <climits>
#include <cstdint>
#include <functional>
#include <iomanip>
#include <locale>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <sstream>
#include <string>
#include <type_traits>
#include <vector>

static_assert( sizeof( int ) == 4 && sizeof( long ) == 8 && sizeof( void* ) == 8 && CHAR_BIT == 8,
               "include/core cases require the pinned LP64 Linux profile" );
static_assert( std::is_same_v<std::chrono::steady_clock::duration, std::chrono::nanoseconds>
                       && std::is_same_v<std::chrono::milliseconds::rep, long>,
               "throttle cases script steady_clock as int64 nanoseconds" );

using STEADY_TIME = std::chrono::steady_clock::time_point;

extern "C" STEADY_TIME __real__ZNSt6chrono3_V212steady_clock3nowEv() noexcept;

namespace
{
/// The scripted steady clock. Native harness state only; never portable code.
struct STEADY_SCRIPT
{
    bool                 armed = false;
    std::vector<int64_t> values;
    size_t               calls = 0;
};

STEADY_SCRIPT g_steady;
} // namespace

extern "C" STEADY_TIME __wrap__ZNSt6chrono3_V212steady_clock3nowEv() noexcept
{
    if( !g_steady.armed )
        return __real__ZNSt6chrono3_V212steady_clock3nowEv();

    // The adapter supplies exactly one value per call; it never reads past the script.
    return STEADY_TIME( std::chrono::nanoseconds( g_steady.values[g_steady.calls++] ) );
}

// ---- private member access ([temp.spec.general]/6) -----------------------------------

namespace
{
using THROTTLE_LAST = STEADY_TIME THROTTLE::*;

struct THROTTLE_LAST_TAG
{
    friend THROTTLE_LAST LastMember( THROTTLE_LAST_TAG );
};

template <THROTTLE_LAST Member>
struct THROTTLE_LAST_ACCESS
{
    friend THROTTLE_LAST LastMember( THROTTLE_LAST_TAG ) { return Member; }
};
} // namespace

template struct THROTTLE_LAST_ACCESS<&THROTTLE::m_last>;

#define LEAK_INSTANCE_ACCESS( NAME, TYPE )                                                     \
    namespace                                                                                  \
    {                                                                                          \
    struct NAME##_TAG                                                                          \
    {                                                                                          \
        friend TYPE* LEAK_AT_EXIT<TYPE>::* InstanceMember( NAME##_TAG );                       \
    };                                                                                         \
    template <TYPE* LEAK_AT_EXIT<TYPE>::* Member>                                              \
    struct NAME##_ACCESS                                                                       \
    {                                                                                          \
        friend TYPE* LEAK_AT_EXIT<TYPE>::* InstanceMember( NAME##_TAG ) { return Member; }     \
    };                                                                                         \
    }                                                                                          \
    template struct NAME##_ACCESS<&LEAK_AT_EXIT<TYPE>::m_instance>;

using LEAK_STRING = std::string;
using LEAK_VECTOR = std::vector<int>;
using LEAK_MAP = std::map<std::string, int>;
using LEAK_SHARED = std::shared_ptr<std::string>;

namespace
{
/// The exception the scripted FLAKY() constructor throws.
struct FLAKY_FAILURE
{
};

/// The FLAKY() script: failures still pending and constructor calls so far. Native harness
/// state only; reset by every leak case.
struct FLAKY_SCRIPT
{
    int64_t failures = 0;
    int64_t attempts = 0;
};

FLAKY_SCRIPT g_flaky;

/// A type whose value-initialization can throw; its value is the attempt that created it.
struct FLAKY
{
    FLAKY()
    {
        ++g_flaky.attempts;

        if( g_flaky.failures > 0 )
        {
            --g_flaky.failures;
            throw FLAKY_FAILURE();
        }

        m_value = int( g_flaky.attempts );
    }

    int m_value = 0;
};
} // namespace

using LEAK_FLAKY = FLAKY;
LEAK_INSTANCE_ACCESS( LEAK_INT, int )
LEAK_INSTANCE_ACCESS( LEAK_STRING, LEAK_STRING )
LEAK_INSTANCE_ACCESS( LEAK_VECTOR, LEAK_VECTOR )
LEAK_INSTANCE_ACCESS( LEAK_MAP, LEAK_MAP )
LEAK_INSTANCE_ACCESS( LEAK_SHARED, LEAK_SHARED )
LEAK_INSTANCE_ACCESS( LEAK_FLAKY, LEAK_FLAKY )

namespace
{
using oracle::JSON;

constexpr size_t  MAX_TEXT = size_t( 1 ) << 16;
constexpr size_t  MAX_STEPS = 256;
constexpr size_t  MAX_CLOCK = 4096;
constexpr int64_t MAX_TOKEN = int64_t( 1 ) << 40;
constexpr int     SLOTS = 4;
constexpr int     LEAK_SLOTS = 2;
const char* const UNDEFINED = "undefined: signed overflow";

[[noreturn]] void Fail( const std::string& aMessage )
{
    throw std::invalid_argument( aMessage );
}


// Exactly the required fields plus any of the optional ones.
void Fields( const JSON& aValue, std::initializer_list<const char*> aRequired )
{
    if( !aValue.is_object() )
        Fail( "expected object" );

    if( aValue.size() != aRequired.size() )
        Fail( "unexpected or missing field" );

    for( const char* key : aRequired )
    {
        if( !aValue.contains( key ) )
            Fail( "unexpected or missing field" );
    }
}


const JSON& Member( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) )
        Fail( "unexpected or missing field" );

    return aInput.at( aName );
}


std::string Text( const JSON& aInput, const char* aName )
{
    if( !Member( aInput, aName ).is_string() )
        Fail( std::string( "expected string: " ) + aName );

    std::string value = aInput.at( aName ).get<std::string>();

    if( value.size() > MAX_TEXT )
        Fail( std::string( "string too long: " ) + aName );

    return value;
}


int64_t Signed( const JSON& aValue, const char* aName, int64_t aMin, int64_t aMax )
{
    const auto fail = [&]() { Fail( std::string( "integer out of range: " ) + aName ); };

    if( !aValue.is_number_integer() )
        fail();

    if( aValue.is_number_unsigned() && aValue.get<uint64_t>() > uint64_t( INT64_MAX ) )
        fail();

    const int64_t number = aValue.get<int64_t>();

    if( number < aMin || number > aMax )
        fail();

    return number;
}


const JSON& Array( const JSON& aInput, const char* aName, size_t aMax )
{
    if( !aInput.at( aName ).is_array() )
        Fail( std::string( "expected array: " ) + aName );

    if( aInput.at( aName ).size() > aMax )
        Fail( std::string( "array too long: " ) + aName );

    return aInput.at( aName );
}


double DecodeBits( const JSON& aValue, const char* aName )
{
    if( !aValue.is_string() )
        Fail( std::string( "expected hex: " ) + aName );

    const std::string text = aValue.get<std::string>();

    if( text.size() != 16 || text.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
        Fail( std::string( "expected hex: " ) + aName );

    return std::bit_cast<double>( std::stoull( text, nullptr, 16 ) );
}


JSON Bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


// ---- typeinfo.h ---------------------------------------------------------------------------

struct ENUMERATOR
{
    const char* name;
    KICAD_T     value;
};

// Every KICAD_T enumerator, in declaration order, named as in the header.
const ENUMERATOR ENUMERATORS[] = {
    { "NOT_USED", NOT_USED },
    { "TYPE_NOT_INIT", TYPE_NOT_INIT },
    { "PCB_T", PCB_T },
    { "SCREEN_T", SCREEN_T },
    { "PCB_FOOTPRINT_T", PCB_FOOTPRINT_T },
    { "PCB_PAD_T", PCB_PAD_T },
    { "PCB_SHAPE_T", PCB_SHAPE_T },
    { "PCB_REFERENCE_IMAGE_T", PCB_REFERENCE_IMAGE_T },
    { "PCB_FIELD_T", PCB_FIELD_T },
    { "PCB_GENERATOR_T", PCB_GENERATOR_T },
    { "PCB_TEXT_T", PCB_TEXT_T },
    { "PCB_TEXTBOX_T", PCB_TEXTBOX_T },
    { "PCB_TABLE_T", PCB_TABLE_T },
    { "PCB_TABLECELL_T", PCB_TABLECELL_T },
    { "PCB_TRACE_T", PCB_TRACE_T },
    { "PCB_VIA_T", PCB_VIA_T },
    { "PCB_ARC_T", PCB_ARC_T },
    { "PCB_MARKER_T", PCB_MARKER_T },
    { "PCB_DIMENSION_T", PCB_DIMENSION_T },
    { "PCB_BARCODE_T", PCB_BARCODE_T },
    { "PCB_DIM_ALIGNED_T", PCB_DIM_ALIGNED_T },
    { "PCB_DIM_LEADER_T", PCB_DIM_LEADER_T },
    { "PCB_DIM_CENTER_T", PCB_DIM_CENTER_T },
    { "PCB_DIM_RADIAL_T", PCB_DIM_RADIAL_T },
    { "PCB_DIM_ORTHOGONAL_T", PCB_DIM_ORTHOGONAL_T },
    { "PCB_TARGET_T", PCB_TARGET_T },
    { "PCB_ZONE_T", PCB_ZONE_T },
    { "PCB_ITEM_LIST_T", PCB_ITEM_LIST_T },
    { "PCB_NETINFO_T", PCB_NETINFO_T },
    { "PCB_GROUP_T", PCB_GROUP_T },
    { "PCB_BOARD_OUTLINE_T", PCB_BOARD_OUTLINE_T },
    { "PCB_POINT_T", PCB_POINT_T },
    { "PCB_FIELD_LOCATE_REFERENCE_T", PCB_FIELD_LOCATE_REFERENCE_T },
    { "PCB_FIELD_LOCATE_VALUE_T", PCB_FIELD_LOCATE_VALUE_T },
    { "PCB_FIELD_LOCATE_FOOTPRINT_T", PCB_FIELD_LOCATE_FOOTPRINT_T },
    { "PCB_FIELD_LOCATE_DATASHEET_T", PCB_FIELD_LOCATE_DATASHEET_T },
    { "PCB_LOCATE_STDVIA_T", PCB_LOCATE_STDVIA_T },
    { "PCB_LOCATE_UVIA_T", PCB_LOCATE_UVIA_T },
    { "PCB_LOCATE_BLINDVIA_T", PCB_LOCATE_BLINDVIA_T },
    { "PCB_LOCATE_BURIEDVIA_T", PCB_LOCATE_BURIEDVIA_T },
    { "PCB_LOCATE_TEXT_T", PCB_LOCATE_TEXT_T },
    { "PCB_LOCATE_HOLE_T", PCB_LOCATE_HOLE_T },
    { "PCB_LOCATE_PTH_T", PCB_LOCATE_PTH_T },
    { "PCB_LOCATE_NPTH_T", PCB_LOCATE_NPTH_T },
    { "PCB_LOCATE_BOARD_EDGE_T", PCB_LOCATE_BOARD_EDGE_T },
    { "PCB_SHAPE_LOCATE_SEGMENT_T", PCB_SHAPE_LOCATE_SEGMENT_T },
    { "PCB_SHAPE_LOCATE_RECT_T", PCB_SHAPE_LOCATE_RECT_T },
    { "PCB_SHAPE_LOCATE_CIRCLE_T", PCB_SHAPE_LOCATE_CIRCLE_T },
    { "PCB_SHAPE_LOCATE_ARC_T", PCB_SHAPE_LOCATE_ARC_T },
    { "PCB_SHAPE_LOCATE_POLY_T", PCB_SHAPE_LOCATE_POLY_T },
    { "PCB_SHAPE_LOCATE_BEZIER_T", PCB_SHAPE_LOCATE_BEZIER_T },
    { "LIB_SYMBOL_T", LIB_SYMBOL_T },
    { "SCH_SHAPE_T", SCH_SHAPE_T },
    { "SCH_FIELD_T", SCH_FIELD_T },
    { "SCH_TEXT_T", SCH_TEXT_T },
    { "SCH_TEXTBOX_T", SCH_TEXTBOX_T },
    { "SCH_PIN_T", SCH_PIN_T },
    { "SCH_MARKER_T", SCH_MARKER_T },
    { "SCH_JUNCTION_T", SCH_JUNCTION_T },
    { "SCH_NO_CONNECT_T", SCH_NO_CONNECT_T },
    { "SCH_BUS_WIRE_ENTRY_T", SCH_BUS_WIRE_ENTRY_T },
    { "SCH_BUS_BUS_ENTRY_T", SCH_BUS_BUS_ENTRY_T },
    { "SCH_LINE_T", SCH_LINE_T },
    { "SCH_BITMAP_T", SCH_BITMAP_T },
    { "SCH_TABLE_T", SCH_TABLE_T },
    { "SCH_TABLECELL_T", SCH_TABLECELL_T },
    { "SCH_LABEL_T", SCH_LABEL_T },
    { "SCH_GLOBAL_LABEL_T", SCH_GLOBAL_LABEL_T },
    { "SCH_HIER_LABEL_T", SCH_HIER_LABEL_T },
    { "SCH_RULE_AREA_T", SCH_RULE_AREA_T },
    { "SCH_DIRECTIVE_LABEL_T", SCH_DIRECTIVE_LABEL_T },
    { "SCH_SYMBOL_T", SCH_SYMBOL_T },
    { "SCH_GROUP_T", SCH_GROUP_T },
    { "SCH_SHEET_PIN_T", SCH_SHEET_PIN_T },
    { "SCH_SHEET_T", SCH_SHEET_T },
    { "SCH_FIELD_LOCATE_REFERENCE_T", SCH_FIELD_LOCATE_REFERENCE_T },
    { "SCH_FIELD_LOCATE_VALUE_T", SCH_FIELD_LOCATE_VALUE_T },
    { "SCH_FIELD_LOCATE_FOOTPRINT_T", SCH_FIELD_LOCATE_FOOTPRINT_T },
    { "SCH_FIELD_LOCATE_DATASHEET_T", SCH_FIELD_LOCATE_DATASHEET_T },
    { "SCH_ITEM_LOCATE_WIRE_T", SCH_ITEM_LOCATE_WIRE_T },
    { "SCH_ITEM_LOCATE_BUS_T", SCH_ITEM_LOCATE_BUS_T },
    { "SCH_ITEM_LOCATE_GRAPHIC_LINE_T", SCH_ITEM_LOCATE_GRAPHIC_LINE_T },
    { "SCH_LABEL_LOCATE_ANY_T", SCH_LABEL_LOCATE_ANY_T },
    { "SCH_LABEL_LOCATE_WIRE_T", SCH_LABEL_LOCATE_WIRE_T },
    { "SCH_LABEL_LOCATE_BUS_T", SCH_LABEL_LOCATE_BUS_T },
    { "SCH_SYMBOL_LOCATE_POWER_T", SCH_SYMBOL_LOCATE_POWER_T },
    { "SCH_LOCATE_ANY_T", SCH_LOCATE_ANY_T },
    { "SCH_SCREEN_T", SCH_SCREEN_T },
    { "SCHEMATIC_T", SCHEMATIC_T },
    { "GERBER_LAYOUT_T", GERBER_LAYOUT_T },
    { "GERBER_DRAW_ITEM_T", GERBER_DRAW_ITEM_T },
    { "GERBER_IMAGE_T", GERBER_IMAGE_T },
    { "WSG_LINE_T", WSG_LINE_T },
    { "WSG_RECT_T", WSG_RECT_T },
    { "WSG_POLY_T", WSG_POLY_T },
    { "WSG_TEXT_T", WSG_TEXT_T },
    { "WSG_BITMAP_T", WSG_BITMAP_T },
    { "WSG_PAGE_T", WSG_PAGE_T },
    { "WS_PROXY_UNDO_ITEM_T", WS_PROXY_UNDO_ITEM_T },
    { "WS_PROXY_UNDO_ITEM_PLUS_T", WS_PROXY_UNDO_ITEM_PLUS_T },
    { "SYMBOL_LIB_TABLE_T", SYMBOL_LIB_TABLE_T },
    { "FP_LIB_TABLE_T", FP_LIB_TABLE_T },
    { "DESIGN_BLOCK_LIB_TABLE_T", DESIGN_BLOCK_LIB_TABLE_T },
    { "SYMBOL_LIBS_T", SYMBOL_LIBS_T },
    { "SEARCH_STACK_T", SEARCH_STACK_T },
    { "S3D_CACHE_T", S3D_CACHE_T },
    { "PCB_SHAPE_LOCATE_ELLIPSE_T", PCB_SHAPE_LOCATE_ELLIPSE_T },
    { "PCB_SHAPE_LOCATE_ELLIPSE_ARC_T", PCB_SHAPE_LOCATE_ELLIPSE_ARC_T },
    { "PCB_CONSTRAINT_T", PCB_CONSTRAINT_T },
    { "PCB_GRID_ITEM_T", PCB_GRID_ITEM_T },
    { "PCB_DRILL_CHART_T", PCB_DRILL_CHART_T },
    { "PCB_DRILL_MAP_T", PCB_DRILL_MAP_T },
    { "PCB_3D_BODY_T", PCB_3D_BODY_T },
    { "PCB_STACKUP_TABLE_T", PCB_STACKUP_TABLE_T },
    { "MAX_STRUCT_TYPE_ID", MAX_STRUCT_TYPE_ID },
};

static_assert( std::size( ENUMERATORS ) == 115 );


KICAD_T ReadType( const JSON& aInput )
{
    // KICAD_T's values are those of a signed 8-bit bit-field ([dcl.enum]/8); converting
    // any other integer is undefined ([expr.static.cast]/10).
    return static_cast<KICAD_T>( int( Signed( aInput.at( "value" ), "value", -128, 127 ) ) );
}


JSON TypeInfo( const JSON& aInput )
{
    Fields( aInput, { "op", "value" } );
    const KICAD_T type = ReadType( aInput );

    return { { "base_type", int( BaseType( type ) ) },
             { "is_null", IsNullType( type ) },
             { "is_generated_table", IsGeneratedTableType( type ) },
             { "is_instantiable", IsInstantiableType( type ) },
             { "is_eeschema", IsEeschemaType( type ) },
             { "is_pcbnew", IsPcbnewType( type ) },
             { "is_single_layer", IsSingleLayerType( type ) },
             { "is_gerbview", IsGerbviewType( type ) },
             { "is_page_layout_editor", IsPageLayoutEditorType( type ) },
             { "is_misc", IsMiscType( type ) },
             { "is_type_correct", IsTypeCorrect( type ) } };
}


JSON Enumerators( const JSON& aInput )
{
    Fields( aInput, { "op" } );
    JSON out = JSON::array();

    for( const ENUMERATOR& e : ENUMERATORS )
        out.push_back( JSON::array( { e.name, int( e.value ) } ) );

    return out;
}


// ---- mirror.h -----------------------------------------------------------------------------

template <typename T>
T ReadScalar( const JSON& aValue, const char* aName )
{
    if constexpr( std::is_same_v<T, double> )
        return DecodeBits( aValue, aName );
    else
        return T( Signed( aValue, aName, std::numeric_limits<T>::min(),
                          std::numeric_limits<T>::max() ) );
}


template <typename T>
JSON WriteScalar( T aValue )
{
    if constexpr( std::is_same_v<T, double> )
        return Bits( aValue );
    else
        return aValue;
}


// -( aPoint - aMirrorRef ) + aMirrorRef must not overflow in the integer instantiations.
template <typename T>
void CheckMirrorVal( T aPoint, T aMirrorRef )
{
    if constexpr( std::is_integral_v<T> )
    {
        T difference;
        T sum;

        if( __builtin_sub_overflow( aPoint, aMirrorRef, &difference )
            || difference == std::numeric_limits<T>::min()
            || __builtin_add_overflow( -difference, aMirrorRef, &sum ) )
        {
            Fail( UNDEFINED );
        }
    }
}


FLIP_DIRECTION ReadDirection( const JSON& aInput )
{
    // A scoped enumeration with the fixed underlying type int: every int is a value.
    return static_cast<FLIP_DIRECTION>(
            int( Signed( aInput.at( "direction" ), "direction", INT_MIN, INT_MAX ) ) );
}


template <typename T>
JSON MirrorScalar( const JSON& aInput, bool aInPlace )
{
    const T point = ReadScalar<T>( aInput.at( "point" ), "point" );
    const T ref = ReadScalar<T>( aInput.at( "ref" ), "ref" );
    CheckMirrorVal( point, ref );

    if( !aInPlace )
        return { { "value", WriteScalar( MIRRORVAL( point, ref ) ) } };

    T updated = point;
    MIRROR( updated, ref );
    return { { "point", WriteScalar( updated ) } };
}


template <typename T>
VECTOR2<T> ReadVector( const JSON& aInput, const char* aName )
{
    const JSON& value = aInput.at( aName );

    if( !value.is_array() || value.size() != 2 )
        Fail( std::string( "expected point: " ) + aName );

    return VECTOR2<T>( ReadScalar<T>( value[0], aName ), ReadScalar<T>( value[1], aName ) );
}


template <typename T>
JSON MirrorVector( const JSON& aInput )
{
    VECTOR2<T>           point = ReadVector<T>( aInput, "point" );
    const VECTOR2<T>     ref = ReadVector<T>( aInput, "ref" );
    const FLIP_DIRECTION direction = ReadDirection( aInput );

    // The guard checks the coordinate the body mirrors, chosen by the same test.
    if( direction == FLIP_DIRECTION::LEFT_RIGHT )
        CheckMirrorVal( point.x, ref.x );
    else
        CheckMirrorVal( point.y, ref.y );

    MIRROR( point, ref, direction );
    return { { "point", JSON::array( { WriteScalar( point.x ), WriteScalar( point.y ) } ) } };
}


JSON Mirror( const JSON& aInput, const std::string& aOp )
{
    if( aOp == "mirror_axis" )
    {
        Fields( aInput, { "op", "direction" } );
        return { { "degrees", Bits( MIRROR_AXIS( ReadDirection( aInput ) ).AsDegrees() ) } };
    }

    if( aOp == "mirror_vector" )
    {
        Fields( aInput, { "op", "type", "point", "ref", "direction" } );
        const std::string type = Text( aInput, "type" );

        if( type == "VECTOR2I" )
            return MirrorVector<int>( aInput );
        if( type == "VECTOR2L" )
            return MirrorVector<int64_t>( aInput );
        if( type == "VECTOR2D" )
            return MirrorVector<double>( aInput );

        Fail( "unknown type" );
    }

    Fields( aInput, { "op", "type", "point", "ref" } );
    const std::string type = Text( aInput, "type" );
    const bool        inPlace = aOp == "mirror";

    if( type == "int" )
        return MirrorScalar<int>( aInput, inPlace );
    if( type == "int64_t" )
        return MirrorScalar<int64_t>( aInput, inPlace );
    if( type == "double" )
        return MirrorScalar<double>( aInput, inPlace );

    Fail( "unknown type" );
}


// ---- throttle.h ---------------------------------------------------------------------------

JSON Throttle( const JSON& aInput )
{
    Fields( aInput, { "op", "interval_ms", "clock" } );
    const int64_t interval = Signed( aInput.at( "interval_ms" ), "interval_ms", INT64_MIN, INT64_MAX );
    const JSON&   clock = Array( aInput, "clock", MAX_CLOCK );

    if( clock.empty() )
        Fail( "expected array: clock" );

    std::vector<int64_t> values;

    for( const JSON& value : clock )
        values.push_back( Signed( value, "clock", INT64_MIN, INT64_MAX ) );

    // m_last( steady_clock::now() - aInterval ): the milliseconds convert to nanoseconds,
    // then the nanosecond counts subtract; either signed overflow is undefined.
    int64_t intervalNs;
    int64_t last;

    if( __builtin_mul_overflow( interval, int64_t( 1000000 ), &intervalNs )
        || __builtin_sub_overflow( values[0], intervalNs, &last ) )
    {
        Fail( "undefined: chrono overflow" );
    }

    struct ARMED
    {
        ARMED( std::vector<int64_t> aValues )
        {
            g_steady = STEADY_SCRIPT{ true, std::move( aValues ), 0 };
        }

        ~ARMED() { g_steady = STEADY_SCRIPT{}; }
    };

    ARMED    armed( values );
    THROTTLE throttle{ std::chrono::milliseconds( interval ) };
    JSON     results = JSON::array();

    for( size_t i = 1; i < values.size(); ++i )
    {
        // now - m_last, read from the genuine object before the call.
        const int64_t previous = ( throttle.*LastMember( THROTTLE_LAST_TAG() ) ).time_since_epoch().count();
        int64_t       elapsed;

        if( __builtin_sub_overflow( values[i], previous, &elapsed ) )
            Fail( "undefined: chrono overflow" );

        results.push_back( throttle.Ready() );
    }

    return { { "results", std::move( results ) },
             { "last", ( throttle.*LastMember( THROTTLE_LAST_TAG() ) ).time_since_epoch().count() },
             { "clock_calls", g_steady.calls } };
}


// ---- leak_at_exit.h -----------------------------------------------------------------------

template <typename T>
T DecodeLeak( const JSON& aValue )
{
    if constexpr( std::is_same_v<T, int> )
    {
        return int( Signed( aValue, "value", INT_MIN, INT_MAX ) );
    }
    else if constexpr( std::is_same_v<T, LEAK_STRING> )
    {
        if( !aValue.is_string() || aValue.get<std::string>().size() > MAX_TEXT )
            Fail( "expected string: value" );

        return aValue.get<std::string>();
    }
    else if constexpr( std::is_same_v<T, LEAK_VECTOR> )
    {
        if( !aValue.is_array() || aValue.size() > MAX_STEPS )
            Fail( "expected array: value" );

        LEAK_VECTOR out;

        for( const JSON& v : aValue )
            out.push_back( int( Signed( v, "value", INT_MIN, INT_MAX ) ) );

        return out;
    }
    else if constexpr( std::is_same_v<T, LEAK_MAP> )
    {
        if( !aValue.is_array() || aValue.size() > MAX_STEPS )
            Fail( "expected array: value" );

        LEAK_MAP out;

        for( const JSON& v : aValue )
        {
            if( !v.is_array() || v.size() != 2 || !v[0].is_string()
                || v[0].get<std::string>().size() > MAX_TEXT )
            {
                Fail( "expected entry: value" );
            }

            out[v[0].get<std::string>()] = int( Signed( v[1], "value", INT_MIN, INT_MAX ) );
        }

        return out;
    }
    else if constexpr( std::is_same_v<T, LEAK_FLAKY> )
    {
        Fail( "flaky values are set by construction" );
    }
    else
    {
        Fail( "shared values are set by share" );
    }
}


template <typename T>
JSON EncodeLeak( const T& aValue )
{
    if constexpr( std::is_same_v<T, LEAK_MAP> )
    {
        JSON out = JSON::array();

        for( const auto& [key, value] : aValue )
            out.push_back( JSON::array( { key, value } ) );

        return out;
    }
    else if constexpr( std::is_same_v<T, LEAK_SHARED> )
    {
        return aValue ? JSON( *aValue ) : JSON();
    }
    else if constexpr( std::is_same_v<T, LEAK_FLAKY> )
    {
        return aValue.m_value;
    }
    else
    {
        return aValue;
    }
}


template <typename T>
T& Access( LEAK_AT_EXIT<T>& aLeak, const std::string& aVia )
{
    if( aVia == "get" )
        return aLeak.Get();
    if( aVia == "deref" )
        return *aLeak;
    if( aVia == "arrow" )
        return *aLeak.operator->();
    if( aVia == "convert" )
        return static_cast<T&>( aLeak );

    Fail( "unknown accessor" );
}


// An index into the pointers retained from destroyed wrappers.
size_t RetainedIndex( const JSON& aStep, size_t aTaken )
{
    const int64_t index = Signed( Member( aStep, "index" ), "index", 0, int64_t( MAX_STEPS ) );

    if( size_t( index ) >= aTaken )
        Fail( "retained pointer not taken" );

    return size_t( index );
}


template <typename T, typename TAG>
JSON RunLeak( const JSON& aSteps )
{
    constexpr bool FLAKY_TYPE = std::is_same_v<T, LEAK_FLAKY>;
    constexpr bool SIZED = !std::is_same_v<T, int> && !std::is_same_v<T, LEAK_SHARED> && !FLAKY_TYPE;

    std::array<std::optional<LEAK_AT_EXIT<T>>, LEAK_SLOTS> slots;
    std::vector<T*>              retained; // Get() results whose wrapper was destroyed
    const LEAK_SHARED            external = std::make_shared<std::string>( "leaked" );
    JSON                         results = JSON::array();

    g_flaky = FLAKY_SCRIPT();

    // Validate every step before any operation.
    for( const JSON& step : aSteps )
    {
        if( !step.is_object() || !step.contains( "step" ) )
            Fail( "expected step object" );
    }

    for( const JSON& step : aSteps )
    {
        const std::string name = Text( step, "step" );

        if( name == "use_count" )
        {
            Fields( step, { "step" } );

            if constexpr( !std::is_same_v<T, LEAK_SHARED> )
                Fail( "use_count needs shared_string" );

            results.push_back( external.use_count() );
            continue;
        }

        if( name == "fail_next" )
        {
            Fields( step, { "step", "count" } );

            if constexpr( !FLAKY_TYPE )
                Fail( "fail_next needs flaky" );

            g_flaky.failures = Signed( Member( step, "count" ), "count", 0, int64_t( MAX_STEPS ) );
            results.push_back( nullptr );
            continue;
        }

        if( name == "attempts" )
        {
            Fields( step, { "step" } );

            if constexpr( !FLAKY_TYPE )
                Fail( "attempts needs flaky" );

            results.push_back( g_flaky.attempts );
            continue;
        }

        if( name == "read_retained" )
        {
            Fields( step, { "step", "index" } );
            const size_t index = RetainedIndex( step, retained.size() );
            results.push_back( EncodeLeak( *retained[index] ) );
            continue;
        }

        if( name == "write_retained" )
        {
            Fields( step, { "step", "index", "value" } );
            const size_t index = RetainedIndex( step, retained.size() );
            T            value = DecodeLeak<T>( step.at( "value" ) );
            *retained[index] = std::move( value );
            results.push_back( nullptr );
            continue;
        }

        const int slot = int( Signed( Member( step, "slot" ), "slot", 0, LEAK_SLOTS - 1 ) );
        std::optional<LEAK_AT_EXIT<T>>& leak = slots[slot];

        if( name == "construct" )
        {
            Fields( step, { "step", "slot" } );

            if( leak )
                Fail( "slot already constructed" );

            leak.emplace();
            results.push_back( nullptr );
            continue;
        }

        if( !leak )
            Fail( "slot not constructed" );

        // A throwing FLAKY() leaves the once_flag unset; the step reports the exception.
        try
        {
            if( name == "created" )
            {
                Fields( step, { "step", "slot" } );
                results.push_back( ( *leak ).*InstanceMember( TAG() ) != nullptr );
            }
            else if( name == "identity" )
            {
                Fields( step, { "step", "slot" } );
                T* const object = &leak->Get();
                results.push_back( object == &**leak && object == leak->operator->()
                                   && object == &static_cast<T&>( *leak ) );
            }
            else if( name == "read" )
            {
                Fields( step, { "step", "slot", "via" } );
                results.push_back( EncodeLeak( Access( *leak, Text( step, "via" ) ) ) );
            }
            else if( name == "write" )
            {
                Fields( step, { "step", "slot", "via", "value" } );
                const std::string via = Text( step, "via" );
                T                 value = DecodeLeak<T>( step.at( "value" ) );
                Access( *leak, via ) = std::move( value );
                results.push_back( nullptr );
            }
            else if( name == "share" )
            {
                Fields( step, { "step", "slot", "via" } );

                if constexpr( std::is_same_v<T, LEAK_SHARED> )
                    Access( *leak, Text( step, "via" ) ) = external;
                else
                    Fail( "share needs shared_string" );

                results.push_back( nullptr );
            }
            else if( name == "size" )
            {
                Fields( step, { "step", "slot" } );

                if constexpr( !SIZED )
                    Fail( "size needs a container" );
                else
                    results.push_back( ( *leak )->size() );
            }
            else if( name == "release" )
            {
                // The object outlives its wrapper; the pointer stays usable.
                Fields( step, { "step", "slot" } );
                T* const object = &leak->Get();
                leak.reset();
                retained.push_back( object );
                results.push_back( nullptr );
            }
            else if( name == "destroy" )
            {
                Fields( step, { "step", "slot" } );
                leak.reset();
                results.push_back( nullptr );
            }
            else
            {
                Fail( "unknown step" );
            }
        }
        catch( const FLAKY_FAILURE& )
        {
            results.push_back( { { "exception", "construction failed" } } );
        }
    }

    return { { "results", std::move( results ) } };
}


JSON Leak( const JSON& aInput )
{
    Fields( aInput, { "op", "type", "steps" } );
    const std::string type = Text( aInput, "type" );
    const JSON&       steps = Array( aInput, "steps", MAX_STEPS );

    if( type == "int" )
        return RunLeak<int, LEAK_INT_TAG>( steps );
    if( type == "string" )
        return RunLeak<LEAK_STRING, LEAK_STRING_TAG>( steps );
    if( type == "vector_int" )
        return RunLeak<LEAK_VECTOR, LEAK_VECTOR_TAG>( steps );
    if( type == "map_string_int" )
        return RunLeak<LEAK_MAP, LEAK_MAP_TAG>( steps );
    if( type == "shared_string" )
        return RunLeak<LEAK_SHARED, LEAK_SHARED_TAG>( steps );
    if( type == "flaky" )
        return RunLeak<LEAK_FLAKY, LEAK_FLAKY_TAG>( steps );

    Fail( "unknown type" );
}


// ---- inplace_function.h -------------------------------------------------------------------

// The signatures the cases instantiate. Each callable kind is a genuine lambda; "shared"
// captures a copy of the case's shared token, whose use_count shows every copy, move and
// destruction of the stored callable.
struct SIG_I64_I32
{
    using SIG = long long( int );
    using TOKEN = std::shared_ptr<long long>;
};
struct SIG_I64_I32_I32
{
    using SIG = long long( int, int );
    using TOKEN = std::shared_ptr<long long>;
};
struct SIG_I64
{
    using SIG = long long();
    using TOKEN = std::shared_ptr<long long>;
};
struct SIG_STRING
{
    using SIG = std::string( std::string );
    using TOKEN = std::shared_ptr<std::string>;
};


std::array<int, 12> ReadBig( const JSON& aSpec )
{
    const JSON& values = aSpec.at( "values" );

    if( !values.is_array() || values.size() != 12 )
        Fail( "expected array: values" );

    std::array<int, 12> out{};

    for( size_t i = 0; i < out.size(); ++i )
        out[i] = int( Signed( values[i], "values", INT_MIN, INT_MAX ) );

    return out;
}


int ReadK( const JSON& aSpec )
{
    return int( Signed( aSpec.at( "k" ), "k", INT_MIN, INT_MAX ) );
}


long long Sum( const std::array<int, 12>& aValues )
{
    long long sum = 0;

    for( int v : aValues )
        sum += v;

    return sum;
}


// Calls aUse with the requested callable, constructed as a prvalue lambda. "big" (48 bytes)
// only exists for the 64-byte capacity: storing it in the default 32 bytes does not compile.
template <typename S, size_t CAP, typename USE>
void WithCallable( const JSON& aSpec, const typename S::TOKEN& aToken, USE&& aUse )
{
    if( !aSpec.is_object() || !aSpec.contains( "kind" ) )
        Fail( "expected callable object" );

    const std::string kind = Text( aSpec, "kind" );

    if constexpr( std::is_same_v<S, SIG_I64_I32> )
    {
        if( kind == "add" )
        {
            Fields( aSpec, { "kind", "k" } );
            const int k = ReadK( aSpec );
            return aUse( [k]( int x ) { return (long long) x + k; } );
        }
        if( kind == "mul" )
        {
            Fields( aSpec, { "kind", "k" } );
            const int k = ReadK( aSpec );
            return aUse( [k]( int x ) { return (long long) x * k; } );
        }
        if( kind == "shared" )
        {
            Fields( aSpec, { "kind", "k" } );
            const int k = ReadK( aSpec );
            return aUse( [p = aToken, k]( int x ) { return *p + x + k; } );
        }
        if( kind == "plain" )
        {
            Fields( aSpec, { "kind" } );
            return aUse( []( int x ) { return (long long) x * 2; } );
        }
        if( kind == "big" )
        {
            Fields( aSpec, { "kind", "values" } );
            const std::array<int, 12> values = ReadBig( aSpec );

            if constexpr( CAP == 64 )
                return aUse( [values]( int x ) { return Sum( values ) + x; } );
            else
                Fail( "callable too large for capacity" );
        }
    }
    else if constexpr( std::is_same_v<S, SIG_I64_I32_I32> )
    {
        if( kind == "add" )
        {
            Fields( aSpec, { "kind", "k" } );
            const int k = ReadK( aSpec );
            return aUse( [k]( int x, int y ) { return (long long) x + y + k; } );
        }
        if( kind == "mul" )
        {
            Fields( aSpec, { "kind", "k" } );
            const int k = ReadK( aSpec );
            return aUse( [k]( int x, int y ) { return (long long) x * k + y; } );
        }
        if( kind == "shared" )
        {
            Fields( aSpec, { "kind" } );
            return aUse( [p = aToken]( int x, int y ) { return *p + x + y; } );
        }
        if( kind == "plain" )
        {
            Fields( aSpec, { "kind" } );
            return aUse( []( int x, int y ) { return (long long) x - y; } );
        }
        if( kind == "big" )
        {
            Fields( aSpec, { "kind", "values" } );
            const std::array<int, 12> values = ReadBig( aSpec );

            if constexpr( CAP == 64 )
                return aUse( [values]( int x, int y ) { return Sum( values ) + x + y; } );
            else
                Fail( "callable too large for capacity" );
        }
    }
    else if constexpr( std::is_same_v<S, SIG_I64> )
    {
        if( kind == "add" )
        {
            Fields( aSpec, { "kind", "k" } );
            const int k = ReadK( aSpec );
            return aUse( [k]() { return (long long) k; } );
        }
        if( kind == "shared" )
        {
            Fields( aSpec, { "kind", "k" } );
            const int k = ReadK( aSpec );
            return aUse( [p = aToken, k]() { return *p + k; } );
        }
        if( kind == "plain" )
        {
            Fields( aSpec, { "kind" } );
            return aUse( []() { return 42LL; } );
        }
        if( kind == "big" )
        {
            Fields( aSpec, { "kind", "values" } );
            const std::array<int, 12> values = ReadBig( aSpec );

            if constexpr( CAP == 64 )
                return aUse( [values]() { return Sum( values ); } );
            else
                Fail( "callable too large for capacity" );
        }
    }
    else
    {
        if( kind == "add" )
        {
            Fields( aSpec, { "kind", "s" } );
            std::string s = Text( aSpec, "s" );
            return aUse( [s = std::move( s )]( std::string aArg ) { return aArg + s; } );
        }
        if( kind == "shared" )
        {
            Fields( aSpec, { "kind" } );
            return aUse( [p = aToken]( std::string aArg ) { return *p + aArg; } );
        }
        if( kind == "plain" )
        {
            Fields( aSpec, { "kind" } );
            return aUse( []( std::string aArg ) { return aArg + aArg; } );
        }
    }

    Fail( "unknown callable" );
}


template <typename S, typename FN>
JSON InvokeOne( const FN& aFunction, const JSON& aArgs )
{
    if( !aArgs.is_array() )
        Fail( "expected array: args" );

    try
    {
        if constexpr( std::is_same_v<S, SIG_I64_I32> )
        {
            if( aArgs.size() != 1 )
                Fail( "wrong argument count" );

            const int x = int( Signed( aArgs[0], "args", INT_MIN, INT_MAX ) );
            return { { "value", aFunction( x ) } };
        }
        else if constexpr( std::is_same_v<S, SIG_I64_I32_I32> )
        {
            if( aArgs.size() != 2 )
                Fail( "wrong argument count" );

            const int x = int( Signed( aArgs[0], "args", INT_MIN, INT_MAX ) );
            const int y = int( Signed( aArgs[1], "args", INT_MIN, INT_MAX ) );
            return { { "value", aFunction( x, y ) } };
        }
        else if constexpr( std::is_same_v<S, SIG_I64> )
        {
            if( !aArgs.empty() )
                Fail( "wrong argument count" );

            return { { "value", aFunction() } };
        }
        else
        {
            if( aArgs.size() != 1 || !aArgs[0].is_string()
                || aArgs[0].get<std::string>().size() > MAX_TEXT )
            {
                Fail( "wrong argument count" );
            }

            return { { "value", aFunction( aArgs[0].get<std::string>() ) } };
        }
    }
    catch( const std::bad_function_call& )
    {
        return { { "exception", "bad_function_call" } };
    }
}


template <typename S, size_t CAP>
JSON RunFunction( const JSON& aInput )
{
    using FN = INPLACE_FUNCTION<typename S::SIG, CAP>;

    typename S::TOKEN token;

    if constexpr( std::is_same_v<S, SIG_STRING> )
        token = std::make_shared<std::string>( Text( aInput, "token" ) );
    else
        token = std::make_shared<long long>( Signed( aInput.at( "token" ), "token", -MAX_TOKEN, MAX_TOKEN ) );

    const JSON& steps = Array( aInput, "steps", MAX_STEPS );

    for( const JSON& step : steps )
    {
        if( !step.is_object() || !step.contains( "step" ) )
            Fail( "expected step object" );
    }

    std::array<std::optional<FN>, SLOTS> slots;
    JSON                                 results = JSON::array();

    auto slotOf = [&]( const JSON& aStep, const char* aName ) -> std::optional<FN>&
    {
        return slots[Signed( Member( aStep, aName ), aName, 0, SLOTS - 1 )];
    };

    auto live = [&]( const JSON& aStep, const char* aName ) -> FN&
    {
        std::optional<FN>& slot = slotOf( aStep, aName );

        if( !slot )
            Fail( "slot not constructed" );

        return *slot;
    };

    auto vacant = [&]( const JSON& aStep ) -> std::optional<FN>&
    {
        std::optional<FN>& slot = slotOf( aStep, "slot" );

        if( slot )
            Fail( "slot already constructed" );

        return slot;
    };

    for( const JSON& step : steps )
    {
        const std::string name = Text( step, "step" );

        if( name == "use_count" )
        {
            Fields( step, { "step" } );
            results.push_back( token.use_count() );
        }
        else if( name == "construct_empty" )
        {
            Fields( step, { "step", "slot" } );
            vacant( step ).emplace();
            results.push_back( nullptr );
        }
        else if( name == "construct" )
        {
            Fields( step, { "step", "slot", "callable" } );
            std::optional<FN>& slot = vacant( step );
            WithCallable<S, CAP>( step.at( "callable" ), token,
                                  [&]( auto&& aCallable ) { slot.emplace( std::move( aCallable ) ); } );
            results.push_back( nullptr );
        }
        else if( name == "copy_construct" )
        {
            Fields( step, { "step", "slot", "from" } );
            FN& from = live( step, "from" );
            vacant( step ).emplace( from );
            results.push_back( nullptr );
        }
        else if( name == "move_construct" )
        {
            Fields( step, { "step", "slot", "from" } );
            FN& from = live( step, "from" );
            vacant( step ).emplace( std::move( from ) );
            results.push_back( nullptr );
        }
        else if( name == "assign" )
        {
            Fields( step, { "step", "slot", "callable" } );
            FN& target = live( step, "slot" );
            WithCallable<S, CAP>( step.at( "callable" ), token,
                                  [&]( auto&& aCallable ) { target = std::move( aCallable ); } );
            results.push_back( nullptr );
        }
        else if( name == "copy_assign" )
        {
            Fields( step, { "step", "slot", "from" } );
            FN& target = live( step, "slot" );
            FN& from = live( step, "from" );
            target = static_cast<const FN&>( from );
            results.push_back( nullptr );
        }
        else if( name == "move_assign" )
        {
            Fields( step, { "step", "slot", "from" } );
            FN& target = live( step, "slot" );
            FN& from = live( step, "from" );
            target = std::move( from );
            results.push_back( nullptr );
        }
        else if( name == "destroy" )
        {
            Fields( step, { "step", "slot" } );
            live( step, "slot" );
            slotOf( step, "slot" ).reset();
            results.push_back( nullptr );
        }
        else if( name == "bool" )
        {
            Fields( step, { "step", "slot" } );
            results.push_back( bool( live( step, "slot" ) ) );
        }
        else if( name == "invoke" )
        {
            Fields( step, { "step", "slot", "args" } );
            const FN& function = live( step, "slot" );
            results.push_back( InvokeOne<S>( function, step.at( "args" ) ) );
        }
        else
        {
            Fail( "unknown step" );
        }
    }

    // The remaining slots are destroyed here, before the final count.
    for( std::optional<FN>& slot : slots )
        slot.reset();

    return { { "results", std::move( results ) }, { "final_use_count", token.use_count() } };
}


template <typename S>
JSON RunFunctionCapacity( const JSON& aInput )
{
    const JSON& capacity = aInput.at( "capacity" );

    if( capacity.is_string() && capacity.get<std::string>() == "default" )
        return RunFunction<S, 4 * sizeof( void* )>( aInput );

    if( capacity.is_number_unsigned() && capacity.get<uint64_t>() == 64 )
    {
        return RunFunction<S, 64>( aInput );
    }

    Fail( "unknown capacity" );
}


JSON Function( const JSON& aInput )
{
    Fields( aInput, { "op", "signature", "capacity", "token", "steps" } );
    const std::string signature = Text( aInput, "signature" );

    if( signature == "i64(i32)" )
        return RunFunctionCapacity<SIG_I64_I32>( aInput );
    if( signature == "i64(i32,i32)" )
        return RunFunctionCapacity<SIG_I64_I32_I32>( aInput );
    if( signature == "i64()" )
        return RunFunctionCapacity<SIG_I64>( aInput );
    if( signature == "string(string)" )
        return RunFunctionCapacity<SIG_STRING>( aInput );

    Fail( "unknown signature" );
}


// ---- inplace_function.h: the callable's special members -----------------------------------

/// The events PROBE records, and how many more copies may succeed.
struct PROBE_LOG
{
    std::vector<std::string> events;
    int64_t                  copiesLeft = 0;
};

/// The exception PROBE's copy constructor throws once the case's copies are used up.
struct PROBE_COPY_FAILURE
{
};

/// A callable that records its construction, copies (and failed copies), moves, calls and
/// destruction (of a live or a moved-from object).
class PROBE
{
public:
    PROBE( PROBE_LOG* aLog, int aId ) : m_log( aLog ), m_id( aId )
    {
        record( "construct " + std::to_string( m_id ) );
    }

    PROBE( const PROBE& aOther ) : m_log( aOther.m_log ), m_id( aOther.m_id )
    {
        if( m_log->copiesLeft == 0 )
        {
            record( "copy-failed " + std::to_string( m_id ) );
            throw PROBE_COPY_FAILURE();
        }

        --m_log->copiesLeft;
        record( "copy " + std::to_string( m_id ) );
    }

    PROBE( PROBE&& aOther ) noexcept : m_log( aOther.m_log ), m_id( aOther.m_id )
    {
        record( "move " + std::to_string( m_id ) );
        aOther.m_moved = true;
    }

    PROBE& operator=( const PROBE& ) = delete;
    PROBE& operator=( PROBE&& ) = delete;

    ~PROBE()
    {
        record( "destroy " + std::to_string( m_id ) + ( m_moved ? " moved-from" : " live" ) );
    }

    long long operator()( int aArg ) const
    {
        record( "call " + std::to_string( m_id ) );
        return (long long) m_id + aArg;
    }

private:
    void record( std::string aEvent ) const { m_log->events.push_back( std::move( aEvent ) ); }

    PROBE_LOG* m_log;
    int        m_id;
    bool       m_moved = false;
};


JSON Lifecycle( const JSON& aInput )
{
    using FN = INPLACE_FUNCTION<long long( int )>;

    Fields( aInput, { "op", "copies", "steps" } );
    PROBE_LOG log;
    log.copiesLeft = Signed( Member( aInput, "copies" ), "copies", 0, int64_t( MAX_STEPS ) );
    const JSON& steps = Array( aInput, "steps", MAX_STEPS );

    for( const JSON& step : steps )
    {
        if( !step.is_object() || !step.contains( "step" ) )
            Fail( "expected step object" );
    }

    std::array<std::optional<FN>, SLOTS> slots;
    JSON                                 results = JSON::array();

    auto slotOf = [&]( const JSON& aStep, const char* aName ) -> std::optional<FN>&
    {
        return slots[Signed( Member( aStep, aName ), aName, 0, SLOTS - 1 )];
    };

    auto live = [&]( const JSON& aStep, const char* aName ) -> FN&
    {
        std::optional<FN>& slot = slotOf( aStep, aName );

        if( !slot )
            Fail( "slot not constructed" );

        return *slot;
    };

    auto vacant = [&]( const JSON& aStep ) -> std::optional<FN>&
    {
        std::optional<FN>& slot = slotOf( aStep, "slot" );

        if( slot )
            Fail( "slot already constructed" );

        return slot;
    };

    auto readId = []( const JSON& aStep )
    {
        return int( Signed( Member( aStep, "id" ), "id", INT_MIN, INT_MAX ) );
    };

    for( const JSON& step : steps )
    {
        const std::string name = Text( step, "step" );
        JSON              outcome = nullptr;

        try
        {
            if( name == "copies" )
            {
                Fields( step, { "step", "count" } );
                log.copiesLeft = Signed( Member( step, "count" ), "count", 0, int64_t( MAX_STEPS ) );
            }
            else if( name == "construct_empty" )
            {
                Fields( step, { "step", "slot" } );
                vacant( step ).emplace();
            }
            else if( name == "construct" )
            {
                Fields( step, { "step", "slot", "id" } );
                std::optional<FN>& slot = vacant( step );
                const int          id = readId( step );
                slot.emplace( PROBE( &log, id ) );
            }
            else if( name == "construct_copy" )
            {
                Fields( step, { "step", "slot", "id" } );
                std::optional<FN>& slot = vacant( step );
                const int          id = readId( step );
                PROBE              probe( &log, id );
                slot.emplace( probe );
            }
            else if( name == "copy_construct" )
            {
                Fields( step, { "step", "slot", "from" } );
                FN& from = live( step, "from" );
                vacant( step ).emplace( static_cast<const FN&>( from ) );
            }
            else if( name == "move_construct" )
            {
                Fields( step, { "step", "slot", "from" } );
                FN& from = live( step, "from" );
                vacant( step ).emplace( std::move( from ) );
            }
            else if( name == "assign" )
            {
                Fields( step, { "step", "slot", "id" } );
                FN&       target = live( step, "slot" );
                const int id = readId( step );
                target = PROBE( &log, id );
            }
            else if( name == "assign_copy" )
            {
                Fields( step, { "step", "slot", "id" } );
                FN&       target = live( step, "slot" );
                const int id = readId( step );
                PROBE     probe( &log, id );
                target = probe;
            }
            else if( name == "copy_assign" )
            {
                Fields( step, { "step", "slot", "from" } );
                FN& target = live( step, "slot" );
                FN& from = live( step, "from" );
                target = static_cast<const FN&>( from );
            }
            else if( name == "move_assign" )
            {
                Fields( step, { "step", "slot", "from" } );
                FN& target = live( step, "slot" );
                FN& from = live( step, "from" );
                target = std::move( from );
            }
            else if( name == "destroy" )
            {
                Fields( step, { "step", "slot" } );
                live( step, "slot" );
                slotOf( step, "slot" ).reset();
            }
            else if( name == "bool" )
            {
                Fields( step, { "step", "slot" } );
                outcome = bool( live( step, "slot" ) );
            }
            else if( name == "invoke" )
            {
                Fields( step, { "step", "slot", "arg" } );
                const FN& function = live( step, "slot" );
                const int arg = int( Signed( Member( step, "arg" ), "arg", INT_MIN, INT_MAX ) );

                try
                {
                    outcome = { { "value", function( arg ) } };
                }
                catch( const std::bad_function_call& )
                {
                    outcome = { { "exception", "bad_function_call" } };
                }
            }
            else
            {
                Fail( "unknown step" );
            }
        }
        catch( const PROBE_COPY_FAILURE& )
        {
            outcome = { { "exception", "copy failed" } };
        }

        results.push_back( { { "outcome", outcome }, { "events", log.events } } );
        log.events.clear();
    }

    // The remaining slots are destroyed in slot order.
    for( std::optional<FN>& slot : slots )
        slot.reset();

    return { { "results", std::move( results ) }, { "final_events", log.events } };
}


// ---- typeinfo.h: IsA and dyn_cast over a fixture hierarchy --------------------------------

// A generic hierarchy with its own ClassOf predicates (not a KiCad production class): it
// instantiates the pinned IsA and dyn_cast templates and counts every ClassOf call.
int g_classOfCalls = 0;

enum class FIX_KIND
{
    BASE,
    DERIVED,
    LEAF,
    OTHER,
    NULLABLE,
    MULTI
};

struct FIX_BASE
{
    FIX_BASE( FIX_KIND aKind, int aValue ) : m_kind( aKind ), m_value( aValue ) {}
    virtual ~FIX_BASE() = default;

    static bool ClassOf( const FIX_BASE* aItem )
    {
        ++g_classOfCalls;
        return aItem != nullptr;
    }

    FIX_KIND m_kind;
    int      m_value;
};

struct FIX_DERIVED : FIX_BASE
{
    using FIX_BASE::FIX_BASE;

    static bool ClassOf( const FIX_BASE* aItem )
    {
        ++g_classOfCalls;
        return aItem && ( aItem->m_kind == FIX_KIND::DERIVED || aItem->m_kind == FIX_KIND::LEAF );
    }
};

struct FIX_LEAF : FIX_DERIVED
{
    using FIX_DERIVED::FIX_DERIVED;

    static bool ClassOf( const FIX_BASE* aItem )
    {
        ++g_classOfCalls;
        return aItem && aItem->m_kind == FIX_KIND::LEAF;
    }
};

struct FIX_OTHER : FIX_BASE
{
    using FIX_BASE::FIX_BASE;

    static bool ClassOf( const FIX_BASE* aItem )
    {
        ++g_classOfCalls;
        return aItem && aItem->m_kind == FIX_KIND::OTHER;
    }
};

/// Its ClassOf accepts a null pointer.
struct FIX_NULLABLE : FIX_BASE
{
    using FIX_BASE::FIX_BASE;

    static bool ClassOf( const FIX_BASE* aItem )
    {
        ++g_classOfCalls;
        return !aItem || aItem->m_kind == FIX_KIND::NULLABLE;
    }
};

/// A first polymorphic base, so FIX_BASE is not at offset 0 of FIX_MULTI.
struct FIX_SIDE
{
    virtual ~FIX_SIDE() = default;
    int m_tag = -1;
};

struct FIX_MULTI : FIX_SIDE, FIX_BASE
{
    explicit FIX_MULTI( int aValue ) : FIX_BASE( FIX_KIND::MULTI, aValue ) {}

    static bool ClassOf( const FIX_BASE* aItem )
    {
        ++g_classOfCalls;
        return aItem && aItem->m_kind == FIX_KIND::MULTI;
    }
};

std::unique_ptr<FIX_BASE> BuildFixture( const std::string& aClass, int aValue )
{
    if( aClass == "base" )
        return std::make_unique<FIX_BASE>( FIX_KIND::BASE, aValue );
    if( aClass == "derived" )
        return std::make_unique<FIX_DERIVED>( FIX_KIND::DERIVED, aValue );
    if( aClass == "leaf" )
        return std::make_unique<FIX_LEAF>( FIX_KIND::LEAF, aValue );
    if( aClass == "other" )
        return std::make_unique<FIX_OTHER>( FIX_KIND::OTHER, aValue );
    if( aClass == "nullable" )
        return std::make_unique<FIX_NULLABLE>( FIX_KIND::NULLABLE, aValue );
    if( aClass == "multi" )
        return std::make_unique<FIX_MULTI>( aValue );

    Fail( "unknown class" );
}


/// A cast's result: null, or the value read through it and whether it is the original
/// object's T part (dynamic_cast, independent of the templates).
template <typename T>
JSON CastResult( const T* aResult, const T* aExpected )
{
    if( !aResult )
        return nullptr;

    return { { "value", aResult->m_value }, { "same", aResult == aExpected } };
}


template <typename T>
JSON ThroughBase( const std::string& aForm, FIX_BASE* aObject, const std::optional<int>& aWrite )
{
    const FIX_BASE* constObject = aObject;

    if( aForm == "isa_ptr" )
        return IsA<T>( constObject );
    // The template argument may name the pointer type. The argument stays const: with a
    // non-const pointer, IsA( const I& ) (I deduced as the pointer) is the better match and
    // ClassOf( &aObject ) does not compile.
    if( aForm == "isa_ptr_type" )
        return IsA<T*>( constObject );
    if( aForm == "isa_ref" )
        return IsA<T>( *constObject );

    const T* expected = dynamic_cast<const T*>( constObject );

    if( aForm == "dyn_cast_const" )
        return CastResult<T>( dyn_cast<const T*>( constObject ), expected );

    T*   result = dyn_cast<T*>( aObject );
    JSON reply = CastResult<T>( result, expected );

    if( result && aWrite )
        result->m_value = *aWrite;

    return reply;
}


template <typename T>
JSON ThroughDerived( const std::string& aForm, FIX_DERIVED* aObject )
{
    const FIX_DERIVED* constObject = aObject;

    if( aForm == "derived_isa" )
        return IsA<T>( constObject );

    if constexpr( std::is_base_of_v<T, FIX_DERIVED> || std::is_base_of_v<FIX_DERIVED, T> )
        return CastResult<T>( dyn_cast<T*>( aObject ), dynamic_cast<const T*>( constObject ) );
    else
        Fail( "cast does not compile" );
}


JSON Isa( const JSON& aInput )
{
    static const std::set<std::string> forms = { "isa_ptr",        "isa_ptr_type", "isa_ref",
                                                 "dyn_cast_const", "dyn_cast",     "derived_isa",
                                                 "derived_dyn_cast" };
    static const std::set<std::string> targets = { "base",  "derived",  "leaf",
                                                   "other", "nullable", "multi" };

    Fields( aInput, { "op", "form", "target", "object", "write" } );
    const std::string form = Text( aInput, "form" );

    if( !forms.count( form ) )
        Fail( "unknown form" );

    const std::string target = Text( aInput, "target" );

    if( !targets.count( target ) )
        Fail( "unknown target" );

    const JSON&               spec = aInput.at( "object" );
    std::unique_ptr<FIX_BASE> object;

    if( !spec.is_null() )
    {
        Fields( spec, { "class", "value" } );
        const std::string cls = Text( spec, "class" );
        const int         value = int( Signed( spec.at( "value" ), "value", INT_MIN, INT_MAX ) );
        object = BuildFixture( cls, value );
    }

    std::optional<int> write;

    if( !aInput.at( "write" ).is_null() )
        write = int( Signed( aInput.at( "write" ), "write", INT_MIN, INT_MAX ) );

    if( write && form != "dyn_cast" )
        Fail( "only dyn_cast writes" );

    if( form == "isa_ref" && !object )
        Fail( "reference needs an object" );

    const bool   derived = form.rfind( "derived_", 0 ) == 0;
    FIX_DERIVED* derivedObject = dynamic_cast<FIX_DERIVED*>( object.get() );

    if( derived && object && !derivedObject )
        Fail( "object is not a FIX_DERIVED" );

    if( form == "derived_dyn_cast" && target != "base" && target != "derived" && target != "leaf" )
        Fail( "cast does not compile" );

    g_classOfCalls = 0;
    JSON result;

    if( derived )
    {
        if( target == "base" )
            result = ThroughDerived<FIX_BASE>( form, derivedObject );
        else if( target == "derived" )
            result = ThroughDerived<FIX_DERIVED>( form, derivedObject );
        else if( target == "leaf" )
            result = ThroughDerived<FIX_LEAF>( form, derivedObject );
        else if( target == "other" )
            result = ThroughDerived<FIX_OTHER>( form, derivedObject );
        else if( target == "nullable" )
            result = ThroughDerived<FIX_NULLABLE>( form, derivedObject );
        else
            result = ThroughDerived<FIX_MULTI>( form, derivedObject );
    }
    else
    {
        if( target == "base" )
            result = ThroughBase<FIX_BASE>( form, object.get(), write );
        else if( target == "derived" )
            result = ThroughBase<FIX_DERIVED>( form, object.get(), write );
        else if( target == "leaf" )
            result = ThroughBase<FIX_LEAF>( form, object.get(), write );
        else if( target == "other" )
            result = ThroughBase<FIX_OTHER>( form, object.get(), write );
        else if( target == "nullable" )
            result = ThroughBase<FIX_NULLABLE>( form, object.get(), write );
        else
            result = ThroughBase<FIX_MULTI>( form, object.get(), write );
    }

    return { { "result", std::move( result ) },
             { "class_of_calls", g_classOfCalls },
             { "object", object ? JSON( object->m_value ) : JSON() } };
}


JSON Run( const JSON& aInput )
{
    if( !aInput.is_object() || !aInput.contains( "op" ) )
        Fail( "unexpected or missing field" );

    const std::string op = Text( aInput, "op" );

    if( op == "typeinfo" )
        return TypeInfo( aInput );
    if( op == "enumerators" )
        return Enumerators( aInput );
    if( op == "mirror_val" || op == "mirror" || op == "mirror_vector" || op == "mirror_axis" )
        return Mirror( aInput, op );
    if( op == "throttle" )
        return Throttle( aInput );
    if( op == "leak_at_exit" )
        return Leak( aInput );
    if( op == "inplace_function" )
        return Function( aInput );
    if( op == "inplace_lifecycle" )
        return Lifecycle( aInput );
    if( op == "isa" )
        return Isa( aInput );

    Fail( "unknown operation" );
}
} // namespace


namespace oracle
{
void RegisterCoreAppIncludeP001( REGISTRY& aRegistry )
{
    aRegistry.Add( "include.core.application.pure.p001", "include/core/*.h", Run );
}
} // namespace oracle
