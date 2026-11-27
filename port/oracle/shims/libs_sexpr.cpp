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
// request with "undefined sexpr operation".
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
constexpr const char* UNDEFINED = "undefined sexpr operation";
constexpr size_t MAX_NODES = 20000;
constexpr size_t MAX_DEPTH = 4000;
constexpr size_t MAX_STRING = 65536;
constexpr size_t MAX_ARGS = 16;


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
} // namespace

namespace oracle
{
void RegisterSexprInclude( REGISTRY& aRegistry )
{
    aRegistry.Add( HEADER_UNIT, "libs/sexpr/include/sexpr/*.h", runHeader );
}
} // namespace oracle
