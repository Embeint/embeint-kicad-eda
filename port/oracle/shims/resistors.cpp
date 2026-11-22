// SPDX-License-Identifier: GPL-3.0-or-later
#include "registry.h"
#include <eseries.h>
#include <resistor_substitution_utils.h>
#include <resistors_access.h>
#include <bit>
#include <algorithm>
#include <cmath>
#include <limits>
#include <memory>
#include <dsnlexer.h>
#include <richio.h>
class REGULATOR_LIST;
#include <datafile_read_write.h>
#include <units_scales.h>

namespace
{
// Standard C++ explicit-instantiation test access ([temp.explicit] exempts its
// arguments from access checking). The pinned header/source remain unmodified;
// these pointers call the actual compiled library methods, never shim algorithms.
template <typename Tag, typename Tag::Type Member>
struct Access
{
    friend typename Tag::Type Get( Tag ) { return Member; }
};

#define MEMBER( Name, TypeName, MemberName )                                                                           \
    struct Name                                                                                                        \
    {                                                                                                                  \
        using Type = TypeName;                                                                                         \
        friend Type Get( Name );                                                                                       \
    };                                                                                                                 \
    template struct Access<Name, &RES_EQUIV_CALC::MemberName>;

using Table = std::vector<RESISTANCE>;
using Pair = std::pair<RESISTANCE&, RESISTANCE&>;
MEMBER( Series, uint32_t RES_EQUIV_CALC::*, m_series )
MEMBER( Target, double RES_EQUIV_CALC::*, m_target )
MEMBER( Mask, std::vector<bool> RES_EQUIV_CALC::*, m_exclude_mask )
MEMBER( Tables, std::vector<Table> RES_EQUIV_CALC::*, m_e_series )
MEMBER( Buffer1, Table RES_EQUIV_CALC::*, m_buffer_1R )
MEMBER( Buffer2, Table RES_EQUIV_CALC::*, m_buffer_2R )
MEMBER( Build, Table ( RES_EQUIV_CALC::* )( const ESERIES::ESERIES_VALUES& ), buildSeriesData )
MEMBER( Prepare1, void ( RES_EQUIV_CALC::* )(), prepare1RBuffer )
MEMBER( Prepare2, void ( RES_EQUIV_CALC::* )(), prepare2RBuffer )
MEMBER( Find, Pair ( RES_EQUIV_CALC::* )( double ), findIn2RBuffer )
MEMBER( Solution2, RESISTANCE ( RES_EQUIV_CALC::* )(), calculate2RSolution )
MEMBER( Solution3, RESISTANCE ( RES_EQUIV_CALC::* )(), calculate3RSolution )
MEMBER( Solution4, RESISTANCE ( RES_EQUIV_CALC::* )(), calculate4RSolution )
#undef MEMBER

struct ReaderTag
{
    using Type = LINE_READER* DSNLEXER::*;
    friend Type               Get( ReaderTag );
};
template <typename Tag, typename Tag::Type Member>
struct ReaderAccess
{
    friend typename Tag::Type Get( Tag ) { return Member; }
};
template struct ReaderAccess<ReaderTag, &DSNLEXER::reader>;

using oracle::JSON;

void Keys( const JSON& value, std::initializer_list<const char*> required,
           std::initializer_list<const char*> optional = {} )
{
    if( !value.is_object() )
        throw std::invalid_argument( "expected object" );

    for( const char* key : required )
        if( !value.contains( key ) )
            throw std::invalid_argument( "missing field" );

    for( auto it = value.begin(); it != value.end(); ++it )
    {
        bool known = false;
        for( const char* key : required )
            known |= it.key() == key;
        for( const char* key : optional )
            known |= it.key() == key;
        if( !known )
            throw std::invalid_argument( "unknown field" );
    }
}

const JSON& Array( const JSON& value, size_t limit )
{
    if( !value.is_array() || value.size() > limit )
        throw std::invalid_argument( "array bound exceeded or wrong type" );
    return value;
}

double Float( const JSON& value )
{
    if( !value.is_number_unsigned() && !( value.is_number_integer() && value.get<int64_t>() >= 0 ) )
        throw std::invalid_argument( "expected u64 IEEE754 bits" );
    return std::bit_cast<double>( value.get<uint64_t>() );
}

uint32_t SeriesSelector( const JSON& action )
{
    const auto& value = action.at( "series" );
    if( ( !value.is_number_unsigned() && !( value.is_number_integer() && value.get<int64_t>() >= 0 ) )
        || value.get<uint64_t>() > UINT32_MAX )
        throw std::invalid_argument( "integer out of range: series" );
    return value.get<uint32_t>();
}

JSON Bits( double value )
{
    return std::bit_cast<uint64_t>( value );
}

JSON Res( const RESISTANCE& value )
{
    JSON parts = JSON::array();
    for( double part : value.parts )
        parts.push_back( Bits( part ) );
    return { { "value_bits", Bits( value.value ) }, { "name", value.name }, { "parts_bits", parts } };
}

JSON TableJSON( const Table& values )
{
    JSON result = JSON::array();
    for( const auto& value : values )
        result.push_back( Res( value ) );
    return result;
}

RESISTANCE ParseRes( const JSON& item )
{
    Keys( item, { "value_bits", "name" }, { "parts_bits", "clear_parts" } );
    double value = Float( item.at( "value_bits" ) );
    auto   name = item.at( "name" ).get<std::string>();
    if( name.size() > 4096 )
        throw std::invalid_argument( "name bound exceeded" );
    std::vector<double> parts;
    if( item.contains( "parts_bits" ) )
        for( const auto& part : Array( item.at( "parts_bits" ), 256 ) )
            parts.push_back( Float( part ) );
    RESISTANCE result = item.contains( "parts_bits" ) ? RESISTANCE( value, name, parts ) : RESISTANCE( value, name );
    if( item.contains( "clear_parts" ) && item.at( "clear_parts" ).get<bool>() )
        result.parts.clear();
    return result;
}

void SortableParts( const RESISTANCE& value )
{
    bool                  nan = false;
    std::optional<double> number;
    bool                  distinct = false;
    for( double part : value.parts )
    {
        if( std::isnan( part ) )
            nan = true;
        else if( !number )
            number = part;
        else if( part != *number )
            distinct = true;
    }
    if( nan && distinct )
        throw std::invalid_argument( "parts violate sort precondition" );
}

JSON State( RES_EQUIV_CALC& calc )
{
    JSON results = JSON::array();
    for( const auto& value : calc.GetResults() )
        results.push_back( value ? Res( *value ) : JSON( nullptr ) );
    return { { "series", calc.*Get( Series{} ) },
             { "target_bits", Bits( calc.*Get( Target{} ) ) },
             { "exclusions", calc.*Get( Mask{} ) },
             { "buffer_1r_count", ( calc.*Get( Buffer1{} ) ).size() },
             { "buffer_2r_count", ( calc.*Get( Buffer2{} ) ).size() },
             { "results", results } };
}

void Initialized( RES_EQUIV_CALC& calc )
{
    auto        series = calc.*Get( Series{} );
    const auto& tables = calc.*Get( Tables{} );
    if( series >= tables.size() )
        throw std::invalid_argument( "calculator series must be 0..4" );
    if( ( calc.*Get( Mask{} ) ).size() < tables[series].size() )
        throw std::invalid_argument( "NewCalc required before Calculate" );
}

ESERIES::ESERIES_VALUES Named( int series )
{
    switch( series )
    {
    case 0: return ESERIES::E1_VALUES();
    case 1: return ESERIES::E3_VALUES();
    case 2: return ESERIES::E6_VALUES();
    case 3: return ESERIES::E12_VALUES();
    case 4: return ESERIES::E24_VALUES();
    case 5: return ESERIES::E48_VALUES();
    case 6: return ESERIES::E96_VALUES();
    case 7: return ESERIES::E192_VALUES();
    default: return ESERIES::ESERIES_VALUES( series );
    }
}

JSON Run( const JSON& input )
{
    const std::string op = input.at( "op" ).get<std::string>();
    if( op == "unit_scales" )
    {
        Keys( input, { "op" } );
        return { { "UNIT_KM", Bits( static_cast<double>( UNIT_KM ) ) },
                 { "UNIT_M", Bits( static_cast<double>( UNIT_M ) ) },
                 { "UNIT_CM", Bits( static_cast<double>( UNIT_CM ) ) },
                 { "UNIT_MM", Bits( static_cast<double>( UNIT_MM ) ) },
                 { "UNIT_MICRON", Bits( static_cast<double>( UNIT_MICRON ) ) },
                 { "UNIT_INCH", Bits( static_cast<double>( UNIT_INCH ) ) },
                 { "UNIT_MIL", Bits( static_cast<double>( UNIT_MIL ) ) },
                 { "UNIT_FEET", Bits( static_cast<double>( UNIT_FEET ) ) },
                 { "UNIT_OZSQFT", Bits( static_cast<double>( UNIT_OZSQFT ) ) },
                 { "UNIT_GHZ", Bits( static_cast<double>( UNIT_GHZ ) ) },
                 { "UNIT_MHZ", Bits( static_cast<double>( UNIT_MHZ ) ) },
                 { "UNIT_KHZ", Bits( static_cast<double>( UNIT_KHZ ) ) },
                 { "UNIT_DEGREE", Bits( static_cast<double>( UNIT_DEGREE ) ) },
                 { "UNIT_RADIAN", Bits( static_cast<double>( UNIT_RADIAN ) ) },
                 { "UNIT_OHM", Bits( static_cast<double>( UNIT_OHM ) ) },
                 { "UNIT_KOHM", Bits( static_cast<double>( UNIT_KOHM ) ) },
                 { "UNIT_OHM_PER_METER", Bits( static_cast<double>( UNIT_OHM_PER_METER ) ) },
                 { "UNIT_OHM_PER_KILOMETER", Bits( static_cast<double>( UNIT_OHM_PER_KILOMETER ) ) },
                 { "UNIT_OHM_PER_FEET", Bits( static_cast<double>( UNIT_OHM_PER_FEET ) ) },
                 { "UNIT_OHM_PER_1000FEET", Bits( static_cast<double>( UNIT_OHM_PER_1000FEET ) ) },
                 { "UNIT_KILOVOLT", Bits( static_cast<double>( UNIT_KILOVOLT ) ) },
                 { "UNIT_VOLT", Bits( static_cast<double>( UNIT_VOLT ) ) },
                 { "UNIT_MILLIVOLT", Bits( static_cast<double>( UNIT_MILLIVOLT ) ) },
                 { "UNIT_MEGAWATT", Bits( static_cast<double>( UNIT_MEGAWATT ) ) },
                 { "UNIT_KILOWATT", Bits( static_cast<double>( UNIT_KILOWATT ) ) },
                 { "UNIT_WATT", Bits( static_cast<double>( UNIT_WATT ) ) },
                 { "UNIT_MILLIWATT", Bits( static_cast<double>( UNIT_MILLIWATT ) ) },
                 { "UNIT_METER_PER_SECOND", Bits( static_cast<double>( UNIT_METER_PER_SECOND ) ) },
                 { "UNIT_KILOMETER_PER_HOUR", Bits( static_cast<double>( UNIT_KILOMETER_PER_HOUR ) ) },
                 { "UNIT_FEET_PER_SECOND", Bits( static_cast<double>( UNIT_FEET_PER_SECOND ) ) },
                 { "UNIT_MILES_PER_HOUR", Bits( static_cast<double>( UNIT_MILES_PER_HOUR ) ) },
                 { "UNIT_SECOND", Bits( static_cast<double>( UNIT_SECOND ) ) },
                 { "UNIT_MSECOND", Bits( static_cast<double>( UNIT_MSECOND ) ) },
                 { "UNIT_USECOND", Bits( static_cast<double>( UNIT_USECOND ) ) },
                 { "UNIT_NSECOND", Bits( static_cast<double>( UNIT_NSECOND ) ) },
                 { "UNIT_PSECOND", Bits( static_cast<double>( UNIT_PSECOND ) ) } };
    }
    if( op == "reader" )
    {
        Keys( input, { "op", "readers", "parsers", "actions" } );
        std::vector<std::unique_ptr<STRING_LINE_READER>> readers;
        for( const auto& item : Array( input.at( "readers" ), 16 ) )
        {
            Keys( item, { "text", "source" } );
            auto text = item.at( "text" ).get<std::string>();
            auto source = item.at( "source" ).get<std::string>();
            if( text.size() > 4096 || source.size() > 4096 )
                throw std::invalid_argument( "reader string bound exceeded" );
            readers.push_back( std::make_unique<STRING_LINE_READER>( text, wxString::FromUTF8( source ) ) );
        }
        auto reader = [&]( const JSON& value ) -> LINE_READER*
        {
            if( value.is_null() )
                return nullptr;
            int index = oracle::Integer( JSON{ { "index", value } }, "index", 0, 15 );
            if( static_cast<size_t>( index ) >= readers.size() )
                throw std::invalid_argument( "reader index out of range" );
            return readers[index].get();
        };
        std::vector<std::unique_ptr<PCB_CALCULATOR_DATAFILE_PARSER>> parsers;
        std::vector<LINE_READER*>                                    initial;
        for( const auto& item : Array( input.at( "parsers" ), 16 ) )
        {
            auto handle = reader( item );
            initial.push_back( handle );
            parsers.push_back( std::make_unique<PCB_CALCULATOR_DATAFILE_PARSER>( handle ) );
        }
        JSON result = JSON::array();
        for( const auto& action : Array( input.at( "actions" ), 64 ) )
        {
            Keys( action, { "op", "parser" }, { "reader" } );
            int index = oracle::Integer( action, "parser", 0, 15 );
            if( static_cast<size_t>( index ) >= parsers.size() )
                throw std::invalid_argument( "parser index out of range" );
            auto& parser = *parsers[index];
            auto  command = action.at( "op" ).get<std::string>();
            if( command == "bind" )
            {
                if( !action.contains( "reader" ) )
                    throw std::invalid_argument( "missing field" );
                parser.*Get( ReaderTag{} ) = reader( action.at( "reader" ) );
            }
            else if( command != "get" || action.contains( "reader" ) )
                throw std::invalid_argument( "unknown reader action" );
            LINE_READER* found = parser.GetReader();
            JSON         identity = nullptr;
            if( found )
            {
                for( size_t i = 0; i < readers.size(); i++ )
                    if( found == readers[i].get() )
                        identity = i;
                if( identity.is_null() )
                    throw std::logic_error( "unregistered real reader" );
            }
            result.push_back( { { "reader", identity },
                                { "same_as_initial", found == initial[index] },
                                { "repeated", found == parser.GetReader() } } );
        }
        return result;
    }
    if( op == "collector" )
    {
        Keys( input, { "op", "target_bits", "candidates" } );
        Table candidates;
        for( const auto& item : Array( input.at( "candidates" ), 256 ) )
        {
            candidates.push_back( ParseRes( item ) );
            SortableParts( candidates.back() );
        }
        try
        {
            auto result = resistor_test::Collect( Float( input.at( "target_bits" ) ), candidates );
            return { { "best", Res( result.best ) },
                     { "again", Res( result.again ) },
                     { "callbacks", result.callbacks } };
        }
        catch( const std::logic_error& error )
        {
            return { { "exception", error.what() } };
        }
    }
    if( op == "helpers" )
    {
        Keys( input, { "op", "first", "second", "text", "symbol" } );
        auto first = ParseRes( input.at( "first" ) );
        auto second = ParseRes( input.at( "second" ) );
        SortableParts( first );
        SortableParts( second );
        auto text = input.at( "text" ).get<std::string>();
        auto symbol = input.at( "symbol" ).get<std::string>();
        if( text.size() > 4096 || symbol.size() != 1 )
            throw std::invalid_argument( "invalid expression probe" );
        JSON combinations = JSON::array();
        for( bool parallel : { false, true } )
            for( bool simple : { false, true } )
                combinations.push_back( Res( resistor_test::Combine( first, second, parallel, simple ) ) );
        return { { "unique", JSON::array( { resistor_test::Unique( first ), resistor_test::Unique( second ) } ) },
                 { "better", resistor_test::Better( first, second ) },
                 { "reverse_better", resistor_test::Better( second, first ) },
                 { "less", resistor_test::Less( first, second ) },
                 { "less_value", resistor_test::Less( first, second.value ) },
                 { "embraced", resistor_test::Embrace( text, symbol[0] ) },
                 { "combinations", combinations } };
    }
    if( op == "series" )
    {
        Keys( input, { "op", "series", "decades" } );
        int  series = oracle::Integer( input, "series", INT32_MIN, INT32_MAX );
        JSON decades = JSON::array();
        for( const auto& exponent : Array( input.at( "decades" ), 1024 ) )
        {
            int  power = oracle::Integer( JSON{ { "exponent", exponent } }, "exponent", INT32_MIN, INT32_MAX );
            JSON values = JSON::array();
            for( double value : ESERIES::ESERIES_IN_DECADE( series, power ) )
                values.push_back( Bits( value ) );
            decades.push_back( { { "exponent", power }, { "values_bits", values } } );
        }
        return { { "values", ESERIES::ESERIES_VALUES( series ) },
                 { "named_values", Named( series ) },
                 { "decades", decades } };
    }
    if( op == "format" )
    {
        Keys( input, { "op", "values_bits" } );
        JSON result = JSON::array();
        for( const auto& value : Array( input.at( "values_bits" ), 4096 ) )
            result.push_back( strValue( Float( value ) ) );
        return result;
    }
    if( op == "resistance" )
    {
        Keys( input, { "op", "values" } );
        JSON result = JSON::array();
        for( const auto& item : Array( input.at( "values" ), 256 ) )
        {
            result.push_back( Res( ParseRes( item ) ) );
        }
        return result;
    }
    if( op != "calculator" )
        throw std::invalid_argument( "unknown operation" );
    Keys( input, { "op", "actions" } );
    RES_EQUIV_CALC calc;
    JSON           result = JSON::array();
    for( const auto& action : Array( input.at( "actions" ), 64 ) )
    {
        const auto command = action.at( "op" ).get<std::string>();
        JSON       returned = nullptr;
        try
        {
            if( command == "set_series" )
            {
                Keys( action, { "op", "series" } );
                calc.SetSeries( SeriesSelector( action ) );
            }
            else if( command == "new" )
            {
                Keys( action, { "op", "target_bits" } );
                if( calc.*Get( Series{} ) > 4 )
                    throw std::invalid_argument( "calculator series must be 0..4" );
                calc.NewCalc( Float( action.at( "target_bits" ) ) );
            }
            else if( command == "exclude" )
            {
                Keys( action, { "op", "values_bits" } );
                for( const auto& item : Array( action.at( "values_bits" ), 1024 ) )
                {
                    double value = Float( item );
                    // Validate only a potential out-of-bounds write. NaN and
                    // nonmatching exclusions are defined even before NewCalc.
                    if( !std::isnan( value ) )
                    {
                        auto        series = calc.*Get( Series{} );
                        const auto& tables = calc.*Get( Tables{} );
                        if( series >= tables.size() )
                            throw std::invalid_argument( "calculator series must be 0..4" );
                        const auto& table = tables[series];
                        auto        it = std::lower_bound( table.begin(), table.end(), value - epsilon,
                                                           []( const RESISTANCE& a, double b )
                                                           {
                                                        return a.value < b;
                                                    } );
                        if( it != table.end() && std::abs( it->value - value ) < epsilon
                            && static_cast<size_t>( it - table.begin() ) >= ( calc.*Get( Mask{} ) ).size() )
                            throw std::invalid_argument( "NewCalc required before Exclude" );
                    }
                    calc.Exclude( value );
                }
            }
            else if( command == "calculate" || command == "prepare" )
            {
                Keys( action, { "op" } );
                Initialized( calc );
                if( command == "prepare" )
                {
                    ( calc.*Get( Prepare1{} ) )();
                    ( calc.*Get( Prepare2{} ) )();
                }
                else
                {
                    const auto& mask = calc.*Get( Mask{} );
                    if( std::find( mask.begin(), mask.end(), false ) == mask.end() )
                        throw std::invalid_argument( "2R buffer must be nonempty" );
                    calc.Calculate();
                }
            }
            else if( command == "solution" || command == "find" )
            {
                bool emptyCollector = command == "solution" && ( calc.*Get( Buffer1{} ) ).empty()
                                      && oracle::Integer( action, "level", 2, 4 ) != 2;
                if( ( calc.*Get( Buffer2{} ) ).empty() && !emptyCollector )
                    throw std::invalid_argument( "2R buffer must be nonempty" );
                if( command == "solution" )
                {
                    Keys( action, { "op", "level" } );
                    int level = oracle::Integer( action, "level", 2, 4 );
                    returned = Res( level == 2   ? ( calc.*Get( Solution2{} ) )()
                                    : level == 3 ? ( calc.*Get( Solution3{} ) )()
                                                 : ( calc.*Get( Solution4{} ) )() );
                }
                else
                {
                    Keys( action, { "op", "values_bits" } );
                    returned = JSON::array();
                    for( const auto& value : Array( action.at( "values_bits" ), 1024 ) )
                    {
                        auto pair = ( calc.*Get( Find{} ) )( Float( value ) );
                        returned.push_back( JSON::array( { Res( pair.first ), Res( pair.second ) } ) );
                    }
                }
            }
            else if( command == "build" )
            {
                Keys( action, { "op", "series" } );
                returned = TableJSON( ( calc.*Get( Build{} ) )(
                        Named( oracle::Integer( action, "series", INT32_MIN, INT32_MAX ) ) ) );
            }
            else if( command == "build_values" )
            {
                Keys( action, { "op", "values" } );
                ESERIES::ESERIES_VALUES values( ESERIES::E1 );
                values.clear();
                for( const auto& item : Array( action.at( "values" ), 256 ) )
                    values.push_back(
                            static_cast<uint16_t>( oracle::Integer( JSON{ { "value", item } }, "value", 0, 65535 ) ) );
                if( values.empty() )
                    throw std::invalid_argument( "series table must be nonempty" );
                if( std::all_of( values.begin(), values.end(),
                                 []( uint16_t value )
                                 {
                                     return value == 0;
                                 } ) )
                    throw std::invalid_argument( "all-zero series table does not terminate" );
                returned = TableJSON( ( calc.*Get( Build{} ) )( values ) );
            }
            else if( command == "buffers" )
            {
                Keys( action, { "op" } );
                returned = { { "one", TableJSON( calc.*Get( Buffer1{} ) ) },
                             { "two", TableJSON( calc.*Get( Buffer2{} ) ) } };
            }
            else if( command == "state" )
                Keys( action, { "op" } );
            else
                throw std::invalid_argument( "unknown action" );
        }
        catch( const std::invalid_argument& )
        {
            throw;
        }
        catch( const std::logic_error& error )
        {
            returned = { { "exception", error.what() } };
        }
        result.push_back( { { "returned", returned }, { "state", State( calc ) } } );
    }
    return result;
}
} // namespace

namespace oracle
{
void RegisterResistors( REGISTRY& registry )
{
    registry.Add( "pcb_calculator.application.pure.p001",
                  "pcb_calculator/eseries.cpp;pcb_calculator/resistor_substitution_utils.cpp", Run );
}
} // namespace oracle
