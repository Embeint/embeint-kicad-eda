// SPDX-License-Identifier: GPL-3.0-or-later
// Request evaluation for the thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002
// oracle adapter (clipper.h, clipper.minkowski.h and the clipper.offset.h/clipper.rectclip.h
// declarations). It is compiled twice with identical text:
//
// - in shims/clipper2_include_p002.cpp against the genuine headers and the pinned clipper2
//   archive (namespace Clipper2Lib), which produces every reply;
// - in reference/clipper2_include_p002_guard.cpp against a UBSan/_GLIBCXX_ASSERTIONS
//   compilation of the same pinned sources (namespace Clipper2Lib renamed by macro), run in a
//   forked child first to decide whether the reference evaluation is undefined. That file also
//   defines INCLUDE_P002_REPLAY, which adds explicit std::vector<bool> bounds traces the
//   library's assertions do not provide on GCC 13.3 (RunDetail); they never change a reply.
//
// The including file defines INCLUDE_P002_NS and has included registry.h and the Clipper2
// headers. Every request validates its whole input and applies the domain and resource guards
// (identical in rust/kicad-clipper2-include-p002-shim) before any reference call. Doubles
// travel as their 16 hex digit bit patterns; points are [x, y, z].
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <limits>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

#ifndef USINGZ
#error "KiCad compiles Clipper2 with the public USINGZ definition"
#endif

namespace INCLUDE_P002_NS
{
namespace
{
using oracle::JSON;
namespace c2 = Clipper2Lib;

// Request limits, identical in the Rust shim.
constexpr size_t  MAX_POINTS = size_t( 1 ) << 16;
constexpr size_t  MAX_NODES = 4096;
constexpr size_t  MAX_VALUES = 4096;
constexpr size_t  MAX_SIMPLIFY = 4096;
constexpr size_t  MAX_RDP = 1024;
// RDP calls per path for a NaN epsilon (rdp_calls in the Rust shim): 4 * MAX_RDP + 16.
constexpr size_t  MAX_RDP_CALLS = MAX_RDP * 4 + 16;
constexpr size_t  MAX_QUADS = size_t( 1 ) << 14;
constexpr size_t  MAX_FLAGS = 4096;
constexpr size_t  MAX_GROUPS = 16;
constexpr size_t  MAX_SETS = 16;
constexpr size_t  MAX_PREAMBLE = 256;
constexpr int64_t STREAM_MIN = -64;
constexpr int64_t STREAM_MAX = 512;
// Clipper2's declared coordinate range (INT64_MAX >> 2), the engine domain.
constexpr int64_t ENGINE_COORD = std::numeric_limits<int64_t>::max() >> 2;
// Minkowski inputs: half the engine domain, so every sum or difference stays inside it.
constexpr int64_t MINKOWSKI_COORD = ENGINE_COORD / 2;
constexpr double  MINKOWSKI_COORD_D = static_cast<double>( MINKOWSKI_COORD );
constexpr double  TWO_POW_63 = 9223372036854775808.0;
// The offset domain (as the src p002 adapter): coordinates, deltas, miter limits and arc
// tolerances within 2^40.
constexpr double  OFFSET_LIMIT = 1099511627776.0;
constexpr int64_t OFFSET_COORD = int64_t( 1 ) << 40;
constexpr double  MAX_ARC_STEPS = 65536.0;
constexpr double  MAX_OFFSET_POINTS = 4194304.0; // 2^22
constexpr double  MAX_ELLIPSE_STEPS = 1048576.0; // 2^20
constexpr double  FP_TOLERANCE = 1e-12;          // clipper.offset.cpp floating_point_tolerance
constexpr double  ARC_CONST = 0.002;             // clipper.offset.cpp arc_const

[[noreturn]] void Invalid( const std::string& aWhat )
{
    throw std::invalid_argument( "invalid input: " + aWhat );
}


void Keys( const JSON& aValue, std::initializer_list<const char*> aFields )
{
    if( !aValue.is_object() || aValue.size() != aFields.size() )
        Invalid( "fields" );

    for( const char* field : aFields )
    {
        if( !aValue.contains( field ) )
            Invalid( "fields" );
    }
}


const JSON& At( const JSON& aValue, const char* aName )
{
    if( !aValue.is_object() || !aValue.contains( aName ) )
        Invalid( aName );

    return aValue.at( aName );
}


const std::string& Text( const JSON& aValue )
{
    if( !aValue.is_string() )
        Invalid( "string" );

    return aValue.get_ref<const std::string&>();
}


bool Bool( const JSON& aValue )
{
    if( !aValue.is_boolean() )
        Invalid( "boolean" );

    return aValue.get<bool>();
}


int64_t I64( const JSON& aValue )
{
    if( aValue.is_number_unsigned() )
    {
        if( aValue.get<uint64_t>() > uint64_t( std::numeric_limits<int64_t>::max() ) )
            Invalid( "int64" );

        return int64_t( aValue.get<uint64_t>() );
    }

    if( !aValue.is_number_integer() )
        Invalid( "int64" );

    return aValue.get<int64_t>();
}


uint64_t U64( const JSON& aValue )
{
    if( aValue.is_number_unsigned() )
        return aValue.get<uint64_t>();

    if( !aValue.is_number_integer() || aValue.get<int64_t>() < 0 )
        Invalid( "uint64" );

    return uint64_t( aValue.get<int64_t>() );
}


int Int( const JSON& aValue )
{
    const int64_t v = I64( aValue );

    if( v < std::numeric_limits<int>::min() || v > std::numeric_limits<int>::max() )
        Invalid( "int" );

    return int( v );
}


int Enum( const JSON& aValue, int aCount )
{
    const int64_t v = I64( aValue );

    if( v < 0 || v >= aCount )
        Invalid( "enum" );

    return int( v );
}


double F64( const JSON& aValue )
{
    if( !aValue.is_string() )
        Invalid( "double" );

    const std::string& s = aValue.get_ref<const std::string&>();

    if( s.size() != 16 )
        Invalid( "double" );

    uint64_t bits = 0;

    for( char c : s )
    {
        uint64_t digit;

        if( c >= '0' && c <= '9' )
            digit = uint64_t( c - '0' );
        else if( c >= 'a' && c <= 'f' )
            digit = uint64_t( c - 'a' + 10 );
        else
            Invalid( "double" );

        bits = ( bits << 4 ) | digit;
    }

    double value;
    std::memcpy( &value, &bits, sizeof value );
    return value;
}


JSON Bits( double aValue )
{
    uint64_t bits;
    std::memcpy( &bits, &aValue, sizeof bits );
    char text[17];
    std::snprintf( text, sizeof text, "%016llx", static_cast<unsigned long long>( bits ) );
    return std::string( text );
}


/// Points read so far in one request.
struct COUNTER
{
    size_t n = 0;

