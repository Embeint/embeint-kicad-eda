// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned libs/sexpr (libs/sexpr/include/sexpr/*.h, libs/sexpr/sexpr.cpp).
//
// The adapter decodes trees and arguments, calls the pinned SEXPR code and encodes every
// observable value exactly; it never computes a reply itself. Trees are built with the public
// constructors and the public m_children member and reported through the inline predicates,
// GetLineNumber and the public m_value/m_children members. The private m_inStreamChild is
// read through standard explicit-instantiation member access.
//
// Every exception the library throws (INVALID_TYPE_EXCEPTION, std::invalid_argument,
// std::length_error, std::bad_alloc) is caught around the library call and reported as a
// result. Before a call whose pinned evaluation could be undefined (an out-of-range
// m_children[] access, a null child dereference, doScan writing a double through an
// int32_t*), a guard walks the reference control flow over the decoded tree and refuses the
// request with "undefined sexpr operation". AsString requests whose output or copy work
// exceeds a fixed bound are refused as "as_string output too large".
//
// Units: libs.sexpr.include.sexpr.application.pure.p001 (runHeader: the inline header
// bodies) and libs.sexpr.application.pure.p001 (runSource: the sexpr.cpp members and
// operators; doScan and doAddChildren are reached by both units' scan and add_children).
//
// Encoding (identical in rust/kicad-sexpr-shim/src/codec.rs): strings are lowercase hex,
// doubles their 16 hex IEEE bits, trees flat pre-order arrays of ["list", line, n],
// ["integer", v, line], ["double", bits, line], ["string", hex, line], ["symbol", hex, line]
// and ["null"].
#include "registry.h"
// sexpr.h first: the pinned isexprable.h is not self-contained when included before it (it
// includes sexpr.h, whose declarations then precede the ISEXPRABLE and SEXPR_LIST they name).
#include <sexpr/sexpr.h>
#include <sexpr/isexprable.h>
#include <sexpr/sexpr_exception.h>
#include <sexpr/sexpr_parser.h>
#include <bit>
#include <cstdint>
#include <cstdio>
#include <memory>
#include <new>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace
{
using oracle::JSON;
using SEXPR::SEXPR_LIST;
using NODE = SEXPR::SEXPR;

constexpr const char* HEADER_UNIT = "libs.sexpr.include.sexpr.application.pure.p001";
constexpr const char* SOURCE_UNIT = "libs.sexpr.application.pure.p001";
constexpr const char* UNDEFINED = "undefined sexpr operation";
constexpr size_t MAX_NODES = 20000;
constexpr size_t MAX_DEPTH = 4000;
constexpr size_t MAX_STRING = 65536;
constexpr size_t MAX_ARGS = 16;
// AsString resource bounds (identical in rust/kicad-sexpr-shim/src/source_ops.rs).
constexpr uint64_t    MAX_INDENT = uint64_t( 1 ) << 20;
constexpr uint64_t    MAX_OUTPUT = uint64_t( 64 ) << 20;
constexpr uint64_t    MAX_WORK = uint64_t( 1 ) << 30;
constexpr const char* TOO_LARGE = "as_string output too large";


// Explicit instantiation may name a private member; the friend returns its pointer.
template <typename TAG, typename TAG::type MEMBER>
struct PRIVATE_ACCESS
{
    friend typename TAG::type Get( TAG ) { return MEMBER; }
};

struct IN_STREAM_CHILD
{
    using type = int SEXPR_LIST::*;
    friend type Get( IN_STREAM_CHILD );
};

template struct PRIVATE_ACCESS<IN_STREAM_CHILD, &SEXPR_LIST::m_inStreamChild>;


int inStreamChild( const SEXPR_LIST& aList )
{
    return aList.*Get( IN_STREAM_CHILD() );
}


[[noreturn]] void refuse( const std::string& aMessage )
{
    throw std::invalid_argument( aMessage );
}


void checkKeys( const JSON& aInput, std::initializer_list<const char*> aFields )
{
    if( !aInput.is_object() )
        refuse( "expected object" );
    if( aInput.size() != aFields.size() )
        refuse( "unexpected or missing field" );
    for( const char* field : aFields )
        if( !aInput.contains( field ) )
            refuse( "unexpected or missing field" );
}


const JSON& field( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) )
        refuse( std::string( "missing: " ) + aName );
    return aInput.at( aName );
}


std::string readText( const JSON& aInput, const char* aName )
{
    const JSON& value = field( aInput, aName );
    if( !value.is_string() )
        refuse( std::string( "expected string: " ) + aName );
    return value.get<std::string>();
}


bool readBool( const JSON& aValue )
{
    if( !aValue.is_boolean() )
        refuse( "expected boolean" );
    return aValue.get<bool>();
}


int64_t readI64( const JSON& aValue )
{
    if( !aValue.is_number_integer()
        || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() > uint64_t( INT64_MAX ) ) )
        refuse( "integer out of range" );
    return aValue.get<int64_t>();
}


int32_t readI32( const JSON& aValue )
{
    const int64_t value = readI64( aValue );
    if( value < INT32_MIN || value > INT32_MAX )
        refuse( "integer out of range" );
    return static_cast<int32_t>( value );
}


uint64_t readU64( const JSON& aValue )
{
    if( !aValue.is_number_unsigned() )
        refuse( "integer out of range" );
    return aValue.get<uint64_t>();
}


int hexDigit( char aByte )
{
    if( aByte >= '0' && aByte <= '9' )
        return aByte - '0';
    if( aByte >= 'a' && aByte <= 'f' )
        return aByte - 'a' + 10;
    return -1;
}


std::string readHex( const JSON& aValue )
{
    if( !aValue.is_string() )
        refuse( "invalid hex" );
    const std::string& text = aValue.get_ref<const std::string&>();
    if( text.size() % 2 != 0 || text.size() > 2 * MAX_STRING )
        refuse( "invalid hex" );
    std::string out;
    for( size_t i = 0; i < text.size(); i += 2 )
    {
        const int high = hexDigit( text[i] );
        const int low = hexDigit( text[i + 1] );
        if( high < 0 || low < 0 )
            refuse( "invalid hex" );
        out.push_back( static_cast<char>( high * 16 + low ) );
    }
    return out;
}


JSON hex( const std::string& aBytes )
{
    static const char* digits = "0123456789abcdef";
    std::string out;
    for( unsigned char byte : aBytes )
    {
        out.push_back( digits[byte >> 4] );
        out.push_back( digits[byte & 15] );
    }
    return out;
}


