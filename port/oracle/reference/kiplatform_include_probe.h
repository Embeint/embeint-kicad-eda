// SPDX-License-Identifier: GPL-3.0-or-later
// Interface of reference/kiplatform_include_probe.cpp, which compiles the unchanged pinned
// kiplatform io.h, secrets.h and ui.h with namespace KIPLATFORM renamed and supplies
// recording bodies for the declarations whose default arguments and inline members the
// header unit owns. Nothing here touches a genuine KIPLATFORM symbol.
#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

namespace kiplatform_include_probe
{
// What a probe body received, and what the call returned.
struct CALL
{
    std::vector<std::wstring>                 strings;  // wxString arguments, in order
    std::vector<uint8_t>                      data;     // AtomicWriteFile's aSize bytes
    bool                                      hasData = false;
    std::vector<std::pair<std::string, bool>> flags;    // pointer/function checks, in order
    bool                                      returned = false;
    bool                                      returnedNull = false;
};

// Call one declaration through the pinned header. aExplicit passes the defaulted
// argument explicitly (non-null); otherwise it is omitted. aReturned is what a bool
// probe body returns.
CALL CallAtomicRename( const std::wstring& aSrc, const std::wstring& aDst, bool aExplicit,
                       bool aReturned );
CALL CallOpenUniqueSiblingTempFile( const std::wstring& aTarget, const std::wstring& aMode,
                                    bool aExplicit );
CALL CallCommitTempFile( const std::wstring& aTemp, const std::wstring& aTarget, bool aExplicit,
                         bool aReturned );
CALL CallAtomicWriteFile( const std::wstring& aTarget, const std::vector<uint8_t>& aData,
                          bool aExplicit, bool aReturned );
CALL CallStoreSecretAsync( const std::wstring& aService, const std::wstring& aKey,
                           const std::wstring& aSecret, bool aExplicit );
CALL CallGetNativeWindowHandles( bool aExplicit );

enum class MAPPED_STATE
{
    EMPTY_FILE,     // the constructor's empty-file return: no mapping, m_data stays null
    MAPPED,         // mmap succeeded: m_data is the mapping, m_isMapped is set
    FALLBACK        // mmap failed: readIntoBuffer filled m_fallbackBuffer
};

struct MAPPED_RESULT
{
    bool                 dataIsNull = true;
    std::string          pointsTo;    // "null", "mapping", "fallback_buffer" or "other"
    std::vector<uint8_t> bytes;       // [Data(), Data() + Size()) when Data() is not null
    size_t               size = 0;
};

// Construct a MAPPED_FILE whose members are those the constructor leaves for aState and
// file contents aBytes, then call the header's inline Data() and Size().
MAPPED_RESULT ProbeMappedFile( MAPPED_STATE aState, const std::vector<uint8_t>& aBytes );
} // namespace kiplatform_include_probe
