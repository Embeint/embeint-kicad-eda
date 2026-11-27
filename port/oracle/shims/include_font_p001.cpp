// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for unit include.font.application.ui.p001: the inline bodies of the pinned
// include/font/font.h and include/font/outline_font.h. Fonts are genuine objects: an
// OUTLINE_FONT built by the pinned outline_font.cpp constructor (font-reference archive), or
// a FONT subclass defined here that overrides only the pure virtuals (and, for the scripted
// kind, the four style queries), standing for a FONT whose dynamic type keeps the defaults.
// The protected/private members the out-of-line loaders would set (m_fontName,
// m_fontFileName, m_face) are set through explicit-instantiation member pointers; m_face
// points to a value-initialised FT_FaceRec owned here with the requested style_flags. Every
// result comes from the header bodies, the pinned font.cpp/outline_font.cpp, the
// distribution wxWidgets and libstdc++. Requests whose evaluation is undefined are refused
// with "undefined font operation"; requests outside the verified domain (a name conversion
// reaching wxFAIL_MSG, a unitbuf stream whose failing sync could throw from the sentry
// destructor) with "outside the verified domain". rust/kicad-font-shim mirrors this file's
// validation order and messages.
#include "registry.h"
#include <font/font_metrics.h>
#include <font/outline_font.h>
#include <font/text_attributes.h>
#include <wx/string.h>
#include <climits>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <ios>
#include <memory>
#include <optional>
#include <ostream>
#include <stdexcept>
#include <streambuf>
#include <string>
#include <vector>

#if !wxUSE_UNICODE_WCHAR
#error "font names are modelled on the wchar_t wxString build"
#endif

static_assert( sizeof( wchar_t ) == 4 && WCHAR_MIN < 0, "names are signed 32-bit wchar_t units" );
static_assert( sizeof( FT_Long ) == 8, "style_flags is a 64-bit long" );

