// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from libstdc++ stl_algo.h/stl_heap.h (GPL-3.0-or-later with GCC Runtime
// Library Exception). Value-only std::sort ties affect KiCad's adjacent lookups.
// Reproduce the Linux oracle's introsort, including its heap fallback and final
// insertion pass, rather than introducing a different order for equal values.

fn adjust<T: Clone>(
    data: &mut [T],
    mut hole: usize,
    len: usize,
    value: T,
    less: &impl Fn(&T, &T) -> bool,
) {
    let top = hole;
    let mut child = hole;
    while child < (len - 1) / 2 {
        child = 2 * (child + 1);
        if less(&data[child], &data[child - 1]) {
            child -= 1;
        }
        data[hole] = data[child].clone();
        hole = child;
    }
    if len % 2 == 0 && child == (len - 2) / 2 {
        child = 2 * (child + 1);
        data[hole] = data[child - 1].clone();
        hole = child - 1;
    }
    while hole > top {
        let parent = (hole - 1) / 2;
        if !less(&data[parent], &value) {
            break;
        }
        data[hole] = data[parent].clone();
        hole = parent;
    }
    data[hole] = value;
}

fn heap<T: Clone>(data: &mut [T], less: &impl Fn(&T, &T) -> bool) {
    if data.len() < 2 {
        return;
    }
    for parent in (0..=(data.len() - 2) / 2).rev() {
        adjust(data, parent, data.len(), data[parent].clone(), less);
    }
    for end in (1..data.len()).rev() {
        let value = data[end].clone();
        data[end] = data[0].clone();
        adjust(data, 0, end, value, less);
    }
}

fn introsort<T: Clone>(mut data: &mut [T], mut depth: u32, less: &impl Fn(&T, &T) -> bool) {
    while data.len() > 16 {
        if depth == 0 {
            heap(data, less);
            return;
        }
        depth -= 1;
        let (a, b, c) = (1, data.len() / 2, data.len() - 1);
        let median = if less(&data[a], &data[b]) {
            if less(&data[b], &data[c]) {
                b
            } else if less(&data[a], &data[c]) {
                c
            } else {
                a
            }
        } else if less(&data[a], &data[c]) {
            a
        } else if less(&data[b], &data[c]) {
            c
        } else {
            b
        };
        data.swap(0, median);
        let mut first = 1;
        let mut last = data.len();
        loop {
            while less(&data[first], &data[0]) {
                first += 1;
            }
            last -= 1;
            while less(&data[0], &data[last]) {
                last -= 1;
            }
            if first >= last {
                break;
            }
            data.swap(first, last);
            first += 1;
        }
        let (left, right) = data.split_at_mut(first);
        introsort(right, depth, less);
        data = left;
    }
}

pub(crate) fn sort<T: Clone>(data: &mut [T], less: impl Fn(&T, &T) -> bool) {
    if data.is_empty() {
        return;
    }
    introsort(
        data,
        (usize::BITS - 1 - data.len().leading_zeros()) * 2,
        &less,
    );
    // The first introsort partition contains the minimum sentinel; a guarded
    // insertion pass has exactly the same moves as libstdc++'s unguarded tail.
    for i in 1..data.len() {
        let value = data[i].clone();
        let mut hole = i;
        while hole > 0 && less(&value, &data[hole - 1]) {
            data[hole] = data[hole - 1].clone();
            hole -= 1;
        }
        data[hole] = value;
    }
}
