// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned ELLIPSE<int> and ELLIPSE<double> (libs/kimath/src/geometry/ellipse.cpp).
// The adapter decodes inputs, encodes every observable value (doubles as exact IEEE bits) and
// refuses operations whose pinned evaluation is undefined before calling them; it never
// computes a reply itself.
#include "registry.h"
#include <geometry/ellipse.h>
#include <bit>
#include <climits>
#include <cmath>
#include <cstdint>
#include <iomanip>
#include <locale>
#include <sstream>
#include <stdexcept>
#include <string>
#include <type_traits>

namespace
{
using oracle::JSON;
constexpr const char* ELLIPSE_UNIT = "libs.kimath.src.geometry.ellipse.application.ui.p001";
constexpr size_t      MAX_STEPS = 8;


void checkKeys( const JSON& aInput, std::initializer_list<const char*> aFields )
{
    if( !aInput.is_object() )
        throw std::invalid_argument( "expected object" );
    if( aInput.size() != aFields.size() )
        throw std::invalid_argument( "unexpected or missing field" );
    for( const char* field : aFields )
        if( !aInput.contains( field ) )
            throw std::invalid_argument( "unexpected or missing field" );
}


std::string readText( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + aName );
    return aInput.at( aName ).get<std::string>();
}


double decodeBits( const JSON& aValue )
{
    if( !aValue.is_string() )
        throw std::invalid_argument( "expected hex" );
    const std::string text = aValue.get<std::string>();
    if( text.size() != 16 || text.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
        throw std::invalid_argument( "invalid hex" );
    return std::bit_cast<double>( std::stoull( text, nullptr, 16 ) );
}


JSON bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


template <typename T>
T decodeScalar( const JSON& aValue )
{
    if constexpr( std::is_same_v<T, double> )
    {
        return decodeBits( aValue );
    }
    else
    {
        if( !aValue.is_number_integer()
            || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() > INT_MAX )
            || aValue.get<int64_t>() < INT_MIN || aValue.get<int64_t>() > INT_MAX )
            throw std::invalid_argument( "integer out of range" );
        return aValue.get<int>();
    }
}


template <typename T>
JSON encodeScalar( T aValue )
{
    if constexpr( std::is_same_v<T, double> )
        return bits( aValue );
    else
        return aValue;
}


template <typename T>
VECTOR2<T> readPoint( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_array() || aInput.at( aName ).size() != 2 )
        throw std::invalid_argument( std::string( "expected point: " ) + aName );
    const JSON& p = aInput.at( aName );
    return VECTOR2<T>( decodeScalar<T>( p[0] ), decodeScalar<T>( p[1] ) );
}


template <typename T>
JSON encodePoint( const VECTOR2<T>& aPoint )
{
    return JSON::array( { encodeScalar( aPoint.x ), encodeScalar( aPoint.y ) } );
}


EDA_ANGLE readAngle( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) )
        throw std::invalid_argument( std::string( "expected angle: " ) + aName );
    return EDA_ANGLE( decodeBits( aInput.at( aName ) ), DEGREES_T );
}


JSON undefinedObservation()
{
    return JSON{ { "error", { { "kind", "invalid_input" }, { "message", "undefined ellipse operation" } } } };
}


template <typename T>
JSON state( const ELLIPSE<T>& aEllipse )
{
    return JSON{ { "center", encodePoint( aEllipse.Center ) },
                 { "radii", { encodeScalar( aEllipse.MajorRadius ), encodeScalar( aEllipse.MinorRadius ) } },
                 { "angles",
                   { bits( aEllipse.Rotation.AsDegrees() ), bits( aEllipse.StartAngle.AsDegrees() ),
                     bits( aEllipse.EndAngle.AsDegrees() ) } } };
}


bool fitsInt( int64_t aValue )
{
    return aValue >= INT_MIN && aValue <= INT_MAX;
}