namespace
{
using oracle::JSON;
constexpr const char* FONT_UNIT = "include.font.application.ui.p001";
constexpr const char* UNDEFINED = "undefined font operation";
constexpr const char* OUTSIDE = "outside the verified domain";
constexpr size_t      MAX_UNITS = 4096;
constexpr int64_t     MAX_WIDTH = 4096;
constexpr int64_t     MAX_CAPACITY = 4096;
constexpr size_t      MAX_CALLS = 8;

[[noreturn]] void Fail( const std::string& aMessage )
{
    throw std::invalid_argument( aMessage );
}


/// An object with every required field and no other than the optional ones.
void Keys( const JSON& aValue, std::initializer_list<const char*> aRequired,
           std::initializer_list<const char*> aOptional = {} )
{
    if( !aValue.is_object() )
        Fail( "expected object" );

    for( const char* field : aRequired )
    {
        if( !aValue.contains( field ) )
            Fail( "unexpected or missing field" );
    }

    for( const auto& item : aValue.items() )
    {
        bool known = false;

        for( const char* field : aRequired )
            known = known || item.key() == field;

        for( const char* field : aOptional )
            known = known || item.key() == field;

        if( !known )
            Fail( "unexpected or missing field" );
    }
}


bool Bool( const JSON& aValue, const char* aField )
{
    const JSON& value = aValue.at( aField );

    if( !value.is_boolean() )
        Fail( std::string( "expected bool: " ) + aField );

    return value.get<bool>();
}


int64_t Integer( const JSON& aValue, int64_t aMin, int64_t aMax, const char* aField )
{
    if( !aValue.is_number_integer() )
        Fail( std::string( "integer out of range: " ) + aField );

    if( aValue.is_number_unsigned() )
    {
        const uint64_t number = aValue.get<uint64_t>();

        if( aMax < 0 || number > static_cast<uint64_t>( aMax ) )
            Fail( std::string( "integer out of range: " ) + aField );

        return static_cast<int64_t>( number );
    }

    const int64_t number = aValue.get<int64_t>();

    if( number < aMin || number > aMax )
        Fail( std::string( "integer out of range: " ) + aField );

    return number;
}


std::vector<wchar_t> Units( const JSON& aValue, const char* aField )
{
    if( !aValue.is_array() || aValue.size() > MAX_UNITS )
        Fail( std::string( "expected units: " ) + aField );

    std::vector<wchar_t> units;

    for( const JSON& unit : aValue )
        units.push_back( static_cast<wchar_t>( Integer( unit, INT_MIN, INT_MAX, aField ) ) );

    return units;
}


/// wxString( const wchar_t*, size_t ): every unit, embedded NULs included.
wxString Text( const std::vector<wchar_t>& aUnits )
{
    return wxString( aUnits.data(), aUnits.size() );
}


JSON UnitsJSON( const wxString& aText )
{
    JSON           units = JSON::array();
    const wchar_t* data = aText.wx_str();

    for( size_t i = 0; i < aText.length(); ++i )
        units.push_back( static_cast<int32_t>( data[i] ) );

    return units;
}


std::string Bits( double aValue )
{
    uint64_t bits = 0;
    std::memcpy( &bits, &aValue, sizeof( bits ) );
    char text[17];
    std::snprintf( text, sizeof( text ), "%016llx", static_cast<unsigned long long>( bits ) );
    return text;
}


double FromBits( const JSON& aValue, const char* aField )
{
    if( !aValue.is_string() || aValue.get_ref<const std::string&>().size() != 16 )
        Fail( std::string( "invalid hex: " ) + aField );

    uint64_t bits = 0;

    for( char c : aValue.get_ref<const std::string&>() )
    {
        int nibble = 0;

        if( c >= '0' && c <= '9' )
            nibble = c - '0';
        else if( c >= 'a' && c <= 'f' )
            nibble = c - 'a' + 10;
        else
            Fail( std::string( "invalid hex: " ) + aField );

        bits = bits << 4 | static_cast<uint64_t>( nibble );
    }

    double value = 0;
    std::memcpy( &value, &bits, sizeof( value ) );
    return value;
}


std::string Hex( const std::string& aBytes )
{
    static const char digits[] = "0123456789abcdef";
    std::string       out;

    for( unsigned char byte : aBytes )
    {
        out.push_back( digits[byte >> 4] );
        out.push_back( digits[byte & 0xF] );
    }

    return out;
}


/// Gives protected and private members as member pointers through an explicit instantiation,
/// which may name them.
template <typename TAG, typename TAG::TYPE MEMBER>
struct FONT_ACCESS
{
    friend typename TAG::TYPE FontMember( TAG ) { return MEMBER; }
};

#define FONT_MEMBER( TAG, TYPE_, POINTER )                                                  \
    struct TAG                                                                              \
    {                                                                                       \
        using TYPE = TYPE_;                                                                 \
        friend TYPE FontMember( TAG );                                                      \
    };                                                                                      \
    template struct FONT_ACCESS<TAG, POINTER>

using LINES_COUNT_FN = unsigned ( KIFONT::FONT::* )( const wxString& ) const;
using SIZE_OF_FN = int ( KIFONT::OUTLINE_FONT::* )( int ) const;
using SIZE_FN = int ( KIFONT::OUTLINE_FONT::* )() const;
FONT_MEMBER( FONT_NAME, wxString KIFONT::FONT::*, &KIFONT::FONT::m_fontName );
FONT_MEMBER( FONT_FILE, wxString KIFONT::FONT::*, &KIFONT::FONT::m_fontFileName );
FONT_MEMBER( OUTLINE_FACE, FT_Face KIFONT::OUTLINE_FONT::*, &KIFONT::OUTLINE_FONT::m_face );
FONT_MEMBER( LINES_COUNT, LINES_COUNT_FN, &KIFONT::FONT::linesCount );
FONT_MEMBER( FACE_SIZE_OF, SIZE_OF_FN, &KIFONT::OUTLINE_FONT::faceSize );
FONT_MEMBER( FACE_SIZE, SIZE_FN, &KIFONT::OUTLINE_FONT::faceSize );
FONT_MEMBER( SUBSCRIPT_SIZE_OF, SIZE_OF_FN, &KIFONT::OUTLINE_FONT::subscriptSize );
FONT_MEMBER( SUBSCRIPT_SIZE, SIZE_FN, &KIFONT::OUTLINE_FONT::subscriptSize );
#undef FONT_MEMBER


/// A FONT whose dynamic type keeps every inline default; its pure virtuals are never reached.
class ORACLE_FONT : public KIFONT::FONT
{
public:
    double GetInterline( double, const KIFONT::METRICS& ) const override
    {
        throw std::logic_error( "adapter font has no interline" );
    }