    void Add( size_t aCount )
    {
        n += aCount;

        if( n > MAX_POINTS )
            Invalid( "size" );
    }
};


const JSON& Array( const JSON& aValue, const char* aWhat )
{
    if( !aValue.is_array() )
        Invalid( aWhat );

    return aValue;
}


/// "i64" (int64_t coordinates) or "f64" (double coordinates).
bool IsDouble( const JSON& aIn )
{
    const std::string& type = Text( At( aIn, "type" ) );

    if( type == "i64" )
        return false;

    if( type == "f64" )
        return true;

    Invalid( "type" );
}


/// PolyPath64 (= PolyTree64) or PolyPathD (= PolyTreeD).
template <typename T>
struct TREE
{
    using type = c2::PolyPath64;
};

template <>
struct TREE<double>
{
    using type = c2::PolyPathD;
};


template <typename T>
T Coord( const JSON& aValue )
{
    if constexpr( std::is_same_v<T, double> )
        return F64( aValue );
    else
        return I64( aValue );
}


template <typename T>
JSON WriteCoord( T aValue )
{
    if constexpr( std::is_same_v<T, double> )
        return Bits( aValue );
    else
        return aValue;
}


template <typename T>
c2::Point<T> ReadPoint( const JSON& aValue, COUNTER& aCount )
{
    if( !aValue.is_array() || aValue.size() != 3 )
        Invalid( "point" );

    aCount.Add( 1 );
    c2::Point<T> p;
    p.x = Coord<T>( aValue[0] );
    p.y = Coord<T>( aValue[1] );
    p.z = I64( aValue[2] );
    return p;
}


template <typename T>
c2::Path<T> ReadPath( const JSON& aValue, COUNTER& aCount )
{
    c2::Path<T> out;

    for( const JSON& p : Array( aValue, "path" ) )
        out.push_back( ReadPoint<T>( p, aCount ) );

    return out;
}


template <typename T>
c2::Paths<T> ReadPaths( const JSON& aValue, COUNTER& aCount )
{
    c2::Paths<T> out;

    for( const JSON& p : Array( aValue, "paths" ) )
        out.push_back( ReadPath<T>( p, aCount ) );

    return out;
}


template <typename T>
c2::Rect<T> ReadRect( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 4 )
        Invalid( "rect" );

    return c2::Rect<T>( Coord<T>( aValue[0] ), Coord<T>( aValue[1] ), Coord<T>( aValue[2] ),
                        Coord<T>( aValue[3] ) );
}


template <typename T>
JSON WritePoint( const c2::Point<T>& aPoint )
{
    return JSON::array( { WriteCoord<T>( aPoint.x ), WriteCoord<T>( aPoint.y ), aPoint.z } );
}


template <typename T>
JSON WritePath( const c2::Path<T>& aPath )
{
    JSON out = JSON::array();

    for( const auto& pt : aPath )
        out.push_back( WritePoint<T>( pt ) );

    return out;
}


template <typename T>
JSON WritePaths( const c2::Paths<T>& aPaths )
{
    JSON out = JSON::array();

    for( const auto& path : aPaths )
        out.push_back( WritePath<T>( path ) );

    return out;
}


/// A tree in preorder: each node's parent index (-1 for the given node's children), hole flag
/// and polygon.
template <typename T, typename NODE>
void WriteTree( const NODE& aNode, int64_t aParent, JSON& aOut )
{
    for( size_t i = 0; i < aNode.Count(); ++i )
    {
        const auto*   child = aNode.Child( i );
        const int64_t index = int64_t( aOut.size() );
        aOut.push_back( JSON{ { "parent", aParent },
                              { "hole", child->IsHole() },
                              { "polygon", WritePath<T>( child->Polygon() ) } } );
        WriteTree<T>( *child, index, aOut );
    }
}


template <typename T, typename NODE>
JSON Tree( const NODE& aNode )
{
    JSON out = JSON::array();
    WriteTree<T>( aNode, -1, out );
    return out;
}


void EngineDomain( const c2::Paths64& aPaths )
{
    for( const auto& path : aPaths )
    {
        for( const auto& p : path )
        {
            if( p.x < -ENGINE_COORD || p.x > ENGINE_COORD || p.y < -ENGINE_COORD
                || p.y > ENGINE_COORD )
                Invalid( "domain" );
        }
    }
}


/// An optional int precision argument (null selects the default argument).
bool OptionalInt( const JSON& aValue, int& aOut )
{
    if( aValue.is_null() )
        return false;

    aOut = Int( aValue );
    return true;
}


/// `std::pow(10, precision)`, as the wrappers compute it.
double Pow10( int aPrecision )
{
    return std::pow( 10, aPrecision );
}


bool PrecisionInRange( int aPrecision )
{
    return aPrecision >= -c2::CLIPPER2_MAX_DEC_PRECISION
           && aPrecision <= c2::CLIPPER2_MAX_DEC_PRECISION;
}


/// Whether `ScalePaths<int64_t, double>(aPaths, aScale, ...)` reports `range_error_i`: its
/// own bounds test, on the bounds it computes (`GetBounds<double, double>`), which only
/// compares and so is defined for every input.
bool ScaleRangeError( const c2::PathsD& aPaths, double aScale )
{
    const c2::RectD r = c2::GetBounds<double, double>( aPaths );
    return ( r.left * aScale ) < c2::min_coord || ( r.right * aScale ) > c2::max_coord
           || ( r.top * aScale ) < c2::min_coord || ( r.bottom * aScale ) > c2::max_coord;
}


template <typename F>
JSON Guarded( F aCall, const char* aField )
{
    try
    {
        return JSON{ { aField, aCall() }, { "exception", nullptr } };
    }
    catch( const c2::Clipper2Exception& e )
    {
        return JSON{ { aField, nullptr }, { "exception", e.what() } };
    }
}

// --- boolean operations --------------------------------------------------------------------

template <typename T>
JSON RunBoolean( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "fn", "clip_type", "fill_rule", "subjects", "clips", "precision",
                 "prefill" } );
    constexpr bool     D = std::is_same_v<T, double>;
    const std::string& fn = Text( At( aIn, "fn" ) );
    const bool         op = fn == "booleanop" || fn == "booleanop_tree";
    const bool         tree = fn == "booleanop_tree";
    const bool         single = fn == "union_subjects";

    if( !op && !single && fn != "intersect" && fn != "union" && fn != "difference" && fn != "xor" )
        Invalid( "fn" );

    COUNTER            count;
    const c2::Paths<T> subjects = ReadPaths<T>( At( aIn, "subjects" ), count );
    c2::Paths<T>       clips;

    if( single )
    {
        if( !At( aIn, "clips" ).is_null() )
            Invalid( "clips" );
    }
    else
    {
        clips = ReadPaths<T>( At( aIn, "clips" ), count );
    }

    c2::ClipType ct = c2::ClipType::NoClip;

    if( op )
        ct = static_cast<c2::ClipType>( Enum( At( aIn, "clip_type" ), 5 ) );
    else if( !At( aIn, "clip_type" ).is_null() )
        Invalid( "clip_type" );

    const c2::FillRule fr = static_cast<c2::FillRule>( Enum( At( aIn, "fill_rule" ), 4 ) );
    int                precision = 2;
    const bool         hasPrecision = OptionalInt( At( aIn, "precision" ), precision );

    if( !D && hasPrecision )
        Invalid( "precision" );

    c2::Paths<T> prefill;

    if( tree )
        prefill = ReadPaths<T>( At( aIn, "prefill" ), count );
    else if( !At( aIn, "prefill" ).is_null() )
        Invalid( "prefill" );

    if constexpr( !D )
    {
        EngineDomain( subjects );
        EngineDomain( clips );
    }

    if( tree )
    {
        typename TREE<T>::type polytree;

        for( const auto& p : prefill )
            polytree.AddChild( p );

        JSON exception = nullptr;

        try
        {
            if constexpr( D )
            {
                if( hasPrecision )
                    c2::BooleanOp( ct, fr, subjects, clips, polytree, precision );
                else
                    c2::BooleanOp( ct, fr, subjects, clips, polytree );
            }
            else
            {
                c2::BooleanOp( ct, fr, subjects, clips, polytree );
            }
        }
        catch( const c2::Clipper2Exception& e )
        {
            exception = e.what();
        }

        return JSON{ { "tree", Tree<T>( polytree ) }, { "exception", exception } };
    }

    return Guarded(
            [&]() -> JSON
            {
                c2::Paths<T> r;

                if constexpr( D )
                {
                    if( fn == "booleanop" )
                        r = hasPrecision ? c2::BooleanOp( ct, fr, subjects, clips, precision )
                                         : c2::BooleanOp( ct, fr, subjects, clips );
                    else if( fn == "intersect" )
                        r = hasPrecision ? c2::Intersect( subjects, clips, fr, precision )
                                         : c2::Intersect( subjects, clips, fr );
                    else if( fn == "union" )
                        r = hasPrecision ? c2::Union( subjects, clips, fr, precision )
                                         : c2::Union( subjects, clips, fr );
                    else if( fn == "difference" )
                        r = hasPrecision ? c2::Difference( subjects, clips, fr, precision )
                                         : c2::Difference( subjects, clips, fr );
                    else if( fn == "xor" )
                        r = hasPrecision ? c2::Xor( subjects, clips, fr, precision )
                                         : c2::Xor( subjects, clips, fr );
                    else
                        r = hasPrecision ? c2::Union( subjects, fr, precision )
                                         : c2::Union( subjects, fr );
                }
                else
                {
                    if( fn == "booleanop" )
                        r = c2::BooleanOp( ct, fr, subjects, clips );
                    else if( fn == "intersect" )
                        r = c2::Intersect( subjects, clips, fr );
                    else if( fn == "union" )
                        r = c2::Union( subjects, clips, fr );
                    else if( fn == "difference" )
                        r = c2::Difference( subjects, clips, fr );
                    else if( fn == "xor" )
                        r = c2::Xor( subjects, clips, fr );
                    else
                        r = c2::Union( subjects, fr );
                }

                return WritePaths<T>( r );
            },
            "paths" );
}