uint64_t readBitsText( const JSON& aValue, size_t aDigits )
{
    if( !aValue.is_string() )
        refuse( "invalid bits" );
    const std::string& text = aValue.get_ref<const std::string&>();
    if( text.size() != aDigits )
        refuse( "invalid bits" );
    uint64_t bits = 0;
    for( char c : text )
    {
        const int digit = hexDigit( c );
        if( digit < 0 )
            refuse( "invalid bits" );
        bits = bits * 16 + static_cast<uint64_t>( digit );
    }
    return bits;
}


double readF64( const JSON& aValue )
{
    return std::bit_cast<double>( readBitsText( aValue, 16 ) );
}


JSON bits64( double aValue )
{
    char text[17];
    std::snprintf( text, sizeof( text ), "%016llx",
                   static_cast<unsigned long long>( std::bit_cast<uint64_t>( aValue ) ) );
    return std::string( text );
}


float readF32( const JSON& aValue )
{
    return std::bit_cast<float>( static_cast<uint32_t>( readBitsText( aValue, 8 ) ) );
}


JSON bits32( float aValue )
{
    char text[9];
    std::snprintf( text, sizeof( text ), "%08x",
                   static_cast<unsigned>( std::bit_cast<uint32_t>( aValue ) ) );
    return std::string( text );
}


// ---------------------------------------------------------------------------------------
// Trees.

std::optional<int32_t> readLine( const JSON& aValue )
{
    if( aValue.is_null() )
        return std::nullopt;
    return readI32( aValue );
}


struct PARSED
{
    std::unique_ptr<NODE> node; // null for a null child
    bool                  isList = false;
    bool                  isNull = false;
    uint64_t              count = 0;
};


template <typename T, typename V>
std::unique_ptr<NODE> makeAtom( const V& aValue, std::optional<int32_t> aLine )
{
    if( aLine )
        return std::make_unique<T>( aValue, *aLine );
    return std::make_unique<T>( aValue );
}


PARSED parseNode( const JSON& aValue )
{
    try
    {
        if( !aValue.is_array() || aValue.empty() || !aValue[0].is_string() )
            refuse( "invalid node" );
        const std::string kind = aValue[0].get<std::string>();
        PARSED parsed;
        if( kind == "null" )
        {
            if( aValue.size() != 1 )
                refuse( "invalid node" );
            parsed.isNull = true;
            return parsed;
        }
        if( aValue.size() != 3 )
            refuse( "invalid node" );
        if( kind == "list" )
        {
            const uint64_t count = readU64( aValue[2] );
            if( count > MAX_NODES )
                refuse( "invalid node" );
            const std::optional<int32_t> line = readLine( aValue[1] );
            parsed.node = line ? std::make_unique<SEXPR_LIST>( *line )
                               : std::make_unique<SEXPR_LIST>();
            parsed.isList = true;
            parsed.count = count;
        }
        else if( kind == "integer" )
        {
            const int64_t value = readI64( aValue[1] );
            parsed.node = makeAtom<SEXPR::SEXPR_INTEGER>( value, readLine( aValue[2] ) );
        }
        else if( kind == "double" )
        {
            const double value = readF64( aValue[1] );
            parsed.node = makeAtom<SEXPR::SEXPR_DOUBLE>( value, readLine( aValue[2] ) );
        }
        else if( kind == "string" )
        {
            const std::string value = readHex( aValue[1] );
            parsed.node = makeAtom<SEXPR::SEXPR_STRING>( value, readLine( aValue[2] ) );
        }
        else if( kind == "symbol" )
        {
            const std::string value = readHex( aValue[1] );
            parsed.node = makeAtom<SEXPR::SEXPR_SYMBOL>( value, readLine( aValue[2] ) );
        }
        else
        {
            refuse( "invalid node" );
        }
        return parsed;
    }
    catch( const std::invalid_argument& )
    {
        refuse( "invalid node" );
    }
}


// Children are added through the public m_children member as soon as they are created, so
// the root owns every node built so far if a later node is refused.
std::unique_ptr<NODE> readTree( const JSON& aValue )
{
    if( !aValue.is_array() )
        refuse( "expected tree" );
    if( aValue.empty() )
        refuse( "empty tree" );
    if( aValue.size() > MAX_NODES )
        refuse( "tree too large" );
    std::unique_ptr<NODE> root;
    struct FRAME
    {
        SEXPR_LIST* list;
        uint64_t    remaining;
    };
    std::vector<FRAME> stack;
    for( size_t i = 0; i < aValue.size(); ++i )
    {
        if( stack.empty() && i > 0 )
            refuse( "trailing nodes" );
        PARSED parsed = parseNode( aValue[i] );
        if( stack.empty() && parsed.isNull )
            refuse( "null root" );
        if( stack.size() + 1 > MAX_DEPTH )
            refuse( "tree too deep" );
        NODE* raw = parsed.node.get();
        if( stack.empty() )
        {
            root = std::move( parsed.node );
        }
        else
        {
            stack.back().list->m_children.push_back( parsed.node.release() );
            stack.back().remaining--;
        }
        if( parsed.isList )
            stack.push_back( { static_cast<SEXPR_LIST*>( raw ), parsed.count } );
        while( !stack.empty() && stack.back().remaining == 0 )
            stack.pop_back();
    }
    if( !stack.empty() )
        refuse( "truncated tree" );
    return root;
}


std::unique_ptr<SEXPR_LIST> readList( const JSON& aValue )
{
    std::unique_ptr<NODE> root = readTree( aValue );
    if( !root->IsList() )
        refuse( "expected list root" );
    return std::unique_ptr<SEXPR_LIST>( static_cast<SEXPR_LIST*>( root.release() ) );
}


std::unique_ptr<NODE> readOptionalTree( const JSON& aValue )
{
    if( aValue.is_null() )
        return nullptr;
    return readTree( aValue );
}


JSON entry( const NODE* aNode )
{
    if( !aNode )
        return JSON::array( { "null" } );
    const JSON line = static_cast<uint64_t>( aNode->GetLineNumber() );
    if( aNode->IsList() )
    {
        const auto* list = static_cast<const SEXPR_LIST*>( aNode );
        return JSON::array( { "list", line, static_cast<uint64_t>( list->m_children.size() ) } );
    }
    if( aNode->IsInteger() )
        return JSON::array(
                { "integer", static_cast<const SEXPR::SEXPR_INTEGER*>( aNode )->m_value, line } );
    if( aNode->IsDouble() )
        return JSON::array(
                { "double", bits64( static_cast<const SEXPR::SEXPR_DOUBLE*>( aNode )->m_value ),
                  line } );
    if( aNode->IsString() )
        return JSON::array(
                { "string", hex( static_cast<const SEXPR::SEXPR_STRING*>( aNode )->m_value ),
                  line } );
    if( aNode->IsSymbol() )
        return JSON::array(
                { "symbol", hex( static_cast<const SEXPR::SEXPR_SYMBOL*>( aNode )->m_value ),
                  line } );
    throw std::logic_error( "node without a type" );
}


