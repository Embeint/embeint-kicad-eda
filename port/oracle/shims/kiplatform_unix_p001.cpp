// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned Unix bodies of libs/kiplatform/os/unix/drivers.cpp and sysinfo.cpp,
// linked from the audited kiplatform-unix-reference archive. The adapter decodes fully
// initialized output-argument fixtures, calls the real SYSINFO methods on a real
// object, and reports every field and whether each buffer kept its storage.
#include "registry.h"
#include <kiplatform/drivers.h>
#include <kiplatform/sysinfo.h>
#include <cstddef>
#include <cstdint>
#include <initializer_list>
#include <limits>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

namespace
{
using oracle::JSON;
constexpr const char* UNIX_PURE_UNIT = "libs.kiplatform.os.unix.application.pure.p001";
// Request bounds, identical in the Rust adapter.
constexpr size_t MAX_ENTRIES = 64;
constexpr size_t MAX_STEPS = 64;
constexpr size_t MAX_TEXT_BYTES = 4096;


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


// Unsigned field in [0, aMax]. nlohmann stores "-0" as a signed zero, accepted here.
uint64_t readUnsigned( const JSON& aValue, const std::string& aName, uint64_t aMax )
{
    if( !aValue.is_number_integer() )
        throw std::invalid_argument( "integer out of range: " + aName );

    if( aValue.is_number_unsigned() )
    {
        const uint64_t number = aValue.get<uint64_t>();

        if( number > aMax )
            throw std::invalid_argument( "integer out of range: " + aName );

        return number;
    }

    const int64_t number = aValue.get<int64_t>();

    if( number < 0 || static_cast<uint64_t>( number ) > aMax )
        throw std::invalid_argument( "integer out of range: " + aName );

    return static_cast<uint64_t>( number );
}


long long readSigned( const JSON& aValue, const std::string& aName )
{
    if( !aValue.is_number_integer() )
        throw std::invalid_argument( "integer out of range: " + aName );

    if( aValue.is_number_unsigned() )
    {
        const uint64_t number = aValue.get<uint64_t>();

        if( number > static_cast<uint64_t>( std::numeric_limits<long long>::max() ) )
            throw std::invalid_argument( "integer out of range: " + aName );

        return static_cast<long long>( number );
    }

    return aValue.get<int64_t>();
}


// std::string bytes travel as lower-case hex so NUL and non-UTF-8 bytes are exact.
std::string readBytes( const JSON& aValue, const std::string& aName )
{
    if( !aValue.is_string() )
        throw std::invalid_argument( "expected string: " + aName );

    const std::string& text = aValue.get_ref<const std::string&>();

    if( text.size() % 2 != 0 || text.size() > 2 * MAX_TEXT_BYTES
        || text.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
    {
        throw std::invalid_argument( "invalid hex: " + aName );
    }

    std::string bytes;
    bytes.reserve( text.size() / 2 );

    for( size_t i = 0; i < text.size(); i += 2 )
        bytes.push_back( static_cast<char>( std::stoi( text.substr( i, 2 ), nullptr, 16 ) ) );

    return bytes;
}


const JSON& readArray( const JSON& aInput, const char* aName, size_t aMax )
{
    const JSON& value = aInput.at( aName );

    if( !value.is_array() )
        throw std::invalid_argument( std::string( "expected array: " ) + aName );

    if( value.size() > aMax )
        throw std::invalid_argument( std::string( "too many entries: " ) + aName );

    return value;
}


JSON hex( const std::string& aBytes )
{
    static constexpr char digits[] = "0123456789abcdef";
    std::string text;
    text.reserve( aBytes.size() * 2 );

    for( const char c : aBytes )
    {
        const unsigned char byte = static_cast<unsigned char>( c );
        text.push_back( digits[byte >> 4] );
        text.push_back( digits[byte & 15] );
    }

    return text;
}


KIPLATFORM::MEMORY_INFO readMemory( const JSON& aValue )
{
    checkKeys( aValue, { "usage", "total_physical", "free_physical", "total_paging", "free_paging",
                         "total_virtual", "free_virtual" },
               "memory" );
    KIPLATFORM::MEMORY_INFO info;
    info.Usage = static_cast<uint32_t>(
            readUnsigned( aValue.at( "usage" ), "memory.usage", std::numeric_limits<uint32_t>::max() ) );
    const uint64_t max64 = std::numeric_limits<uint64_t>::max();
    info.TotalPhysical = readUnsigned( aValue.at( "total_physical" ), "memory.total_physical", max64 );
    info.FreePhysical = readUnsigned( aValue.at( "free_physical" ), "memory.free_physical", max64 );
    info.TotalPaging = readUnsigned( aValue.at( "total_paging" ), "memory.total_paging", max64 );
    info.FreePaging = readUnsigned( aValue.at( "free_paging" ), "memory.free_paging", max64 );
    info.TotalVirtual = readUnsigned( aValue.at( "total_virtual" ), "memory.total_virtual", max64 );
    info.FreeVirtual = readUnsigned( aValue.at( "free_virtual" ), "memory.free_virtual", max64 );
    return info;
}


std::vector<KIPLATFORM::CPU_INFO> readCpus( const JSON& aInput )
{
    std::vector<KIPLATFORM::CPU_INFO> cpus;
    const uint64_t max32 = std::numeric_limits<uint32_t>::max();

    for( const JSON& entry : readArray( aInput, "cpus", MAX_ENTRIES ) )
    {
        checkKeys( entry, { "name", "manufacturer", "number_cores", "number_logical" }, "cpus" );
        KIPLATFORM::CPU_INFO info;
        info.Name = readBytes( entry.at( "name" ), "cpus.name" );
        info.Manufacturer = readBytes( entry.at( "manufacturer" ), "cpus.manufacturer" );
        info.NumberCores = static_cast<uint32_t>(
                readUnsigned( entry.at( "number_cores" ), "cpus.number_cores", max32 ) );
        info.NumberLogical = static_cast<uint32_t>(
                readUnsigned( entry.at( "number_logical" ), "cpus.number_logical", max32 ) );
        cpus.push_back( std::move( info ) );
    }

    return cpus;
}


std::vector<KIPLATFORM::GPU_INFO> readGpus( const JSON& aInput )
{
    std::vector<KIPLATFORM::GPU_INFO> gpus;

    for( const JSON& entry : readArray( aInput, "gpus", MAX_ENTRIES ) )
    {
        checkKeys( entry, { "name", "memory_size", "driver_version", "manufacturer" }, "gpus" );
        KIPLATFORM::GPU_INFO info;
        info.Name = readBytes( entry.at( "name" ), "gpus.name" );
        info.MemorySize = readSigned( entry.at( "memory_size" ), "gpus.memory_size" );
        info.DriverVersion = readBytes( entry.at( "driver_version" ), "gpus.driver_version" );
        info.Manufacturer = readBytes( entry.at( "manufacturer" ), "gpus.manufacturer" );
        gpus.push_back( std::move( info ) );
    }

    return gpus;
}


// The address, length and capacity of every buffer the output arguments own, taken
// once after decoding. Only equality with a later snapshot is reported, so the
// reply never depends on an address.
struct STORAGE
{
    std::vector<const void*> data;
    std::vector<size_t>      sizes;

    void Add( const void* aData, size_t aSize, size_t aCapacity )
    {
        data.push_back( aData );
        sizes.push_back( aSize );
        sizes.push_back( aCapacity );
    }

    bool operator==( const STORAGE& aOther ) const
    {
        return data == aOther.data && sizes == aOther.sizes;
    }
};


STORAGE storage( const std::vector<KIPLATFORM::CPU_INFO>& aCpus,
                 const std::vector<KIPLATFORM::GPU_INFO>& aGpus )
{
    STORAGE result;
    result.Add( aCpus.data(), aCpus.size(), aCpus.capacity() );

    for( const KIPLATFORM::CPU_INFO& cpu : aCpus )
    {
        result.Add( cpu.Name.data(), cpu.Name.size(), cpu.Name.capacity() );
        result.Add( cpu.Manufacturer.data(), cpu.Manufacturer.size(), cpu.Manufacturer.capacity() );
    }

    result.Add( aGpus.data(), aGpus.size(), aGpus.capacity() );

    for( const KIPLATFORM::GPU_INFO& gpu : aGpus )
    {
        result.Add( gpu.Name.data(), gpu.Name.size(), gpu.Name.capacity() );
        result.Add( gpu.DriverVersion.data(), gpu.DriverVersion.size(), gpu.DriverVersion.capacity() );
        result.Add( gpu.Manufacturer.data(), gpu.Manufacturer.size(), gpu.Manufacturer.capacity() );
    }

    return result;
}


JSON state( const KIPLATFORM::MEMORY_INFO& aMemory, const std::vector<KIPLATFORM::CPU_INFO>& aCpus,
            const std::vector<KIPLATFORM::GPU_INFO>& aGpus )
{
    JSON cpus = JSON::array();

    for( const KIPLATFORM::CPU_INFO& cpu : aCpus )
    {
        cpus.push_back( { { "name", hex( cpu.Name ) },
                          { "manufacturer", hex( cpu.Manufacturer ) },
                          { "number_cores", cpu.NumberCores },
                          { "number_logical", cpu.NumberLogical } } );
    }

    JSON gpus = JSON::array();

    for( const KIPLATFORM::GPU_INFO& gpu : aGpus )
    {
        gpus.push_back( { { "name", hex( gpu.Name ) },
                          { "memory_size", static_cast<int64_t>( gpu.MemorySize ) },
                          { "driver_version", hex( gpu.DriverVersion ) },
                          { "manufacturer", hex( gpu.Manufacturer ) } } );
    }

    return { { "memory",
               { { "usage", aMemory.Usage },
                 { "total_physical", aMemory.TotalPhysical },
                 { "free_physical", aMemory.FreePhysical },
                 { "total_paging", aMemory.TotalPaging },
                 { "free_paging", aMemory.FreePaging },
                 { "total_virtual", aMemory.TotalVirtual },
                 { "free_virtual", aMemory.FreeVirtual } } },
             { "cpus", std::move( cpus ) },
             { "gpus", std::move( gpus ) } };
}


JSON runSysinfo( const JSON& aInput )
{
    checkKeys( aInput, { "op", "memory", "cpus", "gpus", "steps" }, "" );
    KIPLATFORM::MEMORY_INFO           memory = readMemory( aInput.at( "memory" ) );
    std::vector<KIPLATFORM::CPU_INFO> cpus = readCpus( aInput );
    std::vector<KIPLATFORM::GPU_INFO> gpus = readGpus( aInput );
    std::vector<std::string>          steps;

    for( const JSON& step : readArray( aInput, "steps", MAX_STEPS ) )
    {
        if( !step.is_string()
            || ( step != "memory" && step != "cpu" && step != "gpu" && step != "reconstruct" ) )
        {
            throw std::invalid_argument( "unknown step" );
        }

        steps.push_back( step.get<std::string>() );
    }

    const STORAGE initial = storage( cpus, gpus );
    JSON          result = { { "before", state( memory, cpus, gpus ) } };
    JSON          observed = JSON::array();
    std::optional<KIPLATFORM::SYSINFO> sysinfo;
    sysinfo.emplace();

    for( const std::string& step : steps )
    {
        JSON entry = { { "step", step } };

        if( step == "reconstruct" )
        {
            // Destroy the object and construct a new one in the same storage.
            sysinfo.reset();
            sysinfo.emplace();
        }
        else if( step == "memory" )
        {
            entry["returned"] = sysinfo->GetMemoryInfo( memory );
        }
        else if( step == "cpu" )
        {
            entry["returned"] = sysinfo->GetCPUInfo( cpus );
        }
        else
        {
            entry["returned"] = sysinfo->GetGPUInfo( gpus );
        }

        entry["state"] = state( memory, cpus, gpus );
        entry["storage_unchanged"] = storage( cpus, gpus ) == initial;
        observed.push_back( std::move( entry ) );
    }

    sysinfo.reset();
    result["steps"] = std::move( observed );
    result["after_destroy"] = { { "state", state( memory, cpus, gpus ) },
                                { "storage_unchanged", storage( cpus, gpus ) == initial } };
    return result;
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "valid_3dconnexion_driver_version" )
    {
        checkKeys( aInput, { "op" }, "" );
        return { { "returned", KIPLATFORM::DRIVERS::Valid3DConnexionDriverVersion() } };
    }

    if( op == "sysinfo" )
        return runSysinfo( aInput );

    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterKiplatformUnixP001( REGISTRY& aRegistry )
{
    aRegistry.Add( UNIX_PURE_UNIT, "kiplatform/os/unix/drivers.cpp, sysinfo.cpp", run );
}
} // namespace oracle