// --- offsetting ----------------------------------------------------------------------------

double StdMin( double a, double b )
{
    return ( b < a ) ? b : a;
}


/// The arc steps per full circle `DoGroupOffset` computes for `abs_delta`.
double StepsPer360( double aAbsDelta, double aArcTolerance )
{
    const double arcTol = ( aArcTolerance > FP_TOLERANCE ) ? StdMin( aAbsDelta, aArcTolerance )
                                                           : aAbsDelta * ARC_CONST;
    return StdMin( c2::PI / std::acos( 1 - arcTol / aAbsDelta ), aAbsDelta * c2::PI );
}


bool OffsetFinite( double aValue )
{
    return std::isfinite( aValue ) && std::fabs( aValue ) <= OFFSET_LIMIT;
}


void OffsetDomain( const c2::Paths64& aPaths )
{
    for( const auto& path : aPaths )
    {
        for( const auto& p : path )
        {
            if( p.x < -OFFSET_COORD || p.x > OFFSET_COORD || p.y < -OFFSET_COORD
                || p.y > OFFSET_COORD )
                Invalid( "domain" );
        }
    }
}


/// The offset guards on the values the offset object receives: delta, miter limit and arc
/// tolerance within the offset domain, and at most 65,536 arc steps per circle and 2^22 points
/// for round joins or ends.
void OffsetGuard( double aDelta, double aMiterLimit, double aArcTolerance, bool aRound,
                  size_t aPoints )
{
    if( !OffsetFinite( aDelta ) || !OffsetFinite( aMiterLimit ) || !OffsetFinite( aArcTolerance ) )
        Invalid( "domain" );

    const double absDelta = std::fabs( aDelta );

    if( aRound && absDelta > FP_TOLERANCE )
    {
        const double steps = StepsPer360( absDelta, aArcTolerance );

        if( !( steps <= MAX_ARC_STEPS ) || double( aPoints ) * ( steps + 16.0 ) > MAX_OFFSET_POINTS )
            Invalid( "arc steps" );
    }
}


c2::JoinType ReadJoin( const JSON& aValue )
{
    return static_cast<c2::JoinType>( Enum( aValue, 4 ) );
}


c2::EndType ReadEnd( const JSON& aValue )
{
    return static_cast<c2::EndType>( Enum( aValue, 5 ) );
}


template <typename T>
JSON RunInflate( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "paths", "delta", "join", "end", "miter_limit", "precision",
                 "arc_tolerance" } );
    constexpr bool     D = std::is_same_v<T, double>;
    COUNTER            count;
    const c2::Paths<T> paths = ReadPaths<T>( At( aIn, "paths" ), count );
    const double       delta = F64( At( aIn, "delta" ) );
    const c2::JoinType jt = ReadJoin( At( aIn, "join" ) );
    const c2::EndType  et = ReadEnd( At( aIn, "end" ) );
    const JSON&        mlIn = At( aIn, "miter_limit" );
    const JSON&        precIn = At( aIn, "precision" );
    const JSON&        atIn = At( aIn, "arc_tolerance" );
    // Default arguments are a suffix: a given argument requires every earlier one.
    const bool         hasMl = !mlIn.is_null();
    const bool         hasPrec = !precIn.is_null();
    const bool         hasAt = !atIn.is_null();
    const double       ml = hasMl ? F64( mlIn ) : 2.0;
    const double       at = hasAt ? F64( atIn ) : 0.0;
    int                precision = 2;

    if( hasPrec )
        precision = Int( precIn );

    if constexpr( D )
    {
        if( ( hasPrec && !hasMl ) || ( hasAt && !hasPrec ) )
            Invalid( "defaults" );
    }
    else
    {
        if( hasPrec || ( hasAt && !hasMl ) )
            Invalid( "defaults" );
    }

    const bool round = jt == c2::JoinType::Round || et == c2::EndType::Round;

    if constexpr( D )
    {
        // ScalePaths<int64_t, double> throws "Values exceed permitted range" when the scaled
        // bounds leave min_coord..max_coord, after the constructor has only stored its
        // arguments and before any conversion or offset work: that defined error needs no
        // offset bound, so the guard applies only when the scaling succeeds.
        if( PrecisionInRange( precision ) && delta != 0.0
            && !ScaleRangeError( paths, Pow10( precision ) ) )
        {
            const double scale = Pow10( precision );

            for( const auto& path : paths )
            {
                for( const auto& p : path )
                {
                    if( !OffsetFinite( p.x * scale ) || !OffsetFinite( p.y * scale ) )
                        Invalid( "domain" );
                }
            }

            OffsetGuard( delta * scale, ml, at * scale, round, count.n );
        }
    }
    else
    {
        if( delta != 0.0 )
        {
            OffsetDomain( paths );
            OffsetGuard( delta, ml, at, round, count.n );
        }
    }

    return Guarded(
            [&]() -> JSON
            {
                if constexpr( D )
                {
                    if( hasAt )
                        return WritePaths<T>( c2::InflatePaths( paths, delta, jt, et, ml, precision, at ) );
                    if( hasPrec )
                        return WritePaths<T>( c2::InflatePaths( paths, delta, jt, et, ml, precision ) );
                    if( hasMl )
                        return WritePaths<T>( c2::InflatePaths( paths, delta, jt, et, ml ) );
                    return WritePaths<T>( c2::InflatePaths( paths, delta, jt, et ) );
                }
                else
                {
                    if( hasAt )
                        return WritePaths<T>( c2::InflatePaths( paths, delta, jt, et, ml, at ) );
                    if( hasMl )
                        return WritePaths<T>( c2::InflatePaths( paths, delta, jt, et, ml ) );
                    return WritePaths<T>( c2::InflatePaths( paths, delta, jt, et ) );
                }
            },
            "paths" );
}


