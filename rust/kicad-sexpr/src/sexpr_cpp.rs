// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2016 Mark Roszko <mark.roszko@gmail.com>
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.

//! `libs/sexpr/sexpr.cpp` (unit `libs.sexpr.application.pure.p001`).
//!
//! In this revision only the minimum co-ported members the header unit needs are here: the
//! two `SEXPR` base constructors (as [`DEFAULT_LINE_NUMBER`] and the line number stored by
//! the derived constructors) and `~SEXPR_LIST` (as an iterative `Drop`). `doScan` and
//! `doAddChildren` return [`SexprError::Unported`]; every other body of `sexpr.cpp` belongs
//! to the source unit's port.

use crate::header::{ChildrenArg, ScanArg, Sexpr, SexprList};
use crate::{SexprError, SOURCE_UNIT};
use alloc::boxed::Box;
use alloc::vec::Vec;

/// `SEXPR::SEXPR( SEXPR_TYPE aType )`: `m_lineNumber( 1 )`. (The other base constructor,
/// `SEXPR( SEXPR_TYPE, size_t aLineNumber )`, stores its argument unchanged.)
pub(crate) const DEFAULT_LINE_NUMBER: u64 = 1;

/// `SEXPR_LIST::~SEXPR_LIST()`: delete every child, then clear the vector. Null children are
/// skipped (`delete nullptr` does nothing). The reference recurses through each child's
/// destructor; this drops the same nodes with an explicit stack, so a deep tree cannot
/// exhaust the native stack.
impl Drop for SexprList {
    fn drop(&mut self) {
        let mut pending: Vec<Box<Sexpr>> = self.children.drain(..).flatten().collect();
        while let Some(mut node) = pending.pop() {
            if let Sexpr::List(list) = &mut *node {
                pending.extend(list.children.drain(..).flatten());
            }
        }
    }
}

impl SexprList {
    /// `size_t SEXPR_LIST::doScan( const SEXPR_SCAN_ARG* args, size_t num_args )`.
    pub(crate) fn do_scan(&mut self, _args: &mut [ScanArg<'_>]) -> Result<u64, SexprError> {
        // UNPORTED(libs.sexpr.application.pure.p001): SEXPR::SEXPR_LIST::doScan
        Err(SexprError::Unported {
            upstream: "SEXPR::SEXPR_LIST::doScan",
            unit: SOURCE_UNIT,
        })
    }

    /// `void SEXPR_LIST::doAddChildren( const SEXPR_CHILDREN_ARG* args, size_t num_args )`.
    pub(crate) fn do_add_children(&mut self, _args: Vec<ChildrenArg>) -> Result<(), SexprError> {
        // UNPORTED(libs.sexpr.application.pure.p001): SEXPR::SEXPR_LIST::doAddChildren
        Err(SexprError::Unported {
            upstream: "SEXPR::SEXPR_LIST::doAddChildren",
            unit: SOURCE_UNIT,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::SexprInteger;

    #[test]
    fn deep_trees_drop_without_recursion() {
        let mut root = SexprList::new();
        for _ in 0..200_000 {
            let mut parent = SexprList::new();
            parent.children.push(Some(Box::new(Sexpr::List(root))));
            parent.children.push(None);
            parent
                .children
                .push(Some(Box::new(Sexpr::from(SexprInteger::new(1)))));
            root = parent;
        }
        drop(root);
    }
}
