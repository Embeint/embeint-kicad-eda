// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for unit libs.kimath.src.bezier_curves.application.ui.p001: the genuine pinned
// libs/kimath/src/bezier_curves.cpp in the kimath archive (BEZIER_POLY's two VECTOR2I
// constructors, both GetPoly overloads and every private helper, approx_int/approx_inv_int
// through getQuadPoly, and both TransformEllipseToBeziers instantiations).
//
// The adapter decodes 32-bit integer points and IEEE bit strings, calls the genuine functions and
// encodes every observable value exactly; it never computes a reply itself. The private members
// are reached through explicit-instantiation member pointers ([temp.explicit] exempts their
// arguments from access checking), so their out-of-line definitions in the archive run. Every
// reply carries the control points after the call.
//
// Refused as "undefined bezier operation", before any genuine call:
// - getQuadPoly's `int n = std::ceil( ... )` of a NaN or of a value outside int ([conv.fpint]);
// - TransformEllipseToBeziers' `static_cast<int>( std::ceil( ... ) )` likewise, and for
//   ELLIPSE<int> a NaN point component converted by the VECTOR2<int> casting constructor
//   (std::clamp returns it unchanged, and converting it is undefined; guarded, though no input
//   reaches it: RotatePoint folds a NaN angle to zero and every other value is finite);
// - a private member whose subscripts leave its vectors (the pinned code indexes m_ctrlPts[0..3]
//   without checks; GCC 13.3's libstdc++ is not built with subscript assertions, so the bounds
//   are checked here explicitly);
// - isFlat's three-point std::max/std::min of a NaN parameter ([alg.min.max] requires
//   Cpp17LessThanComparable values, which a NaN is not).
//
// Harness bounds, "outside the verified domain": GetPoly's loops are run only when the
// iterations of getQuadPoly's loop, recursiveSegmentation's loop and cubicParabolicApprox's loop
// together stay within STEP_BUDGET, and TransformEllipseToBeziers only for at most MAX_BEZIERS
// curves. A curve with a NaN control point can make recursiveSegmentation subdivide without end
// (its stack grows until allocation fails), so these are not predicted from the values: a guard
// below repeats the pinned control flow of GetPoly( std::vector<VECTOR2D>&, double ) statement by
// statement and counts the iterations. Its double arithmetic is the pinned expressions through
// the pinned VECTOR2 operators and libm, so every comparison, and therefore every branch,
// iteration count and output size, equals the genuine call's (a NaN's sign or payload never
// changes a comparison). The genuine output size is checked against the guard's.
//
// wxASSERT( false ) (isFlat, eval, subdivide and GetPoly, for a control point count other than
// three or four) calls wxOnAssert when wxTheAssertHandler is set. A recording handler wraps the
// process's own handler, as in the include/font adapter: it records each call (file name, line,
// function, condition, message) in the reply and then calls the wrapped handler, so the genuine
// continuation follows. kimathLogOverflow (a saturating KiROUND in GetPoly( VECTOR2I )) reaches
// wxFAIL_MSG in math/util.cpp, recorded the same way.
#include "registry.h"
#include <bezier_curves.h>
#include <geometry/eda_angle.h>
#include <geometry/ellipse.h>
#include <math/vector2d.h>
#include <trigo.h>
#include <wx/debug.h>
#include <wx/string.h>
#include <algorithm>
#include <bit>
#include <climits>
#include <cmath>
#include <cstdint>
#include <iomanip>
#include <locale>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