/// The `ClipperOffset` members written in clipper.offset.h: the constructor's default
/// arguments, the accessors, `Clear` and `ErrorCode`, observed through an execution.
JSON RunOffsetMembers( const JSON& aIn )
{
    Keys( aIn, { "op", "ctor", "set", "groups", "clear", "delta" } );
    const JSON& ctor = Array( At( aIn, "ctor" ), "ctor" );

    if( ctor.size() > 4 )
        Invalid( "ctor" );

    double ml = 2.0, at = 0.0;
    bool   pc = false, rs = false;

    if( ctor.size() > 0 )
        ml = F64( ctor[0] );
    if( ctor.size() > 1 )
        at = F64( ctor[1] );
    if( ctor.size() > 2 )
        pc = Bool( ctor[2] );
    if( ctor.size() > 3 )
        rs = Bool( ctor[3] );

    struct SET
    {
        int    which;
        double value;
        bool   flag;
    };

    std::vector<SET> sets;
    const JSON&      setIn = Array( At( aIn, "set" ), "set" );

    if( setIn.size() > MAX_SETS )
        Invalid( "set" );

    double effMl = ml, effAt = at;

    for( const JSON& s : setIn )
    {
        if( !s.is_array() || s.size() != 2 )
            Invalid( "set" );

        const std::string& name = Text( s[0] );
        SET                set{ 0, 0.0, false };

        if( name == "miter_limit" )
        {
            set = { 0, F64( s[1] ), false };
            effMl = set.value;
        }
        else if( name == "arc_tolerance" )
        {
            set = { 1, F64( s[1] ), false };
            effAt = set.value;
        }
        else if( name == "preserve_collinear" )
            set = { 2, 0.0, Bool( s[1] ) };
        else if( name == "reverse_solution" )
            set = { 3, 0.0, Bool( s[1] ) };
        else
            Invalid( "set" );

        sets.push_back( set );
    }

    struct GROUP
    {
        c2::Paths64  paths;
        c2::JoinType join;
        c2::EndType  end;
        bool         single;
    };

    COUNTER            count;
    std::vector<GROUP> groups;
    const JSON&        groupsIn = Array( At( aIn, "groups" ), "groups" );

    if( groupsIn.size() > MAX_GROUPS )
        Invalid( "size" );

    bool round = false;

    for( const JSON& g : groupsIn )
    {
        Keys( g, { "paths", "join", "end", "single" } );
        GROUP group{ ReadPaths<int64_t>( At( g, "paths" ), count ), ReadJoin( At( g, "join" ) ),
                     ReadEnd( At( g, "end" ) ), Bool( At( g, "single" ) ) };

        if( group.single && group.paths.size() != 1 )
            Invalid( "single" );

        round = round || group.join == c2::JoinType::Round || group.end == c2::EndType::Round;
        groups.push_back( std::move( group ) );
    }

    const bool  clear = Bool( At( aIn, "clear" ) );
    const JSON& deltaIn = At( aIn, "delta" );
    double      delta = 0.0;

    if( !deltaIn.is_null() )
    {
        delta = F64( deltaIn );

        for( const GROUP& g : groups )
            OffsetDomain( g.paths );

        OffsetGuard( delta, effMl, effAt, round, count.n );
    }

    c2::ClipperOffset* co = nullptr;
    c2::ClipperOffset  co0;
    c2::ClipperOffset  co1( ml );
    c2::ClipperOffset  co2( ml, at );
    c2::ClipperOffset  co3( ml, at, pc );
    c2::ClipperOffset  co4( ml, at, pc, rs );
    c2::ClipperOffset* all[] = { &co0, &co1, &co2, &co3, &co4 };
    co = all[ctor.size()];

    for( const SET& s : sets )
    {
        switch( s.which )
        {
        case 0: co->MiterLimit( s.value ); break;
        case 1: co->ArcTolerance( s.value ); break;
        case 2: co->PreserveCollinear( s.flag ); break;
        default: co->ReverseSolution( s.flag ); break;
        }
    }

    for( const GROUP& g : groups )
    {
        if( g.single )
            co->AddPath( g.paths[0], g.join, g.end );
        else
            co->AddPaths( g.paths, g.join, g.end );
    }

    if( clear )
        co->Clear();

    JSON solution = nullptr;

    if( !deltaIn.is_null() )
    {
        c2::Paths64 out;
        co->Execute( delta, out );
        solution = WritePaths<int64_t>( out );
    }

    return JSON{ { "miter_limit", Bits( co->MiterLimit() ) },
                 { "arc_tolerance", Bits( co->ArcTolerance() ) },
                 { "preserve_collinear", co->PreserveCollinear() },
                 { "reverse_solution", co->ReverseSolution() },
                 { "error_code", co->ErrorCode() },
                 { "paths", solution } };
}

// --- translation and rectangle clipping ----------------------------------------------------

/// "path" (one path) or "paths".
bool SinglePath( const JSON& aIn )
{
    const std::string& form = Text( At( aIn, "form" ) );

    if( form == "path" )
        return true;

    if( form == "paths" )
        return false;

    Invalid( "form" );
}


template <typename T>
JSON RunTranslate( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "form", "value", "dx", "dy" } );
    COUNTER    count;
    const bool single = SinglePath( aIn );
    const T    dx = Coord<T>( At( aIn, "dx" ) );
    const T    dy = Coord<T>( At( aIn, "dy" ) );

    if( single )
        return WritePath<T>( c2::TranslatePath( ReadPath<T>( At( aIn, "value" ), count ), dx, dy ) );

    return WritePaths<T>( c2::TranslatePaths( ReadPaths<T>( At( aIn, "value" ), count ), dx, dy ) );
}


