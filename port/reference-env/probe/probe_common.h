// SPDX-License-Identifier: GPL-3.0-or-later
// Shared helpers for the reference environment probes: exact bit export, a
// minimal JSON writer and the fixed-seed generator also implemented by
// rust_probe.rs. Probes observe; they never compare against expected values.

#pragma once

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>

namespace probe
{

inline uint64_t Bits( double aValue )
{
    uint64_t bits;
    std::memcpy( &bits, &aValue, sizeof bits );
    return bits;
}

inline double FromBits( uint64_t aBits )
{
    double value;
    std::memcpy( &value, &aBits, sizeof value );
    return value;
}

inline std::string Hex( uint64_t aBits )
{
    char buffer[19];
    std::snprintf( buffer, sizeof buffer, "%016llx", static_cast<unsigned long long>( aBits ) );
    return buffer;
}

inline std::string HexDouble( double aValue )
{
    return Hex( Bits( aValue ) );
}

/// splitmix64 (Steele, Lea and Flood); identical in rust_probe.rs.
struct SPLITMIX
{
    uint64_t state;

    uint64_t Next()
    {
        uint64_t z = ( state += 0x9e3779b97f4a7c15ULL );
        z = ( z ^ ( z >> 30 ) ) * 0xbf58476d1ce4e5b9ULL;
        z = ( z ^ ( z >> 27 ) ) * 0x94d049bb133111ebULL;
        return z ^ ( z >> 31 );
    }
};

constexpr uint64_t SWEEP_SEED = 0x4b69436164526566ULL; // "KiCadRef"
constexpr double   TWO_PI = 6.283185307179586;         // 2 * M_PI exactly as a double

/// Angle number aIndex of the variant sweep. Four interleaved families: the
/// principal range, large arguments, KiCad-style tenth-degree angles, and
/// random finite magnitudes over 2^-30 .. 2^30. Same arithmetic as Rust.
inline double SweepAngle( SPLITMIX& aRng, uint64_t aIndex )
{
    const uint64_t x = aRng.Next();

    switch( aIndex % 4 )
    {
    case 0:
    {
        const double u = static_cast<double>( x >> 11 ) * 0x1.0p-53;
        return ( u * 2.0 - 1.0 ) * TWO_PI;
    }
    case 1:
    {
        const double u = static_cast<double>( x >> 11 ) * 0x1.0p-53;
        return ( u * 2.0 - 1.0 ) * 10000.0;
    }
    case 2:
        return static_cast<double>( x % 3600000 ) / 10000.0 * ( 3.141592653589793 / 180.0 );
    default:
    {
        const uint64_t exponent = 1023 - 30 + ( ( x >> 52 ) % 61 );
        const uint64_t sign = ( x >> 63 ) << 63;
        return FromBits( sign | ( exponent << 52 ) | ( x & 0x000fffffffffffffULL ) );
    }
    }
}

/// Representative libm inputs: zeros, subnormals, normal boundaries, common and
/// reduction-heavy angles, huge values, infinities and quiet/signalling NaNs
/// with distinct payloads and signs. Same list in rust_probe.rs.
constexpr uint64_t LIBM_INPUTS[] = {
    0x0000000000000000ULL, 0x8000000000000000ULL, 0x0000000000000001ULL, 0x800000000000beefULL,
    0x000fffffffffffffULL, 0x0010000000000000ULL, 0x01a56e1fc2f8f359ULL, 0x3e40000000000000ULL,
    0x3e45798ee2308c3aULL, 0x3fe0000000000000ULL, 0x3fe921fb54442d18ULL, 0x3ff0000000000000ULL,
    0x3ff921fb54442d18ULL, 0x4000000000000000ULL, 0x400921fb54442d18ULL, 0x4012d97c7f3321d2ULL,
    0x401921fb54442d18ULL, 0x4024000000000000ULL, 0x4059000000000000ULL, 0x40f86a0000000000ULL,
    0x419921fb38000000ULL, 0x41cdcd6500000000ULL, 0x4330000000000000ULL, 0x4480f0cf064dd592ULL,
    0x44b52d02c7e14af6ULL, 0x7e37e43c8800759cULL, 0x7fefffffffffffffULL, 0xbff0000000000000ULL,
    0xc00921fb54442d18ULL, 0x7ff0000000000000ULL, 0xfff0000000000000ULL, 0x7ff8000000000000ULL,
    0xfff8000000000000ULL, 0x7ff8000000000123ULL, 0x7ff0000000000001ULL, 0x7ff4000000000abcULL,
    0xfff0000000000042ULL,
};

/// Operands for primitive mixed-NaN arithmetic. Same list in rust_probe.rs.
constexpr uint64_t NAN_OPERANDS[] = {
    0x7ff8000000000001ULL, 0x7ff8000000000002ULL, 0xfff8000000000003ULL, 0x7ff0000000000004ULL,
    0x7ff4000000000005ULL, 0x3ff8000000000000ULL, 0x7ff0000000000000ULL,
};

/// Minimal JSON writer: keys and strings are ASCII and escaped conservatively.
class JSON
{
public:
    explicit JSON( FILE* aOut ) : m_out( aOut ), m_first( true ) {}

    void Begin( char aBracket )
    {
        Separator();
        std::fputc( aBracket, m_out );
        m_first = true;
    }

    void End( char aBracket )
    {
        std::fputc( aBracket, m_out );
        m_first = false;
    }

    void Key( const char* aKey )
    {
        Separator();
        String( aKey );
        std::fputc( ':', m_out );
        m_first = true;
    }

    void Value( const std::string& aValue )
    {
        Separator();
        String( aValue.c_str() );
        m_first = false;
    }

    void Raw( const std::string& aValue )
    {
        Separator();
        std::fputs( aValue.c_str(), m_out );
        m_first = false;
    }

    void Field( const char* aKey, const std::string& aValue )
    {
        Key( aKey );
        Value( aValue );
    }

    void FieldRaw( const char* aKey, const std::string& aValue )
    {
        Key( aKey );
        Raw( aValue );
    }

private:
    void Separator()
    {
        if( !m_first )
            std::fputc( ',', m_out );

        m_first = false;
    }

    void String( const char* aText )
    {
        std::fputc( '"', m_out );

        for( const unsigned char* p = reinterpret_cast<const unsigned char*>( aText ); *p; ++p )
        {
            if( *p == '"' || *p == '\\' )
                std::fprintf( m_out, "\\%c", *p );
            else if( *p < 0x20 || *p >= 0x7f )
                std::fprintf( m_out, "\\u%04x", *p );
            else
                std::fputc( *p, m_out );
        }

        std::fputc( '"', m_out );
    }

    FILE* m_out;
    bool  m_first;
};

} // namespace probe