// Visit a node and its descendants in pre-order without recursion.
template <typename F>
void preorder( const NODE* aRoot, F aVisit )
{
    aVisit( aRoot );
    std::vector<std::pair<const SEXPR_LIST*, size_t>> stack;
    if( aRoot && aRoot->IsList() )
        stack.emplace_back( static_cast<const SEXPR_LIST*>( aRoot ), 0 );
    while( !stack.empty() )
    {
        auto& [list, next] = stack.back();
        if( next >= list->m_children.size() )
        {
            stack.pop_back();
            continue;
        }
        const NODE* child = list->m_children[next++];
        aVisit( child );
        if( child && child->IsList() )
            stack.emplace_back( static_cast<const SEXPR_LIST*>( child ), 0 );
    }
}


JSON dump( const NODE* aRoot )
{
    JSON out = JSON::array();
    preorder( aRoot, [&]( const NODE* aNode ) { out.push_back( entry( aNode ) ); } );
    return out;
}


JSON predicates( const NODE* aRoot )
{
    JSON out = JSON::array();
    preorder( aRoot,
              [&]( const NODE* aNode )
              {
                  if( !aNode )
                  {
                      out.push_back( nullptr );
                      return;
                  }
                  out.push_back( JSON::array( { aNode->IsList(), aNode->IsSymbol(),
                                                aNode->IsString(), aNode->IsDouble(),
                                                aNode->IsInteger(),
                                                static_cast<uint64_t>( aNode->GetLineNumber() ) } ) );
              } );
    return out;
}


// ---------------------------------------------------------------------------------------
// Library calls: an exception the pinned code throws is the result.

JSON exceptionResult( const char* aType, const std::string& aWhat )
{
    return JSON{ { "exception", JSON{ { "type", aType }, { "what", hex( aWhat ) } } } };
}


template <typename F>
JSON library( F aCall )
{
    try
    {
        return aCall();
    }
    catch( const SEXPR::INVALID_TYPE_EXCEPTION& error )
    {
        return exceptionResult( "SEXPR::INVALID_TYPE_EXCEPTION", error.what() );
    }
    catch( const std::invalid_argument& error )
    {
        return exceptionResult( "std::invalid_argument", error.what() );
    }
    catch( const std::length_error& error )
    {
        return exceptionResult( "std::length_error", error.what() );
    }
    catch( const std::bad_alloc& error )
    {
        return exceptionResult( "std::bad_alloc", error.what() );
    }
}


// The {"type", "what"} of a library exception aCall throws, or null when it returns.
template <typename F>
JSON caught( F aCall )
{
    try
    {
        aCall();
        return nullptr;
    }
    catch( const SEXPR::INVALID_TYPE_EXCEPTION& error )
    {
        return JSON{ { "type", "SEXPR::INVALID_TYPE_EXCEPTION" }, { "what", hex( error.what() ) } };
    }
    catch( const std::invalid_argument& error )
    {
        return JSON{ { "type", "std::invalid_argument" }, { "what", hex( error.what() ) } };
    }
    catch( const std::length_error& error )
    {
        return JSON{ { "type", "std::length_error" }, { "what", hex( error.what() ) } };
    }
    catch( const std::bad_alloc& error )
    {
        return JSON{ { "type", "std::bad_alloc" }, { "what", hex( error.what() ) } };
    }
}


// ---------------------------------------------------------------------------------------
// Arguments.

const JSON& argList( const JSON& aValue )
{
    if( !aValue.is_array() )
        refuse( "expected arguments" );
    if( aValue.empty() || aValue.size() > MAX_ARGS )
        refuse( "argument count out of range" );
    return aValue;
}


enum class SCAN_KIND
{
    INT,
    LONG_INT,
    DOUBLE,
    STRING,
    IN_STRING,
    CONST_STRING,
    STRING_VALUE,
    C_STR
};


struct SCAN_STORE
{
    SCAN_KIND                       kind = SCAN_KIND::INT;
    int32_t                         i = 0;
    int64_t                         l = 0;
    double                          d = 0;
    std::string                     s;
    bool                            symbol = false;
    std::optional<SEXPR::_IN_STRING> in;
};

using SCAN_STORES = std::vector<std::unique_ptr<SCAN_STORE>>;


SCAN_STORES readScanArgs( const JSON& aValue )
{
    SCAN_STORES stores;
    for( const JSON& item : argList( aValue ) )
    {
        if( !item.is_object() )
            refuse( "expected object" );
        auto        store = std::make_unique<SCAN_STORE>();
        std::string kind = readText( item, "kind" );
        if( kind == "int" )
        {
            checkKeys( item, { "kind", "initial" } );
            store->kind = SCAN_KIND::INT;
            store->i = readI32( item.at( "initial" ) );
        }
        else if( kind == "long_int" )
        {
            checkKeys( item, { "kind", "initial" } );
            store->kind = SCAN_KIND::LONG_INT;
            store->l = readI64( item.at( "initial" ) );
        }
        else if( kind == "double" )
        {
            checkKeys( item, { "kind", "initial" } );
            store->kind = SCAN_KIND::DOUBLE;
            store->d = readF64( item.at( "initial" ) );
        }
        else if( kind == "string" )
        {
            checkKeys( item, { "kind", "initial" } );
            store->kind = SCAN_KIND::STRING;
            store->s = readHex( item.at( "initial" ) );
        }
        else if( kind == "in_string" )
        {
            checkKeys( item, { "kind", "symbol", "initial" } );
            store->kind = SCAN_KIND::IN_STRING;
            store->symbol = readBool( item.at( "symbol" ) );
            store->s = readHex( item.at( "initial" ) );
        }
        else if( kind == "const_string" || kind == "string_value" || kind == "c_str" )
        {
            checkKeys( item, { "kind", "value" } );
            store->kind = kind == "const_string"   ? SCAN_KIND::CONST_STRING
                          : kind == "string_value" ? SCAN_KIND::STRING_VALUE
                                                   : SCAN_KIND::C_STR;
            store->s = readHex( item.at( "value" ) );
        }
        else
        {
            refuse( "unknown argument kind" );
        }
        stores.push_back( std::move( store ) );
    }
    return stores;
}


