// SPDX-License-Identifier: GPL-3.0-or-later
// Calls the real pinned kimath util.cpp. No result arithmetic lives in this adapter.
//
// The two logging functions are called for real and their wx host calls are observed:
// - kimathLogOverflow's wxFAIL_MSG: kimath_log.h's LOG_SCOPE records each wxOnAssert call
//   from util.cpp and calls the process's own handler, which prints and returns.
// - kimathLogDebug's level query and wxVLogWarning: the request sets the global log level and
//   wxLog::EnableLogging for the call (both restored afterwards), and a recording log target,
//   active for the call only, records each record that reaches it (level, formatted message,
//   source line, function and component), then passes it to the previous target.
// The variadic call is built from typed arguments ("lld": long long, "f": binary64 bits) over
// a format in a closed grammar (printable ASCII and newline text, "%%", "%lld", "%f"); other
// formats cannot be built and are malformed requests. A call that would format with a missing
// or mistyped argument is undefined and refused before the call; when the level queries
// disable the message the arguments are never read and the call is made.
#include "kimath_log.h"
#include "registry.h"
#include <math/util.h>
#include <wx/log.h>
#include <algorithm>
#include <bit>
#include <climits>
#include <cstdint>
#include <initializer_list>
#include <limits>
#include <string>
#include <variant>
#include <vector>

#if !defined( __SIZEOF_INT128__ ) || defined( _MSC_VER )
#error "math util oracle requires the Linux int128 branch"
#endif
static_assert( sizeof( int ) == 4 && sizeof( int64_t ) == 8 && CHAR_BIT == 8 );

namespace
{
using oracle::JSON;
constexpr const char* MATH_UTIL_UNIT = "libs.kimath.src.math.util.application.ui.p001";

void Keys( const JSON& aInput, std::initializer_list<const char*> aFields )
{
    if( !aInput.is_object() )
        throw std::invalid_argument( "expected object" );

    if( aInput.size() != aFields.size() )
        throw std::invalid_argument( "unexpected or missing field" );

    for( const char* field : aFields )
    {
        if( !aInput.contains( field ) )
            throw std::invalid_argument( "unexpected or missing field" );
    }
}

std::string Text( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + aName );

    return aInput.at( aName ).get<std::string>();
}

int64_t Signed( const JSON& aInput, const char* aName, int64_t aMin, int64_t aMax )
{
    const JSON& value = aInput.at( aName );

    if( !value.is_number_integer()
        || ( value.is_number_unsigned() && value.get<uint64_t>() > uint64_t( aMax ) ) )
    {
        throw std::invalid_argument( std::string( "integer out of range: " ) + aName );
    }

    const int64_t number = value.get<int64_t>();

    if( number < aMin || number > aMax )
        throw std::invalid_argument( std::string( "integer out of range: " ) + aName );

    return number;
}

uint64_t Unsigned( const JSON& aInput, const char* aName )
{
    const JSON& value = aInput.at( aName );

    if( !value.is_number_integer() || ( !value.is_number_unsigned() && value.get<int64_t>() < 0 ) )
        throw std::invalid_argument( std::string( "integer out of range: " ) + aName );

    return value.get<uint64_t>();
}

enum class FORMAT_PIECE
{
    TEXT,
    PERCENT,
    LONG_LONG,
    DOUBLE
};


/// The closed format grammar: printable ASCII and newline text, "%%", "%lld" and "%f".
std::vector<FORMAT_PIECE> ParseFormat( const std::string& aFormat )
{
    std::vector<FORMAT_PIECE> pieces;

    for( size_t i = 0; i < aFormat.size(); )
    {
        const unsigned char c = static_cast<unsigned char>( aFormat[i] );

        if( c != '%' )
        {
            if( ( c < 0x20 || c > 0x7e ) && c != '\n' )
                throw std::invalid_argument( "unsupported format" );

            pieces.push_back( FORMAT_PIECE::TEXT );
            i++;
        }
        else if( aFormat.compare( i, 2, "%%" ) == 0 )
        {
            pieces.push_back( FORMAT_PIECE::PERCENT );
            i += 2;
        }
        else if( aFormat.compare( i, 4, "%lld" ) == 0 )
        {
            pieces.push_back( FORMAT_PIECE::LONG_LONG );
            i += 4;
        }
        else if( aFormat.compare( i, 2, "%f" ) == 0 )
        {
            pieces.push_back( FORMAT_PIECE::DOUBLE );
            i += 2;
        }
        else
        {
            throw std::invalid_argument( "unsupported format" );
        }
    }

    return pieces;
}