template <typename T>
JSON RunRectClip( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "lines", "form", "rect", "value", "precision" } );
    constexpr bool    D = std::is_same_v<T, double>;
    COUNTER           count;
    const bool        lines = Bool( At( aIn, "lines" ) );
    const bool        single = SinglePath( aIn );
    const c2::Rect<T> rect = ReadRect<T>( At( aIn, "rect" ) );
    c2::Path<T>       path;
    c2::Paths<T>      paths;

    if( single )
        path = ReadPath<T>( At( aIn, "value" ), count );
    else
        paths = ReadPaths<T>( At( aIn, "value" ), count );

    int        precision = 2;
    const bool hasPrecision = OptionalInt( At( aIn, "precision" ), precision );

    if( !D && hasPrecision )
        Invalid( "precision" );

    return Guarded(
            [&]() -> JSON
            {
                if constexpr( D )
                {
                    if( lines )
                    {
                        if( single )
                            return WritePaths<T>( hasPrecision ? c2::RectClipLines( rect, path, precision )
                                                               : c2::RectClipLines( rect, path ) );
                        return WritePaths<T>( hasPrecision ? c2::RectClipLines( rect, paths, precision )
                                                           : c2::RectClipLines( rect, paths ) );
                    }

                    if( single )
                        return WritePaths<T>( hasPrecision ? c2::RectClip( rect, path, precision )
                                                           : c2::RectClip( rect, path ) );
                    return WritePaths<T>( hasPrecision ? c2::RectClip( rect, paths, precision )
                                                       : c2::RectClip( rect, paths ) );
                }
                else
                {
                    if( lines )
                        return WritePaths<T>( single ? c2::RectClipLines( rect, path )
                                                     : c2::RectClipLines( rect, paths ) );

                    return WritePaths<T>( single ? c2::RectClip( rect, path )
                                                 : c2::RectClip( rect, paths ) );
                }
            },
            "paths" );
}

// --- polytrees and streams -----------------------------------------------------------------

void ApplyState( std::ostringstream& aStream, const JSON& aState )
{
    static const std::vector<std::pair<const char*, std::ios_base::fmtflags>> names = {
        { "boolalpha", std::ios_base::boolalpha }, { "dec", std::ios_base::dec },
        { "fixed", std::ios_base::fixed },         { "hex", std::ios_base::hex },
        { "internal", std::ios_base::internal },   { "left", std::ios_base::left },
        { "oct", std::ios_base::oct },             { "right", std::ios_base::right },
        { "scientific", std::ios_base::scientific }, { "showbase", std::ios_base::showbase },
        { "showpoint", std::ios_base::showpoint }, { "showpos", std::ios_base::showpos },
        { "skipws", std::ios_base::skipws },       { "unitbuf", std::ios_base::unitbuf },
        { "uppercase", std::ios_base::uppercase },
    };
    Keys( aState, { "flags", "precision", "width", "fill" } );
    const JSON& list = Array( aState.at( "flags" ), "state" );

    std::ios_base::fmtflags  flags{};
    std::vector<std::string> seen;

    for( const JSON& item : list )
    {
        const std::string& name = Text( item );
        const auto found = std::find_if( names.begin(), names.end(),
                                         [&]( const auto& aEntry ) { return name == aEntry.first; } );

        if( found == names.end() || std::find( seen.begin(), seen.end(), name ) != seen.end() )
            Invalid( "state" );

        seen.push_back( name );
        flags |= found->second;
    }

    auto bounded = [&]( const char* aName )
    {
        const int64_t value = I64( aState.at( aName ) );

        if( value < STREAM_MIN || value > STREAM_MAX )
            Invalid( "state" );

        return value;
    };
    const int64_t      precision = bounded( "precision" );
    const int64_t      width = bounded( "width" );
    const std::string& fill = Text( aState.at( "fill" ) );

    if( fill.size() != 1 || fill[0] < 0x20 || fill[0] > 0x7e )
        Invalid( "state" );

    aStream.flags( flags );
    aStream.precision( static_cast<std::streamsize>( precision ) );
    aStream.width( static_cast<std::streamsize>( width ) );
    aStream.fill( fill[0] );
}


JSON StreamReply( const std::ostringstream& aStream )
{
    return JSON{ { "text", aStream.str() },
                 { "width_after", static_cast<int64_t>( aStream.width() ) } };
}


template <typename T>
JSON RunPolyTree( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "nodes", "fn", "node", "state" } );
    constexpr bool     D = std::is_same_v<T, double>;
    COUNTER            count;
    const JSON&        nodesIn = Array( At( aIn, "nodes" ), "nodes" );
    const std::string& fn = Text( At( aIn, "fn" ) );

    if( nodesIn.size() > MAX_NODES )
        Invalid( "size" );

    using NODE = typename TREE<T>::type;
    NODE               root;
    std::vector<NODE*> nodes;

    for( const JSON& n : nodesIn )
    {
        Keys( n, { "parent", "polygon" } );
        const int64_t     parent = I64( At( n, "parent" ) );
        const c2::Path<T> polygon = ReadPath<T>( At( n, "polygon" ), count );

        if( parent < -1 || parent >= int64_t( nodes.size() ) )
            Invalid( "parent" );

        NODE* owner = parent < 0 ? &root : nodes[size_t( parent )];
        nodes.push_back( owner->AddChild( polygon ) );
    }

    const int64_t node = I64( At( aIn, "node" ) );

    if( node < -1 || node >= int64_t( nodes.size() ) )
        Invalid( "node" );

    const NODE& pp = node < 0 ? root : *nodes[size_t( node )];
    const JSON&            state = At( aIn, "state" );

    if( fn != "stream" && !state.is_null() )
        Invalid( "state" );

    if( fn == "to_paths" )
    {
        if constexpr( D )
            return WritePaths<T>( c2::PolyTreeToPathsD( pp ) );
        else
            return WritePaths<T>( c2::PolyTreeToPaths64( pp ) );
    }

    if( fn == "path_to_paths" )
    {
        c2::Paths<T> out;

        if constexpr( D )
            c2::details::PolyPathToPathsD( pp, out );
        else
            c2::details::PolyPathToPaths64( pp, out );

        return WritePaths<T>( out );
    }

    if( fn == "contains_children" || fn == "node_contains" )
    {
        if constexpr( D )
            Invalid( "fn" );
        else
            return fn == "contains_children" ? c2::CheckPolytreeFullyContainsChildren( pp )
                                             : c2::details::PolyPath64ContainsChildren( pp );
    }

    if( fn == "stream" )
    {
        std::ostringstream stream;

        if( !state.is_null() )
            ApplyState( stream, state );

        stream << pp;
        return StreamReply( stream );
    }

    Invalid( "fn" );
}


JSON RunOutline( const JSON& aIn )
{
    Keys( aIn, { "op", "idx", "is_hole", "count", "preamble", "state" } );
    const uint64_t     idx = U64( At( aIn, "idx" ) );
    const bool         isHole = Bool( At( aIn, "is_hole" ) );
    const uint64_t     cnt = U64( At( aIn, "count" ) );
    const std::string& preamble = Text( At( aIn, "preamble" ) );

    if( preamble.size() > MAX_PREAMBLE )
        Invalid( "preamble" );

    for( char c : preamble )
    {
        if( c < 0x20 || c > 0x7e )
            Invalid( "preamble" );
    }

    std::ostringstream stream;
    const JSON&        state = At( aIn, "state" );

    if( !state.is_null() )
        ApplyState( stream, state );

    c2::details::OutlinePolyPath( stream, idx, isHole, cnt, preamble );
    return StreamReply( stream );
}

// --- MakePath ------------------------------------------------------------------------------

