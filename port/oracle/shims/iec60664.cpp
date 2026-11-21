// SPDX-License-Identifier: GPL-3.0-or-later
#include "registry.h"
#include <iec60664.h>
#include <bit>
#include <cmath>
#include <limits>

namespace oracle
{
namespace
{
using PD = IEC60664::POLLUTION_DEGREE;
using MG = IEC60664::MATERIAL_GROUP;
using FIELD = IEC60664::FIELD;
using INS = IEC60664::INSULATION_TYPE;
using OV = IEC60664::OV_CATEGORY;

// C++ permits private member names in explicit instantiations (C++20 [temp.spec]).
// These test accessors call the actual compiled helpers, including dead helpers,
// and read state without changing the pinned header, object layout or flags.
template<typename Tag, typename Tag::TYPE Member>
struct ACCESS
{
    friend typename Tag::TYPE MemberPointer( Tag )
    {
        return Member;
    }
};

#define IEC_ACCESS( Name, Type, Member ) \
    struct Name { using TYPE = Type; friend TYPE MemberPointer( Name ); }; \
    template struct ACCESS<Name, &IEC60664::Member>;

IEC_ACCESS( Groove, double (IEC60664::*)( PD, double ), GetMinGrooveWidth )
IEC_ACCESS( Altitude, double (IEC60664::*)( double ), GetClearanceAltitudeCorrectionFactor )
IEC_ACCESS( Rms, double (IEC60664::*)( double, FIELD ), GetClearanceForRMSVoltage )
IEC_ACCESS( Cti, MG (IEC60664::*)( double ), GetMaterialGroupFromCTI )
IEC_ACCESS( Transient, double (IEC60664::*)( double, PD, FIELD ), GetClearanceToWithstandTransientVoltage )
IEC_ACCESS( Peaks, double (IEC60664::*)( double, FIELD ), GetClearanceToWithstandPeaks )
IEC_ACCESS( Basic, double (IEC60664::*)( double, PD, MG ), GetBasicCreepageDistance )
IEC_ACCESS( Clearance, double (IEC60664::*)( PD, FIELD, double ), ComputeClearanceDistance )
IEC_ACCESS( Creepage, double (IEC60664::*)( PD, MG ), ComputeCreepageDistance )
IEC_ACCESS( Rated, double IEC60664::*, m_ratedVoltage )
IEC_ACCESS( Frequency, double IEC60664::*, m_frequency )
IEC_ACCESS( Coated, bool IEC60664::*, m_coated )
IEC_ACCESS( Pcb, bool IEC60664::*, m_pcbMaterial )
IEC_ACCESS( RmsState, double IEC60664::*, m_RMSvoltage )
IEC_ACCESS( PeakState, double IEC60664::*, m_peakVoltage )
IEC_ACCESS( TransientState, double IEC60664::*, m_transientVoltage )
IEC_ACCESS( AltitudeState, double IEC60664::*, m_altitude )
IEC_ACCESS( FieldState, FIELD IEC60664::*, m_field )
IEC_ACCESS( OvState, OV IEC60664::*, m_overvoltageCat )
IEC_ACCESS( MgState, MG IEC60664::*, m_materialGroup )
IEC_ACCESS( PdState, PD IEC60664::*, m_pollutionDegree )
IEC_ACCESS( InsState, INS IEC60664::*, m_insulationType )
#undef IEC_ACCESS

uint64_t Bits( double aValue )
{
    static_assert( sizeof( double ) == sizeof( uint64_t ) && std::numeric_limits<double>::is_iec559 );
    return std::bit_cast<uint64_t>( aValue );
}


double Number( const JSON& aInput, const char* aKey )
{
    const JSON& value = aInput.at( aKey );

    if( !value.is_number_integer()
        || ( !value.is_number_unsigned() && value.get<int64_t>() < 0 ) )
        throw std::invalid_argument( "expected u64 bits" );

    const double result = std::bit_cast<double>( value.get<uint64_t>() );

    if( !std::isfinite( result ) )
        throw std::invalid_argument( "expected finite bits" );

    return result;
}


void Keys( const JSON& aValue, std::initializer_list<const char*> aRequired,
           std::initializer_list<const char*> aOptional = {} )
{
    if( !aValue.is_object() )
        throw std::invalid_argument( "expected object" );

    for( const char* key : aRequired )
    {
        if( !aValue.contains( key ) )
            throw std::invalid_argument( "missing field" );
    }

    for( const auto& [key, value] : aValue.items() )
    {
        bool known = false;

        for( const char* candidate : aRequired )
            known |= key == candidate;

        for( const char* candidate : aOptional )
            known |= key == candidate;

        if( !known )
            throw std::invalid_argument( "unexpected field" );
    }
}


int Enum( const JSON& aValue, const char* aKey )
{
    return Integer( aValue, aKey, std::numeric_limits<int>::min(), std::numeric_limits<int>::max() );
}


JSON State( IEC60664& aCalc )
{
    return {
        { "rated_voltage", Bits( aCalc.*MemberPointer( Rated{} ) ) },
        { "frequency", Bits( aCalc.*MemberPointer( Frequency{} ) ) },
        { "coated", aCalc.*MemberPointer( Coated{} ) },
        { "pcb_material", aCalc.*MemberPointer( Pcb{} ) },
        { "rms_voltage", Bits( aCalc.*MemberPointer( RmsState{} ) ) },
        { "peak_voltage", Bits( aCalc.*MemberPointer( PeakState{} ) ) },
        { "transient_voltage", Bits( aCalc.*MemberPointer( TransientState{} ) ) },
        { "altitude", Bits( aCalc.*MemberPointer( AltitudeState{} ) ) },
        { "clearance", Bits( aCalc.GetClearanceDistance() ) },
        { "creepage", Bits( aCalc.GetCreepageDistance() ) },
        { "min_groove_width", Bits( aCalc.GetMinGrooveWidth() ) },
        { "field", static_cast<int>( aCalc.*MemberPointer( FieldState{} ) ) },
        { "overvoltage_category", static_cast<int>( aCalc.*MemberPointer( OvState{} ) ) },
        { "material_group", static_cast<int>( aCalc.*MemberPointer( MgState{} ) ) },
        { "pollution_degree", static_cast<int>( aCalc.*MemberPointer( PdState{} ) ) },
        { "insulation_type", static_cast<int>( aCalc.*MemberPointer( InsState{} ) ) }
    };
}


void Set( IEC60664& aCalc, const JSON& aStep )
{
    Keys( aStep, { "op", "field", "value" } );
    const std::string field = aStep.at( "field" ).get<std::string>();

    if( field == "altitude" )
        aCalc.SetAltitude( Number( aStep, "value" ) );
    else if( field == "frequency" )
        aCalc.SetFrequency( Number( aStep, "value" ) );
    else if( field == "rated_voltage" )
        aCalc.SetRatedVoltage( Number( aStep, "value" ) );
    else if( field == "rms_voltage" )
        aCalc.SetRMSVoltage( Number( aStep, "value" ) );
    else if( field == "peak_voltage" )
        aCalc.SetPeakVoltage( Number( aStep, "value" ) );
    else if( field == "transient_voltage" )
        aCalc.SetTransientVoltage( Number( aStep, "value" ) );
    else if( field == "pollution_degree" )
        aCalc.SetPollutionDegree( static_cast<PD>( Enum( aStep, "value" ) ) );
    else if( field == "material_group" )
        aCalc.SetMaterialGroup( static_cast<MG>( Enum( aStep, "value" ) ) );
    else if( field == "field" )
        aCalc.SetField( static_cast<FIELD>( Enum( aStep, "value" ) ) );
    else if( field == "insulation_type" )
        aCalc.SetInsulationType( static_cast<INS>( Enum( aStep, "value" ) ) );
    else if( field == "overvoltage_category" )
        aCalc.SetOvervoltageCategory( static_cast<OV>( Enum( aStep, "value" ) ) );
    else if( field == "pcb_material" )
        aCalc.SetPCBMaterial( aStep.at( "value" ).get<bool>() );
    else
        throw std::invalid_argument( "unknown setter" );
}


JSON Step( IEC60664& aCalc, const JSON& aStep )
{
    const std::string op = aStep.at( "op" ).get<std::string>();
    JSON result = nullptr;

    if( op == "categories" )
    {
        Keys( aStep, { "op" } );
        result = {
            { "pollution", { static_cast<int>( PD::PD1 ), static_cast<int>( PD::PD2 ),
                             static_cast<int>( PD::PD3 ), static_cast<int>( PD::PD4 ) } },
            { "overvoltage", { static_cast<int>( OV::OV_I ), static_cast<int>( OV::OV_II ),
                               static_cast<int>( OV::OV_III ), static_cast<int>( OV::OV_IV ) } },
            { "material", { static_cast<int>( MG::MG_I ), static_cast<int>( MG::MG_II ),
                            static_cast<int>( MG::MG_IIIa ), static_cast<int>( MG::MG_IIIb ),
                            static_cast<int>( MG::NOT_INSULATING ) } },
            { "insulation", { static_cast<int>( INS::FUNCTIONAL ), static_cast<int>( INS::BASIC ),
                              static_cast<int>( INS::REINFORCED ) } },
            { "field", { static_cast<int>( FIELD::INHOMOGENEOUS ), static_cast<int>( FIELD::HOMOGENEOUS ) } },
            { "waveform", { static_cast<int>( IEC60664::WAVEFORM::AC_50_OR_60HZ ),
                            static_cast<int>( IEC60664::WAVEFORM::IMPULSE_1_2_50 ) } }
        };
    }
    else if( op == "set" )
        Set( aCalc, aStep );
    else if( op == "state" || op == "compute" || op == "impulse" )
    {
        Keys( aStep, { "op" } );

        if( op == "compute" ) result = aCalc.Compute();
        else if( op == "impulse" ) result = Bits( aCalc.GetRatedImpulseWithstandVoltage() );
    }
    else if( op == "groove" )
    {
        Keys( aStep, { "op", "pd" }, { "distance" } );
        result = Bits( ( aCalc.*MemberPointer( Groove{} ) )(
                static_cast<PD>( Enum( aStep, "pd" ) ),
                aStep.contains( "distance" ) ? Number( aStep, "distance" ) : 100.0 ) );
    }
    else if( op == "altitude" || op == "cti" )
    {
        Keys( aStep, { "op", "voltage" } );

        if( op == "altitude" )
            result = Bits( ( aCalc.*MemberPointer( Altitude{} ) )( Number( aStep, "voltage" ) ) );
        else
            result = static_cast<int>( ( aCalc.*MemberPointer( Cti{} ) )( Number( aStep, "voltage" ) ) );
    }
    else if( op == "rms" )
    {
        Keys( aStep, { "op", "voltage" }, { "field" } );
        result = Bits( ( aCalc.*MemberPointer( Rms{} ) )(
                Number( aStep, "voltage" ), aStep.contains( "field" )
                        ? static_cast<FIELD>( Enum( aStep, "field" ) ) : FIELD::INHOMOGENEOUS ) );
    }
    else if( op == "transient" )
    {
        Keys( aStep, { "op", "voltage", "pd", "field" } );
        result = Bits( ( aCalc.*MemberPointer( Transient{} ) )( Number( aStep, "voltage" ),
                static_cast<PD>( Enum( aStep, "pd" ) ), static_cast<FIELD>( Enum( aStep, "field" ) ) ) );
    }
    else if( op == "peaks" )
    {
        Keys( aStep, { "op", "voltage", "field" } );
        result = Bits( ( aCalc.*MemberPointer( Peaks{} ) )( Number( aStep, "voltage" ),
                static_cast<FIELD>( Enum( aStep, "field" ) ) ) );
    }
    else if( op == "basic_creepage" )
    {
        Keys( aStep, { "op", "voltage", "pd", "mg" } );
        result = Bits( ( aCalc.*MemberPointer( Basic{} ) )( Number( aStep, "voltage" ),
                static_cast<PD>( Enum( aStep, "pd" ) ), static_cast<MG>( Enum( aStep, "mg" ) ) ) );
    }
    else if( op == "clearance" )
    {
        Keys( aStep, { "op", "pd", "field", "altitude" } );
        result = Bits( ( aCalc.*MemberPointer( Clearance{} ) )(
                static_cast<PD>( Enum( aStep, "pd" ) ), static_cast<FIELD>( Enum( aStep, "field" ) ),
                Number( aStep, "altitude" ) ) );
    }
    else if( op == "creepage" )
    {
        Keys( aStep, { "op", "pd", "mg" } );
        result = Bits( ( aCalc.*MemberPointer( Creepage{} ) )(
                static_cast<PD>( Enum( aStep, "pd" ) ), static_cast<MG>( Enum( aStep, "mg" ) ) ) );
    }
    else
        throw std::invalid_argument( "unknown operation" );

    return { { "value", result }, { "state", State( aCalc ) } };
}


JSON Calculate( const JSON& aInput )
{
    Keys( aInput, { "steps" } );
    const JSON& steps = aInput.at( "steps" );

    if( !steps.is_array() || steps.empty() || steps.size() > 256 )
        throw std::invalid_argument( "expected 1..256 steps" );

    IEC60664 calc;
    JSON results = JSON::array();

    for( const JSON& step : steps )
        results.push_back( Step( calc, step ) );

    return results;
}
} // namespace


void RegisterIec60664( REGISTRY& aRegistry )
{
    aRegistry.Add( "pcb_calculator.calculator_panels.application.pure.p001",
                   "pcb_calculator/calculator_panels/iec60664.cpp;iec60664.h:IEC60664", Calculate );
}
} // namespace oracle
