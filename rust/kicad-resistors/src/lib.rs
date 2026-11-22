// SPDX-License-Identifier: GPL-3.0-or-later
//! Faithful owned-context port of KiCad's E-series and resistor substitution.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod eseries;
mod format;
mod fp;
use fp::{parallel, serial};
pub mod reader;
pub mod scales;
mod sort;
pub use format::str_value;

/// Pure calculator API plus the separately owned, documented reader co-port.
pub mod application {
    pub use crate::{
        eseries, reader, resistance_less, resistance_less_value, scales, str_value, Calculator,
        Error, Resistance,
    };
}

pub const EPSILON: f64 = 1e-12;

#[derive(Clone, Debug, PartialEq)]
pub struct Resistance {
    pub value: f64,
    pub name: String,
    pub parts: Vec<f64>,
}

impl Resistance {
    pub fn new(value: f64, name: String, mut parts: Vec<f64>) -> Self {
        if parts.is_empty() {
            parts.push(value);
        }
        Self { value, name, parts }
    }
}

pub fn resistance_less(first: &Resistance, second: &Resistance) -> bool {
    first.value < second.value
}

pub fn resistance_less_value(first: &Resistance, second: f64) -> bool {
    first.value < second
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A typed boundary refusal for a C++ precondition that otherwise causes UB.
    Domain(&'static str),
    /// The actual C++ SolutionCollector::GetBest exception, with its exact text.
    Logic(&'static str),
}

/// Test boundary corresponding to the reference library's source-local accessor.
/// Production searches above use the same collector and combination helpers.
pub mod testing {
    use super::*;
    use std::cell::Cell;

    pub fn collect(
        target: f64,
        candidates: &[Resistance],
    ) -> Result<(Resistance, Resistance, usize), Error> {
        let mut collector = Collector::new(target);
        let callbacks = Cell::new(0usize);
        for candidate in candidates {
            collector.add(
                (candidate, candidate),
                |value| value,
                |found| {
                    callbacks.set(callbacks.get() + 1);
                    found.clone()
                },
            );
        }
        Ok((collector.best()?, collector.best()?, callbacks.get()))
    }
    pub fn unique(value: &Resistance) -> usize {
        Collector::unique(value)
    }
    pub fn better(candidate: &Resistance, best: &Resistance) -> bool {
        Collector::better(candidate, best)
    }
    pub fn embrace(text: &str, symbol: char) -> String {
        super::embrace(text, symbol)
    }
    pub fn combine(
        first: &Resistance,
        second: &Resistance,
        parallel: bool,
        simple: bool,
    ) -> Resistance {
        super::combine(first, second, parallel, simple)
    }
}

fn embrace(text: &str, required: char) -> String {
    let mut level = 0i64;
    let mut needed = false;
    for c in text.chars() {
        if c == '(' {
            level += 1;
        } else if c == ')' {
            level -= 1;
        } else if c == required && level == 0 {
            needed = true;
        }
    }
    if needed {
        format!("({text})")
    } else {
        text.into()
    }
}

fn combine(a: &Resistance, b: &Resistance, par: bool, simple: bool) -> Resistance {
    let (operator, required) = if par { (" | ", '+') } else { (" + ", '|') };
    let name = if simple {
        format!("{}{operator}{}", a.name, b.name)
    } else {
        format!(
            "{}{operator}{}",
            embrace(&a.name, required),
            embrace(&b.name, required)
        )
    };
    let mut parts = a.parts.clone();
    parts.extend_from_slice(&b.parts);
    Resistance::new(
        if par {
            parallel(a.value, b.value)
        } else {
            serial(a.value, b.value)
        },
        name,
        parts,
    )
}

/// The collector invokes the real result continuation only for an improvement
/// or an epsilon tie, in the same order as C++'s std::function callbacks.
struct Collector {
    target: f64,
    deviation: f64,
    best: Option<Resistance>,
}

impl Collector {
    fn new(target: f64) -> Self {
        Self {
            target,
            deviation: f64::INFINITY,
            best: None,
        }
    }

    fn unique(res: &Resistance) -> usize {
        let mut parts = res.parts.clone();
        sort::sort(&mut parts, |a, b| a < b);
        let mut count = 0;
        let mut last = 0.0;
        let mut first = true;
        for value in parts {
            if first || (value - last).abs() > EPSILON {
                count += 1;
                last = value;
                first = false;
            }
        }
        count
    }

    fn better(candidate: &Resistance, best: &Resistance) -> bool {
        let (a, b) = (Self::unique(candidate), Self::unique(best));
        if a != b {
            return a < b;
        }
        if candidate.parts.len() != best.parts.len() {
            return candidate.parts.len() < best.parts.len();
        }
        candidate.name < best.name
    }

    fn consider(
        &mut self,
        value: f64,
        found: &Resistance,
        result: &impl Fn(&Resistance) -> Resistance,
    ) {
        let deviation = (value - self.target).abs();
        if deviation + EPSILON < self.deviation {
            self.deviation = deviation;
            self.best = Some(result(found));
        } else if (deviation - self.deviation).abs() < EPSILON {
            let candidate = result(found);
            if self
                .best
                .as_ref()
                .is_none_or(|best| Self::better(&candidate, best))
            {
                self.best = Some(candidate);
            }
        }
    }

    fn add(
        &mut self,
        pair: (&Resistance, &Resistance),
        value: impl Fn(f64) -> f64,
        result: impl Fn(&Resistance) -> Resistance,
    ) {
        self.consider(value(pair.0.value), pair.0, &result);
        self.consider(value(pair.1.value), pair.1, &result);
    }

    fn best(&self) -> Result<Resistance, Error> {
        self.best
            .clone()
            .ok_or(Error::Logic("Empty solution collector"))
    }
}

#[derive(Clone, Debug)]
pub struct Calculator {
    series_data: Vec<Vec<Resistance>>,
    exclusions: Vec<bool>,
    buffer_1r: Vec<Resistance>,
    buffer_2r: Vec<Resistance>,
    series: u32,
    target: f64,
    results: [Option<Resistance>; 3],
}

impl Default for Calculator {
    fn default() -> Self {
        Self {
            series_data: (0..5).map(Self::build_series).collect(),
            exclusions: Vec::new(),
            buffer_1r: Vec::new(),
            buffer_2r: Vec::new(),
            series: 2,
            target: 0.0,
            results: [None, None, None],
        }
    }
}

impl Calculator {
    pub fn series(&self) -> u32 {
        self.series
    }
    pub fn target(&self) -> f64 {
        self.target
    }
    pub fn series_data(&self) -> &[Vec<Resistance>] {
        &self.series_data
    }
    pub fn exclusions(&self) -> &[bool] {
        &self.exclusions
    }
    pub fn buffer_1r(&self) -> &[Resistance] {
        &self.buffer_1r
    }
    pub fn buffer_2r(&self) -> &[Resistance] {
        &self.buffer_2r
    }

    pub fn build_series(series: i32) -> Vec<Resistance> {
        let list = eseries::values(series);
        // Every immutable named table starts at 100. The general owned helper
        // below accepts the actual caller-supplied first element instead.
        Self::build_values(&list, 100.0)
    }

    pub fn build_series_data(list: &[u16]) -> Result<Vec<Resistance>, Error> {
        let first = *list
            .first()
            .ok_or(Error::Domain("series table must be nonempty"))?;
        if list.iter().all(|value| *value == 0) {
            return Err(Error::Domain("all-zero series table does not terminate"));
        }
        Ok(Self::build_values(list, f64::from(first)))
    }

    fn build_values(list: &[u16], first: f64) -> Vec<Resistance> {
        let mut result = Vec::new();
        let mut decade = 10.0;
        loop {
            let multiplier = decade / first;
            for list_value in list {
                let value = fp::scale(multiplier, *list_value);
                result.push(Resistance::new(value, str_value(value), Vec::new()));
                if value >= 1e6 {
                    return result;
                }
            }
            decade *= 10.0;
        }
    }

    pub fn set_series(&mut self, series: u32) {
        self.series = series;
    }

    fn selected(&self) -> Result<&[Resistance], Error> {
        self.series_data
            .get(self.series as usize)
            .map(Vec::as_slice)
            .ok_or(Error::Domain("calculator series must be 0..4"))
    }

    pub fn new_calc(&mut self, target: f64) -> Result<(), Error> {
        self.target = target;
        let len = self.selected()?.len();
        self.exclusions.resize(len, false);
        self.exclusions.fill(false);
        self.results.fill(None);
        Ok(())
    }

    pub fn exclude(&mut self, value: f64) -> Result<(), Error> {
        if value.is_nan() {
            return Ok(());
        }
        let series = self.selected()?;
        let i = series.partition_point(|r| resistance_less_value(r, value - EPSILON));
        if series
            .get(i)
            .is_some_and(|r| (r.value - value).abs() < EPSILON)
        {
            let mask = self
                .exclusions
                .get_mut(i)
                .ok_or(Error::Domain("NewCalc required before Exclude"))?;
            *mask = true;
        }
        Ok(())
    }

    pub fn prepare_1r(&mut self) -> Result<(), Error> {
        let series = self.selected()?;
        if self.exclusions.len() < series.len() {
            return Err(Error::Domain("NewCalc required before Calculate"));
        }
        self.buffer_1r = series
            .iter()
            .zip(&self.exclusions)
            .filter(|(_, excluded)| !**excluded)
            .map(|(r, _)| r.clone())
            .collect();
        Ok(())
    }

    pub fn prepare_2r(&mut self) {
        self.buffer_2r.clear();
        for (i, a) in self.buffer_1r.iter().enumerate() {
            for b in &self.buffer_1r[i..] {
                self.buffer_2r.push(combine(a, b, false, true));
                self.buffer_2r.push(combine(a, b, true, true));
            }
        }
        sort::sort(&mut self.buffer_2r, resistance_less);
    }

    pub fn find(&self, target: f64) -> Result<(&Resistance, &Resistance), Error> {
        let first = self
            .buffer_2r
            .first()
            .ok_or(Error::Domain("2R buffer must be nonempty"))?;
        let last = self
            .buffer_2r
            .last()
            .ok_or(Error::Domain("2R buffer must be nonempty"))?;
        if target.is_nan() {
            return Ok((first, first));
        }
        if target <= first.value || target >= last.value {
            return Ok((first, last));
        }
        let i = self
            .buffer_2r
            .partition_point(|r| resistance_less_value(r, target));
        if i == 0 {
            Ok((first, first))
        } else if i == self.buffer_2r.len() {
            Ok((last, last))
        } else {
            Ok((&self.buffer_2r[i - 1], &self.buffer_2r[i]))
        }
    }

    pub fn solution_2r(&self) -> Result<Resistance, Error> {
        let mut solution = Collector::new(self.target);
        solution.add(self.find(self.target)?, |value| value, Clone::clone);
        solution.best()
    }

    pub fn solution_3r(&self) -> Result<Resistance, Error> {
        let mut solution = Collector::new(self.target);
        for r in &self.buffer_1r {
            solution.add(
                self.find(self.target - r.value)?,
                |v| serial(v, r.value),
                |found| combine(found, r, false, false),
            );
            solution.add(
                self.find(self.target * r.value / (r.value - self.target))?,
                |v| parallel(v, r.value),
                |found| combine(found, r, true, false),
            );
        }
        solution.best()
    }

    pub fn solution_4r(&self) -> Result<Resistance, Error> {
        let mut solution = Collector::new(self.target);
        for rr in &self.buffer_2r {
            solution.add(
                self.find(self.target - rr.value)?,
                |v| serial(v, rr.value),
                |found| combine(found, rr, false, false),
            );
            solution.add(
                self.find(self.target * rr.value / (rr.value - self.target))?,
                |v| parallel(v, rr.value),
                |found| combine(found, rr, true, false),
            );
        }
        for r1 in &self.buffer_1r {
            for r2 in &self.buffer_1r {
                solution.add(
                    self.find(
                        (self.target - r1.value) * r2.value / (r1.value + r2.value - self.target),
                    )?,
                    |v| serial(r1.value, parallel(r2.value, v)),
                    |found| combine(r1, &combine(r2, found, true, false), false, false),
                );
                solution.add(
                    self.find(self.target * r1.value / (r1.value - self.target) - r2.value)?,
                    |v| parallel(r1.value, serial(r2.value, v)),
                    |found| combine(r1, &combine(r2, found, false, false), true, false),
                );
            }
        }
        solution.best()
    }

    pub fn calculate(&mut self) -> Result<(), Error> {
        self.prepare_1r()?;
        self.prepare_2r();
        let r2 = self.solution_2r()?;
        let need_3r = (r2.value - self.target).abs() > EPSILON;
        self.results[0] = Some(r2);
        if need_3r {
            let r3 = self.solution_3r()?;
            let need_4r = (r3.value - self.target).abs() > EPSILON;
            self.results[1] = Some(r3);
            if need_4r {
                self.results[2] = Some(self.solution_4r()?);
            }
        }
        Ok(())
    }

    pub fn get_results(&self) -> &[Option<Resistance>; 3] {
        &self.results
    }
}