namespace
{
template <typename Tag, typename Tag::Type Member>
struct Access
{
    friend typename Tag::Type Get( Tag ) { return Member; }
};

#define MEMBER( Name, TypeName, MemberExpr )                                                       \
    struct Name                                                                                    \
    {                                                                                              \
        using Type = TypeName;                                                                     \
        friend Type Get( Name );                                                                   \
    };                                                                                             \
    template struct Access<Name, MemberExpr>;

MEMBER( MinSegLen, double BEZIER_POLY::*, &BEZIER_POLY::m_minSegLen )
MEMBER( CtrlPts, std::vector<VECTOR2D> BEZIER_POLY::*, &BEZIER_POLY::m_ctrlPts )
MEMBER( IsNaN, bool ( BEZIER_POLY::* )() const, &BEZIER_POLY::isNaN )
MEMBER( IsFlat, bool ( BEZIER_POLY::* )( double ) const, &BEZIER_POLY::isFlat )
MEMBER( Eval, VECTOR2D ( BEZIER_POLY::* )( double ), &BEZIER_POLY::eval )
MEMBER( Subdivide, void ( BEZIER_POLY::* )( double, BEZIER_POLY&, BEZIER_POLY& ),
        &BEZIER_POLY::subdivide )
MEMBER( NumberOfInflectionPoints, int ( BEZIER_POLY::* )(), &BEZIER_POLY::numberOfInflectionPoints )
MEMBER( FindInflectionPoints, int ( BEZIER_POLY::* )( double&, double& ),
        &BEZIER_POLY::findInflectionPoints )
MEMBER( ThirdControlPointDeviation, double ( BEZIER_POLY::* )(),
        &BEZIER_POLY::thirdControlPointDeviation )
#undef MEMBER

using oracle::JSON;
constexpr const char* BEZIER_UNIT = "libs.kimath.src.bezier_curves.application.ui.p001";

/// The most control points, prior output points or prior curves a request may give.
constexpr std::size_t MAX_POINTS = 4096;

/// The most loop iterations (getQuadPoly, recursiveSegmentation and cubicParabolicApprox
/// together) one GetPoly call may run.
constexpr uint64_t STEP_BUDGET = uint64_t( 1 ) << 20;

/// The most curves one TransformEllipseToBeziers call may add.
constexpr int MAX_BEZIERS = 4096;

[[noreturn]] void undefined()
{
    throw std::invalid_argument( "undefined bezier operation" );
}


[[noreturn]] void outside()
{
    throw std::invalid_argument( "outside the verified domain" );
}


void checkKeys( const JSON& aInput, std::initializer_list<const char*> aRequired,
                std::initializer_list<const char*> aOptional = {} )
{
    if( !aInput.is_object() )
        throw std::invalid_argument( "expected object" );

    std::size_t present = 0;

    for( const char* field : aRequired )
    {
        if( !aInput.contains( field ) )
            throw std::invalid_argument( "unexpected or missing field" );
        ++present;
    }

    for( const char* field : aOptional )
        present += aInput.contains( field ) ? 1 : 0;

    if( aInput.size() != present )
        throw std::invalid_argument( "unexpected or missing field" );
}


std::string readText( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + aName );
    return aInput.at( aName ).get<std::string>();
}


int readInt( const JSON& aValue )
{
    if( !aValue.is_number_integer()
        || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() > INT_MAX )
        || aValue.get<int64_t>() < INT_MIN || aValue.get<int64_t>() > INT_MAX )
        throw std::invalid_argument( "integer out of range" );
    return aValue.get<int>();
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


const JSON& pair( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "expected point" );
    return aValue;
}


VECTOR2I readPointI( const JSON& aValue )
{
    const JSON& p = pair( aValue );
    return VECTOR2I( readInt( p[0] ), readInt( p[1] ) );
}


VECTOR2D readPointD( const JSON& aValue )
{
    const JSON& p = pair( aValue );
    return VECTOR2D( decodeBits( p[0] ), decodeBits( p[1] ) );
}


const JSON& readArray( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_array() )
        throw std::invalid_argument( std::string( "expected array: " ) + aName );
    if( aInput.at( aName ).size() > MAX_POINTS )
        throw std::invalid_argument( std::string( "too many entries: " ) + aName );
    return aInput.at( aName );
}


std::vector<VECTOR2I> readPointsI( const JSON& aInput, const char* aName )
{
    std::vector<VECTOR2I> points;
    for( const JSON& point : readArray( aInput, aName ) )
        points.push_back( readPointI( point ) );
    return points;
}


std::vector<VECTOR2D> readPointsD( const JSON& aInput, const char* aName )
{
    std::vector<VECTOR2D> points;
    for( const JSON& point : readArray( aInput, aName ) )
        points.push_back( readPointD( point ) );
    return points;
}


// An angle is given as the exact bits of its value in degrees.
EDA_ANGLE readAngle( const JSON& aValue )
{
    return EDA_ANGLE( decodeBits( aValue ), DEGREES_T );
}


JSON pointI( const VECTOR2I& aValue )
{
    return JSON::array( { aValue.x, aValue.y } );
}


JSON pointD( const VECTOR2D& aValue )
{
    return JSON::array( { bits( aValue.x ), bits( aValue.y ) } );
}


JSON pointsD( const std::vector<VECTOR2D>& aPoints )
{
    JSON out = JSON::array();
    for( const VECTOR2D& point : aPoints )
        out.push_back( pointD( point ) );
    return out;
}