// Each argument through the public constructor its kind selects.
std::vector<SEXPR::SEXPR_SCAN_ARG> scanArgs( SCAN_STORES& aStores )
{
    std::vector<SEXPR::SEXPR_SCAN_ARG> args;
    for( auto& store : aStores )
    {
        switch( store->kind )
        {
        case SCAN_KIND::INT: args.push_back( SEXPR::SEXPR_SCAN_ARG( &store->i ) ); break;
        case SCAN_KIND::LONG_INT: args.push_back( SEXPR::SEXPR_SCAN_ARG( &store->l ) ); break;
        case SCAN_KIND::DOUBLE: args.push_back( SEXPR::SEXPR_SCAN_ARG( &store->d ) ); break;
        case SCAN_KIND::STRING: args.push_back( SEXPR::SEXPR_SCAN_ARG( &store->s ) ); break;
        case SCAN_KIND::IN_STRING:
            if( store->symbol )
                store->in.emplace( SEXPR::AsSymbol( store->s ) );
            else
                store->in.emplace( SEXPR::AsString( store->s ) );
            args.push_back( SEXPR::SEXPR_SCAN_ARG( *store->in ) );
            break;
        case SCAN_KIND::CONST_STRING:
        {
            const std::string* value = &store->s;
            args.push_back( SEXPR::SEXPR_SCAN_ARG( value ) );
            break;
        }
        case SCAN_KIND::STRING_VALUE:
            args.push_back( SEXPR::SEXPR_SCAN_ARG( std::string( store->s ) ) );
            break;
        case SCAN_KIND::C_STR: args.push_back( SEXPR::SEXPR_SCAN_ARG( store->s.c_str() ) ); break;
        }
    }
    return args;
}


JSON scanResults( const SCAN_STORES& aStores )
{
    JSON out = JSON::array();
    for( const auto& store : aStores )
    {
        switch( store->kind )
        {
        case SCAN_KIND::INT: out.push_back( store->i ); break;
        case SCAN_KIND::LONG_INT: out.push_back( store->l ); break;
        case SCAN_KIND::DOUBLE: out.push_back( bits64( store->d ) ); break;
        case SCAN_KIND::STRING:
        case SCAN_KIND::IN_STRING: out.push_back( hex( store->s ) ); break;
        default: out.push_back( nullptr ); break;
        }
    }
    return out;
}


// Refuse a scan whose pinned doScan would reach undefined behaviour: GetChild( i ) past the
// last child, a null child, or the INT branch, which writes a double through the int32_t*.
void guardScan( const SEXPR_LIST& aList, const SCAN_STORES& aStores )
{
    for( size_t i = 0; i < aStores.size(); ++i )
    {
        if( i >= aList.m_children.size() || !aList.m_children[i] )
            refuse( UNDEFINED );
        const NODE*       child = aList.m_children[i];
        const SCAN_STORE& arg = *aStores[i];
        switch( arg.kind )
        {
        case SCAN_KIND::DOUBLE:
            if( !child->IsDouble() && !child->IsInteger() )
                return;
            break;
        case SCAN_KIND::INT:
            if( child->IsInteger() )
                refuse( UNDEFINED );
            return;
        case SCAN_KIND::STRING: break;
        case SCAN_KIND::LONG_INT:
            if( !child->IsInteger() )
                return;
            break;
        case SCAN_KIND::IN_STRING:
            if( arg.symbol ? !child->IsSymbol() : !child->IsString() )
                return;
            break;
        case SCAN_KIND::CONST_STRING:
        case SCAN_KIND::STRING_VALUE:
        case SCAN_KIND::C_STR:
        {
            const std::string compared =
                    arg.kind == SCAN_KIND::C_STR ? std::string( arg.s.c_str() ) : arg.s;
            if( child->IsSymbol()
                && static_cast<const SEXPR::SEXPR_SYMBOL*>( child )->m_value != compared )
                return;
            if( child->IsString()
                && static_cast<const SEXPR::SEXPR_STRING*>( child )->m_value != compared )
                return;
            break;
        }
        }
    }
}


struct CHILD_SPEC
{
    enum class KIND
    {
        INT,
        LONG_INT,
        DOUBLE,
        STRING,
        C_STR,
        OUT_STRING,
        SEXPR
    } kind = KIND::INT;
    int32_t               i = 0;
    int64_t               l = 0;
    double                d = 0;
    std::string           s;
    bool                  symbol = false;
    std::unique_ptr<NODE> tree;
};

using CHILD_SPECS = std::vector<std::unique_ptr<CHILD_SPEC>>;


CHILD_SPECS readChildrenArgs( const JSON& aValue )
{
    CHILD_SPECS specs;
    for( const JSON& item : argList( aValue ) )
    {
        if( !item.is_object() )
            refuse( "expected object" );
        auto        spec = std::make_unique<CHILD_SPEC>();
        std::string kind = readText( item, "kind" );
        using K = CHILD_SPEC::KIND;
        if( kind == "int" )
        {
            checkKeys( item, { "kind", "value" } );
            spec->kind = K::INT;
            spec->i = readI32( item.at( "value" ) );
        }
        else if( kind == "long_int" )
        {
            checkKeys( item, { "kind", "value" } );
            spec->kind = K::LONG_INT;
            spec->l = readI64( item.at( "value" ) );
        }
        else if( kind == "double" )
        {
            checkKeys( item, { "kind", "value" } );
            spec->kind = K::DOUBLE;
            spec->d = readF64( item.at( "value" ) );
        }
        else if( kind == "string" || kind == "c_str" )
        {
            checkKeys( item, { "kind", "value" } );
            spec->kind = kind == "string" ? K::STRING : K::C_STR;
            spec->s = readHex( item.at( "value" ) );
        }
        else if( kind == "out_string" )
        {
            checkKeys( item, { "kind", "symbol", "value" } );
            spec->kind = K::OUT_STRING;
            spec->s = readHex( item.at( "value" ) );
            spec->symbol = readBool( item.at( "symbol" ) );
        }
        else if( kind == "sexpr" )
        {
            checkKeys( item, { "kind", "tree" } );
            spec->kind = K::SEXPR;
            spec->tree = readOptionalTree( item.at( "tree" ) );
        }
        else
        {
            refuse( "unknown argument kind" );
        }
        specs.push_back( std::move( spec ) );
    }
    return specs;
}


