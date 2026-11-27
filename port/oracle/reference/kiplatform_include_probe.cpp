// SPDX-License-Identifier: GPL-3.0-or-later
// Probe for libs.kiplatform.include.kiplatform.application.pure.p001.
//
// The unchanged pinned kiplatform/io.h, secrets.h and ui.h are compiled here with the
// namespace KIPLATFORM renamed, so this translation unit can supply bodies for declarations
// that the oracle does not otherwise link, without defining or replacing a genuine
// KIPLATFORM symbol (common/richio.cpp, linked by another adapter, references some of the
// genuine IO functions). The header text, its default arguments and its inline members are
// exactly the pinned bytes; only the namespace name differs.
//
// The bodies below are recording test doubles: they report the arguments a call through the
// header delivers, which shows the header's default arguments. The MAPPED_FILE constructor
// double leaves the members in each state the pinned Unix constructor (os/unix/io.cpp) and
// readIntoBuffer (os/common/io.cpp) can leave them, so the header's inline Data() and Size()
// run on those states without file I/O.
#define KIPLATFORM KIPLATFORM_INCLUDE_P001_PROBE
#include <kiplatform/io.h>
#include <kiplatform/secrets.h>
#include <kiplatform/ui.h>
#undef KIPLATFORM

#include "kiplatform_include_probe.h"

#include <wx/string.h>

#include <algorithm>
#include <stdexcept>

namespace PROBE = KIPLATFORM_INCLUDE_P001_PROBE;
using kiplatform_include_probe::CALL;
using kiplatform_include_probe::MAPPED_RESULT;
using kiplatform_include_probe::MAPPED_STATE;

namespace
{
// The single call in progress. The oracle answers one request at a time.
CALL                 g_call;
bool                 g_returned = false;
MAPPED_STATE         g_mappedState = MAPPED_STATE::EMPTY_FILE;
std::vector<uint8_t> g_fileBytes;
std::vector<uint8_t> g_mapping;
const uint8_t*       g_fallbackData = nullptr;


std::wstring Units( const wxString& aText )
{
    return std::wstring( aText.wc_str(), aText.length() );
}


wxString Text( const std::wstring& aUnits )
{
    const wxString text( aUnits.data(), aUnits.size() );

    if( Units( text ) != aUnits )
        throw std::logic_error( "wxString did not retain every unit" );

    return text;
}


void Begin( bool aReturned )
{
    g_call = CALL();
    g_returned = aReturned;
}


void Flag( const char* aName, bool aValue )
{
    g_call.flags.emplace_back( aName, aValue );
}
} // namespace


bool PROBE::IO::AtomicRename( const wxString& aSrc, const wxString& aDst, wxString* aError )
{
    g_call.strings = { Units( aSrc ), Units( aDst ) };
    Flag( "error_is_null", aError == nullptr );
    return g_returned;
}


FILE* PROBE::IO::OpenUniqueSiblingTempFile( const wxString& aTargetPath, const wxString& aMode,
                                            wxString* aTempPathOut, wxString* aError )
{
    g_call.strings = { Units( aTargetPath ), Units( aMode ) };
    Flag( "temp_path_out_is_null", aTempPathOut == nullptr );
    Flag( "error_is_null", aError == nullptr );
    return nullptr;
}


bool PROBE::IO::CommitTempFile( const wxString& aTempPath, const wxString& aTargetPath,
                                wxString* aError )
{
    g_call.strings = { Units( aTempPath ), Units( aTargetPath ) };
    Flag( "error_is_null", aError == nullptr );
    return g_returned;
}


bool PROBE::IO::AtomicWriteFile( const wxString& aTargetPath, const void* aData, size_t aSize,
                                 wxString* aError )
{
    g_call.strings = { Units( aTargetPath ) };
    const uint8_t* bytes = static_cast<const uint8_t*>( aData );
    g_call.data.assign( bytes, bytes + aSize );
    g_call.hasData = true;
    Flag( "error_is_null", aError == nullptr );
    return g_returned;
}


void PROBE::SECRETS::StoreSecretAsync( const wxString& aService, const wxString& aKey,
                                       const wxString& aSecret, std::function<void( bool )> aDone )
{
    g_call.strings = { Units( aService ), Units( aKey ), Units( aSecret ) };
    Flag( "done_is_empty", !aDone );
}


void PROBE::UI::GetNativeWindowHandles( wxWindow* aWindow, void** aHandle, void** aDisplay,
                                        int* aDisplayType )
{
    Flag( "window_is_null", aWindow == nullptr );
    Flag( "handle_is_null", aHandle == nullptr );
    Flag( "display_is_null", aDisplay == nullptr );
    Flag( "display_type_is_null", aDisplayType == nullptr );
}


