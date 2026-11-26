// SPDX-License-Identifier: GPL-3.0-or-later
//! McIlroy's adversary ("A killer adversary for quicksort", Software: Practice and
//! Experience 29(4), 1999), run against the ported libstdc++ `sort` and `nth_element`.
//! It fixes element values lazily so every pivot is as bad as possible, which drives
//! introsort and introselect into their depth-limit heap fallbacks. The resulting
//! values are fixed public inputs for `filter_kruskal` cases (see
//! `adversary-weights.json`, checked by the crate's tests). Harness code.

use kicad_core::stdalgo::{nth_element, sort, StdAlgoError};

/// The values that make `sort` (or `nth_element` at `nth`) of `0..n` positions
/// compare as the adversary decided.
///
/// # Errors
/// Only if the ported algorithm reports an invalid ordering, which the adversary's
/// consistent answers never cause.
pub fn killer(n: usize, nth: Option<usize>) -> Result<Vec<u64>, StdAlgoError<()>> {
    let gas = n as u64;
    let mut val = vec![gas; n];
    let mut solid = 0u64;
    let mut candidate = 0usize;
    let mut items: Vec<usize> = (0..n).collect();
    {
        let mut less = |x: &usize, y: &usize| {
            let (x, y) = (*x, *y);
            let both_gas = val.get(x) == Some(&gas) && val.get(y) == Some(&gas);
            if both_gas {
                let frozen = if x == candidate { x } else { y };
                if let Some(slot) = val.get_mut(frozen) {
                    *slot = solid;
                }
                solid += 1;
            }
            if val.get(x) == Some(&gas) {
                candidate = x;
            } else if val.get(y) == Some(&gas) {
                candidate = y;
            }
            val.get(x) < val.get(y)
        };
        match nth {
            None => sort::<_, _, ()>(&mut items, &mut less)?,
            Some(nth) => nth_element::<_, _, ()>(&mut items, nth, &mut less)?,
        }
    }
    for v in &mut val {
        if *v == gas {
            *v = solid;
            solid += 1;
        }
    }
    Ok(val)
}

/// The committed adversarial weight lists: `sort-<n>` and `nth-<n>`.
pub fn weights() -> Result<Vec<(String, Vec<u64>)>, StdAlgoError<()>> {
    Ok(vec![
        ("sort-1000".into(), killer(1000, None)?),
        ("sort-1024".into(), killer(1024, None)?),
        ("nth-2048".into(), killer(2048, Some(1024))?),
        ("nth-4096".into(), killer(4096, Some(2048))?),
    ])
}
