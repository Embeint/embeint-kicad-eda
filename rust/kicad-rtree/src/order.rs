// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2001-2023 Free Software Foundation, Inc.
// Copyright (c) 1994 Hewlett-Packard Company; 1996, 1997 Silicon Graphics.
// See NOTICE.libstdc++.txt and COPYING.RUNTIME for retained upstream notices.
// Port of libstdc++'s introsort and heap ordering (GCC runtime exception applies
// to the original implementation). See README for the measured ABI boundary.
use crate::{Error, Result};
use core::cmp::Ordering;

fn at<T>(a: &[T], i: usize) -> Result<&T> {
    a.get(i).ok_or(Error::Invariant)
}
fn put<T>(a: &mut [T], i: usize, value: T) -> Result<()> {
    *a.get_mut(i).ok_or(Error::Invariant)? = value;
    Ok(())
}
fn swap<T>(a: &mut [T], i: usize, j: usize) -> Result<()> {
    if i >= a.len() || j >= a.len() {
        return Err(Error::Invariant);
    }
    a.swap(i, j);
    Ok(())
}
fn less<T, F: Fn(&T, &T) -> Ordering>(a: &[T], i: usize, j: usize, cmp: &F) -> Result<bool> {
    Ok(cmp(at(a, i)?, at(a, j)?) == Ordering::Less)
}
fn insertion<T: Clone, F: Fn(&T, &T) -> Ordering>(
    a: &mut [T],
    first: usize,
    last: usize,
    cmp: &F,
) -> Result<()> {
    if first == last {
        return Ok(());
    }
    for i in first + 1..last {
        let value = at(a, i)?.clone();
        let mut hole = i;
        while hole > first && cmp(&value, at(a, hole - 1)?) == Ordering::Less {
            let prev = at(a, hole - 1)?.clone();
            put(a, hole, prev)?;
            hole -= 1;
        }
        put(a, hole, value)?;
    }
    Ok(())
}
fn intro<T: Clone, F: Fn(&T, &T) -> Ordering>(
    a: &mut [T],
    first: usize,
    mut last: usize,
    mut depth: usize,
    cmp: &F,
) -> Result<()> {
    while last - first > 16 {
        if depth == 0 {
            let range = a.get_mut(first..last).ok_or(Error::Invariant)?;
            if range.len() > 1 {
                for parent in (0..=(range.len() - 2) / 2).rev() {
                    let value = at(range, parent)?.clone();
                    adjust_heap(range, parent, range.len(), value, &|a, b| {
                        cmp(a, b) == Ordering::Less
                    })?;
                }
            }
            for n in (2..=range.len()).rev() {
                heap_pop(range, n, &|a, b| cmp(a, b) == Ordering::Less)?;
            }
            return Ok(());
        }
        depth -= 1;
        let (x, y, z) = (first + 1, first + (last - first) / 2, last - 1);
        // __move_median_to_first, including its asymmetric equivalent-key paths.
        let median = if less(a, x, y, cmp)? {
            if less(a, y, z, cmp)? {
                y
            } else if less(a, x, z, cmp)? {
                z
            } else {
                x
            }
        } else if less(a, x, z, cmp)? {
            x
        } else if less(a, y, z, cmp)? {
            z
        } else {
            y
        };
        swap(a, first, median)?;
        let (mut left, mut right) = (first + 1, last);
        loop {
            while less(a, left, first, cmp)? {
                left += 1;
            }
            right -= 1;
            while less(a, first, right, cmp)? {
                right -= 1;
            }
            if left >= right {
                break;
            }
            swap(a, left, right)?;
            left += 1;
        }
        intro(a, left, last, depth, cmp)?;
        last = left;
    }
    Ok(())
}
/// Equivalent-key permutations are observable in tree traversal, so Rust's sort
/// cannot replace the reference's std::sort.
pub(crate) fn sort<T: Clone, F: Fn(&T, &T) -> Ordering>(a: &mut [T], cmp: F) -> Result<()> {
    let n = a.len();
    if n < 2 {
        return Ok(());
    }
    let depth = (usize::BITS - 1 - n.leading_zeros()) as usize * 2;
    intro(a, 0, n, depth, &cmp)?;
    // The guarded form produces the same moves as __final_insertion_sort: the
    // first 16 establish the sentinel used by its unguarded suffix.
    insertion(a, 0, n, &cmp)
}
/// __push_heap with the reference comparator (true means the parent loses).
pub(crate) fn heap_push<T: Clone, F: Fn(&T, &T) -> bool>(
    a: &mut [T],
    hole: usize,
    cmp: &F,
) -> Result<()> {
    let value = at(a, hole)?.clone();
    push_value(a, hole, 0, value, cmp)
}
fn push_value<T: Clone, F: Fn(&T, &T) -> bool>(
    a: &mut [T],
    mut hole: usize,
    top: usize,
    value: T,
    cmp: &F,
) -> Result<()> {
    while hole > top {
        let parent = (hole - 1) / 2;
        if !cmp(at(a, parent)?, &value) {
            break;
        }
        let v = at(a, parent)?.clone();
        put(a, hole, v)?;
        hole = parent;
    }
    put(a, hole, value)
}
/// __pop_heap / __adjust_heap: equal children select the right child, then the
/// displaced value bubbles upward only for a strict comparison.
pub(crate) fn heap_pop<T: Clone, F: Fn(&T, &T) -> bool>(
    a: &mut [T],
    n: usize,
    cmp: &F,
) -> Result<()> {
    if n < 2 || n > a.len() {
        return Err(Error::Invariant);
    }
    let value = at(a, n - 1)?.clone();
    let root = at(a, 0)?.clone();
    put(a, n - 1, root)?;
    adjust_heap(a, 0, n - 1, value, cmp)
}
fn adjust_heap<T: Clone, F: Fn(&T, &T) -> bool>(
    a: &mut [T],
    mut hole: usize,
    len: usize,
    value: T,
    cmp: &F,
) -> Result<()> {
    let top = hole;
    let mut second = hole;
    while second < (len - 1) / 2 {
        second = 2 * (second + 1);
        if cmp(at(a, second)?, at(a, second - 1)?) {
            second -= 1;
        }
        let v = at(a, second)?.clone();
        put(a, hole, v)?;
        hole = second;
    }
    if len % 2 == 0 && second == (len - 2) / 2 {
        second = 2 * (second + 1);
        let v = at(a, second - 1)?.clone();
        put(a, hole, v)?;
        hole = second - 1;
    }
    push_value(a, hole, top, value, cmp)
}
