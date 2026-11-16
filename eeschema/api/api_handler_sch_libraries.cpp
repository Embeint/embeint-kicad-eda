/*
 * This program source code file is part of KiCad, a free EDA CAD application.
 *
 * Copyright The KiCad Developers, see AUTHORS.txt for contributors.
 * @author Jon Evans <jon@craftyjon.com>
 *
 * This program is free software: you can redistribute it and/or modify it
 * under the terms of the GNU General Public License as published by the
 * Free Software Foundation, either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

#include <api/api_handler_sch_libraries.h>
#include <api/api_utils.h>

#include <libraries/symbol_library_adapter.h>
#include <pgm_base.h>
#include <project_sch.h>
#include <settings/settings_manager.h>
#include <lib_symbol.h>
#include <sch_io/sch_io_mgr.h>
#include <wildcards_and_files_ext.h>
#include <wx/filename.h>


LIBRARY_MANAGER_ADAPTER* API_HANDLER_SCH_LIBRARIES::adapterForProject( PROJECT& aProject ) const
{
    return PROJECT_SCH::SymbolLibAdapter( &aProject );
}


std::vector<wxString> API_HANDLER_SCH_LIBRARIES::getItemNames( LIBRARY_MANAGER_ADAPTER& aAdapter,
                                                               const wxString& aNickname ) const
{
    return static_cast<SYMBOL_LIBRARY_ADAPTER&>( aAdapter ).GetSymbolNames( aNickname );
}


bool API_HANDLER_SCH_LIBRARIES::packLibraryItem( const LIB_ID& aId, google::protobuf::Any& aOutput ) const
{
    SYMBOL_LIBRARY_ADAPTER* adapter =
            static_cast<SYMBOL_LIBRARY_ADAPTER*>( adapterForProject( Pgm().GetSettingsManager().Prj() ) );

    LIB_SYMBOL* libSymbol = adapter->LoadSymbol( aId );

    if( !libSymbol )
        return false;

    // Clients get the inherited drawings resolved, so derived symbols are reported flattened.
    std::unique_ptr<LIB_SYMBOL> flattened;

    if( libSymbol->IsDerived() )
    {
        flattened = libSymbol->Flatten();

        if( flattened )
            flattened->SetParentName( libSymbol->GetParentName() );
    }
    else if( !libSymbol->GetParentName().IsEmpty() )
    {
        wxLogTrace( traceApi, "Cannot resolve the parent of derived symbol '%s'", aId.Format().wx_str() );
    }

    ( flattened ? flattened.get() : libSymbol )->Serialize( aOutput );

    return true;
}



wxString API_HANDLER_SCH_LIBRARIES::pluginTypeForUri( const wxString& aUri ) const
{
    // A library about to be created has no file to sniff yet
    if( wxFileName( aUri ).GetExt().IsSameAs( FILEEXT::KiCadSymbolLibFileExtension, false ) )
        return SCH_IO_MGR::ShowType( SCH_IO_MGR::SCH_KICAD );

    return SCH_IO_MGR::ShowType( SCH_IO_MGR::GuessPluginTypeFromLibPath( aUri ) );
}


int API_HANDLER_SCH_LIBRARIES::scoreSearchMatch( LIBRARY_MANAGER_ADAPTER& aAdapter, const wxString& aNickname,
                                                 const wxString& aItemName,
                                                 const std::vector<wxString>& aTerms ) const
{
    if( int nameScore = API_HANDLER_LIBRARIES::scoreSearchMatch( aAdapter, aNickname, aItemName, aTerms ) )
        return nameScore + 1000;

    LIB_SYMBOL* symbol = static_cast<SYMBOL_LIBRARY_ADAPTER&>( aAdapter ).LoadSymbol( aNickname, aItemName );

    if( !symbol )
        return 0;

    const wxString name = aItemName.Lower();
    const wxString keywords = symbol->GetKeyWords().Lower();
    const wxString description = symbol->GetDescription().Lower();
    int            score = 0;

    // Every term must appear somewhere; names weigh most, then keywords, then descriptions
    for( const wxString& term : aTerms )
    {
        if( name.Matches( term ) || name.Contains( term ) )
            score += 20;
        else if( keywords.Contains( term ) )
            score += 10;
        else if( description.Contains( term ) )
            score += 5;
        else
            return 0;
    }

    return score;
}