// The constructor form, with the default arguments taken by omitting "start" (and "end"), or
// "end" alone. Returns false when the pinned constructor would be undefined.
template <typename T>
bool construct( const JSON& aInput, ELLIPSE<T>& aOut )
{
    const std::string form = readText( aInput, "form" );
    if( form == "default" )
    {
        checkKeys( aInput, { "form" } );
        aOut = ELLIPSE<T>();
        return true;
    }

    const bool hasStart = aInput.contains( "start" );
    const bool hasEnd = aInput.contains( "end" );
    if( hasEnd && !hasStart )
        throw std::invalid_argument( "end angle without start angle" );

    if( form == "radii" )
    {
        if( hasEnd )
            checkKeys( aInput, { "form", "center", "major", "minor", "rotation", "start", "end" } );
        else if( hasStart )
            checkKeys( aInput, { "form", "center", "major", "minor", "rotation", "start" } );
        else
            checkKeys( aInput, { "form", "center", "major", "minor", "rotation" } );
        const VECTOR2<T> center = readPoint<T>( aInput, "center" );
        const T          major = decodeScalar<T>( aInput.at( "major" ) );
        const T          minor = decodeScalar<T>( aInput.at( "minor" ) );
        const EDA_ANGLE  rotation = readAngle( aInput, "rotation" );
        if( hasEnd )
            aOut = ELLIPSE<T>( center, major, minor, rotation, readAngle( aInput, "start" ),
                               readAngle( aInput, "end" ) );
        else if( hasStart )
            aOut = ELLIPSE<T>( center, major, minor, rotation, readAngle( aInput, "start" ) );
        else
            aOut = ELLIPSE<T>( center, major, minor, rotation );
        return true;
    }

    if( form == "major" )
    {
        if( hasEnd )
            checkKeys( aInput, { "form", "center", "major", "ratio", "start", "end" } );
        else if( hasStart )
            checkKeys( aInput, { "form", "center", "major", "ratio", "start" } );
        else
            checkKeys( aInput, { "form", "center", "major", "ratio" } );
        const VECTOR2<T> center = readPoint<T>( aInput, "center" );
        const VECTOR2<T> major = readPoint<T>( aInput, "major" );
        const double     ratio = decodeBits( aInput.at( "ratio" ) );
        const EDA_ANGLE  start = hasStart ? readAngle( aInput, "start" ) : ANGLE_0;
        const EDA_ANGLE  end = hasEnd ? readAngle( aInput, "end" ) : FULL_CIRCLE;

        if constexpr( std::is_integral_v<T> )
        {
            // NumericType( MajorRadius * aRatio ): [conv.fpint] leaves a NaN or a value whose
            // truncation is not representable undefined. The guard evaluates the same header
            // EuclideanNorm and product; the reply comes from the pinned constructor.
            const double product = major.EuclideanNorm() * ratio;
            if( std::isnan( product ) || std::trunc( product ) < -2147483648.0
                || std::trunc( product ) >= 2147483648.0 )
                return false;
        }

        // start/end are decoded above only so that every input is validated before the guard;
        // the calls still take the default arguments when they were omitted.
        if( hasEnd )
            aOut = ELLIPSE<T>( center, major, ratio, start, end );
        else if( hasStart )
            aOut = ELLIPSE<T>( center, major, ratio, start );
        else
            aOut = ELLIPSE<T>( center, major, ratio );
        return true;
    }

    throw std::invalid_argument( "unknown constructor form" );
}


// GetPointAtAngle, or the undefined observation when its final int addition would overflow.
// The guard calls the same pinned body on a copy centred on the origin, whose addition cannot
// overflow, and checks the real centre's sum in 64 bits.
template <typename T>
JSON point( const ELLIPSE<T>& aEllipse, const EDA_ANGLE& aAngle, bool& aUndefined )
{
    if constexpr( std::is_integral_v<T> )
    {
        ELLIPSE<T> origin = aEllipse;
        origin.Center = VECTOR2<T>( 0, 0 );
        const VECTOR2<T> offset = origin.GetPointAtAngle( aAngle );
        if( !fitsInt( int64_t( aEllipse.Center.x ) + offset.x )
            || !fitsInt( int64_t( aEllipse.Center.y ) + offset.y ) )
        {
            aUndefined = true;
            return undefinedObservation();
        }
    }
    return encodePoint( aEllipse.GetPointAtAngle( aAngle ) );
}