JSON pointsI( const std::vector<VECTOR2I>& aPoints )
{
    JSON out = JSON::array();
    for( const VECTOR2I& point : aPoints )
        out.push_back( pointI( point ) );
    return out;
}


JSON state( const BEZIER_POLY& aPoly )
{
    return JSON{ { "min_seg_len", bits( aPoly.*Get( MinSegLen{} ) ) },
                 { "ctrl_pts", pointsD( aPoly.*Get( CtrlPts{} ) ) } };
}


/// The wxOnAssert calls of the current operation, and the handler the recorder wraps.
JSON*             g_assertions = nullptr;
wxAssertHandler_t g_wrappedHandler = nullptr;


/// Records one wxOnAssert call, then calls the wrapped handler.
void RecordAssertion( const wxString& aFile, int aLine, const wxString& aFunc,
                      const wxString& aCond, const wxString& aMsg )
{
    if( g_assertions )
    {
        const std::string file = aFile.utf8_string();
        const std::size_t slash = file.find_last_of( '/' );

        g_assertions->push_back( JSON{ { "file", slash == std::string::npos
                                                         ? file
                                                         : file.substr( slash + 1 ) },
                                       { "line", aLine },
                                       { "function", aFunc.utf8_string() },
                                       { "condition", aCond.utf8_string() },
                                       { "message", aMsg.utf8_string() } } );
    }

    if( g_wrappedHandler )
        g_wrappedHandler( aFile, aLine, aFunc, aCond, aMsg );
}


/// Installs RecordAssertion around the process's handler for one operation, and only if a
/// handler is installed (wxASSERT calls wxOnAssert only then).
class ASSERTION_SCOPE
{
public:
    explicit ASSERTION_SCOPE( JSON& aCalls )
    {
        g_assertions = &aCalls;
        g_wrappedHandler = wxTheAssertHandler;

        if( g_wrappedHandler )
            wxSetAssertHandler( RecordAssertion );
    }

    ~ASSERTION_SCOPE()
    {
        if( g_wrappedHandler )
            wxSetAssertHandler( g_wrappedHandler );

        g_assertions = nullptr;
        g_wrappedHandler = nullptr;
    }

    ASSERTION_SCOPE( const ASSERTION_SCOPE& ) = delete;
    ASSERTION_SCOPE& operator=( const ASSERTION_SCOPE& ) = delete;
};


// The pinned control flow of GetPoly( std::vector<VECTOR2D>&, double ), only to count its loop
// iterations and to find getQuadPoly's undefined conversion before the genuine call. The
// expressions are the pinned ones (bezier_curves.cpp), through the pinned VECTOR2 operators.
// Only the four-point bodies of the private helpers are needed: GetPoly reaches the helpers
// only for a three- or four-point curve, and every curve the cubic path makes has four points.
namespace guard
{
using POINTS = std::vector<VECTOR2D>;

struct STEPS
{
    uint64_t count = 0;

