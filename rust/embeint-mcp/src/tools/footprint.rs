// SPDX-License-Identifier: GPL-3.0-or-later
//! Footprint libraries, the Footprint Editor, footprint generation and 3D model plans and checks.
//!
//! See [`super`] for the module contract.  Not yet ported: every tool here is served by the
//! Python server until it is added.

use embeint_ipc::client::Error;

use super::args::{Args, Param};
use super::{Ctx, Output};

pub fn params(tool: &str) -> Option<&'static [Param]> {
    let _ = tool;
    None
}

pub async fn run(name: &str, args: &Args, ctx: &Ctx<'_>) -> Option<Result<Output, Error>> {
    params(name)?;
    let _ = (args, ctx);
    None
}