// Each argument through the public constructor its kind selects. Tree pointers are released
// to the arguments: the list takes ownership when they are added.
std::vector<SEXPR::SEXPR_CHILDREN_ARG> childrenArgs( CHILD_SPECS& aSpecs )
{
    std::vector<SEXPR::SEXPR_CHILDREN_ARG> args;
    using K = CHILD_SPEC::KIND;
    for( auto& spec : aSpecs )
    {
        switch( spec->kind )
        {
        case K::INT: args.push_back( SEXPR::SEXPR_CHILDREN_ARG( spec->i ) ); break;
        case K::LONG_INT: args.push_back( SEXPR::SEXPR_CHILDREN_ARG( spec->l ) ); break;
        case K::DOUBLE: args.push_back( SEXPR::SEXPR_CHILDREN_ARG( spec->d ) ); break;
        case K::STRING:
        {
            const std::string& value = spec->s;
            args.push_back( SEXPR::SEXPR_CHILDREN_ARG( value ) );
            break;
        }
        case K::C_STR: args.push_back( SEXPR::SEXPR_CHILDREN_ARG( spec->s.c_str() ) ); break;
        case K::OUT_STRING:
        {
            const std::string&       value = spec->s;
            const SEXPR::_OUT_STRING out =
                    spec->symbol ? SEXPR::AsSymbol( value ) : SEXPR::AsString( value );
            args.push_back( SEXPR::SEXPR_CHILDREN_ARG( out ) );
            break;
        }
        case K::SEXPR: args.push_back( SEXPR::SEXPR_CHILDREN_ARG( spec->tree.release() ) ); break;
        }
    }
    return args;
}


// The variadic templates, instantiated for 1 to MAX_ARGS arguments.
template <size_t... I>
size_t scanN( SEXPR_LIST& aList, const std::vector<SEXPR::SEXPR_SCAN_ARG>& aArgs,
              std::index_sequence<I...> )
{
    return aList.Scan( aArgs[I]... );
}


template <size_t... I>
void addN( SEXPR_LIST& aList, const std::vector<SEXPR::SEXPR_CHILDREN_ARG>& aArgs,
           std::index_sequence<I...> )
{
    aList.AddChildren( aArgs[I]... );
}


template <size_t... I>
SEXPR_LIST* newN( const std::vector<SEXPR::SEXPR_CHILDREN_ARG>& aArgs, std::index_sequence<I...> )
{
    return new SEXPR_LIST( aArgs[I]... );
}


template <size_t... N>
size_t scanAny( SEXPR_LIST& aList, const std::vector<SEXPR::SEXPR_SCAN_ARG>& aArgs,
                std::index_sequence<N...> )
{
    size_t result = 0;
    ( ( aArgs.size() == N + 1 ? ( result = scanN( aList, aArgs, std::make_index_sequence<N + 1>{} ),
                                  0 )
                              : 0 ),
      ... );
    return result;
}


template <size_t... N>
void addAny( SEXPR_LIST& aList, const std::vector<SEXPR::SEXPR_CHILDREN_ARG>& aArgs,
             std::index_sequence<N...> )
{
    ( ( aArgs.size() == N + 1 ? ( addN( aList, aArgs, std::make_index_sequence<N + 1>{} ), 0 ) : 0 ),
      ... );
}


template <size_t... N>
SEXPR_LIST* newAny( const std::vector<SEXPR::SEXPR_CHILDREN_ARG>& aArgs, std::index_sequence<N...> )
{
    SEXPR_LIST* result = nullptr;
    ( ( aArgs.size() == N + 1 ? ( result = newN( aArgs, std::make_index_sequence<N + 1>{} ), 0 )
                              : 0 ),
      ... );
    return result;
}


// ---------------------------------------------------------------------------------------
// Operations shared by both units.

JSON scan( const JSON& aInput )
{
    checkKeys( aInput, { "op", "tree", "args" } );
    std::unique_ptr<SEXPR_LIST> list = readList( aInput.at( "tree" ) );
    SCAN_STORES                 stores = readScanArgs( aInput.at( "args" ) );
    guardScan( *list, stores );
    return library(
            [&]
            {
                std::vector<SEXPR::SEXPR_SCAN_ARG> args = scanArgs( stores );
                const size_t                       returned =
                        scanAny( *list, args, std::make_index_sequence<MAX_ARGS>{} );
                return JSON{ { "args", scanResults( stores ) },
                             { "returned", static_cast<uint64_t>( returned ) } };
            } );
}


JSON addChildren( const JSON& aInput )
{
    checkKeys( aInput, { "op", "tree", "args" } );
    std::unique_ptr<SEXPR_LIST> list = readList( aInput.at( "tree" ) );
    CHILD_SPECS                 specs = readChildrenArgs( aInput.at( "args" ) );
    return library(
            [&]
            {
                std::vector<SEXPR::SEXPR_CHILDREN_ARG> args = childrenArgs( specs );
                addAny( *list, args, std::make_index_sequence<MAX_ARGS>{} );
                return JSON{ { "in_stream_child", inStreamChild( *list ) },
                             { "tree", dump( list.get() ) } };
            } );
}


JSON constructList( const JSON& aInput )
{
    const std::string ctor = readText( aInput, "ctor" );
    if( ctor == "default" )
    {
        checkKeys( aInput, { "op", "ctor" } );
        SEXPR_LIST list;
        return JSON{ { "in_stream_child", inStreamChild( list ) }, { "tree", dump( &list ) } };
    }
    if( ctor == "line" )
    {
        checkKeys( aInput, { "op", "ctor", "line" } );
        SEXPR_LIST list( readI32( aInput.at( "line" ) ) );
        return JSON{ { "in_stream_child", inStreamChild( list ) }, { "tree", dump( &list ) } };
    }
    if( ctor == "variadic" )
    {
        checkKeys( aInput, { "op", "ctor", "args" } );
        CHILD_SPECS specs = readChildrenArgs( aInput.at( "args" ) );
        return library(
                [&]
                {
                    std::vector<SEXPR::SEXPR_CHILDREN_ARG> args = childrenArgs( specs );
                    std::unique_ptr<SEXPR_LIST>            list(
                            newAny( args, std::make_index_sequence<MAX_ARGS>{} ) );
                    return JSON{ { "in_stream_child", inStreamChild( *list ) },
                                 { "tree", dump( list.get() ) } };
                } );
    }
    refuse( "unknown constructor" );
}


// ---------------------------------------------------------------------------------------
// libs.sexpr.include.sexpr.application.pure.p001

template <typename EXCEPTION>
JSON exceptionWhat( const std::string& aMessage )
{
    const EXCEPTION object( aMessage );
    const std::string what = object.what();
    std::string       viaStd;
    try
    {
        throw EXCEPTION( aMessage );
    }
    catch( const std::exception& error )
    {
        viaStd = error.what();
    }
    return JSON{ { "what", hex( what ) }, { "what_via_std_exception", hex( viaStd ) } };
}