template <typename T>
JSON step( ELLIPSE<T>& aEllipse, const JSON& aStep, bool& aUndefined )
{
    const std::string op = readText( aStep, "op" );
    if( op == "mirror" )
    {
        checkKeys( aStep, { "op", "ref", "direction" } );
        const VECTOR2<T>  ref = readPoint<T>( aStep, "ref" );
        const std::string direction = readText( aStep, "direction" );
        FLIP_DIRECTION    flip;
        if( direction == "left_right" )
            flip = FLIP_DIRECTION::LEFT_RIGHT;
        else if( direction == "top_bottom" )
            flip = FLIP_DIRECTION::TOP_BOTTOM;
        else
            throw std::invalid_argument( "unknown flip direction" );

        if constexpr( std::is_integral_v<T> )
        {
            // 2 * aRef - Center in int: either signed operation may overflow.
            const int64_t r = flip == FLIP_DIRECTION::LEFT_RIGHT ? ref.x : ref.y;
            const int64_t c = flip == FLIP_DIRECTION::LEFT_RIGHT ? aEllipse.Center.x : aEllipse.Center.y;
            if( !fitsInt( 2 * r ) || !fitsInt( 2 * r - c ) )
            {
                aUndefined = true;
                return undefinedObservation();
            }
        }
        aEllipse.Mirror( ref, flip );
        return state( aEllipse );
    }
    if( op == "subtended" )
    {
        checkKeys( aStep, { "op" } );
        return bits( aEllipse.GetSubtendedAngle().AsDegrees() );
    }
    if( op == "point" )
    {
        checkKeys( aStep, { "op", "angle" } );
        return point( aEllipse, readAngle( aStep, "angle" ), aUndefined );
    }
    if( op == "start_point" || op == "end_point" )
    {
        checkKeys( aStep, { "op" } );
        // The header wrappers forward the stored angle; the guard checks the same angle.
        bool       undefined = false;
        const JSON guard = point( aEllipse, op == "start_point" ? aEllipse.StartAngle : aEllipse.EndAngle,
                                  undefined );
        if( undefined )
        {
            aUndefined = true;
            return guard;
        }
        return encodePoint( op == "start_point" ? aEllipse.GetArcStartPoint() : aEllipse.GetArcEndPoint() );
    }
    if( op == "angle_at" )
    {
        checkKeys( aStep, { "op", "point" } );
        const VECTOR2<T> pt = readPoint<T>( aStep, "point" );
        if constexpr( std::is_floating_point_v<T> )
        {
            // std::max( 1.0, radius ) requires ordered (non-NaN) values ([alg.min.max]).
            if( std::isnan( aEllipse.MajorRadius ) || std::isnan( aEllipse.MinorRadius ) )
            {
                aUndefined = true;
                return undefinedObservation();
            }
        }
        return bits( aEllipse.GetAngleAtPoint( pt ).AsDegrees() );
    }
    throw std::invalid_argument( "unknown step" );
}


template <typename T>
JSON runType( const JSON& aInput )
{
    const JSON& steps = aInput.at( "steps" );
    if( !steps.is_array() || steps.size() > MAX_STEPS )
        throw std::invalid_argument( "steps out of range" );
    // Validate every step's shape before any pinned call, so a malformed request never
    // reaches the library.
    for( const JSON& s : steps )
        if( !s.is_object() )
            throw std::invalid_argument( "expected step object" );

    ELLIPSE<T> ellipse;
    if( !construct<T>( aInput.at( "construct" ), ellipse ) )
        return JSON{ { "state", undefinedObservation() }, { "steps", JSON::array() } };

    JSON out = JSON{ { "state", state( ellipse ) }, { "steps", JSON::array() } };
    // Evaluation stops at the first undefined observation.
    for( const JSON& s : steps )
    {
        bool undefined = false;
        out["steps"].push_back( step<T>( ellipse, s, undefined ) );
        if( undefined )
            break;
    }
    return out;
}


JSON run( const JSON& aInput )
{
    checkKeys( aInput, { "op", "type", "construct", "steps" } );
    if( readText( aInput, "op" ) != "ellipse" )
        throw std::invalid_argument( "unknown operation" );
    if( !aInput.at( "construct" ).is_object() )
        throw std::invalid_argument( "expected construct object" );
    const std::string type = readText( aInput, "type" );
    if( type == "i32" )
        return runType<int>( aInput );
    if( type == "f64" )
        return runType<double>( aInput );
    throw std::invalid_argument( "unknown ellipse type" );
}
} // namespace

namespace oracle
{
void RegisterEllipse( REGISTRY& aRegistry )
{
    aRegistry.Add( ELLIPSE_UNIT, "geometry/ellipse.cpp", run );
}
} // namespace oracle