template <typename E>
E Element( const JSON& aValue )
{
    if constexpr( std::is_same_v<E, double> )
    {
        return F64( aValue );
    }
    else if constexpr( std::is_same_v<E, uint64_t> )
    {
        return U64( aValue );
    }
    else
    {
        const int64_t v = I64( aValue );

        if( v < int64_t( std::numeric_limits<E>::min() ) || v > int64_t( std::numeric_limits<E>::max() ) )
            Invalid( "element" );

        return E( v );
    }
}


template <typename E, size_t N>
JSON MakeArray( const std::string& aFn, const std::vector<E>& aValues )
{
    E list[N];

    for( size_t i = 0; i < N; ++i )
        list[i] = aValues[i];

    if constexpr( N % 2 == 0 )
    {
        if( aFn == "make_path_d" )
            return WritePath<double>( c2::MakePathD( list ) );

        if constexpr( std::is_integral_v<E> )
        {
            if( aFn == "make_path" )
                return WritePath<int64_t>( c2::MakePath( list ) );
        }
    }

    if constexpr( N % 3 == 0 )
    {
        if( aFn == "make_path_zd" )
            return WritePath<double>( c2::MakePathZD( list ) );

        if constexpr( std::is_integral_v<E> )
        {
            if( aFn == "make_path_z" )
                return WritePath<int64_t>( c2::MakePathZ( list ) );
        }
    }

    Invalid( "array size" );
}


template <typename E>
JSON MakeTyped( const JSON& aIn, const std::string& aFn )
{
    const JSON& valuesIn = Array( At( aIn, "values" ), "values" );

    if( valuesIn.size() > MAX_VALUES )
        Invalid( "size" );

    std::vector<E> values;

    for( const JSON& v : valuesIn )
        values.push_back( Element<E>( v ) );

    const bool array = Bool( At( aIn, "array" ) );
    const bool pair = aFn == "make_path" || aFn == "make_path_d";

    if( !std::is_integral_v<E> && ( aFn == "make_path" || aFn == "make_path_z" ) )
        Invalid( "elem" );

    if( !pair && !array )
        Invalid( "array" );

    return Guarded(
            [&]() -> JSON
            {
                if( !array )
                {
                    if( aFn == "make_path_d" )
                        return WritePath<double>( c2::MakePathD( values ) );

                    if constexpr( std::is_integral_v<E> )
                        return WritePath<int64_t>( c2::MakePath( values ) );
                    else
                        Invalid( "elem" );
                }

                if( pair )
                {
                    switch( values.size() )
                    {
                    case 2: return MakeArray<E, 2>( aFn, values );
                    case 4: return MakeArray<E, 4>( aFn, values );
                    case 6: return MakeArray<E, 6>( aFn, values );
                    case 8: return MakeArray<E, 8>( aFn, values );
                    default: Invalid( "array size" );
                    }
                }

                switch( values.size() )
                {
                case 3: return MakeArray<E, 3>( aFn, values );
                case 6: return MakeArray<E, 6>( aFn, values );
                case 9: return MakeArray<E, 9>( aFn, values );
                default: Invalid( "array size" );
                }
            },
            "path" );
}


JSON RunMakePath( const JSON& aIn )
{
    Keys( aIn, { "op", "fn", "elem", "values", "array" } );
    const std::string& fn = Text( At( aIn, "fn" ) );

    if( fn != "make_path" && fn != "make_path_d" && fn != "make_path_z" && fn != "make_path_zd" )
        Invalid( "fn" );

    const std::string& elem = Text( At( aIn, "elem" ) );

    if( elem == "i64" )
        return MakeTyped<int64_t>( aIn, fn );
    if( elem == "i32" )
        return MakeTyped<int32_t>( aIn, fn );
    if( elem == "u64" )
        return MakeTyped<uint64_t>( aIn, fn );
    if( elem == "u8" )
        return MakeTyped<uint8_t>( aIn, fn );
    if( elem == "f64" )
        return MakeTyped<double>( aIn, fn );

    Invalid( "elem" );
}

// --- path utilities ------------------------------------------------------------------------

template <typename T>
JSON RunTrim( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "path", "precision", "open" } );
    constexpr bool    D = std::is_same_v<T, double>;
    COUNTER           count;
    const c2::Path<T> path = ReadPath<T>( At( aIn, "path" ), count );
    const JSON&       openIn = At( aIn, "open" );
    const bool        hasOpen = !openIn.is_null();
    const bool        open = hasOpen && Bool( openIn );

    if constexpr( D )
    {
        const int precision = Int( At( aIn, "precision" ) );
        return Guarded(
                [&]() -> JSON
                {
                    return WritePath<T>( hasOpen ? c2::TrimCollinear( path, precision, open )
                                                 : c2::TrimCollinear( path, precision ) );
                },
                "path" );
    }
    else
    {
        if( !At( aIn, "precision" ).is_null() )
            Invalid( "precision" );

        return Guarded(
                [&]() -> JSON
                {
                    return WritePath<T>( hasOpen ? c2::TrimCollinear( path, open )
                                                 : c2::TrimCollinear( path ) );
                },
                "path" );
    }
}


template <typename T>
JSON RunDistance( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "a", "b" } );
    COUNTER            count;
    const c2::Point<T> a = ReadPoint<T>( At( aIn, "a" ), count );
    const c2::Point<T> b = ReadPoint<T>( At( aIn, "b" ), count );
    return Bits( c2::Distance( a, b ) );
}


template <typename T>
JSON RunLength( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "path", "closed" } );
    COUNTER           count;
    const c2::Path<T> path = ReadPath<T>( At( aIn, "path" ), count );
    const JSON&       closed = At( aIn, "closed" );
    return Bits( closed.is_null() ? c2::Length( path ) : c2::Length( path, Bool( closed ) ) );
}


template <typename T>
JSON RunNearCollinear( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "a", "b", "c", "value" } );
    COUNTER            count;
    const c2::Point<T> a = ReadPoint<T>( At( aIn, "a" ), count );
    const c2::Point<T> b = ReadPoint<T>( At( aIn, "b" ), count );
    const c2::Point<T> c = ReadPoint<T>( At( aIn, "c" ), count );
    return c2::NearCollinear( a, b, c, F64( At( aIn, "value" ) ) );
}


/// The resource guard for `Ellipse`: at most 2^20 points. A radius at or below zero returns
/// an empty path, so it is never refused; a NaN estimate is left to the reference (undefined).
void EllipseGuard( double aRadiusX, double aRadiusY, uint64_t aSteps )
{
    if( aRadiusX <= 0 )
        return;

    if( aSteps > 2 )
    {
        if( double( aSteps ) > MAX_ELLIPSE_STEPS )
            Invalid( "steps" );

        return;
    }

    const double radiusY = aRadiusY <= 0 ? aRadiusX : aRadiusY;
    const double estimate = c2::PI * std::sqrt( ( aRadiusX + radiusY ) / 2 );

    if( estimate > MAX_ELLIPSE_STEPS )
        Invalid( "steps" );
}