    double GetSuperSubSizeMultiplier() const override
    {
        throw std::logic_error( "adapter font has no size multiplier" );
    }

    double GetSuperSubBaselineOffset( TEXT_STYLE_FLAGS ) const override
    {
        throw std::logic_error( "adapter font has no baseline offset" );
    }

    VECTOR2I GetTextAsGlyphs( BOX2I*, std::vector<std::unique_ptr<KIFONT::GLYPH>>*,
                              const wxString&, const VECTOR2I&, const VECTOR2I&,
                              const EDA_ANGLE&, bool, const VECTOR2I&,
                              TEXT_STYLE_FLAGS ) const override
    {
        throw std::logic_error( "adapter font has no glyphs" );
    }
};


/// A FONT whose four style queries return scripted answers (any dynamic type may override
/// them, as STROKE_FONT and OUTLINE_FONT do).
class SCRIPTED_FONT : public ORACLE_FONT
{
public:
    SCRIPTED_FONT( bool aStroke, bool aOutline, bool aBold, bool aItalic ) :
            m_stroke( aStroke ), m_outline( aOutline ), m_bold( aBold ), m_italic( aItalic )
    {}

    bool IsStroke() const override { return m_stroke; }
    bool IsOutline() const override { return m_outline; }
    bool IsBold() const override { return m_bold; }
    bool IsItalic() const override { return m_italic; }

private:
    bool m_stroke;
    bool m_outline;
    bool m_bold;
    bool m_italic;
};


struct FONT_HOLDER
{
    std::unique_ptr<FT_FaceRec>    face;
    std::unique_ptr<KIFONT::FONT>  font;
    KIFONT::OUTLINE_FONT*          outline = nullptr;
    std::vector<wchar_t>           name;
};


/// {"kind": "base"|"scripted"|"outline", ...}; aOutlineOnly requires an outline font.
FONT_HOLDER MakeFont( const JSON& aSpec, bool aOutlineOnly )
{
    if( !aSpec.is_object() || !aSpec.contains( "kind" ) || !aSpec.at( "kind" ).is_string() )
        Fail( "expected font" );

    const std::string kind = aSpec.at( "kind" ).get<std::string>();
    FONT_HOLDER       holder;

    if( kind == "base" && !aOutlineOnly )
    {
        Keys( aSpec, { "kind", "name" } );
        holder.name = Units( aSpec.at( "name" ), "name" );
        holder.font = std::make_unique<ORACLE_FONT>();
    }
    else if( kind == "scripted" && !aOutlineOnly )
    {
        Keys( aSpec, { "kind", "name", "stroke", "outline", "bold", "italic" } );
        holder.name = Units( aSpec.at( "name" ), "name" );
        // Validated in this order (function arguments are unsequenced).
        const bool stroke = Bool( aSpec, "stroke" );
        const bool outline = Bool( aSpec, "outline" );
        const bool bold = Bool( aSpec, "bold" );
        const bool italic = Bool( aSpec, "italic" );
        holder.font = std::make_unique<SCRIPTED_FONT>( stroke, outline, bold, italic );
    }
    else if( kind == "outline" )
    {
        Keys( aSpec, { "kind", "name", "file", "face", "fake_bold", "fake_ital" } );
        holder.name = Units( aSpec.at( "name" ), "name" );
        const std::vector<wchar_t> file = Units( aSpec.at( "file" ), "file" );
        const JSON&                face = aSpec.at( "face" );
        std::optional<FT_Long>     styleFlags;

        if( !face.is_null() )
        {
            Keys( face, { "style_flags" } );
            styleFlags = Integer( face.at( "style_flags" ), INT64_MIN, INT64_MAX, "style_flags" );
        }

        const bool fakeBold = Bool( aSpec, "fake_bold" );
        const bool fakeItal = Bool( aSpec, "fake_ital" );

        // The pinned constructor (outline_font.cpp), then what LoadFont/loadFace would store.
        auto outline = std::make_unique<KIFONT::OUTLINE_FONT>();
        ( *outline ).*FontMember( FONT_FILE{} ) = Text( file );

        if( styleFlags )
        {
            holder.face = std::make_unique<FT_FaceRec>();
            holder.face->style_flags = *styleFlags;
            ( *outline ).*FontMember( OUTLINE_FACE{} ) = holder.face.get();
        }

        if( fakeBold )
            outline->SetFakeBold();

        if( fakeItal )
            outline->SetFakeItal();

        holder.outline = outline.get();
        holder.font = std::move( outline );
    }
    else
    {
        Fail( "unknown font kind" );
    }

    ( *holder.font ).*FontMember( FONT_NAME{} ) = Text( holder.name );
    return holder;
}


/// Whether converting the name for the stream reaches wxFAIL_MSG: a unit before the first NUL
/// whose code (unit & 0x7fffffff) exceeds 0x10FFFF, which wxConvLibc in the C locale rejects
/// and wxMBConvStrictUTF8 asserts on.
bool NameReachesAssert( const std::vector<wchar_t>& aName )
{
    for( wchar_t unit : aName )
    {
        if( unit == 0 )
            return false;

        if( ( static_cast<uint32_t>( unit ) & 0x7fffffffu ) > 0x10ffffu )
            return true;
    }

    return false;
}


/// Whether NameAsToken's utf8_str() reaches wxFAIL_MSG: it converts every unit with an
/// explicit length (past NUL units) through wxMBConvStrictUTF8.
bool AnyUnitReachesAssert( const std::vector<wchar_t>& aName )
{
    for( wchar_t unit : aName )
    {
        if( ( static_cast<uint32_t>( unit ) & 0x7fffffffu ) > 0x10ffffu )
            return true;
    }

    return false;
}


JSON StateJSON( std::ios::iostate aState )
{
    return JSON{ { "bad", ( aState & std::ios::badbit ) != 0 },
                 { "eof", ( aState & std::ios::eofbit ) != 0 },
                 { "fail", ( aState & std::ios::failbit ) != 0 } };
}


std::ios::iostate State( const JSON& aValue, const char* aField )
{
    Keys( aValue, { "bad", "eof", "fail" } );
    std::ios::iostate state = std::ios::goodbit;

    if( Bool( aValue, "bad" ) )
        state |= std::ios::badbit;

    if( Bool( aValue, "eof" ) )
        state |= std::ios::eofbit;

    if( Bool( aValue, "fail" ) )
        state |= std::ios::failbit;

    (void) aField;
    return state;
}


/// A buffer with no put area: overflow accepts up to the capacity, sync succeeds or fails.
class SINK : public std::streambuf
{
public:
    SINK( std::optional<size_t> aCapacity, bool aSyncFails ) :
            m_capacity( aCapacity ), m_syncFails( aSyncFails )
    {}