using LOG_ARG = std::variant<long long, double>;
constexpr size_t MAX_LOG_ARGS = 4;


/// "args": up to four {"lld": integer} or {"f": 16 lowercase hex digits} objects.
std::vector<LOG_ARG> LogArgs( const JSON& aInput )
{
    const JSON& list = aInput.at( "args" );

    if( !list.is_array() || list.size() > MAX_LOG_ARGS )
        throw std::invalid_argument( "invalid args" );

    std::vector<LOG_ARG> args;

    for( const JSON& arg : list )
    {
        if( arg.is_object() && arg.size() == 1 && arg.contains( "lld" ) )
        {
            args.emplace_back( static_cast<long long>( Signed( arg, "lld", INT64_MIN, INT64_MAX ) ) );
        }
        else if( arg.is_object() && arg.size() == 1 && arg.contains( "f" ) )
        {
            const std::string bits = Text( arg, "f" );

            if( bits.size() != 16 || bits.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
                throw std::invalid_argument( "invalid hex: f" );

            args.emplace_back( std::bit_cast<double>( std::stoull( bits, nullptr, 16 ) ) );
        }
        else
        {
            throw std::invalid_argument( "invalid args" );
        }
    }

    return args;
}


/// Every conversion has an argument of its type; excess arguments are ignored, as C specifies.
bool ArgsMatch( const std::vector<FORMAT_PIECE>& aPieces, const std::vector<LOG_ARG>& aArgs )
{
    size_t next = 0;

    for( FORMAT_PIECE piece : aPieces )
    {
        if( piece == FORMAT_PIECE::LONG_LONG || piece == FORMAT_PIECE::DOUBLE )
        {
            if( next >= aArgs.size()
                || std::holds_alternative<long long>( aArgs[next] )
                           != ( piece == FORMAT_PIECE::LONG_LONG ) )
            {
                return false;
            }

            next++;
        }
    }

    return true;
}


/// Calls kimathLogDebug( aFormat, args... ) with the typed arguments in order.
template <typename... BOUND>
void CallLogDebug( const char* aFormat, const std::vector<LOG_ARG>& aArgs, size_t aIndex,
                   BOUND... aBound )
{
    if( aIndex == aArgs.size() )
    {
        kimathLogDebug( aFormat, aBound... );
        return;
    }

    if constexpr( sizeof...( BOUND ) < MAX_LOG_ARGS )
    {
        if( const long long* value = std::get_if<long long>( &aArgs[aIndex] ) )
            CallLogDebug( aFormat, aArgs, aIndex + 1, aBound..., *value );
        else
            CallLogDebug( aFormat, aArgs, aIndex + 1, aBound..., std::get<double>( aArgs[aIndex] ) );
    }
    else
    {
        throw std::logic_error( "too many log arguments" );
    }
}


/// Sets the global log level and wxLog::EnableLogging for one call, then restores both.
class LOG_LEVEL_SCOPE
{
public:
    LOG_LEVEL_SCOPE( wxLogLevel aLevel, bool aLogging ) :
            m_level( wxLog::GetLogLevel() ), m_logging( wxLog::EnableLogging( aLogging ) )
    {
        wxLog::SetLogLevel( aLevel );
    }

    ~LOG_LEVEL_SCOPE()
    {
        wxLog::SetLogLevel( m_level );
        wxLog::EnableLogging( m_logging );
    }

    LOG_LEVEL_SCOPE( const LOG_LEVEL_SCOPE& ) = delete;
    LOG_LEVEL_SCOPE& operator=( const LOG_LEVEL_SCOPE& ) = delete;

private:
    wxLogLevel m_level;
    bool       m_logging;
};


/// The active log target for one call: records each record, then passes it to the target that
/// was active (or would have been created) before, which is restored afterwards.
class RECORDING_TARGET : public wxLog
{
public:
    explicit RECORDING_TARGET( JSON& aEvents ) :
            m_events( aEvents ), m_previous( wxLog::GetActiveTarget() )
    {
        wxLog::SetActiveTarget( this );
    }

    ~RECORDING_TARGET() override { wxLog::SetActiveTarget( m_previous ); }

    RECORDING_TARGET( const RECORDING_TARGET& ) = delete;
    RECORDING_TARGET& operator=( const RECORDING_TARGET& ) = delete;

protected:
    void DoLogRecord( wxLogLevel aLevel, const wxString& aMsg,
                      const wxLogRecordInfo& aInfo ) override
    {
        m_events.push_back( JSON{ { "call", "wxLog" },
                                  { "level", aLevel },
                                  { "message", aMsg.utf8_string() },
                                  { "line", aInfo.line },
                                  { "function", aInfo.func ? aInfo.func : "" },
                                  { "component", aInfo.component ? aInfo.component : "" } } );

        if( m_previous )
            m_previous->LogRecord( aLevel, aMsg, aInfo );
    }

private:
    JSON&  m_events;
    wxLog* m_previous;
};


JSON Run( const JSON& aInput )
{
    const std::string op = Text( aInput, "op" );

    if( op == "hypot_wide" )
    {
        Keys( aInput, { "op", "x", "y" } );
        const uint64_t x = Unsigned( aInput, "x" );
        const uint64_t y = Unsigned( aInput, "y" );
        return JSON{ { "value", RoundedHypotWide( x, y ) } };
    }

    if( op == "rescale_i32" || op == "rescale_i64_int128" )
    {
        Keys( aInput, { "op", "numerator", "value", "denominator" } );
        const bool narrow = op == "rescale_i32";
        const int64_t min = narrow ? std::numeric_limits<int>::min() : INT64_MIN;
        const int64_t max = narrow ? std::numeric_limits<int>::max() : INT64_MAX;
        const int64_t n = Signed( aInput, "numerator", min, max );
        const int64_t v = Signed( aInput, "value", min, max );
        const int64_t d = Signed( aInput, "denominator", min, max );

        // The products and rounding adjustment fit their native intermediate widths.
        // Division by zero is the only undefined arithmetic evaluation on this branch.
        if( d == 0 )
            throw std::invalid_argument( "zero denominator" );

        if( narrow )
            return JSON{ { "value", rescale<int>( int( n ), int( v ), int( d ) ) } };

        return JSON{ { "value", rescale<int64_t>( n, v, d ) } };
    }

    if( op == "log_debug" )
    {
        Keys( aInput, { "op", "format", "args", "log_level", "logging" } );
        const std::string format = Text( aInput, "format" );
        const std::vector<FORMAT_PIECE> pieces = ParseFormat( format );
        const std::vector<LOG_ARG> args = LogArgs( aInput );
        const int64_t level = Signed( aInput, "log_level", 0, wxLOG_Max );

        if( !aInput.at( "logging" ).is_boolean() )
            throw std::invalid_argument( "expected boolean: logging" );

        const bool logging = aInput.at( "logging" ).get<bool>();

        // Formatting happens only past both level queries (wxLOG_Debug, then wxLOG_Warning).
        if( logging && level >= wxLOG_Debug && !ArgsMatch( pieces, args ) )
            throw std::invalid_argument( "undefined format arguments" );

        JSON events = JSON::array();
        {
            LOG_LEVEL_SCOPE  levels( static_cast<wxLogLevel>( level ), logging );
            RECORDING_TARGET target( events );
            CallLogDebug( format.c_str(), args, 0 );
        }
        return JSON{ { "kimath_log", events } };
    }

    if( op == "log_overflow" )
    {
        Keys( aInput, { "op", "bits", "type_name" } );
        const std::string bits = Text( aInput, "bits" );

        if( bits.size() != 16 || bits.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
            throw std::invalid_argument( "invalid hex: bits" );

        const std::string typeName = Text( aInput, "type_name" );
        const double      value = std::bit_cast<double>( std::stoull( bits, nullptr, 16 ) );
        JSON              events = JSON::array();
        {
            oracle::kimath_log::LOG_SCOPE scope( events );
            // The const char* stops at the first NUL.
            kimathLogOverflow( value, typeName.c_str() );
        }
        return JSON{ { "kimath_log", events } };
    }

    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterMathUtil( REGISTRY& aRegistry )
{
    aRegistry.Add( MATH_UTIL_UNIT, "libs/kimath/src/math/util.cpp", Run );
}
} // namespace oracle