    void Add( uint64_t aCount )
    {
        if( aCount > STEP_BUDGET - count )
            outside();
        count += aCount;
    }
};


bool isNaN( const POINTS& p )
{
    for( const VECTOR2D& pt : p )
    {
        if( std::isnan( pt.x ) || std::isnan( pt.y ) )
            return true;
    }

    return false;
}


bool isFlat( const POINTS& p, double aMaxError )
{
    VECTOR2D delta = p[3] - p[0];

    VECTOR2D D21 = p[1] - p[0];
    VECTOR2D D31 = p[2] - p[0];

    double cross1 = delta.Cross( D21 );
    double cross2 = delta.Cross( D31 );

    double inv_delta_sq = 1.0 / delta.SquaredEuclideanNorm();
    double d1 = ( cross1 * cross1 ) * inv_delta_sq;
    double d2 = ( cross2 * cross2 ) * inv_delta_sq;

    double factor = ( cross1 * cross2 > 0.0 ) ? 3.0 / 4.0 : 4.0 / 9.0;
    double f2 = factor * factor;
    double tol = aMaxError * aMaxError;

    return d1 * f2 <= tol && d2 * f2 <= tol;
}


void subdivide( const POINTS& p, double aT, POINTS& aLeft, POINTS& aRight )
{
    VECTOR2D left_ctrl1 = p[0] + aT * ( p[1] - p[0] );
    VECTOR2D tmp = p[1] + aT * ( p[2] - p[1] );
    VECTOR2D left_ctrl2 = left_ctrl1 + aT * ( tmp - left_ctrl1 );
    VECTOR2D right_ctrl2 = p[2] + aT * ( p[3] - p[2] );
    VECTOR2D right_ctrl1 = tmp + aT * ( right_ctrl2 - tmp );
    VECTOR2D shared = left_ctrl2 + aT * ( right_ctrl1 - left_ctrl2 );

    aLeft[0] = p[0];
    aLeft[1] = left_ctrl1;
    aLeft[2] = left_ctrl2;
    aLeft[3] = shared;

    aRight[3] = p[3];
    aRight[2] = right_ctrl2;
    aRight[1] = right_ctrl1;
    aRight[0] = shared;
}


int numberOfInflectionPoints( const POINTS& p )
{
    VECTOR2D D21 = p[1] - p[0];
    VECTOR2D D32 = p[2] - p[1];
    VECTOR2D D43 = p[3] - p[2];

    double cross1 = D21.Cross( D32 ) * D32.Cross( D43 );
    double cross2 = D21.Cross( D32 ) * D21.Cross( D43 );

    if( cross1 < 0.0 )
        return 1;
    else if( cross2 > 0.0 )
        return 0;

    bool b1 = D21.Dot( D32 ) > 0.0;
    bool b2 = D32.Dot( D43 ) > 0.0;

    if( b1 ^ b2 )
        return 0;

    return -1;
}


double thirdControlPointDeviation( const POINTS& p )
{
    VECTOR2D delta = p[1] - p[0];
    double   len_sq = delta.SquaredEuclideanNorm();

    if( len_sq < 1e-6 )
        return 0.0;

    double len = std::sqrt( len_sq );
    double r = ( p[1].y - p[0].y ) / len;
    double s = ( p[0].x - p[1].x ) / len;
    double u = ( p[1].x * p[0].y - p[0].x * p[1].y ) / len;

    return std::abs( r * p[2].x + s * p[2].y + u );
}


int findInflectionPoints( const POINTS& p, double& aT1, double& aT2 )
{
    VECTOR2D A{ ( -p[0].x + 3 * p[1].x - 3 * p[2].x + p[3].x ),
                ( -p[0].y + 3 * p[1].y - 3 * p[2].y + p[3].y ) };
    VECTOR2D B{ ( 3 * p[0].x - 6 * p[1].x + 3 * p[2].x ),
                ( 3 * p[0].y - 6 * p[1].y + 3 * p[2].y ) };
    VECTOR2D C{ ( -3 * p[0].x + 3 * p[1].x ), ( -3 * p[0].y + 3 * p[1].y ) };

    double a = 3 * A.Cross( B );
    double b = 3 * A.Cross( C );
    double c = B.Cross( C );

    double r2 = ( b * b - 4 * a * c );

    aT1 = 0.0;
    aT2 = 0.0;

    if( r2 >= 0.0 && a != 0.0 )
    {
        double r = std::sqrt( r2 );

        double t1 = ( ( -b + r ) / ( 2 * a ) );
        double t2 = ( ( -b - r ) / ( 2 * a ) );

        if( ( t1 > 0.0 && t1 < 1.0 ) && ( t2 > 0.0 && t2 < 1.0 ) )
        {
            if( t1 > t2 )
                std::swap( t1, t2 );

            aT1 = t1;
            aT2 = t2;

            if( t2 - t1 > 0.00001 )
                return 2;
            else
                return 1;
        }
        else if( t1 > 0.0 && t1 < 1.0 )
        {
            aT1 = t1;
            return 1;
        }
        else if( t2 > 0.0 && t2 < 1.0 )
        {
            aT1 = t2;
            return 1;
        }

        return 0;
    }

    return 0;
}


std::size_t recursiveSegmentation( const POINTS& p, double aThreshhold, STEPS& aSteps )
{
    std::vector<POINTS> stack;
    POINTS              left( 4 );
    POINTS              right( 4 );
    std::size_t         pushed = 0;

    stack.push_back( p );

    while( !stack.empty() )
    {
        aSteps.Add( 1 );
        POINTS* bezier = &stack.back();

        if( ( *bezier )[3] == ( *bezier )[0] )
        {
            stack.pop_back();
        }
        else if( isFlat( *bezier, aThreshhold ) )
        {
            ++pushed;
            stack.pop_back();
        }
        else
        {
            subdivide( *bezier, 0.5, left, right );
            *bezier = right;
            stack.push_back( left );
        }
    }

    return pushed;
}


std::size_t cubicParabolicApprox( const POINTS& aCurve, double aMaxError, STEPS& aSteps )
{
    POINTS      c = aCurve;
    POINTS      b1( 4 );
    POINTS      b2( 4 );
    std::size_t pushed = 0;

    for( ;; )
    {
        aSteps.Add( 1 );

        if( isNaN( c ) )
            break;

        if( isFlat( c, aMaxError ) )
        {
            ++pushed;
            break;
        }

        double d = thirdControlPointDeviation( c );

        if( d == 0.0 )
        {
            pushed += recursiveSegmentation( c, aMaxError, aSteps );
            break;
        }

        double t = 2 * std::sqrt( aMaxError / ( 3.0 * d ) );

        if( t > 1.0 )
        {
            pushed += recursiveSegmentation( c, aMaxError, aSteps );
            break;
        }

        subdivide( c, t, b1, b2 );

        if( isFlat( b1, aMaxError ) )
            ++pushed;
        else
            pushed += recursiveSegmentation( b1, aMaxError, aSteps );

        c = b2;
    }

    return pushed;
}


std::size_t getCubicPoly( const POINTS& p, double aMaxError, STEPS& aSteps )
{
    std::size_t pushed = 1;

    if( numberOfInflectionPoints( p ) == 0 )
        return pushed + cubicParabolicApprox( p, aMaxError, aSteps );

    double t1, t2;
    int    numOfIfP = findInflectionPoints( p, t1, t2 );

    if( numOfIfP == 2 )
    {
        POINTS sub1( 4 ), tmp1( 4 ), sub2( 4 ), sub3( 4 );

        subdivide( p, t1, sub1, tmp1 );
        pushed += cubicParabolicApprox( sub1, aMaxError, aSteps );

        numOfIfP = findInflectionPoints( tmp1, t1, t2 );

        if( numOfIfP == 2 || numOfIfP == 1 )
            subdivide( tmp1, t1, sub2, sub3 );
        else
            return pushed + 1;

        pushed += recursiveSegmentation( sub2, aMaxError, aSteps );
        pushed += cubicParabolicApprox( sub3, aMaxError, aSteps );
    }
    else if( numOfIfP == 1 )
    {
        POINTS sub1( 4 ), sub2( 4 );
        subdivide( p, t1, sub1, sub2 );
        pushed += cubicParabolicApprox( sub1, aMaxError, aSteps );
        pushed += cubicParabolicApprox( sub2, aMaxError, aSteps );
    }
    else
    {
        pushed += cubicParabolicApprox( p, aMaxError, aSteps );
    }

    return pushed;
}


double approx_int( double x )
{
    const double d = 0.6744897501960817;
    const double d4 = d * d * d * d;
    return x / ( 1.0 - d + std::pow( d4 + x * x * 0.25, 0.25 ) );
}


// getQuadPoly up to its loop: the undefined conversion of n, then the n iterations.
std::size_t getQuadPoly( const POINTS& p, double aMaxError, STEPS& aSteps )
{
    double ddx = 2 * p[1].x - p[0].x - p[2].x;
    double ddy = 2 * p[1].y - p[0].y - p[2].y;
    double u0 = ( p[1].x - p[0].x ) * ddx + ( p[1].y - p[0].y ) * ddy;
    double u2 = ( p[2].x - p[1].x ) * ddx + ( p[2].y - p[1].y ) * ddy;
    double cross = ( p[2].x - p[0].x ) * ddy - ( p[2].y - p[0].y ) * ddx;
    double x0 = u0 / cross;
    double x2 = u2 / cross;
    double scale = std::abs( cross ) / ( std::hypot( ddx, ddy ) * std::abs( x2 - x0 ) );

    double a0 = approx_int( x0 );
    double a2 = approx_int( x2 );

    double count = std::ceil( 0.5 * std::abs( a2 - a0 ) * std::sqrt( scale / aMaxError ) );

    // [conv.fpint]: the truncated value must be representable; ceil already truncated.
    if( !( count >= double( INT_MIN ) && count <= double( INT_MAX ) ) )
        undefined();

    const int n = static_cast<int>( count );

    if( n > 0 )
        aSteps.Add( uint64_t( n ) );

    return 2 + std::size_t( std::max( n, 0 ) );
}


// GetPoly( std::vector<VECTOR2D>&, double ) for the points GetPoly sees: the points it appends.
std::size_t getPoly( const POINTS& p, double aMaxError )
{
    STEPS steps;

    if( aMaxError <= 0.0 )
        aMaxError = 10.0;

    if( p.size() == 3 )
        return getQuadPoly( p, aMaxError, steps );
    else if( p.size() == 4 )
        return getCubicPoly( p, aMaxError, steps );

    return 0;
}
} // namespace guard