JSON runHeader( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );
    if( op == "predicates" )
    {
        checkKeys( aInput, { "op", "tree" } );
        std::unique_ptr<NODE> tree = readTree( aInput.at( "tree" ) );
        return JSON{ { "predicates", predicates( tree.get() ) }, { "tree", dump( tree.get() ) } };
    }
    if( op == "construct_list" )
        return constructList( aInput );
    if( op == "exception" )
    {
        checkKeys( aInput, { "op", "class", "message" } );
        const std::string message = readHex( aInput.at( "message" ) );
        const std::string cls = readText( aInput, "class" );
        if( cls == "PARSE_EXCEPTION" )
            return exceptionWhat<SEXPR::PARSE_EXCEPTION>( message );
        if( cls == "INVALID_TYPE_EXCEPTION" )
            return exceptionWhat<SEXPR::INVALID_TYPE_EXCEPTION>( message );
        refuse( "unknown exception class" );
    }
    if( op == "isexprable" )
    {
        checkKeys( aInput, { "op", "tree" } );
        std::unique_ptr<NODE> tree = readTree( aInput.at( "tree" ) );
        SEXPR::ISEXPRABLE     object;
        std::unique_ptr<NODE> serialized( object.SerializeSEXPR() );
        object.DeserializeSEXPR( *tree );
        return JSON{ { "serialized", serialized ? dump( serialized.get() ) : JSON( nullptr ) },
                     { "tree", dump( tree.get() ) } };
    }
    if( op == "out_string" )
    {
        checkKeys( aInput, { "op", "which", "value" } );
        const std::string  value = readHex( aInput.at( "value" ) );
        const std::string  which = readText( aInput, "which" );
        if( which != "symbol" && which != "string" )
            refuse( "unknown wrapper" );
        const SEXPR::_OUT_STRING out =
                which == "symbol" ? SEXPR::AsSymbol( value ) : SEXPR::AsString( value );
        return JSON{ { "same_object", &out._String == &value },
                     { "string", hex( out._String ) },
                     { "symbol", out._Symbol } };
    }
    if( op == "in_string" )
    {
        checkKeys( aInput, { "op", "which", "value", "assign" } );
        std::string       value = readHex( aInput.at( "value" ) );
        const std::string assign = readHex( aInput.at( "assign" ) );
        const std::string which = readText( aInput, "which" );
        if( which != "symbol" && which != "string" )
            refuse( "unknown wrapper" );
        SEXPR::_IN_STRING in = which == "symbol" ? SEXPR::AsSymbol( value ) : SEXPR::AsString( value );
        const bool        symbol = in._Symbol;
        const bool        same = &in._String == &value;
        in._String = assign;
        return JSON{ { "after", hex( value ) }, { "same_object", same }, { "symbol", symbol } };
    }
    if( op == "scan" )
        return scan( aInput );
    if( op == "add_children" )
        return addChildren( aInput );
    refuse( "unknown operation" );
}


// ---------------------------------------------------------------------------------------
// libs.sexpr.application.pure.p001

std::vector<uint64_t> readPath( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() > MAX_DEPTH )
        refuse( "invalid path" );
    std::vector<uint64_t> path;
    for( const JSON& step : aValue )
    {
        if( !step.is_number_unsigned() )
            refuse( "invalid path" );
        path.push_back( step.get<uint64_t>() );
    }
    return path;
}


// The node reached through m_children: every step a list, an index in range, a non-null child.
NODE* navigate( NODE* aRoot, const std::vector<uint64_t>& aPath )
{
    NODE* node = aRoot;
    for( uint64_t step : aPath )
    {
        if( !node->IsList() )
            refuse( "invalid path" );
        const SEXPR::SEXPR_VECTOR& children = static_cast<SEXPR_LIST*>( node )->m_children;
        if( step >= children.size() || !children[step] )
            refuse( "invalid path" );
        node = children[step];
    }
    return node;
}


// Walk every node AsString( aLevel ) opens, in its order, with the level it passes and the
// node's depth below aRoot (aRoot at depth 1). A null child would be dereferenced (undefined)
// and is refused; an indentation that append refuses with length_error ends the walk (the
// library throws there). Before that, an indentation above MAX_INDENT, an estimated output
// above MAX_OUTPUT or an estimated copy work above MAX_WORK (bytes times depth: each frame's
// text is appended to every enclosing frame) is refused as a resource bound.
void guardAsString( const NODE* aRoot, uint64_t aLevel )
{
    struct ITEM
    {
        const NODE* node;
        uint64_t    level;
        uint64_t    depth;
        bool        separated;
    };

    const uint64_t    maxSize = std::string().max_size();
    uint64_t          total = 0;
    uint64_t          work = 0;
    std::vector<ITEM> stack{ { aRoot, aLevel, 1, false } };
    while( !stack.empty() )
    {
        const ITEM item = stack.back();
        stack.pop_back();
        uint64_t bytes = item.separated ? 1 : 0;
        if( !item.node )
            refuse( UNDEFINED );
        if( item.node->IsList() )
        {
            const uint64_t local = item.level != 0 ? 1 : 0;
            const uint64_t indent = item.level * 2; // size_t arithmetic, modulo 2^64
            if( maxSize - local < indent )
                return;
            if( indent > MAX_INDENT )
                refuse( TOO_LARGE );
            bytes += indent + 3;
            const SEXPR::SEXPR_VECTOR& children =
                    static_cast<const SEXPR_LIST*>( item.node )->m_children;
            for( size_t i = children.size(); i-- > 0; )
                stack.push_back( { children[i], item.level + 1, item.depth + 1, i > 0 } );
        }
        else if( item.node->IsString() )
        {
            bytes += static_cast<const SEXPR::SEXPR_STRING*>( item.node )->m_value.size() + 2;
        }
        else if( item.node->IsSymbol() )
        {
            bytes += static_cast<const SEXPR::SEXPR_SYMBOL*>( item.node )->m_value.size() + 2;
        }
        else if( item.node->IsInteger() )
        {
            bytes += 12;
        }
        else
        {
            bytes += 400;
        }
        total += bytes;
        work += bytes * item.depth;
        if( total > MAX_OUTPUT || work > MAX_WORK )
            refuse( TOO_LARGE );
    }
}