PROBE::IO::MAPPED_FILE::MAPPED_FILE( const wxString& )
{
    switch( g_mappedState )
    {
    case MAPPED_STATE::EMPTY_FILE:
        // fstat reported zero bytes: m_size = 0 and the constructor returns before mmap.
        m_size = 0;
        return;

    case MAPPED_STATE::MAPPED:
        // mmap of a non-empty file succeeded.
        g_mapping = g_fileBytes;
        m_data = g_mapping.data();
        m_size = g_mapping.size();
        m_isMapped = true;
        return;

    case MAPPED_STATE::FALLBACK:
        // mmap failed; readIntoBuffer resized the buffer to the file length, read it and
        // pointed m_data at the buffer.
        m_fallbackBuffer.resize( g_fileBytes.size() );
        std::copy( g_fileBytes.begin(), g_fileBytes.end(), m_fallbackBuffer.begin() );
        m_data = m_fallbackBuffer.data();
        m_size = m_fallbackBuffer.size();
        g_fallbackData = m_fallbackBuffer.data();
        return;
    }
}


PROBE::IO::MAPPED_FILE::~MAPPED_FILE()
{
    // The "mapping" is the probe's own buffer; there is nothing to unmap.
}


namespace kiplatform_include_probe
{
CALL CallAtomicRename( const std::wstring& aSrc, const std::wstring& aDst, bool aExplicit,
                       bool aReturned )
{
    Begin( aReturned );
    wxString error;
    g_call.returned = aExplicit ? PROBE::IO::AtomicRename( Text( aSrc ), Text( aDst ), &error )
                                : PROBE::IO::AtomicRename( Text( aSrc ), Text( aDst ) );
    return g_call;
}


CALL CallOpenUniqueSiblingTempFile( const std::wstring& aTarget, const std::wstring& aMode,
                                    bool aExplicit )
{
    Begin( false );
    wxString tempPath;
    wxString error;
    FILE*    file = aExplicit ? PROBE::IO::OpenUniqueSiblingTempFile( Text( aTarget ), Text( aMode ),
                                                                      &tempPath, &error )
                              : PROBE::IO::OpenUniqueSiblingTempFile( Text( aTarget ), Text( aMode ),
                                                                      &tempPath );
    g_call.returnedNull = file == nullptr;
    return g_call;
}


CALL CallCommitTempFile( const std::wstring& aTemp, const std::wstring& aTarget, bool aExplicit,
                         bool aReturned )
{
    Begin( aReturned );
    wxString error;
    g_call.returned = aExplicit
                              ? PROBE::IO::CommitTempFile( Text( aTemp ), Text( aTarget ), &error )
                              : PROBE::IO::CommitTempFile( Text( aTemp ), Text( aTarget ) );
    return g_call;
}


CALL CallAtomicWriteFile( const std::wstring& aTarget, const std::vector<uint8_t>& aData,
                          bool aExplicit, bool aReturned )
{
    Begin( aReturned );
    wxString error;
    g_call.returned = aExplicit ? PROBE::IO::AtomicWriteFile( Text( aTarget ), aData.data(),
                                                              aData.size(), &error )
                                : PROBE::IO::AtomicWriteFile( Text( aTarget ), aData.data(),
                                                              aData.size() );
    return g_call;
}


CALL CallStoreSecretAsync( const std::wstring& aService, const std::wstring& aKey,
                           const std::wstring& aSecret, bool aExplicit )
{
    Begin( false );

    if( aExplicit )
        PROBE::SECRETS::StoreSecretAsync( Text( aService ), Text( aKey ), Text( aSecret ),
                                          []( bool ) {} );
    else
        PROBE::SECRETS::StoreSecretAsync( Text( aService ), Text( aKey ), Text( aSecret ) );

    return g_call;
}


CALL CallGetNativeWindowHandles( bool aExplicit )
{
    Begin( false );
    void* handle = nullptr;
    void* display = nullptr;
    int   displayType = 0;

    if( aExplicit )
        PROBE::UI::GetNativeWindowHandles( nullptr, &handle, &display, &displayType );
    else
        PROBE::UI::GetNativeWindowHandles( nullptr, &handle, &display );

    return g_call;
}


MAPPED_RESULT ProbeMappedFile( MAPPED_STATE aState, const std::vector<uint8_t>& aBytes )
{
    g_mappedState = aState;
    g_fileBytes = aBytes;
    g_mapping.clear();
    g_mapping.shrink_to_fit();
    g_fallbackData = nullptr;
    MAPPED_RESULT result;

    {
        const PROBE::IO::MAPPED_FILE file( wxString( "probe" ) );
        const uint8_t*               data = file.Data();
        result.size = file.Size();
        result.dataIsNull = data == nullptr;

        if( data == nullptr )
            result.pointsTo = "null";
        else if( !g_mapping.empty() && data == g_mapping.data() )
            result.pointsTo = "mapping";
        else if( g_fallbackData != nullptr && data == g_fallbackData )
            result.pointsTo = "fallback_buffer";
        else
            result.pointsTo = "other";

        if( data != nullptr )
            result.bytes.assign( data, data + result.size );
    }

    g_mapping.clear();
    g_mapping.shrink_to_fit();
    return result;
}
} // namespace kiplatform_include_probe