/// Builds the BEZIER_POLY a request names, through the constructor its "form" selects.
BEZIER_POLY readPoly( const JSON& aInput )
{
    const std::string form = readText( aInput, "form" );

    if( form == "points4" )
    {
        const JSON& points = readArray( aInput, "points" );

        if( points.size() != 4 )
            throw std::invalid_argument( "expected four points" );

        return BEZIER_POLY( readPointI( points[0] ), readPointI( points[1] ),
                            readPointI( points[2] ), readPointI( points[3] ) );
    }

    if( form == "vector_i" )
        return BEZIER_POLY( readPointsI( aInput, "points" ) );

    if( form == "vector_d" )
        return BEZIER_POLY( readPointsD( aInput, "points" ) );

    throw std::invalid_argument( "unknown form" );
}


/// A private member's subscripts must stay inside m_ctrlPts.
void requireSize( const BEZIER_POLY& aPoly, std::size_t aSize )
{
    if( ( aPoly.*Get( CtrlPts{} ) ).size() < aSize )
        undefined();
}


/// The guard's count for this poly and error, where GetPoly( VECTOR2D ) reaches its helpers.
std::size_t guardGetPoly( const BEZIER_POLY& aPoly, double aMaxError )
{
    return guard::getPoly( aPoly.*Get( CtrlPts{} ), aMaxError );
}


