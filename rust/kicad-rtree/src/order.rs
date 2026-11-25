// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
// Copyright (C) 2001-2022 Free Software Foundation, Inc.
// Copyright (c) 1994 Hewlett-Packard Company
// Copyright (c) 1996, 1997 Silicon Graphics Computer Systems, Inc.
// Original notices: ../GNU-ALGORITHMS-NOTICE.txt.
//! Observable ordering of the Linux libstdc++ reference: introsort (threshold 16)
//! and heap adjustment. Algorithms adapted from GCC libstdc++ stl_algo.h and
//! stl_heap.h (GPLv3 with GCC runtime exception). No pointer/address tie breaker.
use crate::error::{push, Error};

fn at<T: Clone>(v: &[T], i: usize) -> Result<T, Error> {
    v.get(i).cloned().ok_or(Error::Invariant)
}
fn set<T>(v: &mut [T], i: usize, x: T) -> Result<(), Error> {
    *v.get_mut(i).ok_or(Error::Invariant)? = x;
    Ok(())
}
fn swap<T>(v: &mut [T], a: usize, b: usize) -> Result<(), Error> {
    if a >= v.len() || b >= v.len() {
        return Err(Error::Invariant);
    }
    v.swap(a, b);
    Ok(())
}

pub(crate) fn heap_push<T: Clone, F: Fn(&T, &T) -> bool>(
    v: &mut Vec<T>,
    value: T,
    less: &F,
) -> Result<(), Error> {
    let hole = v.len();
    push(v, value.clone())?;
    sift_up(v, hole, 0, value, less)
}
fn sift_up<T: Clone, F: Fn(&T, &T) -> bool>(
    v: &mut [T],
    mut hole: usize,
    top: usize,
    value: T,
    less: &F,
) -> Result<(), Error> {
    while hole > top {
        let parent = (hole - 1) / 2;
        let p = at(v, parent)?;
        if !less(&p, &value) {
            break;
        }
        set(v, hole, p)?;
        hole = parent;
    }
    set(v, hole, value)
}
fn adjust<T: Clone, F: Fn(&T, &T) -> bool>(
    v: &mut [T],
    mut hole: usize,
    len: usize,
    value: T,
    less: &F,
) -> Result<(), Error> {
    let top = hole;
    let mut child = hole;
    while child < (len - 1) / 2 {
        child = 2 * (child + 1);
        if less(&at(v, child)?, &at(v, child - 1)?) {
            child -= 1;
        }
        set(v, hole, at(v, child)?)?;
        hole = child;
    }
    if len % 2 == 0 && child == (len - 2) / 2 {
        child = 2 * (child + 1);
        set(v, hole, at(v, child - 1)?)?;
        hole = child - 1;
    }
    sift_up(v, hole, top, value, less)
}
pub(crate) fn heap_pop<T: Clone, F: Fn(&T, &T) -> bool>(
    v: &mut Vec<T>,
    less: &F,
) -> Result<Option<T>, Error> {
    let Some(value) = v.pop() else {
        return Ok(None);
    };
    if v.is_empty() {
        return Ok(Some(value));
    }
    let result = at(v, 0)?;
    let len = v.len();
    adjust(v, 0, len, value, less)?;
    Ok(Some(result))
}
fn heap_sort<T: Clone, F: Fn(&T, &T) -> bool>(v: &mut [T], less: &F) -> Result<(), Error> {
    let len = v.len();
    if len < 2 {
        return Ok(());
    }
    for parent in (0..=(len - 2) / 2).rev() {
        adjust(v, parent, len, at(v, parent)?, less)?;
    }
    for end in (1..len).rev() {
        let value = at(v, end)?;
        set(v, end, at(v, 0)?)?;
        adjust(v, 0, end, value, less)?;
    }
    Ok(())
}
fn insertion<T: Clone, F: Fn(&T, &T) -> bool>(v: &mut [T], less: &F) -> Result<(), Error> {
    for i in 1..v.len() {
        let value = at(v, i)?;
        let mut j = i;
        while j > 0 && less(&value, &at(v, j - 1)?) {
            set(v, j, at(v, j - 1)?)?;
            j -= 1;
        }
        set(v, j, value)?;
    }
    Ok(())
}
fn intro<T: Clone, F: Fn(&T, &T) -> bool>(
    v: &mut [T],
    mut depth: u32,
    less: &F,
) -> Result<(), Error> {
    let mut end = v.len();
    while end > 16 {
        if depth == 0 {
            return heap_sort(v.get_mut(..end).ok_or(Error::Invariant)?, less);
        }
        depth -= 1;
        let (a, b, c) = (1, end / 2, end - 1);
        let median = if less(&at(v, a)?, &at(v, b)?) {
            if less(&at(v, b)?, &at(v, c)?) {
                b
            } else if less(&at(v, a)?, &at(v, c)?) {
                c
            } else {
                a
            }
        } else if less(&at(v, a)?, &at(v, c)?) {
            a
        } else if less(&at(v, b)?, &at(v, c)?) {
            c
        } else {
            b
        };
        swap(v, 0, median)?;
        let pivot = at(v, 0)?;
        let (mut first, mut last) = (1, end);
        let cut = loop {
            while less(&at(v, first)?, &pivot) {
                first += 1;
            }
            last -= 1;
            while less(&pivot, &at(v, last)?) {
                last -= 1;
            }
            if first >= last {
                break first;
            }
            swap(v, first, last)?;
            first += 1;
        };
        intro(v.get_mut(cut..end).ok_or(Error::Invariant)?, depth, less)?;
        end = cut;
    }
    Ok(())
}
/// The final insertion pass is guarded here. After introsort its sentinel means
/// this has the same moves/comparisons as libstdc++'s unguarded tail pass.
pub fn sort<T: Clone, F: Fn(&T, &T) -> bool>(v: &mut [T], less: F) -> Result<(), Error> {
    if v.is_empty() {
        return Ok(());
    }
    let depth = 2 * (usize::BITS - 1 - v.len().leading_zeros());
    intro(v, depth, &less)?;
    insertion(v, &less)
}
