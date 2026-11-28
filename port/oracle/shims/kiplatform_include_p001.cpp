// SPDX-License-Identifier: GPL-3.0-or-later
// Adapter for libs.kiplatform.include.kiplatform.application.pure.p001: the public headers
// libs/kiplatform/include/kiplatform/{app,drivers,environment,io,secrets,sysinfo,
// sysinfo_gtk,touchpad,ui,webview}.h at the pin, all included unchanged.
//
// It observes what the headers define: constants and enumerator values, initialization of
// the header's aggregates, the defaulted FILE_LOCK constructor, the gesture callback type,
// the virtual destructor of the touchpad handler base, and (through
// reference/kiplatform_include_probe.cpp, which compiles the same header text in a renamed
// namespace) the inline MAPPED_FILE accessors and the nullptr default arguments. No
// platform body (os/unix, os/common, port/wxgtk) is linked or called.
#include "registry.h"
#include "reference/kiplatform_include_probe.h"

#include <kiplatform/app.h>
#include <kiplatform/drivers.h>
#include <kiplatform/environment.h>
#include <kiplatform/io.h>
#include <kiplatform/secrets.h>
#include <kiplatform/sysinfo.h>
#include <kiplatform/touchpad.h>
#include <kiplatform/ui.h>
#include <kiplatform/webview.h>

#include <wx/string.h>

#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <functional>
#include <initializer_list>
#include <limits>
#include <memory>
#include <new>
#include <stdexcept>
#include <string>
#include <vector>

namespace
{
using oracle::JSON;
using kiplatform_include_probe::CALL;
using kiplatform_include_probe::MAPPED_STATE;
namespace IO = KIPLATFORM::IO;
namespace UI = KIPLATFORM::UI;

constexpr const char* INCLUDE_UNIT = "libs.kiplatform.include.kiplatform.application.pure.p001";
// Request bounds, identical in the Rust adapter.
constexpr size_t MAX_UNITS = 4096;
constexpr size_t MAX_BYTES = 4096;
constexpr size_t MAX_ENTRIES = 64;
constexpr size_t HANDLER_SLOTS = 8;

static_assert( wxUSE_UNICODE_WCHAR && !wxUSE_UNICODE_UTF8 && sizeof( wchar_t ) == 4
                       && WCHAR_MIN < 0,
               "strings are encoded as signed 32-bit wchar_t units" );


// Read the private POSIX members of FILE_LOCK. Access checking does not apply to the names in
// an explicit instantiation ([temp.spec.general]), so these member pointers are well defined.
template <typename TAG, typename TAG::TYPE MEMBER>
struct MEMBER_ACCESS
{
    friend typename TAG::TYPE Member( TAG ) { return MEMBER; }
};

struct FD_MEMBER
{
    using TYPE = int IO::FILE_LOCK::*;
    friend TYPE Member( FD_MEMBER );
};

struct STATE_MEMBER
{
    using TYPE = IO::FILE_LOCK::STATE IO::FILE_LOCK::*;
    friend TYPE Member( STATE_MEMBER );
};

template struct MEMBER_ACCESS<FD_MEMBER, &IO::FILE_LOCK::m_fd>;
template struct MEMBER_ACCESS<STATE_MEMBER, &IO::FILE_LOCK::m_state>;


void checkKeys( const JSON& aInput, std::initializer_list<const char*> aFields,
                const std::string& aWhere )
{
    const std::string message =
            aWhere.empty() ? "unexpected or missing field" : "unexpected or missing field: " + aWhere;

    if( !aInput.is_object() || aInput.size() != aFields.size() )
        throw std::invalid_argument( message );

    for( const char* field : aFields )
    {
        if( !aInput.contains( field ) )
            throw std::invalid_argument( message );
    }
}


std::string readText( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + aName );

    return aInput.at( aName ).get<std::string>();
}


// Integer in [aMin, aMax]. nlohmann stores "-0" as a signed zero, accepted here.
int64_t readInteger( const JSON& aValue, const std::string& aName, int64_t aMin, int64_t aMax )
{
    if( !aValue.is_number_integer() )
        throw std::invalid_argument( "integer out of range: " + aName );

    if( aValue.is_number_unsigned() )
    {
        const uint64_t number = aValue.get<uint64_t>();

        if( number > static_cast<uint64_t>( aMax ) )
            throw std::invalid_argument( "integer out of range: " + aName );

        return static_cast<int64_t>( number );
    }

    const int64_t number = aValue.get<int64_t>();

    if( number < aMin || number > aMax )
        throw std::invalid_argument( "integer out of range: " + aName );

    return number;
}