template <typename T>
VECTOR2<T> readEllipsePoint( const JSON& aValue );

template <>
VECTOR2<int> readEllipsePoint<int>( const JSON& aValue )
{
    return readPointI( aValue );
}

template <>
VECTOR2<double> readEllipsePoint<double>( const JSON& aValue )
{
    return readPointD( aValue );
}


template <typename T>
T readEllipseScalar( const JSON& aValue );

template <>
int readEllipseScalar<int>( const JSON& aValue )
{
    return readInt( aValue );
}

template <>
double readEllipseScalar<double>( const JSON& aValue )
{
    return decodeBits( aValue );
}


template <typename T>
JSON ellipsePoint( const VECTOR2<T>& aValue );

template <>
JSON ellipsePoint<int>( const VECTOR2<int>& aValue )
{
    return pointI( aValue );
}

template <>
JSON ellipsePoint<double>( const VECTOR2<double>& aValue )
{
    return pointD( aValue );
}


// TransformEllipseToBeziers up to its loop, then (for int) the points it converts: the pinned
// expressions, calling the genuine RotatePoint.
template <typename T>
void guardEllipse( const ELLIPSE<T>& aEllipse )
{
    EDA_ANGLE arcAngle = -( aEllipse.EndAngle - aEllipse.StartAngle );

    if( arcAngle >= ANGLE_0 )
        arcAngle -= ANGLE_360;

    const int    minBeziersPerCircle = 4;
    const double count =
            std::ceil( std::abs( arcAngle.AsRadians() / ( 2 * M_PI / minBeziersPerCircle ) ) );

    if( !( count >= double( INT_MIN ) && count <= double( INT_MAX ) ) )
        undefined();

    const int numBeziers = static_cast<int>( count );

    if( numBeziers > MAX_BEZIERS )
        outside();

    if constexpr( std::is_integral_v<T> )
    {
        const double angleIncrement = arcAngle.AsRadians() / numBeziers;
        double       theta = angleIncrement;
        double       k = ( 4. / 3. ) * std::tan( theta / 4 );

        BEZIER<double> first = { { 1, 0 },
                                 { 1, k },
                                 { std::cos( theta ) + k * std::sin( theta ),
                                   std::sin( theta ) - k * std::cos( theta ) },
                                 { std::cos( theta ), std::sin( theta ) } };

        auto transformPoint = [&]( VECTOR2D aPoint, const double aAngle ) -> VECTOR2D
        {
            RotatePoint( aPoint,
                         -EDA_ANGLE( aAngle - aEllipse.StartAngle.AsRadians(), RADIANS_T ) );
            aPoint *= VECTOR2D( aEllipse.MajorRadius, aEllipse.MinorRadius );
            RotatePoint( aPoint, -aEllipse.Rotation );
            aPoint += aEllipse.Center;
            return aPoint;
        };

        for( int i = 0; i < numBeziers; i++ )
        {
            for( const VECTOR2D& control : { first.Start, first.C1, first.C2, first.End } )
            {
                const VECTOR2D point = transformPoint( control, i * angleIncrement );

                // VECTOR2<int>'s casting constructor converts std::clamp( v, INT_MIN, INT_MAX ).
                if( std::isnan( point.x ) || std::isnan( point.y ) )
                    undefined();
            }
        }
    }
}