// One SEXPR member on the node reached from the root through aInput.path.
JSON getter( const JSON& aInput )
{
    const std::string method = readText( aInput, "method" );
    const char*       extra = nullptr;
    if( method == "GetChild" )
        extra = "index";
    else if( method == "AsString" )
        extra = "level";
    else if( method == "AddChild" )
        extra = "child";
    else if( method != "GetChildren" && method != "GetNumberOfChildren"
             && method != "GetLongInteger" && method != "GetInteger" && method != "GetFloat"
             && method != "GetDouble" && method != "GetString" && method != "GetSymbol"
             && method != "GetList" )
        refuse( "unknown method" );
    if( extra )
        checkKeys( aInput, { "op", "tree", "path", "method", extra } );
    else
        checkKeys( aInput, { "op", "tree", "path", "method" } );
    std::unique_ptr<NODE> tree = readTree( aInput.at( "tree" ) );
    NODE*                 node = navigate( tree.get(), readPath( aInput.at( "path" ) ) );

    if( method == "GetChildren" )
    {
        return library(
                [&]
                {
                    const SEXPR::SEXPR_VECTOR* children = node->GetChildren();
                    JSON                       trees = JSON::array();
                    for( const NODE* child : *children )
                        trees.push_back( dump( child ) );
                    const bool same =
                            children == &static_cast<const SEXPR_LIST*>( node )->m_children;
                    return JSON{ { "children", trees }, { "same_vector", same } };
                } );
    }
    if( method == "GetChild" )
    {
        const uint64_t index = readU64( aInput.at( "index" ) );
        if( node->IsList() && index >= static_cast<SEXPR_LIST*>( node )->m_children.size() )
            refuse( UNDEFINED );
        return library(
                [&]
                {
                    const NODE* child = node->GetChild( index );
                    const bool  same =
                            child == static_cast<const SEXPR_LIST*>( node )->m_children[index];
                    return JSON{ { "child", child ? dump( child ) : JSON( nullptr ) },
                                 { "same_object", same } };
                } );
    }
    if( method == "GetNumberOfChildren" )
    {
        return library(
                [&] { return JSON{ { "value", uint64_t( node->GetNumberOfChildren() ) } }; } );
    }
    if( method == "GetLongInteger" )
        return library( [&] { return JSON{ { "value", node->GetLongInteger() } }; } );
    if( method == "GetInteger" )
        return library( [&] { return JSON{ { "value", node->GetInteger() } }; } );
    if( method == "GetFloat" )
        return library( [&] { return JSON{ { "value", bits32( node->GetFloat() ) } }; } );
    if( method == "GetDouble" )
        return library( [&] { return JSON{ { "value", bits64( node->GetDouble() ) } }; } );
    if( method == "GetString" )
        return library( [&] { return JSON{ { "value", hex( node->GetString() ) } }; } );
    if( method == "GetSymbol" )
        return library( [&] { return JSON{ { "value", hex( node->GetSymbol() ) } }; } );
    if( method == "GetList" )
    {
        return library(
                [&]
                {
                    SEXPR_LIST* list = node->GetList();
                    return JSON{ { "same_object", static_cast<NODE*>( list ) == node },
                                 { "tree", dump( list ) } };
                } );
    }
    if( method == "AsString" )
    {
        const JSON& level = aInput.at( "level" );
        // A null level calls AsString() with its default argument.
        const std::optional<uint64_t> value =
                level.is_null() ? std::nullopt : std::optional<uint64_t>( readU64( level ) );
        guardAsString( node, value.value_or( 0 ) );
        return library(
                [&]
                {
                    const std::string text = value ? node->AsString( *value ) : node->AsString();
                    return JSON{ { "value", hex( text ) } };
                } );
    }
    // AddChild: the list owns the child once it is added; on a throw it stays with the adapter.
    std::unique_ptr<NODE> child = readOptionalTree( aInput.at( "child" ) );
    return library(
            [&]
            {
                node->AddChild( child.get() );
                child.release();
                return JSON{ { "tree", dump( node ) } };
            } );
}


// One operator<< right operand.
struct ITEM
{
    enum class KIND
    {
        DOUBLE,
        FLOAT,
        INT64,
        INT32,
        STRING,
        OUT_STRING,
        ISEXPRABLE,
        LIST,
        SEXPR
    } kind = KIND::DOUBLE;
    double                      d = 0;
    float                       f = 0;
    int64_t                     l = 0;
    int32_t                     i = 0;
    std::string                 s;
    bool                        symbol = false;
    std::unique_ptr<SEXPR_LIST> list;
    std::unique_ptr<NODE>       tree;
};


ITEM readItem( const JSON& aValue )
{
    if( !aValue.is_object() )
        refuse( "expected object" );
    ITEM              item;
    const std::string kind = readText( aValue, "kind" );
    using K = ITEM::KIND;
    if( kind == "double" )
    {
        checkKeys( aValue, { "kind", "value" } );
        item.kind = K::DOUBLE;
        item.d = readF64( aValue.at( "value" ) );
    }
    else if( kind == "float" )
    {
        checkKeys( aValue, { "kind", "value" } );
        item.kind = K::FLOAT;
        item.f = readF32( aValue.at( "value" ) );
    }
    else if( kind == "int64" )
    {
        checkKeys( aValue, { "kind", "value" } );
        item.kind = K::INT64;
        item.l = readI64( aValue.at( "value" ) );
    }
    else if( kind == "int32" )
    {
        checkKeys( aValue, { "kind", "value" } );
        item.kind = K::INT32;
        item.i = readI32( aValue.at( "value" ) );
    }
    else if( kind == "string" )
    {
        checkKeys( aValue, { "kind", "value" } );
        item.kind = K::STRING;
        item.s = readHex( aValue.at( "value" ) );
    }
    else if( kind == "out_string" )
    {
        checkKeys( aValue, { "kind", "symbol", "value" } );
        item.kind = K::OUT_STRING;
        item.s = readHex( aValue.at( "value" ) );
        item.symbol = readBool( aValue.at( "symbol" ) );
    }
    else if( kind == "isexprable" )
    {
        checkKeys( aValue, { "kind" } );
        item.kind = K::ISEXPRABLE;
    }
    else if( kind == "list" )
    {
        checkKeys( aValue, { "kind", "tree" } );
        item.kind = K::LIST;
        if( !aValue.at( "tree" ).is_null() )
            item.list = readList( aValue.at( "tree" ) );
    }
    else if( kind == "sexpr" )
    {
        checkKeys( aValue, { "kind", "tree" } );
        item.kind = K::SEXPR;
        item.tree = readOptionalTree( aValue.at( "tree" ) );
    }
    else
    {
        refuse( "unknown argument kind" );
    }
    return item;
}