bool readBool( const JSON& aValue, const std::string& aName )
{
    if( !aValue.is_boolean() )
        throw std::invalid_argument( "expected boolean: " + aName );

    return aValue.get<bool>();
}


const JSON& readArray( const JSON& aValue, const std::string& aName, size_t aMax )
{
    if( !aValue.is_array() )
        throw std::invalid_argument( "expected array: " + aName );

    if( aValue.size() > aMax )
        throw std::invalid_argument( "too many entries: " + aName );

    return aValue;
}


// A wxString as its signed 32-bit wchar_t units.
std::wstring readUnits( const JSON& aValue, const std::string& aName )
{
    std::wstring units;

    for( const JSON& unit : readArray( aValue, aName, MAX_UNITS ) )
    {
        units.push_back( static_cast<wchar_t>( readInteger( unit, aName,
                                                            std::numeric_limits<int32_t>::min(),
                                                            std::numeric_limits<int32_t>::max() ) ) );
    }

    return units;
}


// Bytes travel as lower-case hex so every value is exact.
std::vector<uint8_t> readBytes( const JSON& aValue, const std::string& aName )
{
    if( !aValue.is_string() )
        throw std::invalid_argument( "invalid hex: " + aName );

    const std::string& text = aValue.get_ref<const std::string&>();

    if( text.size() % 2 != 0 || text.size() > 2 * MAX_BYTES
        || text.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
    {
        throw std::invalid_argument( "invalid hex: " + aName );
    }

    std::vector<uint8_t> bytes;

    for( size_t i = 0; i < text.size(); i += 2 )
        bytes.push_back( static_cast<uint8_t>( std::stoi( text.substr( i, 2 ), nullptr, 16 ) ) );

    return bytes;
}


// A double as the 16 lower-case hex digits of its IEEE-754 bits.
double readDouble( const JSON& aValue, const std::string& aName )
{
    if( !aValue.is_string() || aValue.get_ref<const std::string&>().size() != 16
        || aValue.get_ref<const std::string&>().find_first_not_of( "0123456789abcdef" )
                   != std::string::npos )
    {
        throw std::invalid_argument( "invalid double: " + aName );
    }

    const uint64_t bits = std::stoull( aValue.get<std::string>(), nullptr, 16 );
    double         value;
    std::memcpy( &value, &bits, sizeof( value ) );
    return value;
}


JSON hex( const uint8_t* aData, size_t aSize )
{
    static constexpr char digits[] = "0123456789abcdef";
    std::string text;

    for( size_t i = 0; i < aSize; ++i )
    {
        text.push_back( digits[aData[i] >> 4] );
        text.push_back( digits[aData[i] & 15] );
    }

    return text;
}


JSON hex( const std::string& aBytes )
{
    return hex( reinterpret_cast<const uint8_t*>( aBytes.data() ), aBytes.size() );
}


JSON bits( double aValue )
{
    uint64_t number;
    std::memcpy( &number, &aValue, sizeof( number ) );
    char text[17];
    std::snprintf( text, sizeof( text ), "%016llx", static_cast<unsigned long long>( number ) );
    return std::string( text );
}


JSON units( const std::wstring& aUnits )
{
    JSON array = JSON::array();

    for( const wchar_t unit : aUnits )
        array.push_back( static_cast<int32_t>( unit ) );

    return array;
}


JSON units( const wxString& aText )
{
    return units( std::wstring( aText.wc_str(), aText.length() ) );
}


wxString text( const std::wstring& aUnits )
{
    const wxString result( aUnits.data(), aUnits.size() );

    if( std::wstring( result.wc_str(), result.length() ) != aUnits )
        throw std::logic_error( "wxString did not retain every unit" );

    return result;
}


JSON gestureJson( const UI::TOUCHPAD_GESTURE& aGesture )
{
    return { { "pan_x", bits( aGesture.panX ) },
             { "pan_y", bits( aGesture.panY ) },
             { "zoom_factor", bits( aGesture.zoomFactor ) },
             { "zoom_anchor", JSON::array( { aGesture.zoomAnchor.x, aGesture.zoomAnchor.y } ) } };
}


JSON attrsJson( const IO::TARGET_ATTRS& aAttrs )
{
    return { { "value", aAttrs.value }, { "captured", aAttrs.captured } };
}


JSON proxyJson( const KIPLATFORM::ENV::PROXY_CONFIG& aConfig )
{
    return { { "host", units( aConfig.host ) },
             { "username", units( aConfig.username ) },
             { "password", units( aConfig.password ) } };
}


JSON lockJson( const IO::FILE_LOCK& aLock )
{
    return { { "fd", aLock.*Member( FD_MEMBER() ) },
             { "state", static_cast<int>( aLock.*Member( STATE_MEMBER() ) ) } };
}


// Construct a T in storage first filled with 0xa5 bytes, so a member the initialization
// does not write is visible, report it, then destroy it.
template <typename T, typename MAKE, typename REPORT>
JSON fresh( MAKE aMake, REPORT aReport )
{
    alignas( T ) unsigned char storage[sizeof( T )];
    std::memset( storage, 0xa5, sizeof( storage ) );
    T*   object = aMake( static_cast<void*>( storage ) );
    JSON result = aReport( *object );
    object->~T();
    return result;
}


JSON constants()
{
    using STATE = IO::FILE_LOCK::STATE;
    return { { "cloud_sync_buffer_size", IO::CLOUD_SYNC_BUFFER_SIZE },
             { "file_lock_states",
               { { "NONE", static_cast<int>( STATE::NONE ) },
                 { "HELD", static_cast<int>( STATE::HELD ) },
                 { "BUSY", static_cast<int>( STATE::BUSY ) },
                 { "UNSUPPORTED", static_cast<int>( STATE::UNSUPPORTED ) } } },
             { "native_display_types",
               { { "DISPLAY_UNKNOWN", static_cast<int>( UI::DISPLAY_UNKNOWN ) },
                 { "DISPLAY_X11", static_cast<int>( UI::DISPLAY_X11 ) },
                 { "DISPLAY_WAYLAND", static_cast<int>( UI::DISPLAY_WAYLAND ) },
                 { "DISPLAY_WIN32", static_cast<int>( UI::DISPLAY_WIN32 ) },
                 { "DISPLAY_COCOA", static_cast<int>( UI::DISPLAY_COCOA ) } } } };
}


JSON valueInit()
{
    using KIPLATFORM::ENV::PROXY_CONFIG;
    JSON result;

    // FILE_LOCK's destructor (os/common/io.cpp) is not linked: placement new never
    // destroys, and a FILE_LOCK holds nothing to release, so the object is abandoned.
    {
        alignas( IO::FILE_LOCK ) unsigned char storage[sizeof( IO::FILE_LOCK )];
        std::memset( storage, 0xa5, sizeof( storage ) );
        result["file_lock_default_init"] = lockJson( *::new( storage ) IO::FILE_LOCK );
        std::memset( storage, 0xa5, sizeof( storage ) );
        result["file_lock_value_init"] = lockJson( *::new( storage ) IO::FILE_LOCK() );
    }

    result["target_attrs_default_init"] = fresh<IO::TARGET_ATTRS>(
            []( void* p ) { return ::new( p ) IO::TARGET_ATTRS; }, attrsJson );
    result["target_attrs_value_init"] = fresh<IO::TARGET_ATTRS>(
            []( void* p ) { return ::new( p ) IO::TARGET_ATTRS{}; }, attrsJson );
    result["proxy_config_default_init"] = fresh<PROXY_CONFIG>(
            []( void* p ) { return ::new( p ) PROXY_CONFIG; }, proxyJson );
    result["proxy_config_value_init"] = fresh<PROXY_CONFIG>(
            []( void* p ) { return ::new( p ) PROXY_CONFIG{}; }, proxyJson );
    result["touchpad_gesture_value_init"] = fresh<UI::TOUCHPAD_GESTURE>(
            []( void* p ) { return ::new( p ) UI::TOUCHPAD_GESTURE{}; }, gestureJson );
    result["gpu_info_value_init"] = fresh<KIPLATFORM::GPU_INFO>(
            []( void* p ) { return ::new( p ) KIPLATFORM::GPU_INFO{}; },
            []( const KIPLATFORM::GPU_INFO& aInfo ) -> JSON
            {
                return { { "name", hex( aInfo.Name ) },
                         { "memory_size", static_cast<int64_t>( aInfo.MemorySize ) },
                         { "driver_version", hex( aInfo.DriverVersion ) },
                         { "manufacturer", hex( aInfo.Manufacturer ) } };
            } );
    result["cpu_info_value_init"] = fresh<KIPLATFORM::CPU_INFO>(
            []( void* p ) { return ::new( p ) KIPLATFORM::CPU_INFO{}; },
            []( const KIPLATFORM::CPU_INFO& aInfo ) -> JSON
            {
                return { { "name", hex( aInfo.Name ) },
                         { "manufacturer", hex( aInfo.Manufacturer ) },
                         { "number_cores", aInfo.NumberCores },
                         { "number_logical", aInfo.NumberLogical } };
            } );
    result["memory_info_value_init"] = fresh<KIPLATFORM::MEMORY_INFO>(
            []( void* p ) { return ::new( p ) KIPLATFORM::MEMORY_INFO{}; },
            []( const KIPLATFORM::MEMORY_INFO& aInfo ) -> JSON
            {
                return { { "usage", aInfo.Usage },
                         { "total_physical", aInfo.TotalPhysical },
                         { "free_physical", aInfo.FreePhysical },
                         { "total_paging", aInfo.TotalPaging },
                         { "free_paging", aInfo.FreePaging },
                         { "total_virtual", aInfo.TotalVirtual },
                         { "free_virtual", aInfo.FreeVirtual } };
            } );
    return result;
}


JSON targetAttrs( const JSON& aInput )
{
    const bool both = aInput.contains( "captured" );

    if( both )
        checkKeys( aInput, { "op", "value", "captured" }, "" );
    else
        checkKeys( aInput, { "op", "value" }, "" );

    const uint32_t value = static_cast<uint32_t>(
            readInteger( aInput.at( "value" ), "value", 0, std::numeric_limits<uint32_t>::max() ) );
    // Only the brace form changes with "captured"; an omitted member takes its default
    // member initializer.
    const IO::TARGET_ATTRS braced = both ? IO::TARGET_ATTRS{ value, readBool( aInput.at( "captured" ),
                                                                            "captured" ) }
                                         : IO::TARGET_ATTRS{ value };
    const IO::TARGET_ATTRS copy = braced;
    return { { "braced", attrsJson( braced ) }, { "copy", attrsJson( copy ) } };
}


JSON proxyConfig( const JSON& aInput )
{
    checkKeys( aInput, { "op", "host", "username", "password" }, "" );
    const std::wstring host = readUnits( aInput.at( "host" ), "host" );
    const std::wstring username = readUnits( aInput.at( "username" ), "username" );
    const std::wstring password = readUnits( aInput.at( "password" ), "password" );
    KIPLATFORM::ENV::PROXY_CONFIG config;
    JSON                          result = { { "default", proxyJson( config ) } };
    config.host = text( host );
    config.username = text( username );
    config.password = text( password );
    const KIPLATFORM::ENV::PROXY_CONFIG copy( config );
    result["assigned"] = proxyJson( config );
    result["copy"] = proxyJson( copy );
    return result;
}


JSON touchpadCallback( const JSON& aInput )
{
    checkKeys( aInput, { "op", "callback", "gestures" }, "" );
    const std::string kind = readText( aInput, "callback" );

    if( kind != "set" && kind != "empty" )
        throw std::invalid_argument( "unknown callback" );

    std::vector<UI::TOUCHPAD_GESTURE> gestures;

    for( const JSON& entry : readArray( aInput.at( "gestures" ), "gestures", MAX_ENTRIES ) )
    {
        checkKeys( entry, { "pan_x", "pan_y", "zoom_factor", "zoom_anchor" }, "gestures" );
        UI::TOUCHPAD_GESTURE gesture{};
        gesture.panX = readDouble( entry.at( "pan_x" ), "gestures.pan_x" );
        gesture.panY = readDouble( entry.at( "pan_y" ), "gestures.pan_y" );
        gesture.zoomFactor = readDouble( entry.at( "zoom_factor" ), "gestures.zoom_factor" );
        const JSON& anchor = entry.at( "zoom_anchor" );

        if( !anchor.is_array() || anchor.size() != 2 )
            throw std::invalid_argument( "expected point: gestures.zoom_anchor" );

        const int64_t minimum = std::numeric_limits<int32_t>::min();
        const int64_t maximum = std::numeric_limits<int32_t>::max();
        gesture.zoomAnchor.x = static_cast<int>(
                readInteger( anchor.at( 0 ), "gestures.zoom_anchor", minimum, maximum ) );
        gesture.zoomAnchor.y = static_cast<int>(
                readInteger( anchor.at( 1 ), "gestures.zoom_anchor", minimum, maximum ) );
        gestures.push_back( gesture );
    }

    std::vector<UI::TOUCHPAD_GESTURE> received;
    UI::TOUCHPAD_GESTURE_CALLBACK     callback;

    if( kind == "set" )
    {
        callback = [&received]( const UI::TOUCHPAD_GESTURE& aGesture )
        {
            received.push_back( aGesture );
        };
    }

    JSON calls = JSON::array();

    for( const UI::TOUCHPAD_GESTURE& gesture : gestures )
    {
        try
        {
            const size_t before = received.size();
            callback( gesture );

            if( received.size() != before + 1 )
                throw std::logic_error( "callback target did not run once" );

            calls.push_back( { { "received", gestureJson( received.back() ) } } );
        }
        catch( const std::bad_function_call& )
        {
            calls.push_back( { { "error", "bad_function_call" } } );
        }
    }

    return { { "is_set", static_cast<bool>( callback ) }, { "calls", std::move( calls ) } };
}


// A derived handler that records its construction and destruction.
class RECORDING_HANDLER : public UI::TOUCHPAD_GESTURE_HANDLER
{
public:
    RECORDING_HANDLER( int aId, JSON& aEvents ) : m_id( aId ), m_events( aEvents )
    {
        m_events.push_back( { { "event", "construct" }, { "id", m_id } } );
    }

    ~RECORDING_HANDLER() override
    {
        m_events.push_back( { { "event", "destroy" }, { "id", m_id } } );
    }

private:
    int   m_id;
    JSON& m_events;
};


JSON touchpadHandlers( const JSON& aInput )
{
    checkKeys( aInput, { "op", "steps" }, "" );
    std::vector<std::pair<bool, size_t>> steps;

    for( const JSON& entry : readArray( aInput.at( "steps" ), "steps", MAX_ENTRIES ) )
    {
        checkKeys( entry, { "action", "slot" }, "steps" );
        const std::string action = readText( entry, "action" );

        if( action != "create" && action != "release" )
            throw std::invalid_argument( "unknown action" );

        const size_t slot = static_cast<size_t>(
                readInteger( entry.at( "slot" ), "steps.slot", 0, HANDLER_SLOTS - 1 ) );
        steps.emplace_back( action == "create", slot );
    }

    JSON events = JSON::array();

    {
        // Owners as CreateTouchpadGestureHandler returns them: pointers to the base class.
        std::array<std::unique_ptr<UI::TOUCHPAD_GESTURE_HANDLER>, HANDLER_SLOTS> slots;
        int nextId = 0;

        for( const auto& [create, slot] : steps )
        {
            if( create )
                slots[slot] = std::make_unique<RECORDING_HANDLER>( nextId++, events );
            else
                slots[slot].reset();
        }

        // Release what is left in slot order, before the array's own destruction.
        for( std::unique_ptr<UI::TOUCHPAD_GESTURE_HANDLER>& owner : slots )
            owner.reset();
    }

    return { { "events", std::move( events ) } };
}


JSON mappedFile( const JSON& aInput )
{
    checkKeys( aInput, { "op", "state", "bytes" }, "" );
    const std::string name = readText( aInput, "state" );
    MAPPED_STATE      state;

    if( name == "empty_file" )
        state = MAPPED_STATE::EMPTY_FILE;
    else if( name == "mapped" )
        state = MAPPED_STATE::MAPPED;
    else if( name == "fallback" )
        state = MAPPED_STATE::FALLBACK;
    else
        throw std::invalid_argument( "unknown state" );

    const std::vector<uint8_t> bytes = readBytes( aInput.at( "bytes" ), "bytes" );

    // The constructor returns before mmap for an empty file, and only maps a non-empty one.
    if( state == MAPPED_STATE::EMPTY_FILE && !bytes.empty() )
        throw std::invalid_argument( "empty_file state has no bytes" );

    if( state == MAPPED_STATE::MAPPED && bytes.empty() )
        throw std::invalid_argument( "mapped state needs at least one byte" );

    const kiplatform_include_probe::MAPPED_RESULT result =
            kiplatform_include_probe::ProbeMappedFile( state, bytes );

    if( result.pointsTo == "other" )
        throw std::logic_error( "Data() points at an unknown buffer" );

    return { { "data", result.dataIsNull ? JSON( nullptr )
                                         : hex( result.bytes.data(), result.bytes.size() ) },
             { "points_to", result.pointsTo },
             { "size", result.size } };
}


JSON callJson( const CALL& aCall, bool aBoolResult, bool aFileResult )
{
    JSON strings = JSON::array();

    for( const std::wstring& value : aCall.strings )
        strings.push_back( units( value ) );

    JSON flags = JSON::object();

    for( const auto& [name, value] : aCall.flags )
        flags[name] = value;

    JSON result = { { "strings", std::move( strings ) }, { "flags", std::move( flags ) } };

    if( aCall.hasData )
        result["data"] = hex( aCall.data.data(), aCall.data.size() );

    if( aBoolResult )
        result["returned"] = aCall.returned;

    if( aFileResult )
        result["returned_null"] = aCall.returnedNull;

    return result;
}


JSON defaultArguments( const JSON& aInput )
{
    const std::string function = readText( aInput, "function" );
    size_t            count;
    bool              hasData = false;
    bool              hasReturned = true;

    if( function == "atomic_rename" )
    {
        checkKeys( aInput, { "op", "function", "strings", "explicit", "returned" }, "" );
        count = 2;
    }
    else if( function == "open_unique_sibling_temp_file" )
    {
        checkKeys( aInput, { "op", "function", "strings", "explicit" }, "" );
        count = 2;
        hasReturned = false;
    }
    else if( function == "commit_temp_file" )
    {
        checkKeys( aInput, { "op", "function", "strings", "explicit", "returned" }, "" );
        count = 2;
    }
    else if( function == "atomic_write_file" )
    {
        checkKeys( aInput, { "op", "function", "strings", "data", "explicit", "returned" }, "" );
        count = 1;
        hasData = true;
    }
    else if( function == "store_secret_async" )
    {
        checkKeys( aInput, { "op", "function", "strings", "explicit" }, "" );
        count = 3;
        hasReturned = false;
    }
    else if( function == "get_native_window_handles" )
    {
        checkKeys( aInput, { "op", "function", "explicit" }, "" );
        count = 0;
        hasReturned = false;
    }
    else
    {
        throw std::invalid_argument( "unknown function" );
    }

    std::vector<std::wstring> strings;

    if( count > 0 )
    {
        const JSON& array = readArray( aInput.at( "strings" ), "strings", count );

        if( array.size() != count )
            throw std::invalid_argument( "wrong string count" );

        for( const JSON& value : array )
            strings.push_back( readUnits( value, "strings" ) );
    }

    const std::vector<uint8_t> data =
            hasData ? readBytes( aInput.at( "data" ), "data" ) : std::vector<uint8_t>();
    const bool explicitArgument = readBool( aInput.at( "explicit" ), "explicit" );
    const bool returned = hasReturned ? readBool( aInput.at( "returned" ), "returned" ) : false;

    namespace PROBE = kiplatform_include_probe;

    if( function == "atomic_rename" )
        return callJson( PROBE::CallAtomicRename( strings[0], strings[1], explicitArgument, returned ),
                         true, false );

    if( function == "open_unique_sibling_temp_file" )
        return callJson( PROBE::CallOpenUniqueSiblingTempFile( strings[0], strings[1],
                                                               explicitArgument ),
                         false, true );

    if( function == "commit_temp_file" )
        return callJson( PROBE::CallCommitTempFile( strings[0], strings[1], explicitArgument,
                                                    returned ),
                         true, false );

    if( function == "atomic_write_file" )
        return callJson( PROBE::CallAtomicWriteFile( strings[0], data, explicitArgument, returned ),
                         true, false );

    if( function == "store_secret_async" )
        return callJson( PROBE::CallStoreSecretAsync( strings[0], strings[1], strings[2],
                                                      explicitArgument ),
                         false, false );

    return callJson( PROBE::CallGetNativeWindowHandles( explicitArgument ), false, false );
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "constants" || op == "value_init" )
    {
        checkKeys( aInput, { "op" }, "" );
        return op == "constants" ? constants() : valueInit();
    }

    if( op == "target_attrs" )
        return targetAttrs( aInput );

    if( op == "proxy_config" )
        return proxyConfig( aInput );

    if( op == "touchpad_callback" )
        return touchpadCallback( aInput );

    if( op == "touchpad_handlers" )
        return touchpadHandlers( aInput );

    if( op == "mapped_file" )
        return mappedFile( aInput );

    if( op == "default_arguments" )
        return defaultArguments( aInput );

    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterKiplatformIncludeP001( REGISTRY& aRegistry )
{
    aRegistry.Add( INCLUDE_UNIT, "libs/kiplatform/include/kiplatform/*.h", run );
}
} // namespace oracle
