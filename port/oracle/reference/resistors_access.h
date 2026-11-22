// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <resistor_substitution_utils.h>

namespace resistor_test
{
struct COLLECTED
{
    RESISTANCE best;
    size_t     callbacks;
};

// Test-only entry points into the actual compiled pinned source. No algorithms
// are defined in this header or in the JSON adapter.
COLLECTED   Collect( double target, std::vector<RESISTANCE>& candidates );
int         Unique( const RESISTANCE& value );
bool        Better( const RESISTANCE& candidate, const RESISTANCE& best );
std::string Embrace( const std::string& text, char symbol );
RESISTANCE  Combine( const RESISTANCE& first, const RESISTANCE& second, bool parallel, bool simple );
bool        Less( const RESISTANCE& first, const RESISTANCE& second );
bool        Less( const RESISTANCE& first, double second );
} // namespace resistor_test
