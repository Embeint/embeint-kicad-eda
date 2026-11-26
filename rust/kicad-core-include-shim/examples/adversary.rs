// SPDX-License-Identifier: GPL-3.0-or-later
//! Print `adversary-weights.json` (McIlroy adversary inputs for the ported libstdc++
//! sort and nth_element). `cargo run -p kicad-core-include-shim --example adversary >
//! rust/kicad-core-include-shim/adversary-weights.json`; the crate's tests check the
//! committed file equals this output.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

fn main() -> Result<(), String> {
    let weights = kicad_core_include_shim::adversary::weights().map_err(|e| format!("{e:?}"))?;
    print!("{}", kicad_core_include_shim::adversary_json(&weights));
    Ok(())
}