// list << item << ...: each operator<< overload on the list, with the list it returns.
JSON insert( const JSON& aInput )
{
    checkKeys( aInput, { "op", "tree", "items" } );
    std::unique_ptr<SEXPR_LIST> list = readList( aInput.at( "tree" ) );
    std::vector<ITEM>           items;
    for( const JSON& item : argList( aInput.at( "items" ) ) )
        items.push_back( readItem( item ) );
    return library(
            [&]
            {
                bool returnsSelf = true;
                for( ITEM& item : items )
                {
                    SEXPR_LIST* returned = nullptr;
                    using K = ITEM::KIND;
                    switch( item.kind )
                    {
                    case K::DOUBLE: returned = &( *list << item.d ); break;
                    case K::FLOAT: returned = &( *list << item.f ); break;
                    case K::INT64: returned = &( *list << item.l ); break;
                    case K::INT32: returned = &( *list << item.i ); break;
                    case K::STRING: returned = &( *list << std::string( item.s ) ); break;
                    case K::OUT_STRING:
                    {
                        const std::string& value = item.s;
                        returned = &( *list << ( item.symbol ? SEXPR::AsSymbol( value )
                                                             : SEXPR::AsString( value ) ) );
                        break;
                    }
                    case K::ISEXPRABLE:
                    {
                        const SEXPR::ISEXPRABLE object{};
                        returned = &( *list << object );
                        break;
                    }
                    case K::LIST: returned = &( *list << item.list.release() ); break;
                    case K::SEXPR: returned = &( *list << item.tree.release() ); break;
                    }
                    returnsSelf = returnsSelf && returned == list.get();
                }
                return JSON{ { "in_stream_child", inStreamChild( *list ) },
                             { "returns_self", returnsSelf },
                             { "tree", dump( list.get() ) } };
            } );
}


// One operator>> right operand and the value it holds.
struct TARGET
{
    enum class KIND
    {
        INT32,
        INT64,
        FLOAT,
        DOUBLE,
        STRING,
        IN_STRING,
        ISEXPRABLE
    } kind = KIND::INT32;
    int32_t     i = 0;
    int64_t     l = 0;
    float       f = 0;
    double      d = 0;
    std::string s;
    bool        symbol = false;
};


TARGET readTarget( const JSON& aValue )
{
    if( !aValue.is_object() )
        refuse( "expected object" );
    TARGET            target;
    const std::string kind = readText( aValue, "kind" );
    using K = TARGET::KIND;
    if( kind == "int32" )
    {
        checkKeys( aValue, { "kind", "initial" } );
        target.kind = K::INT32;
        target.i = readI32( aValue.at( "initial" ) );
    }
    else if( kind == "int64" )
    {
        checkKeys( aValue, { "kind", "initial" } );
        target.kind = K::INT64;
        target.l = readI64( aValue.at( "initial" ) );
    }
    else if( kind == "float" )
    {
        checkKeys( aValue, { "kind", "initial" } );
        target.kind = K::FLOAT;
        target.f = readF32( aValue.at( "initial" ) );
    }
    else if( kind == "double" )
    {
        checkKeys( aValue, { "kind", "initial" } );
        target.kind = K::DOUBLE;
        target.d = readF64( aValue.at( "initial" ) );
    }
    else if( kind == "string" )
    {
        checkKeys( aValue, { "kind", "initial" } );
        target.kind = K::STRING;
        target.s = readHex( aValue.at( "initial" ) );
    }
    else if( kind == "in_string" )
    {
        checkKeys( aValue, { "kind", "symbol", "initial" } );
        target.kind = K::IN_STRING;
        target.symbol = readBool( aValue.at( "symbol" ) );
        target.s = readHex( aValue.at( "initial" ) );
    }
    else if( kind == "isexprable" )
    {
        checkKeys( aValue, { "kind" } );
        target.kind = K::ISEXPRABLE;
    }
    else
    {
        refuse( "unknown argument kind" );
    }
    return target;
}


// list >> target >> ...: each operator>> overload in turn; the chain stops at the first
// exception, reported with every target's value. Every overload but the ISEXPRABLE one
// dereferences GetChild( m_inStreamChild ), which is refused when it is past the last child
// or null.
JSON extract( const JSON& aInput )
{
    checkKeys( aInput, { "op", "tree", "targets" } );
    std::unique_ptr<SEXPR_LIST> list = readList( aInput.at( "tree" ) );
    std::vector<TARGET>         targets;
    for( const JSON& target : argList( aInput.at( "targets" ) ) )
        targets.push_back( readTarget( target ) );
    uint64_t completed = 0;
    bool     returnsSelf = true;
    JSON     raised = nullptr;
    using K = TARGET::KIND;
    for( TARGET& target : targets )
    {
        if( target.kind != K::ISEXPRABLE )
        {
            const int index = inStreamChild( *list );
            if( index < 0 || static_cast<size_t>( index ) >= list->m_children.size()
                || !list->m_children[index] )
                refuse( UNDEFINED );
        }
        SEXPR_LIST* returned = nullptr;
        raised = caught(
                [&]
                {
                    switch( target.kind )
                    {
                    case K::INT32: returned = &( *list >> target.i ); break;
                    case K::INT64: returned = &( *list >> target.l ); break;
                    case K::FLOAT: returned = &( *list >> target.f ); break;
                    case K::DOUBLE: returned = &( *list >> target.d ); break;
                    case K::STRING: returned = &( *list >> target.s ); break;
                    case K::IN_STRING:
                        returned = &( *list >> ( target.symbol ? SEXPR::AsSymbol( target.s )
                                                               : SEXPR::AsString( target.s ) ) );
                        break;
                    case K::ISEXPRABLE:
                    {
                        SEXPR::ISEXPRABLE object;
                        returned = &( *list >> object );
                        break;
                    }
                    }
                } );
        if( !raised.is_null() )
            break;
        returnsSelf = returnsSelf && returned == list.get();
        completed++;
    }
    JSON values = JSON::array();
    for( const TARGET& target : targets )
    {
        switch( target.kind )
        {
        case K::INT32: values.push_back( target.i ); break;
        case K::INT64: values.push_back( target.l ); break;
        case K::FLOAT: values.push_back( bits32( target.f ) ); break;
        case K::DOUBLE: values.push_back( bits64( target.d ) ); break;
        case K::STRING:
        case K::IN_STRING: values.push_back( hex( target.s ) ); break;
        case K::ISEXPRABLE: values.push_back( nullptr ); break;
        }
    }
    return JSON{ { "completed", completed },
                 { "exception", raised },
                 { "in_stream_child", inStreamChild( *list ) },
                 { "returns_self", returnsSelf },
                 { "targets", values } };
}


JSON runSource( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );
    if( op == "getter" )
        return getter( aInput );
    if( op == "insert" )
        return insert( aInput );
    if( op == "extract" )
        return extract( aInput );
    if( op == "scan" )
        return scan( aInput );
    if( op == "add_children" )
        return addChildren( aInput );
    if( op == "construct_list" )
        return constructList( aInput );
    refuse( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterSexprInclude( REGISTRY& aRegistry )
{
    aRegistry.Add( HEADER_UNIT, "libs/sexpr/include/sexpr/*.h", runHeader );
}


void RegisterSexprSource( REGISTRY& aRegistry )
{
    aRegistry.Add( SOURCE_UNIT, "libs/sexpr/sexpr.cpp", runSource );
}
} // namespace oracle