template <typename T>
JSON ellipseToBeziers( const JSON& aInput )
{
    checkKeys( aInput, { "op", "type", "center", "major", "minor", "rotation", "start", "end",
                         "output" } );

    ELLIPSE<T> ellipse;
    ellipse.Center = readEllipsePoint<T>( aInput.at( "center" ) );
    ellipse.MajorRadius = readEllipseScalar<T>( aInput.at( "major" ) );
    ellipse.MinorRadius = readEllipseScalar<T>( aInput.at( "minor" ) );
    ellipse.Rotation = readAngle( aInput.at( "rotation" ) );
    ellipse.StartAngle = readAngle( aInput.at( "start" ) );
    ellipse.EndAngle = readAngle( aInput.at( "end" ) );

    std::vector<BEZIER<T>> beziers;

    for( const JSON& curve : readArray( aInput, "output" ) )
    {
        if( !curve.is_array() || curve.size() != 4 )
            throw std::invalid_argument( "expected curve" );

        beziers.emplace_back( readEllipsePoint<T>( curve[0] ), readEllipsePoint<T>( curve[1] ),
                              readEllipsePoint<T>( curve[2] ), readEllipsePoint<T>( curve[3] ) );
    }

    guardEllipse( ellipse );
    TransformEllipseToBeziers( ellipse, beziers );

    JSON out = JSON::array();

    for( const BEZIER<T>& curve : beziers )
    {
        out.push_back( JSON::array( { ellipsePoint<T>( curve.Start ), ellipsePoint<T>( curve.C1 ),
                                      ellipsePoint<T>( curve.C2 ), ellipsePoint<T>( curve.End ) } ) );
    }

    return JSON{ { "output", out } };
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "construct" )
    {
        checkKeys( aInput, { "op", "form", "points" } );
        return state( readPoly( aInput ) );
    }

    if( op == "get_poly_i" )
    {
        checkKeys( aInput, { "op", "form", "points", "output" }, { "max_error" } );
        BEZIER_POLY           poly = readPoly( aInput );
        std::vector<VECTOR2I> output = readPointsI( aInput, "output" );
        const bool            defaulted = !aInput.contains( "max_error" );
        const int             maxError = defaulted ? 10 : readInt( aInput.at( "max_error" ) );

        // GetPoly( VECTOR2I ) clears its output and passes its int error to the double overload.
        const std::size_t expected = guardGetPoly( poly, maxError );

        JSON assertions = JSON::array();
        {
            ASSERTION_SCOPE scope( assertions );

            if( defaulted )
                poly.GetPoly( output );
            else
                poly.GetPoly( output, maxError );
        }

        if( output.size() != expected )
            throw std::runtime_error( "bezier guard disagrees with the genuine output size" );

        JSON reply = state( poly );
        reply["output"] = pointsI( output );
        reply["assertions"] = assertions;
        return reply;
    }

    if( op == "get_poly_d" )
    {
        checkKeys( aInput, { "op", "form", "points", "output" }, { "max_error" } );
        BEZIER_POLY           poly = readPoly( aInput );
        std::vector<VECTOR2D> output = readPointsD( aInput, "output" );
        const bool            defaulted = !aInput.contains( "max_error" );
        const double          maxError = defaulted ? 10.0 : decodeBits( aInput.at( "max_error" ) );
        const std::size_t     prior = output.size();
        const std::size_t     expected = guardGetPoly( poly, maxError );

        JSON assertions = JSON::array();
        {
            ASSERTION_SCOPE scope( assertions );

            if( defaulted )
                poly.GetPoly( output );
            else
                poly.GetPoly( output, maxError );
        }

        if( output.size() != prior + expected )
            throw std::runtime_error( "bezier guard disagrees with the genuine output size" );

        JSON reply = state( poly );
        reply["output"] = pointsD( output );
        reply["assertions"] = assertions;
        return reply;
    }

    if( op == "is_nan" )
    {
        checkKeys( aInput, { "op", "points" } );
        BEZIER_POLY poly( readPointsD( aInput, "points" ) );
        const bool  value = ( poly.*Get( IsNaN{} ) )();
        JSON        reply = state( poly );
        reply["value"] = value;
        return reply;
    }

    if( op == "is_flat" )
    {
        checkKeys( aInput, { "op", "points", "max_error" } );
        BEZIER_POLY                  poly( readPointsD( aInput, "points" ) );
        const double                 maxError = decodeBits( aInput.at( "max_error" ) );
        const std::vector<VECTOR2D>& p = poly.*Get( CtrlPts{} );

        if( p.size() == 3 )
        {
            // The parameter std::max and std::min receive.
            VECTOR2D D21 = p[1] - p[0];
            VECTOR2D D31 = p[2] - p[0];

            if( std::isnan( D21.Dot( D31 ) / D31.SquaredEuclideanNorm() ) )
                undefined();
        }

        JSON assertions = JSON::array();
        bool value = false;
        {
            ASSERTION_SCOPE scope( assertions );
            value = ( poly.*Get( IsFlat{} ) )( maxError );
        }

        JSON reply = state( poly );
        reply["value"] = value;
        reply["assertions"] = assertions;
        return reply;
    }

    if( op == "eval" )
    {
        checkKeys( aInput, { "op", "points", "t" } );
        BEZIER_POLY  poly( readPointsD( aInput, "points" ) );
        const double t = decodeBits( aInput.at( "t" ) );
        JSON         assertions = JSON::array();
        VECTOR2D     value;
        {
            ASSERTION_SCOPE scope( assertions );
            value = ( poly.*Get( Eval{} ) )( t );
        }

        JSON reply = state( poly );
        reply["value"] = pointD( value );
        reply["assertions"] = assertions;
        return reply;
    }

    if( op == "subdivide" )
    {
        checkKeys( aInput, { "op", "points", "t", "left", "right" } );
        BEZIER_POLY  poly( readPointsD( aInput, "points" ) );
        BEZIER_POLY  left( readPointsD( aInput, "left" ) );
        BEZIER_POLY  right( readPointsD( aInput, "right" ) );
        const double t = decodeBits( aInput.at( "t" ) );
        const std::size_t size = ( poly.*Get( CtrlPts{} ) ).size();

        if( size == 3 || size == 4 )
        {
            requireSize( left, size );
            requireSize( right, size );
        }

        JSON assertions = JSON::array();
        {
            ASSERTION_SCOPE scope( assertions );
            ( poly.*Get( Subdivide{} ) )( t, left, right );
        }

        JSON reply = state( poly );
        reply["left"] = pointsD( left.*Get( CtrlPts{} ) );
        reply["right"] = pointsD( right.*Get( CtrlPts{} ) );
        reply["assertions"] = assertions;
        return reply;
    }

    if( op == "number_of_inflection_points" )
    {
        checkKeys( aInput, { "op", "points" } );
        BEZIER_POLY poly( readPointsD( aInput, "points" ) );
        requireSize( poly, 4 );
        const int value = ( poly.*Get( NumberOfInflectionPoints{} ) )();
        JSON      reply = state( poly );
        reply["value"] = value;
        return reply;
    }

    if( op == "find_inflection_points" )
    {
        checkKeys( aInput, { "op", "points", "t1", "t2" } );
        BEZIER_POLY poly( readPointsD( aInput, "points" ) );
        double      t1 = decodeBits( aInput.at( "t1" ) );
        double      t2 = decodeBits( aInput.at( "t2" ) );
        requireSize( poly, 4 );
        const int value = ( poly.*Get( FindInflectionPoints{} ) )( t1, t2 );
        JSON      reply = state( poly );
        reply["value"] = value;
        reply["t1"] = bits( t1 );
        reply["t2"] = bits( t2 );
        return reply;
    }

    if( op == "third_control_point_deviation" )
    {
        checkKeys( aInput, { "op", "points" } );
        BEZIER_POLY poly( readPointsD( aInput, "points" ) );
        requireSize( poly, 3 );
        const double value = ( poly.*Get( ThirdControlPointDeviation{} ) )();
        JSON         reply = state( poly );
        reply["value"] = bits( value );
        return reply;
    }

    if( op == "ellipse_to_beziers" )
    {
        const std::string type = readText( aInput, "type" );

        if( type == "double" )
            return ellipseToBeziers<double>( aInput );
        if( type == "int" )
            return ellipseToBeziers<int>( aInput );
        throw std::invalid_argument( "unknown type" );
    }

    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterBezierCurves( REGISTRY& aRegistry )
{
    aRegistry.Add( BEZIER_UNIT, "bezier_curves.cpp", run );
}
} // namespace oracle
