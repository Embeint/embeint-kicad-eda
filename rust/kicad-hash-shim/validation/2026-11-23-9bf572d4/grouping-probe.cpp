// Probe: libstdc++ runtime num_put<char> grouping with zero/negative groups (std::hex, setw).
#include <cstdint>
#include <iomanip>
#include <iostream>
#include <locale>
#include <sstream>
#include <string>

struct GROUPING : std::numpunct<char>
{
    GROUPING( std::string aGrouping ) : m_grouping( std::move( aGrouping ) ) {}
    char        do_thousands_sep() const override { return '.'; }
    std::string do_grouping() const override { return m_grouping; }
    std::string m_grouping;
};

int main()
{
    const std::string groupings[] = { std::string( "\1\2\0", 3 ), std::string( "\1\2", 2 ),
                                      std::string( "\3\0", 2 ), std::string( "\2\x80", 2 ),
                                      std::string( "\2\x7f", 2 ), std::string( "\1\0\3", 3 ) };
    for( const std::string& grouping : groupings )
    {
        std::ostringstream ss;
        ss.imbue( std::locale( std::locale::classic(), new GROUPING( grouping ) ) );
        ss << std::hex << std::uppercase << std::setfill( '0' ) << std::setw( 16 )
           << uint64_t( 0xABCDEF ) << ' ' << uint64_t( 0x123456789ABCDEF0 );
        std::cout << ss.str() << '\n';
    }
#ifdef __GLIBCXX__
    std::cout << "__GLIBCXX__ " << __GLIBCXX__ << '\n';
#endif
}
