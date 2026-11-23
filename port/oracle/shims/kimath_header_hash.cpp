// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for unit libs.kimath.include.application.pure.p001: the pinned
// header-only hash.h, hash_128.h, mmh3_hash.h, md5_hash.h validity accessors and
// TRANSFORM constructors/inequality. Only input validation, conversion, calls into
// the pinned headers/library and serialization live here; no algorithm is copied.
// The operation schema is documented in rust/kicad-hash-shim/README.md.
#include "registry.h"
#include <hash.h>
#include <hash_128.h>
#include <md5_hash.h>
#include <mmh3_hash.h>
#include <transform.h>

#include <algorithm>
#include <array>
#include <bit>
#include <climits>
#include <cstring>
#include <initializer_list>
#include <locale>
#include <memory>
#include <optional>
#include <variant>
#include <vector>

static_assert( sizeof( std::size_t ) == 8 && sizeof( int ) == 4 && CHAR_BIT == 8
                       && std::endian::native == std::endian::little,
               "kimath header hash cases require the pinned 64-bit little-endian Linux contract" );

namespace
{
// Standard C++ explicit-instantiation test access ([temp.explicit] exempts its
// arguments from access checking). The pinned headers remain unmodified; these
// pointers reach the actual private members and helpers, never shim algorithms.
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

using BLOCKS = uint32_t[4];
MEMBER( H1, uint64_t MMH3_HASH::*, &MMH3_HASH::h1 )
MEMBER( H2, uint64_t MMH3_HASH::*, &MMH3_HASH::h2 )
MEMBER( Len, uint32_t MMH3_HASH::*, &MMH3_HASH::len )
MEMBER( Blocks, BLOCKS MMH3_HASH::*, &MMH3_HASH::blocks )
MEMBER( HashBlock, void ( MMH3_HASH::* )(), &MMH3_HASH::hashBlock )
MEMBER( HashTail, void ( MMH3_HASH::* )(), &MMH3_HASH::hashTail )
MEMBER( HashFinal, void ( MMH3_HASH::* )( HASH_128& ), &MMH3_HASH::hashFinal )
MEMBER( GetBlock64, uint64_t ( MMH3_HASH::* )( int ), &MMH3_HASH::getblock64 )
MEMBER( Fmix64, uint64_t ( * )( uint64_t ), &MMH3_HASH::fmix64 )
#undef MEMBER

using oracle::JSON;

constexpr std::size_t MAX_STEPS = 4096;
constexpr uint64_t    MAX_MMH3_BYTES = 16ULL * 1024 * 1024;
constexpr uint64_t    MAX_MD5_BYTES = 1024ULL * 1024;

using VALUE = std::variant<bool, int8_t, uint8_t, int16_t, uint16_t, int32_t, uint32_t, int64_t,
                           uint64_t, float, double, std::string>;
// Three-argument calls are instantiated for a representative subset only, to keep
// the number of template instantiations bounded.
using VALUE3 = std::variant<bool, int32_t, uint64_t, double, std::string>;


void Keys( const JSON& aValue, std::initializer_list<const char*> aRequired,
           std::initializer_list<const char*> aOptional = {} )
{
    if( !aValue.is_object() )
        throw std::invalid_argument( "expected object" );

    for( const char* key : aRequired )
    {
        if( !aValue.contains( key ) )
            throw std::invalid_argument( "unexpected or missing field" );
    }

    for( const auto& [key, value] : aValue.items() )
    {
        if( std::find_if( aRequired.begin(), aRequired.end(),
                          [&]( const char* k ) { return key == k; } )
                    == aRequired.end()
            && std::find_if( aOptional.begin(), aOptional.end(),
                             [&]( const char* k ) { return key == k; } )
                       == aOptional.end() )
        {
            throw std::invalid_argument( "unexpected or missing field" );
        }
    }
}


// A JSON integer within [aMin, aMax]; floats, booleans and strings are refused.
int64_t Signed( const JSON& aObject, const std::string& aName, int64_t aMin, int64_t aMax )
{
    const JSON& value = aObject.at( aName );
    bool        ok = value.is_number_integer();
    int64_t     number = 0;

    if( ok && value.is_number_unsigned() )
    {
        const uint64_t unsignedNumber = value.get<uint64_t>();
        ok = aMax >= 0 && unsignedNumber <= static_cast<uint64_t>( aMax );
        number = ok ? static_cast<int64_t>( unsignedNumber ) : 0;
    }
    else if( ok )
    {
        number = value.get<int64_t>();
    }

    if( !ok || number < aMin || number > aMax )
        throw std::invalid_argument( "integer out of range: " + aName );

    return number;
}


uint64_t Unsigned( const JSON& aObject, const std::string& aName, uint64_t aMax )
{
    const JSON& value = aObject.at( aName );
    bool        ok = value.is_number_integer();
    uint64_t    number = 0;

    if( ok && value.is_number_unsigned() )
    {
        number = value.get<uint64_t>();
    }
    else if( ok )
    {
        const int64_t signedNumber = value.get<int64_t>();
        ok = signedNumber >= 0;
        number = ok ? static_cast<uint64_t>( signedNumber ) : 0;
    }

    if( !ok || number > aMax )
        throw std::invalid_argument( "integer out of range: " + aName );

    return number;
}


bool Boolean( const JSON& aObject, const std::string& aName )
{
    const JSON& value = aObject.at( aName );

    if( !value.is_boolean() )
        throw std::invalid_argument( "expected boolean: " + aName );

    return value.get<bool>();
}


// Lower-case hexadecimal of even length, at most aMaxBytes bytes (or exactly aExact).
std::vector<uint8_t> Hex( const JSON& aObject, const std::string& aName, std::size_t aMaxBytes,
                          bool aExact = false )
{
    const JSON& value = aObject.at( aName );

    if( !value.is_string() )
        throw std::invalid_argument( "invalid hex: " + aName );

    const std::string& text = value.get_ref<const std::string&>();

    if( text.size() % 2 != 0 || text.size() / 2 > aMaxBytes
        || ( aExact && text.size() / 2 != aMaxBytes ) )
    {
        throw std::invalid_argument( "invalid hex: " + aName );
    }

    auto nibble = [&]( char c ) -> uint8_t
    {
        if( c >= '0' && c <= '9' )
            return static_cast<uint8_t>( c - '0' );

        if( c >= 'a' && c <= 'f' )
            return static_cast<uint8_t>( c - 'a' + 10 );

        throw std::invalid_argument( "invalid hex: " + aName );
    };

    std::vector<uint8_t> bytes;
    bytes.reserve( text.size() / 2 );

    for( std::size_t i = 0; i < text.size(); i += 2 )
        bytes.push_back( static_cast<uint8_t>( nibble( text[i] ) << 4 | nibble( text[i + 1] ) ) );

    return bytes;
}


std::string ToHex( const uint8_t* aData, std::size_t aSize )
{
    static const char digits[] = "0123456789abcdef";
    std::string       text;

    for( std::size_t i = 0; i < aSize; ++i )
    {
        text.push_back( digits[aData[i] >> 4] );
        text.push_back( digits[aData[i] & 15] );
    }

    return text;
}


VALUE Value( const JSON& aValue )
{
    if( !aValue.is_object() || !aValue.contains( "type" ) || !aValue.at( "type" ).is_string() )
        throw std::invalid_argument( "invalid value type" );

    const std::string type = aValue.at( "type" ).get<std::string>();

    if( type == "bool" )
    {
        Keys( aValue, { "type", "value" } );
        return Boolean( aValue, "value" );
    }

    if( type == "f32" || type == "f64" )
    {
        Keys( aValue, { "type", "bits" } );
        const std::vector<uint8_t> bytes = Hex( aValue, "bits", type == "f32" ? 4 : 8, true );

        // The text is big-endian (most significant nibble first), like the Rust shim.
        uint64_t bits = 0;

        for( uint8_t byte : bytes )
            bits = bits << 8 | byte;

        if( type == "f32" )
            return std::bit_cast<float>( static_cast<uint32_t>( bits ) );

        return std::bit_cast<double>( bits );
    }

    if( type == "string" )
    {
        Keys( aValue, { "type", "hex" } );
        const std::vector<uint8_t> bytes = Hex( aValue, "hex", 4096 );
        return std::string( bytes.begin(), bytes.end() );
    }

    auto integer = [&]( int64_t aMin, int64_t aMax )
    {
        Keys( aValue, { "type", "value" } );
        return Signed( aValue, "value", aMin, aMax );
    };

    if( type == "i8" )
        return static_cast<int8_t>( integer( INT8_MIN, INT8_MAX ) );
    if( type == "u8" )
        return static_cast<uint8_t>( integer( 0, UINT8_MAX ) );
    if( type == "i16" )
        return static_cast<int16_t>( integer( INT16_MIN, INT16_MAX ) );
    if( type == "u16" )
        return static_cast<uint16_t>( integer( 0, UINT16_MAX ) );
    if( type == "i32" )
        return static_cast<int32_t>( integer( INT32_MIN, INT32_MAX ) );
    if( type == "u32" )
        return static_cast<uint32_t>( integer( 0, UINT32_MAX ) );
    if( type == "i64" )
        return static_cast<int64_t>( integer( INT64_MIN, INT64_MAX ) );
    if( type == "u64" )
    {
        Keys( aValue, { "type", "value" } );
        return Unsigned( aValue, "value", UINT64_MAX );
    }

    throw std::invalid_argument( "invalid value type" );
}


std::vector<VALUE> Values( const JSON& aInput, std::size_t aMax )
{
    const JSON& values = aInput.at( "values" );

    if( !values.is_array() )
        throw std::invalid_argument( "expected values array" );

    if( values.size() > aMax )
        throw std::invalid_argument( "too many values" );

    std::vector<VALUE> result;

    for( const JSON& value : values )
        result.push_back( Value( value ) );

    return result;
}


VALUE3 Narrow( const VALUE& aValue )
{
    return std::visit(
            []( const auto& v ) -> VALUE3
            {
                using T = std::decay_t<decltype( v )>;

                if constexpr( std::is_same_v<T, bool> || std::is_same_v<T, int32_t>
                              || std::is_same_v<T, uint64_t> || std::is_same_v<T, double>
                              || std::is_same_v<T, std::string> )
                {
                    return v;
                }
                else
                {
                    throw std::invalid_argument( "unsupported value type for three values" );
                }
            },
            aValue );
}


// Calls the pinned variadic template with exactly the requested argument count.
void Combine( std::size_t& aSeed, const std::vector<VALUE>& aValues )
{
    switch( aValues.size() )
    {
    case 0: hash_combine( aSeed ); break;
    case 1: std::visit( [&]( const auto& a ) { hash_combine( aSeed, a ); }, aValues[0] ); break;
    case 2:
        std::visit( [&]( const auto& a, const auto& b ) { hash_combine( aSeed, a, b ); },
                    aValues[0], aValues[1] );
        break;
    default:
    {
        const VALUE3 a = Narrow( aValues[0] );
        const VALUE3 b = Narrow( aValues[1] );
        const VALUE3 c = Narrow( aValues[2] );
        std::visit( [&]( const auto& x, const auto& y, const auto& z )
                    { hash_combine( aSeed, x, y, z ); },
                    a, b, c );
    }
    }
}


std::size_t Val( const std::vector<VALUE>& aValues )
{
    switch( aValues.size() )
    {
    case 0: return hash_val();
    case 1: return std::visit( []( const auto& a ) { return hash_val( a ); }, aValues[0] );
    case 2:
        return std::visit( []( const auto& a, const auto& b ) { return hash_val( a, b ); },
                           aValues[0], aValues[1] );
    default:
    {
        const VALUE3 a = Narrow( aValues[0] );
        const VALUE3 b = Narrow( aValues[1] );
        const VALUE3 c = Narrow( aValues[2] );
        return std::visit( []( const auto& x, const auto& y, const auto& z )
                           { return hash_val( x, y, z ); },
                           a, b, c );
    }
    }
}


JSON StdHashOp( const JSON& aInput )
{
    Keys( aInput, { "op", "values" } );
    JSON hashes = JSON::array();

    for( const VALUE& value : Values( aInput, 64 ) )
    {
        hashes.push_back( std::visit(
                []( const auto& v ) -> std::size_t
                { return std::hash<std::decay_t<decltype( v )>>()( v ); },
                value ) );
    }

    return { { "hashes", std::move( hashes ) } };
}


JSON HashCombineOp( const JSON& aInput )
{
    Keys( aInput, { "op", "seed", "values" } );
    std::size_t              seed = Unsigned( aInput, "seed", UINT64_MAX );
    const std::vector<VALUE> values = Values( aInput, 3 );

    if( values.size() == 3 )
    {
        for( const VALUE& value : values )
            Narrow( value );
    }

    Combine( seed, values );
    return { { "seed", seed } };
}


JSON HashValOp( const JSON& aInput )
{
    Keys( aInput, { "op", "values" } );
    const std::vector<VALUE> values = Values( aInput, 3 );

    if( values.size() == 3 )
    {
        for( const VALUE& value : values )
            Narrow( value );
    }

    return { { "hash", Val( values ) } };
}


// Repeated single-value calls, as KiCad's call sites issue them.
JSON HashCombineEachOp( const JSON& aInput )
{
    Keys( aInput, { "op", "seed", "values" } );
    std::size_t seed = Unsigned( aInput, "seed", UINT64_MAX );
    JSON        seeds = JSON::array();

    for( const VALUE& value : Values( aInput, 256 ) )
    {
        Combine( seed, { value } );
        seeds.push_back( seed );
    }

    return { { "seeds", std::move( seeds ) } };
}


JSON View( const HASH_128& aHash )
{
    uint8_t bytes[16];
    std::memcpy( bytes, &aHash, sizeof( bytes ) );
    uint32_t words[4];
    std::memcpy( words, &aHash, sizeof( words ) );

    return { { "string", aHash.ToString() },
             { "value8", ToHex( bytes, 16 ) },
             { "value32", { words[0], words[1], words[2], words[3] } },
             { "value64", { aHash.Value64[0], aHash.Value64[1] } } };
}


// A global C++ locale whose numpunct<char> has the caller's thousands separator and
// grouping; every other facet is classic.
class GROUPING_FACET : public std::numpunct<char>
{
public:
    GROUPING_FACET( char aSep, std::string aGrouping ) :
            m_sep( aSep ), m_grouping( std::move( aGrouping ) )
    {}

protected:
    char        do_thousands_sep() const override { return m_sep; }
    std::string do_grouping() const override { return m_grouping; }

private:
    char        m_sep;
    std::string m_grouping;
};


// Installs a global locale for the reference's own std::stringstream and restores
// the previous one, including when a call throws.
class GLOBAL_LOCALE
{
public:
    explicit GLOBAL_LOCALE( const std::locale& aLocale ) : m_previous( std::locale::global( aLocale ) )
    {}