template <typename T>
JSON RunEllipse( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "form", "center", "rect", "radius_x", "radius_y", "steps" } );
    COUNTER            count;
    const std::string& form = Text( At( aIn, "form" ) );
    const JSON&        stepsIn = At( aIn, "steps" );
    const bool         hasSteps = !stepsIn.is_null();
    const uint64_t     steps = hasSteps ? U64( stepsIn ) : 0;

    if( form == "rect" )
    {
        if( !At( aIn, "center" ).is_null() || !At( aIn, "radius_x" ).is_null()
            || !At( aIn, "radius_y" ).is_null() )
            Invalid( "form" );

        const c2::Rect<T> rect = ReadRect<T>( At( aIn, "rect" ) );
        // A conservative estimate of the radii, computed without the reference's integer
        // subtraction (which may overflow: undefined, left to the reference).
        const double rx = ( double( rect.right ) - double( rect.left ) ) * 0.5;
        const double ry = ( double( rect.bottom ) - double( rect.top ) ) * 0.5;
        EllipseGuard( rx, ry, steps );
        return WritePath<T>( hasSteps ? c2::Ellipse( rect, steps ) : c2::Ellipse( rect ) );
    }

    if( form != "center" || !At( aIn, "rect" ).is_null() )
        Invalid( "form" );

    const c2::Point<T> center = ReadPoint<T>( At( aIn, "center" ), count );
    const double       rx = F64( At( aIn, "radius_x" ) );
    const JSON&        ryIn = At( aIn, "radius_y" );
    const bool         hasRy = !ryIn.is_null();
    const double       ry = hasRy ? F64( ryIn ) : 0.0;

    if( hasSteps && !hasRy )
        Invalid( "defaults" );

    EllipseGuard( rx, ry, steps );

    if( hasSteps )
        return WritePath<T>( c2::Ellipse( center, rx, ry, steps ) );
    if( hasRy )
        return WritePath<T>( c2::Ellipse( center, rx, ry ) );
    return WritePath<T>( c2::Ellipse( center, rx ) );
}


template <typename T>
void MaxPathLength( const c2::Paths<T>& aPaths, size_t aMax )
{
    for( const auto& p : aPaths )
    {
        if( p.size() > aMax )
            Invalid( "size" );
    }
}


template <typename T>
JSON RunSimplify( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "form", "value", "epsilon", "closed" } );
    COUNTER      count;
    const bool   single = SinglePath( aIn );
    c2::Paths<T> paths;

    if( single )
        paths.push_back( ReadPath<T>( At( aIn, "value" ), count ) );
    else
        paths = ReadPaths<T>( At( aIn, "value" ), count );

    MaxPathLength( paths, MAX_SIMPLIFY );
    const double epsilon = F64( At( aIn, "epsilon" ) );
    const JSON&  closedIn = At( aIn, "closed" );
    const bool   hasClosed = !closedIn.is_null();
    const bool   closed = hasClosed && Bool( closedIn );

    if( single )
        return WritePath<T>( hasClosed ? c2::SimplifyPath( paths[0], epsilon, closed )
                                       : c2::SimplifyPath( paths[0], epsilon ) );

    return WritePaths<T>( hasClosed ? c2::SimplifyPaths( paths, epsilon, closed )
                                    : c2::SimplifyPaths( paths, epsilon ) );
}


template <typename T>
JSON RunContains( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "path1", "path2" } );
    COUNTER           count;
    const c2::Path<T> path1 = ReadPath<T>( At( aIn, "path1" ), count );
    const c2::Path<T> path2 = ReadPath<T>( At( aIn, "path2" ), count );
    return c2::Path2ContainsPath1( path1, path2 );
}


/// Follow `RDP(path, 0, len - 1, epsSqrd, flags)`'s calls in the reference's order (the
/// `begin..idx` call returns before the `idx..end` call starts) and refuse the request if one
/// repeats an active ancestor's arguments or the calls exceed `MAX_RDP_CALLS`.
///
/// A call's control flow depends only on `path`, `begin` and `end` (it writes `flags` but never
/// reads them), so a call that repeats an active ancestor's arguments recurses forever; a
/// recursion that never ends must repeat one, since the arguments are indices of `path`.
/// This happens only for a NaN `epsSqrd`, when `max_d <= epsSqrd` never returns early and no
/// point moves `idx` off 0. Each step uses the reference's own comparisons and
/// `PerpendicDistFromLineSqrd` in the same order, so arithmetic the reference evaluates
/// first (an overflow the replay traps) is reached first here too.
template <typename T>
void RdpNanGuard( const c2::Path<T>& aPath, double aEpsSqrd )
{
    struct CALL
    {
        size_t begin, end, depth;
    };

    std::vector<CALL>                      pending{ { 0, aPath.size() - 1, 0 } };
    std::vector<std::pair<size_t, size_t>> chain;
    size_t                                 calls = 0;

    while( !pending.empty() )
    {
        const CALL call = pending.back();
        pending.pop_back();

        if( ++calls > MAX_RDP_CALLS )
            Invalid( "epsilon" );

        // Return from every call deeper than this one's parent.
        chain.resize( std::min( chain.size(), call.depth ) );

        if( std::find( chain.begin(), chain.end(), std::make_pair( call.begin, call.end ) )
            != chain.end() )
            Invalid( "epsilon" );

        chain.emplace_back( call.begin, call.end );
        size_t begin = call.begin, end = call.end, idx = 0;
        double max_d = 0;

        while( end > begin && aPath[begin] == aPath[end] )
            --end;

        for( size_t i = begin + 1; i < end; ++i )
        {
            double d = c2::PerpendicDistFromLineSqrd( aPath[i], aPath[begin], aPath[end] );

            if( d <= max_d )
                continue;

            max_d = d;
            idx = i;
        }

        if( max_d <= aEpsSqrd )
            continue;

        // Pushed in reverse so that the begin..idx call runs first.
        if( idx < end - 1 )
            pending.push_back( { idx, end, call.depth + 1 } );

        if( idx > begin + 1 )
            pending.push_back( { begin, idx, call.depth + 1 } );
    }
}


template <typename T>
JSON RunRdp( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "form", "value", "epsilon" } );
    COUNTER      count;
    const bool   single = SinglePath( aIn );
    c2::Paths<T> paths;

    if( single )
        paths.push_back( ReadPath<T>( At( aIn, "value" ), count ) );
    else
        paths = ReadPaths<T>( At( aIn, "value" ), count );

    // RDP copies the path into every recursive call; bound the depth times the length.
    MaxPathLength( paths, MAX_RDP );
    const double epsilon = F64( At( aIn, "epsilon" ) );

    // A NaN epsilon can make RDP recurse forever; only those paths (and ones beyond the call
    // bound) are refused. Shorter paths are copied without an RDP call.
    if( std::isnan( epsilon ) )
    {
        for( const c2::Path<T>& path : paths )
        {
            if( path.size() >= 5 )
                RdpNanGuard( path, c2::Sqr( epsilon ) );
        }
    }

    if( single )
        return WritePath<T>( c2::RamerDouglasPeucker( paths[0], epsilon ) );

    return WritePaths<T>( c2::RamerDouglasPeucker( paths, epsilon ) );
}


