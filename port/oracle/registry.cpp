// SPDX-License-Identifier: GPL-3.0-or-later
#include "registry.h"

namespace oracle
{
void RegisterUtilBox( REGISTRY& aRegistry );
void RegisterGeometryHeaders( REGISTRY& aRegistry );
void RegisterExample( REGISTRY& aRegistry );
void RegisterMd5( REGISTRY& aRegistry );
void RegisterIec60664( REGISTRY& aRegistry );
void RegisterResistors( REGISTRY& aRegistry );
void RegisterKimathMath( REGISTRY& aRegistry );
void RegisterKimathHeaderHash( REGISTRY& aRegistry );
void RegisterRtreeNode( REGISTRY& aRegistry );
void RegisterMathUtil( REGISTRY& aRegistry );
void RegisterVector( REGISTRY& aRegistry );
void RegisterDynamicRtree( REGISTRY& aRegistry );
void RegisterCowPackedRtree( REGISTRY& aRegistry );
void RegisterEdaAngleSource( REGISTRY& aRegistry );
void RegisterArcChordParams( REGISTRY& aRegistry );
void RegisterDistribute( REGISTRY& aRegistry );
void RegisterArcConstruction( REGISTRY& aRegistry );
void RegisterEllipse( REGISTRY& aRegistry );
void RegisterCircle( REGISTRY& aRegistry );
void RegisterClipper2CoreEngine( REGISTRY& aRegistry );
void RegisterClipper2EngineP001( REGISTRY& aRegistry );
void RegisterClipper2EngineP002( REGISTRY& aRegistry );
void RegisterCoreP001( REGISTRY& aRegistry );
void RegisterCoreIncludeP001( REGISTRY& aRegistry );
void RegisterSeg( REGISTRY& aRegistry );
void RegisterClipper2RectClip( REGISTRY& aRegistry );
void RegisterCoreMapHelpers( REGISTRY& aRegistry );
void RegisterTransformSource( REGISTRY& aRegistry );
void RegisterVector2Wx( REGISTRY& aRegistry );
void RegisterPoissonDisk( REGISTRY& aRegistry );
void RegisterKiplatformPolicy( REGISTRY& aRegistry );
void RegisterKiplatformUnixP001( REGISTRY& aRegistry );
void RegisterCoreWxStlCompat( REGISTRY& aRegistry );
void RegisterCoreAppIncludeP001( REGISTRY& aRegistry );
void RegisterSexprInclude( REGISTRY& aRegistry );
void RegisterShapeHeadersP003( REGISTRY& aRegistry );
void RegisterClipper2IncludeP002( REGISTRY& aRegistry );
void RegisterSexprSource( REGISTRY& aRegistry );


void REGISTRY::Add( const std::string& aUnit, const std::string& aUpstream, SHIM aShim )
{
    if( aUnit.empty() || aUpstream.empty() || !aShim
        || !m_units.emplace( aUnit, UNIT{ aUpstream, aShim } ).second )
    {
        throw std::logic_error( "invalid or duplicate oracle registration: " + aUnit );
    }
}


REGISTRY MakeRegistry()
{
    REGISTRY registry;
    RegisterUtilBox( registry );
    RegisterGeometryHeaders( registry );
    RegisterExample( registry );
    RegisterMd5( registry );
    RegisterIec60664( registry );
    RegisterResistors( registry );
    RegisterKimathMath( registry );
    RegisterKimathHeaderHash( registry );
    RegisterRtreeNode( registry );
    RegisterMathUtil( registry );
    RegisterVector( registry );
    RegisterDynamicRtree( registry );
    RegisterCowPackedRtree( registry );
    RegisterEdaAngleSource( registry );
    RegisterArcChordParams( registry );
    RegisterDistribute( registry );
    RegisterArcConstruction( registry );
    RegisterEllipse( registry );
    RegisterCircle( registry );
    RegisterClipper2CoreEngine( registry );
    RegisterClipper2EngineP001( registry );
    RegisterClipper2EngineP002( registry );
    RegisterCoreP001( registry );
    RegisterCoreIncludeP001( registry );
    RegisterSeg( registry );
    RegisterClipper2RectClip( registry );
    RegisterCoreMapHelpers( registry );
    RegisterTransformSource( registry );
    RegisterVector2Wx( registry );
    RegisterPoissonDisk( registry );
    RegisterKiplatformPolicy( registry );
    RegisterKiplatformUnixP001( registry );
    RegisterCoreWxStlCompat( registry );
    RegisterCoreAppIncludeP001( registry );
    RegisterSexprInclude( registry );
    RegisterShapeHeadersP003( registry );
    RegisterClipper2IncludeP002( registry );
    RegisterSexprSource( registry );
    return registry;
}


int Integer( const JSON& aInput, const std::string& aName, int aMin, int aMax )
{
    const JSON& value = aInput.at( aName );

    if( !value.is_number_integer() )
        throw std::invalid_argument( "integer out of range: " + aName );

    if( value.is_number_unsigned() )
    {
        const uint64_t number = value.get<uint64_t>();

        if( aMax < 0 || number > static_cast<uint64_t>( aMax )
            || ( aMin > 0 && number < static_cast<uint64_t>( aMin ) ) )
        {
            throw std::invalid_argument( "integer out of range: " + aName );
        }

        return static_cast<int>( number );
    }

    const int64_t number = value.get<int64_t>();

    if( number < aMin || number > aMax )
        throw std::invalid_argument( "integer out of range: " + aName );

    return static_cast<int>( number );
}
} // namespace oracle