    ~GLOBAL_LOCALE() { std::locale::global( m_previous ); }

    GLOBAL_LOCALE( const GLOBAL_LOCALE& ) = delete;
    GLOBAL_LOCALE& operator=( const GLOBAL_LOCALE& ) = delete;

private:
    std::locale m_previous;
};


constexpr std::size_t MAX_GROUPING_BYTES = 64;


std::optional<std::locale> Locale( const JSON& aInput )
{
    if( !aInput.contains( "locale" ) )
        return std::nullopt;

    const JSON& locale = aInput.at( "locale" );
    Keys( locale, { "thousands_sep", "grouping" } );
    const char sep = static_cast<char>( Unsigned( locale, "thousands_sep", 255 ) );
    const std::vector<uint8_t> grouping = Hex( locale, "grouping", 4096 );

    if( grouping.size() > MAX_GROUPING_BYTES )
    {
        throw std::invalid_argument( "grouping of " + std::to_string( grouping.size() )
                                     + " bytes exceeds " + std::to_string( MAX_GROUPING_BYTES ) );
    }

    return std::locale( std::locale::classic(),
                        new GROUPING_FACET( sep, std::string( grouping.begin(), grouping.end() ) ) );
}


// HASH_128::ToString under the caller's global locale, as bytes (the separator need
// not be ASCII).
std::string Grouped( const HASH_128& aHash, const std::locale& aLocale )
{
    std::string text;
    {
        GLOBAL_LOCALE guard( aLocale );
        text = aHash.ToString();
    }
    return ToHex( reinterpret_cast<const uint8_t*>( text.data() ), text.size() );
}


JSON Hash128Op( const JSON& aInput )
{
    Keys( aInput, { "op", "a", "b" }, { "locale" } );
    const std::vector<uint8_t> aBytes = Hex( aInput, "a", 16, true );
    const std::vector<uint8_t> bBytes = Hex( aInput, "b", 16, true );
    const std::optional<std::locale> locale = Locale( aInput );

    const HASH_128 fresh{};
    HASH_128       a;
    HASH_128       b;
    std::memcpy( a.Value64, aBytes.data(), 16 );
    std::memcpy( b.Value64, bBytes.data(), 16 );

    JSON result = { { "new", View( fresh ) },
                    { "a", View( a ) },
                    { "b", View( b ) },
                    { "equal", a == b },
                    { "reverse_equal", b == a },
                    { "self_equal", a == a } };
    JSON grouped = JSON::object();

    if( locale )
    {
        grouped["new"] = Grouped( fresh, *locale );
        grouped["a"] = Grouped( a, *locale );
        grouped["b"] = Grouped( b, *locale );
    }

    a.Clear();
    result["cleared"] = { { "view", View( a ) },
                          { "equals_new", a == fresh },
                          { "equals_b", a == b } };

    if( locale )
    {
        grouped["cleared"] = Grouped( a, *locale );
        result["grouped"] = std::move( grouped );
    }

    return result;
}


// The reference constructors and reset() never initialize MMH3_HASH::blocks. The adapter
// records which block bytes the pinned members have written (mmh3_hash.h:74-155, as
// read from the source) so that it never executes a member that would read an unwritten
// byte, and never reads one itself. This bookkeeping computes no hash value and does
// not touch the object.
constexpr uint16_t ALL_BYTES = 0xFFFF;


uint16_t Span( std::size_t aStart, std::size_t aEnd )
{
    uint16_t mask = 0;

    for( std::size_t byte = aStart; byte < aEnd && byte < 16; ++byte )
        mask |= static_cast<uint16_t>( 1U << byte );

    return mask;
}


void RequireWritten( uint16_t aWritten, const char* aReader, uint16_t aRead )
{
    const uint16_t unwritten = aRead & static_cast<uint16_t>( ~aWritten );

    if( !unwritten )
        return;

    std::string message = std::string( aReader ) + " reads unwritten block bytes";
    const char* separator = " ";

    for( int byte = 0; byte < 16; ++byte )
    {
        if( unwritten & ( 1U << byte ) )
        {
            message += separator + std::to_string( byte );
            separator = ",";
        }
    }

    throw std::invalid_argument( message );
}


// addData( data, length ): refuse when completing a pending block would hash unwritten
// bytes; otherwise return the written set after the call.
uint16_t AddDataWrites( uint16_t aWritten, uint32_t aLen, std::size_t aLength )
{
    std::size_t remaining = aLength;
    const std::size_t pending = aLen % 16;

    if( pending > 0 && remaining > 0 )
    {
        const std::size_t fill = std::min<std::size_t>( remaining, 16 - pending );
        aWritten |= Span( pending, pending + fill );

        if( static_cast<uint32_t>( aLen + fill ) % 16 != 0 )
            return aWritten;

        RequireWritten( aWritten, "hashBlock", ALL_BYTES );
        remaining -= fill;
    }

    // Whole blocks and a zero-padded tail write all 16 bytes.
    return remaining > 0 ? ALL_BYTES : aWritten;
}


// addDataV1( data, length ) never reads an unwritten byte.
uint16_t AddDataV1Writes( uint16_t aWritten, std::size_t aLength )
{
    std::size_t remaining = aLength;

    if( remaining >= 16 )
        aWritten = ALL_BYTES;

    remaining %= 16;

    if( remaining > 0 )
        aWritten |= Span( 0, remaining + 4 - ( remaining + 4 ) % 4 );

    return aWritten;
}


// add( int32_t ): refuse when the block it completes would hash unwritten bytes.
uint16_t AddIntWrites( uint16_t aWritten, uint32_t aLen )
{
    const std::size_t slot = ( aLen % 16 ) / 4 * 4;
    aWritten |= Span( slot, slot + 4 );

    if( static_cast<uint32_t>( aLen + 4 ) % 16 == 0 )
        RequireWritten( aWritten, "hashBlock", ALL_BYTES );

    return aWritten;
}


JSON Snapshot( MMH3_HASH& aHash, uint16_t aWritten, const std::string& aOp )
{
    // Read only written bytes; an unwritten byte has no value and is shown as "??".
    const unsigned char* bytes = reinterpret_cast<const unsigned char*>( aHash.*Get( Blocks{} ) );
    std::string          blocks;

    for( int byte = 0; byte < 16; ++byte )
    {
        if( aWritten & ( 1U << byte ) )
            blocks += ToHex( &bytes[byte], 1 );
        else
            blocks += "??";
    }

    return { { "op", aOp },
             { "h1", aHash.*Get( H1{} ) },
             { "h2", aHash.*Get( H2{} ) },
             { "len", aHash.*Get( Len{} ) },
             { "blocks", blocks } };
}


JSON Mmh3Op( const JSON& aInput )
{
    Keys( aInput, { "op", "steps" } );
    const JSON& steps = aInput.at( "steps" );

    if( !steps.is_array() )
        throw std::invalid_argument( "expected steps array" );

    if( steps.empty() || steps.size() > MAX_STEPS )
        throw std::invalid_argument( "steps outside 1..4096" );

    std::optional<MMH3_HASH> hash;
    uint16_t                 written = 0;
    JSON                     events = JSON::array();
    uint64_t                 total = 0;

    for( const JSON& step : steps )
    {
        if( !step.is_object() || !step.contains( "op" ) || !step.at( "op" ).is_string() )
            throw std::invalid_argument( "expected op string" );

        const std::string op = step.at( "op" ).get<std::string>();

        if( op != "new" && !hash )
            throw std::invalid_argument( "first step must be new" );

        JSON extra = JSON::object();

        if( op == "new" )
        {
            Keys( step, { "op" }, { "seed" } );

            if( step.contains( "seed" ) )
                hash.emplace( static_cast<uint32_t>( Unsigned( step, "seed", UINT32_MAX ) ) );
            else
                hash.emplace();

            written = 0;
        }
        else if( op == "reset" )
        {
            Keys( step, { "op" }, { "seed" } );

            if( step.contains( "seed" ) )
                hash->reset( static_cast<uint32_t>( Unsigned( step, "seed", UINT32_MAX ) ) );
            else
                hash->reset();
        }
        else if( op == "data" || op == "data_v1" || op == "string" || op == "chars" )
        {
            Keys( step, { "op", "hex" }, { "repeat" } );
            const std::vector<uint8_t> data = Hex( step, "hex", 65536 );
            const uint64_t repeat = step.contains( "repeat" ) ? Unsigned( step, "repeat", 4096 )
                                                              : 1;
            total += data.size() * repeat;

            if( total > MAX_MMH3_BYTES )
                throw std::invalid_argument( "byte budget exceeded" );

            const std::string       text( data.begin(), data.end() );
            const std::vector<char> chars( data.begin(), data.end() );

            for( uint64_t count = 0; count < repeat; ++count )
            {
                const uint32_t len = *hash.*Get( Len{} );
                const uint16_t after = op == "data_v1" ? AddDataV1Writes( written, data.size() )
                                                       : AddDataWrites( written, len, data.size() );

                if( op == "data" )
                    hash->addData( data.data(), data.size() );
                else if( op == "data_v1" )
                    hash->addDataV1( data.data(), data.size() );
                else if( op == "string" )
                    hash->add( text );
                else
                    hash->add( chars );

                written = after;
            }
        }
        else if( op == "int" )
        {
            Keys( step, { "op", "value" } );
            total += 4;

            if( total > MAX_MMH3_BYTES )
                throw std::invalid_argument( "byte budget exceeded" );

            const int32_t  value = static_cast<int32_t>( Signed( step, "value", INT32_MIN, INT32_MAX ) );
            const uint16_t after = AddIntWrites( written, *hash.*Get( Len{} ) );
            hash->add( value );
            written = after;
        }
        else if( op == "digest" )
        {
            Keys( step, { "op" } );
            RequireWritten( written, "hashTail", Span( 0, *hash.*Get( Len{} ) & 15 ) );
            extra["digest"] = View( hash->digest() );
        }
        else if( op == "state" )
        {
            Keys( step, { "op", "h1", "h2", "blocks", "len" } );
            const uint64_t             h1 = Unsigned( step, "h1", UINT64_MAX );
            const uint64_t             h2 = Unsigned( step, "h2", UINT64_MAX );
            const std::vector<uint8_t> blocks = Hex( step, "blocks", 16, true );
            const uint32_t len = static_cast<uint32_t>( Unsigned( step, "len", UINT32_MAX ) );
            *hash.*Get( H1{} ) = h1;
            *hash.*Get( H2{} ) = h2;
            std::memcpy( *hash.*Get( Blocks{} ), blocks.data(), 16 );
            *hash.*Get( Len{} ) = len;
            written = ALL_BYTES;
        }
        else if( op == "hash_block" || op == "hash_tail" )
        {
            Keys( step, { "op" } );

            if( op == "hash_block" )
            {
                RequireWritten( written, "hashBlock", ALL_BYTES );
                ( *hash.*Get( HashBlock{} ) )();
            }
            else
            {
                RequireWritten( written, "hashTail", Span( 0, *hash.*Get( Len{} ) & 15 ) );
                ( *hash.*Get( HashTail{} ) )();
            }
        }
        else if( op == "hash_final" )
        {
            Keys( step, { "op" } );
            HASH_128 out;
            ( *hash.*Get( HashFinal{} ) )( out );
            extra["out"] = View( out );
        }
        else if( op == "getblock64" )
        {
            Keys( step, { "op", "index" } );
            const int index = static_cast<int>( Signed( step, "index", 0, 1 ) );
            RequireWritten( written, "getblock64", index == 0 ? Span( 0, 8 ) : Span( 8, 16 ) );
            extra["block"] = ( *hash.*Get( GetBlock64{} ) )( index );
        }
        else
        {
            throw std::invalid_argument( "unknown operation" );
        }

        JSON event = Snapshot( *hash, written, op );

        for( auto& [key, value] : extra.items() )
            event[key] = value;

        events.push_back( std::move( event ) );
    }

    return { { "events", std::move( events ) } };
}


JSON RotlOp( const JSON& aInput )
{
    Keys( aInput, { "op", "x", "r" } );
    const uint64_t x = Unsigned( aInput, "x", UINT64_MAX );
    // ( x >> ( 64 - r ) ) is defined only for 1 <= r <= 63.
    const int8_t r = static_cast<int8_t>( Signed( aInput, "r", 1, 63 ) );
    return { { "value", mmh3_rotl64( x, r ) } };
}


JSON Fmix64Op( const JSON& aInput )
{
    Keys( aInput, { "op", "k" } );
    return { { "value", Get( Fmix64{} )( Unsigned( aInput, "k", UINT64_MAX ) ) } };
}


JSON Md5Snapshot( MD5_HASH& aHash, const std::string& aOp )
{
    return { { "op", aOp }, { "valid", aHash.IsValid() }, { "compact", aHash.Format( true ) } };
}


JSON Md5ValidOp( const JSON& aInput )
{
    Keys( aInput, { "op", "steps" } );
    const JSON& steps = aInput.at( "steps" );

    if( !steps.is_array() )
        throw std::invalid_argument( "expected steps array" );

    if( steps.empty() || steps.size() > MAX_STEPS )
        throw std::invalid_argument( "steps outside 1..4096" );

    std::unique_ptr<MD5_HASH> hash;
    JSON                      events = JSON::array();
    uint64_t                  total = 0;

    for( const JSON& step : steps )
    {
        if( !step.is_object() || !step.contains( "op" ) || !step.at( "op" ).is_string() )
            throw std::invalid_argument( "expected op string" );

        const std::string op = step.at( "op" ).get<std::string>();

        if( op != "new" && !hash )
            throw std::invalid_argument( "first step must be new" );

        if( op == "new" )
        {
            Keys( step, { "op" } );
            hash = std::make_unique<MD5_HASH>();
        }
        else if( op == "set_valid" )
        {
            Keys( step, { "op", "value" } );
            hash->SetValid( Boolean( step, "value" ) );
        }
        else if( op == "init" || op == "finalize" || op == "copy" || op == "assign" )
        {
            Keys( step, { "op" } );

            if( op == "init" )
            {
                hash->Init();
            }
            else if( op == "finalize" )
            {
                hash->Finalize();
            }
            else if( op == "copy" )
            {
                hash = std::make_unique<MD5_HASH>( *hash );
            }
            else
            {
                auto fresh = std::make_unique<MD5_HASH>();
                *fresh = *hash;
                hash = std::move( fresh );
            }
        }
        else if( op == "bytes" )
        {
            Keys( step, { "op", "hex" } );
            std::vector<uint8_t> data = Hex( step, "hex", 4096 );
            total += data.size();

            if( total > MAX_MD5_BYTES )
                throw std::invalid_argument( "byte budget exceeded" );

            // Empty Hash calls read no bytes; provide a real address rather than null.
            uint8_t empty = 0;
            hash->Hash( data.empty() ? &empty : data.data(), static_cast<uint32_t>( data.size() ) );
        }
        else if( op == "int" )
        {
            Keys( step, { "op", "value" } );
            total += 4;

            if( total > MAX_MD5_BYTES )
                throw std::invalid_argument( "byte budget exceeded" );

            hash->Hash( static_cast<int>( Signed( step, "value", INT32_MIN, INT32_MAX ) ) );
        }
        else if( op == "inspect" )
        {
            Keys( step, { "op" } );
        }
        else
        {
            throw std::invalid_argument( "unknown operation" );
        }

        events.push_back( Md5Snapshot( *hash, op ) );
    }

    return { { "events", std::move( events ) } };
}


TRANSFORM Transform( const JSON& aInput, const std::string& aName )
{
    const JSON& value = aInput.at( aName );

    if( value.is_null() )
        return TRANSFORM();

    if( !value.is_array() || value.size() != 4 )
        throw std::invalid_argument( "expected transform: " + aName );

    int members[4];

    for( int i = 0; i < 4; ++i )
    {
        const JSON wrapper = { { aName, value[i] } };
        members[i] = static_cast<int>( Signed( wrapper, aName, INT32_MIN, INT32_MAX ) );
    }

    return TRANSFORM( members[0], members[1], members[2], members[3] );
}


JSON TransformJson( const TRANSFORM& aTransform )
{
    return { aTransform.x1, aTransform.y1, aTransform.x2, aTransform.y2 };
}


JSON TransformOp( const JSON& aInput )
{
    Keys( aInput, { "op", "a", "b" } );
    const TRANSFORM a = Transform( aInput, "a" );
    const TRANSFORM b = Transform( aInput, "b" );

    return { { "a", TransformJson( a ) },
             { "b", TransformJson( b ) },
             { "equal", a == b },
             { "not_equal", a != b },
             { "reverse_not_equal", b != a },
             { "self_not_equal", a != a } };
}


JSON KimathHeaderHash( const JSON& aInput )
{
    if( !aInput.contains( "op" ) || !aInput.at( "op" ).is_string() )
        throw std::invalid_argument( "expected op string" );

    const std::string op = aInput.at( "op" ).get<std::string>();

    if( op == "std_hash" )
        return StdHashOp( aInput );
    if( op == "hash_combine" )
        return HashCombineOp( aInput );
    if( op == "hash_val" )
        return HashValOp( aInput );
    if( op == "hash_combine_each" )
        return HashCombineEachOp( aInput );
    if( op == "hash128" )
        return Hash128Op( aInput );
    if( op == "mmh3" )
        return Mmh3Op( aInput );
    if( op == "rotl" )
        return RotlOp( aInput );
    if( op == "fmix64" )
        return Fmix64Op( aInput );
    if( op == "md5_valid" )
        return Md5ValidOp( aInput );
    if( op == "transform" )
        return TransformOp( aInput );

    throw std::invalid_argument( "unknown operation" );
}
} // namespace


namespace oracle
{
void RegisterKimathHeaderHash( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.kimath.include.application.pure.p001",
                   "libs/kimath/include/{hash.h,hash_128.h,mmh3_hash.h,md5_hash.h,transform.h}",
                   KimathHeaderHash );
}
} // namespace oracle