template <typename T>
JSON RunMinkowski( const JSON& aIn )
{
    Keys( aIn, { "op", "type", "sum", "pattern", "path", "closed", "decimal_places" } );
    constexpr bool    D = std::is_same_v<T, double>;
    COUNTER           count;
    const bool        sum = Bool( At( aIn, "sum" ) );
    const c2::Path<T> pattern = ReadPath<T>( At( aIn, "pattern" ), count );
    const c2::Path<T> path = ReadPath<T>( At( aIn, "path" ), count );
    const bool        closed = Bool( At( aIn, "closed" ) );
    int               dp = 2;
    const bool        hasDp = OptionalInt( At( aIn, "decimal_places" ), dp );

    if( !D && hasDp )
        Invalid( "decimal_places" );

    if( pattern.size() * path.size() > MAX_QUADS )
        Invalid( "size" );

    if constexpr( D )
    {
        const double scale = Pow10( dp );

        // Scaled coordinates beyond half the engine domain that still convert (below 2^63)
        // are refused; larger or NaN products are left to the reference (undefined).
        auto check = [&]( const c2::PathD& aPath )
        {
            for( const auto& p : aPath )
            {
                for( double v : { p.x * scale, p.y * scale } )
                {
                    if( std::fabs( v ) > MINKOWSKI_COORD_D && std::fabs( v ) < TWO_POW_63 )
                        Invalid( "domain" );
                }
            }
        };
        check( pattern );
        check( path );
    }
    else
    {
        for( const c2::Path64* p : { &pattern, &path } )
        {
            for( const auto& pt : *p )
            {
                if( pt.x < -MINKOWSKI_COORD || pt.x > MINKOWSKI_COORD || pt.y < -MINKOWSKI_COORD
                    || pt.y > MINKOWSKI_COORD )
                    Invalid( "domain" );
            }
        }
    }

    return Guarded(
            [&]() -> JSON
            {
                if constexpr( D )
                {
                    if( sum )
                        return WritePaths<T>( hasDp ? c2::MinkowskiSum( pattern, path, closed, dp )
                                                    : c2::MinkowskiSum( pattern, path, closed ) );
                    return WritePaths<T>( hasDp ? c2::MinkowskiDiff( pattern, path, closed, dp )
                                                : c2::MinkowskiDiff( pattern, path, closed ) );
                }
                else
                {
                    return WritePaths<T>( sum ? c2::MinkowskiSum( pattern, path, closed )
                                              : c2::MinkowskiDiff( pattern, path, closed ) );
                }
            },
            "paths" );
}


#ifdef INCLUDE_P002_REPLAY
// The replay's subscript checks come from _GLIBCXX_ASSERTIONS, but GCC 13.3's
// std::vector<bool>::operator[] has none (GCC 15's has), and UBSan cannot see a read of a spare
// bit in an allocated word. So the replay (only) first reads the flags at the indices
// details::GetNext and details::GetPrior read, in their order, and traps, as a failed
// assertion would, on one outside the vector. The genuine evaluation is unchanged.
//
// Within SimplifyPath these functions cannot overrun: its loop keeps at least two vertices
// unflagged (it stops when GetPrior and GetNext agree), and RDP and RamerDouglasPeucker only
// index flags with vertex indices (findings: clipper2-include-p002-port.md).
bool TraceFlag( const std::vector<bool>& aFlags, size_t aIndex )
{
    if( aIndex >= aFlags.size() )
        __builtin_trap();

    return aFlags[aIndex];
}


void TraceGetNext( size_t aCurrent, size_t aHigh, const std::vector<bool>& aFlags )
{
    ++aCurrent;

    while( aCurrent <= aHigh && TraceFlag( aFlags, aCurrent ) )
        ++aCurrent;

    if( aCurrent <= aHigh )
        return;

    aCurrent = 0;

    while( TraceFlag( aFlags, aCurrent ) )
        ++aCurrent;
}


void TraceGetPrior( size_t aCurrent, size_t aHigh, const std::vector<bool>& aFlags )
{
    if( aCurrent == 0 )
        aCurrent = aHigh;
    else
        --aCurrent;

    while( aCurrent > 0 && TraceFlag( aFlags, aCurrent ) )
        --aCurrent;

    if( !TraceFlag( aFlags, aCurrent ) )
        return;

    aCurrent = aHigh;

    while( TraceFlag( aFlags, aCurrent ) )
        --aCurrent;
}
#endif


JSON RunDetail( const JSON& aIn )
{
    Keys( aIn, { "op", "fn", "current", "high", "flags" } );
    const std::string& fn = Text( At( aIn, "fn" ) );
    const JSON&        flagsIn = Array( At( aIn, "flags" ), "flags" );

    if( flagsIn.empty() || flagsIn.size() > MAX_FLAGS )
        Invalid( "size" );

    std::vector<bool> flags;

    for( const JSON& f : flagsIn )
        flags.push_back( Bool( f ) );

    const uint64_t high = U64( At( aIn, "high" ) );
    const uint64_t current = U64( At( aIn, "current" ) );

    // SimplifyPath's contract: one flag per vertex, high the last index.
    if( high + 1 != flags.size() || current > high )
        Invalid( "detail" );

#ifdef INCLUDE_P002_REPLAY
    if( fn == "get_next" )
        TraceGetNext( current, high, flags );
    else if( fn == "get_prior" )
        TraceGetPrior( current, high, flags );
#endif

    if( fn == "get_next" )
        return c2::details::GetNext( current, high, flags );

    if( fn == "get_prior" )
        return c2::details::GetPrior( current, high, flags );

    Invalid( "fn" );
}


template <typename T>
JSON RunTyped( const std::string& aOp, const JSON& aIn )
{
    if( aOp == "boolean" )
        return RunBoolean<T>( aIn );
    if( aOp == "inflate" )
        return RunInflate<T>( aIn );
    if( aOp == "translate" )
        return RunTranslate<T>( aIn );
    if( aOp == "rectclip" )
        return RunRectClip<T>( aIn );
    if( aOp == "polytree" )
        return RunPolyTree<T>( aIn );
    if( aOp == "trim" )
        return RunTrim<T>( aIn );
    if( aOp == "distance" )
        return RunDistance<T>( aIn );
    if( aOp == "length" )
        return RunLength<T>( aIn );
    if( aOp == "near_collinear" )
        return RunNearCollinear<T>( aIn );
    if( aOp == "ellipse" )
        return RunEllipse<T>( aIn );
    if( aOp == "simplify" )
        return RunSimplify<T>( aIn );
    if( aOp == "contains" )
        return RunContains<T>( aIn );
    if( aOp == "rdp" )
        return RunRdp<T>( aIn );
    if( aOp == "minkowski" )
        return RunMinkowski<T>( aIn );

    Invalid( "op" );
}
} // namespace


JSON Evaluate( const JSON& aIn )
{
    const std::string& op = Text( At( aIn, "op" ) );

    if( op == "make_path" )
        return RunMakePath( aIn );
    if( op == "outline" )
        return RunOutline( aIn );
    if( op == "detail" )
        return RunDetail( aIn );
    if( op == "offset_members" )
        return RunOffsetMembers( aIn );
    if( op == "version" )
    {
        Keys( aIn, { "op" } );
        return JSON{ { "version", CLIPPER2_VERSION } };
    }

    return IsDouble( aIn ) ? RunTyped<double>( op, aIn ) : RunTyped<int64_t>( op, aIn );
}
} // namespace INCLUDE_P002_NS