    std::string bytes;
    int64_t     syncs = 0;

protected:
    int_type overflow( int_type aChar ) override
    {
        if( traits_type::eq_int_type( aChar, traits_type::eof() ) )
            return traits_type::not_eof( aChar );

        if( m_capacity && bytes.size() >= *m_capacity )
            return traits_type::eof();

        bytes.push_back( traits_type::to_char_type( aChar ) );
        return aChar;
    }

    int sync() override
    {
        ++syncs;
        return m_syncFails ? -1 : 0;
    }

private:
    std::optional<size_t> m_capacity;
    bool                  m_syncFails;
};


JSON RunInsert( const JSON& aInput )
{
    Keys( aInput, { "op", "font", "pointer", "stream" } );
    const bool  pointer = Bool( aInput, "pointer" );
    const JSON& fontSpec = aInput.at( "font" );
    const JSON& spec = aInput.at( "stream" );
    Keys( spec, { "state", "exceptions", "flags", "width", "fill", "buffer" } );
    const std::ios::iostate state = State( spec.at( "state" ), "state" );
    const std::ios::iostate mask = State( spec.at( "exceptions" ), "exceptions" );
    std::ios::fmtflags      flags{};
    const JSON&             flagList = spec.at( "flags" );

    if( !flagList.is_array() || flagList.size() > 4 )
        Fail( "expected flags" );

    for( const JSON& flag : flagList )
    {
        const std::string name = flag.is_string() ? flag.get<std::string>() : std::string();
        std::ios::fmtflags bit{};

        if( name == "left" )
            bit = std::ios::left;
        else if( name == "right" )
            bit = std::ios::right;
        else if( name == "internal" )
            bit = std::ios::internal;
        else if( name == "unitbuf" )
            bit = std::ios::unitbuf;
        else
            Fail( "expected flags" );

        if( flags & bit )
            Fail( "expected flags" );

        flags |= bit;
    }

    const int64_t width = Integer( spec.at( "width" ), -MAX_WIDTH, MAX_WIDTH, "width" );
    const char    fill = static_cast<char>( Integer( spec.at( "fill" ), 0, 255, "fill" ) );
    const JSON&   buffer = spec.at( "buffer" );
    std::optional<size_t> capacity;
    bool                  syncFails = false;

    if( !buffer.is_null() )
    {
        Keys( buffer, { "capacity", "sync_fails" } );

        if( !buffer.at( "capacity" ).is_null() )
            capacity = Integer( buffer.at( "capacity" ), 0, MAX_CAPACITY, "capacity" );

        syncFails = Bool( buffer, "sync_fails" );
    }

    FONT_HOLDER holder;

    if( fontSpec.is_null() )
    {
        if( !pointer )
            Fail( "expected font" );

        // os << *aFont with a null aFont.
        Fail( UNDEFINED );
    }

    holder = MakeFont( fontSpec, false );

    if( NameReachesAssert( holder.name ) )
        Fail( OUTSIDE );

    // libstdc++ 14's ~sentry syncs a unitbuf stream and calls setstate( badbit ) from a
    // noexcept destructor when sync fails: with a nonempty mask that may terminate.
    if( ( flags & std::ios::unitbuf ) && !buffer.is_null() && syncFails
        && mask != std::ios::goodbit )
    {
        Fail( OUTSIDE );
    }

    std::unique_ptr<SINK> sink;

    if( !buffer.is_null() )
        sink = std::make_unique<SINK>( capacity, syncFails );

    std::ostream os( sink.get() );
    os.setf( flags );
    os.width( width );
    os.fill( fill );
    os.clear( state );
    bool setupThrew = false;

    try
    {
        os.exceptions( mask );
    }
    catch( const std::ios_base::failure& )
    {
        setupThrew = true;
    }

    std::string outcome = "returned";

    try
    {
        if( pointer )
        {
            const KIFONT::FONT* font = holder.font.get();
            os << font;
        }
        else
        {
            os << static_cast<const KIFONT::FONT&>( *holder.font );
        }
    }
    catch( const std::ios_base::failure& )
    {
        outcome = "failure";
    }

    return JSON{ { "bytes", Hex( sink ? sink->bytes : std::string() ) },
                 { "state", StateJSON( os.rdstate() ) },
                 { "width", static_cast<int64_t>( os.width() ) },
                 { "sync_calls", sink ? sink->syncs : 0 },
                 { "outcome", outcome },
                 { "setup_threw", setupThrew } };
}


int SizeArgument( const JSON& aInput )
{
    return static_cast<int>( Integer( aInput.at( "size" ), INT_MIN, INT_MAX, "size" ) );
}


/// faceSize( aSize ) is defined when aSize * 64 does not overflow and the truncated product
/// with 1.4 is an int.
bool FaceSizeDefined( int aSize )
{
    const int64_t scaled = static_cast<int64_t>( aSize ) * 64;

    if( scaled < INT_MIN || scaled > INT_MAX )
        return false;

    const double value = static_cast<double>( scaled ) * 1.4;
    return value > -2147483649.0 && value < 2147483648.0;
}


JSON Run( const JSON& aInput )
{
    if( !aInput.is_object() || !aInput.contains( "op" ) || !aInput.at( "op" ).is_string() )
        Fail( "expected op" );

    const std::string op = aInput.at( "op" ).get<std::string>();

    if( op == "text_style" )
    {
        Keys( aInput, { "op" } );
        using PERMISSION = KIFONT::OUTLINE_FONT::EMBEDDING_PERMISSION;
        return JSON{ { "values", JSON::array( { static_cast<int>( TEXT_STYLE::BOLD ),
                                                static_cast<int>( TEXT_STYLE::ITALIC ),
                                                static_cast<int>( TEXT_STYLE::SUBSCRIPT ),
                                                static_cast<int>( TEXT_STYLE::SUPERSCRIPT ),
                                                static_cast<int>( TEXT_STYLE::OVERBAR ),
                                                static_cast<int>( TEXT_STYLE::UNDERLINE ) } ) },
                     { "italic_tilt", Bits( ITALIC_TILT ) },
                     { "embedding_permission",
                       JSON::array( { static_cast<int>( PERMISSION::INSTALLABLE ),
                         static_cast<int>( PERMISSION::EDITABLE ),
                         static_cast<int>( PERMISSION::PRINT_PREVIEW_ONLY ),
                         static_cast<int>( PERMISSION::RESTRICTED ),
                         static_cast<int>( PERMISSION::INVALID ) } ) } };
    }

    if( op == "style_predicates" )
    {
        Keys( aInput, { "op", "flags" } );
        const auto flags = static_cast<TEXT_STYLE_FLAGS>(
                Integer( aInput.at( "flags" ), 0, UINT32_MAX, "flags" ) );
        return JSON{ { "bold", IsBold( flags ) }, { "italic", IsItalic( flags ) },
                     { "subscript", IsSubscript( flags ) },
                     { "superscript", IsSuperscript( flags ) } };
    }

    if( op == "queries" )
    {
        Keys( aInput, { "op", "font" } );
        FONT_HOLDER         holder = MakeFont( aInput.at( "font" ), false );
        const KIFONT::FONT& font = *holder.font;
        return JSON{ { "stroke", font.IsStroke() }, { "outline", font.IsOutline() },
                     { "bold", font.IsBold() }, { "italic", font.IsItalic() },
                     { "name", UnitsJSON( font.GetName() ) } };
    }

    if( op == "lines_count" )
    {
        Keys( aInput, { "op", "font", "text" } );
        FONT_HOLDER    holder = MakeFont( aInput.at( "font" ), false );
        const wxString text = Text( Units( aInput.at( "text" ), "text" ) );
        const KIFONT::FONT& font = *holder.font;
        return JSON{ { "count", ( font.*FontMember( LINES_COUNT{} ) )( text ) } };
    }

    if( op == "name_as_token" )
    {
        Keys( aInput, { "op", "font" } );
        FONT_HOLDER holder = MakeFont( aInput.at( "font" ), false );

        if( AnyUnitReachesAssert( holder.name ) )
            Fail( OUTSIDE );

        // In the wchar_t build the pointer is into m_fontName's own conversion cache, valid
        // until the name is converted again.
        const char* token = holder.font->NameAsToken();
        return JSON{ { "bytes", Hex( std::string( token ) ) } };
    }

    if( op == "insert" )
        return RunInsert( aInput );

    if( op == "draw" )
    {
        Keys( aInput, { "op", "font", "text", "position", "mouse_pos", "active_url" } );
        FONT_HOLDER    holder = MakeFont( aInput.at( "font" ), false );
        const wxString text = Text( Units( aInput.at( "text" ), "text" ) );
        auto point = []( const JSON& aValue ) -> VECTOR2I
        {
            if( !aValue.is_array() || aValue.size() != 2 )
                Fail( "expected point" );

            const int x = static_cast<int>( Integer( aValue.at( 0 ), INT_MIN, INT_MAX, "point" ) );
            const int y = static_cast<int>( Integer( aValue.at( 1 ), INT_MIN, INT_MAX, "point" ) );
            return VECTOR2I( x, y );
        };
        const VECTOR2I          position = point( aInput.at( "position" ) );
        std::optional<VECTOR2I> mouse;

        if( !aInput.at( "mouse_pos" ).is_null() )
            mouse = point( aInput.at( "mouse_pos" ) );

        std::optional<wxString> url;

        if( !aInput.at( "active_url" ).is_null() )
            url = Text( Units( aInput.at( "active_url" ), "active_url" ) );

        // No GAL exists in the oracle: aGal is nullptr.
        const TEXT_ATTRIBUTES attributes;
        holder.font->Draw( nullptr, text, position, attributes, KIFONT::METRICS::Default(), mouse,
                           url ? &*url : nullptr );
        return JSON{ { "active_url", url ? UnitsJSON( *url ) : JSON( nullptr ) } };
    }

    // The remaining operations read an OUTLINE_FONT.
    if( op == "outline" )
    {
        Keys( aInput, { "op", "font" } );
        FONT_HOLDER                  holder = MakeFont( aInput.at( "font" ), true );
        const KIFONT::OUTLINE_FONT& font = *holder.outline;
        const FT_Face&               face = font.GetFace();
        return JSON{ { "stroke", font.IsStroke() },
                     { "outline", font.IsOutline() },
                     { "bold", font.IsBold() },
                     { "italic", font.IsItalic() },
                     { "fake_bold", font.IsFakeBold() },
                     { "fake_italic", font.IsFakeItalic() },
                     { "name", UnitsJSON( font.GetName() ) },
                     { "file", UnitsJSON( font.GetFileName() ) },
                     { "face", face ? JSON{ { "style_flags", static_cast<int64_t>( face->style_flags ) } }
                                    : JSON( nullptr ) },
                     { "multiplier", Bits( font.GetSuperSubSizeMultiplier() ) },
                     { "face_size", ( font.*FontMember( FACE_SIZE{} ) )() },
                     { "subscript_size", ( font.*FontMember( SUBSCRIPT_SIZE{} ) )() } };
    }

    if( op == "set_fake" )
    {
        Keys( aInput, { "op", "font", "calls" } );
        FONT_HOLDER holder = MakeFont( aInput.at( "font" ), true );
        const JSON& calls = aInput.at( "calls" );

        if( !calls.is_array() || calls.size() > MAX_CALLS )
            Fail( "expected calls" );

        KIFONT::OUTLINE_FONT& font = *holder.outline;
        JSON                  states = JSON::array();

        for( const JSON& call : calls )
        {
            if( call == "bold" )
                font.SetFakeBold();
            else if( call == "ital" )
                font.SetFakeItal();
            else
                Fail( "expected calls" );

            states.push_back( JSON::array( { font.IsFakeBold(), font.IsFakeItalic(),
                                             font.IsBold(), font.IsItalic() } ) );
        }

        return JSON{ { "states", states } };
    }

    if( op == "em_size" )
    {
        Keys( aInput, { "op", "font", "size" } );
        FONT_HOLDER  holder = MakeFont( aInput.at( "font" ), true );
        const double size = FromBits( aInput.at( "size" ), "size" );
        return JSON{ { "value", Bits( holder.outline->GetEmSize( size ) ) } };
    }

    if( op == "baseline_offset" )
    {
        Keys( aInput, { "op", "font", "flags" } );
        FONT_HOLDER holder = MakeFont( aInput.at( "font" ), true );
        const auto  flags = static_cast<TEXT_STYLE_FLAGS>(
                Integer( aInput.at( "flags" ), 0, UINT32_MAX, "flags" ) );
        const KIFONT::FONT& font = *holder.font;
        return JSON{ { "value", Bits( font.GetSuperSubBaselineOffset( flags ) ) } };
    }

    if( op == "face_size" || op == "subscript_size" )
    {
        Keys( aInput, { "op", "font", "size" } );
        FONT_HOLDER holder = MakeFont( aInput.at( "font" ), true );
        const int   size = SizeArgument( aInput );

        if( !FaceSizeDefined( size ) )
            Fail( UNDEFINED );

        const KIFONT::OUTLINE_FONT& font = *holder.outline;
        const int value = op == "face_size" ? ( font.*FontMember( FACE_SIZE_OF{} ) )( size )
                                            : ( font.*FontMember( SUBSCRIPT_SIZE_OF{} ) )( size );
        return JSON{ { "value", value } };
    }

    Fail( "unknown op" );
}
} // namespace


namespace oracle
{
void RegisterIncludeFontP001( REGISTRY& aRegistry )
{
    aRegistry.Add( FONT_UNIT, "include/font/font.h, include/font/outline_font.h", Run );
}
} // namespace oracle
