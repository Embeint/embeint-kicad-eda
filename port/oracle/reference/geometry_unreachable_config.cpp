// SPDX-License-Identifier: GPL-3.0-or-later
// SHAPE_POLY_SET's vtable retains triangulation/fracture functions even when the
// adapter only constructs contours for the header-owned stripe index. Those
// unrelated functions reference application configuration. Do not fabricate a
// configuration value or silently select a triangulation algorithm: fail closed
// if any compared operation actually reaches that application dependency.
#include <advanced_config.h>
#include <stdexcept>

const ADVANCED_CFG& ADVANCED_CFG::GetCfg()
{
    throw std::logic_error( "unexpected ADVANCED_CFG access in geometry oracle" );
}
