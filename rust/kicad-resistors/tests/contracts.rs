// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_resistors::{eseries, Calculator, Error, Resistance};

#[test]
fn contexts_own_their_state_and_new_calc_resets_only_reference_fields() -> Result<(), Error> {
    let mut first = Calculator::default();
    let second = Calculator::default();
    assert_eq!(first.series(), 2);
    assert_eq!(first.target().to_bits(), 0);
    assert!(first.exclusions().is_empty());
    first.new_calc(20.)?;
    first.calculate()?;
    assert!(first.get_results()[0].is_some());
    assert!(second.get_results().iter().all(Option::is_none));
    first.exclude(10.)?;
    assert!(first.exclusions().contains(&true));
    let previous = first.buffer_2r().to_vec();
    first.new_calc(123.456)?;
    assert!(first.get_results().iter().all(Option::is_none));
    assert!(first.exclusions().iter().all(|excluded| !excluded));
    assert_eq!(first.buffer_2r(), previous);
    assert!(second.exclusions().is_empty());
    Ok(())
}

#[test]
fn undefined_cpp_preconditions_are_typed_refusals() -> Result<(), Error> {
    let mut calc = Calculator::default();
    assert!(matches!(calc.calculate(), Err(Error::Domain(_))));
    assert!(matches!(calc.find(0.), Err(Error::Domain(_))));
    calc.set_series(u32::MAX);
    assert!(matches!(calc.new_calc(0.), Err(Error::Domain(_))));
    calc.set_series(0);
    calc.new_calc(1.)?;
    for exponent in 1..=6 {
        calc.exclude(10f64.powi(exponent))?;
    }
    calc.prepare_1r()?;
    calc.prepare_2r();
    assert!(calc.buffer_2r().is_empty());
    assert!(matches!(calc.calculate(), Err(Error::Domain(_))));
    Ok(())
}

#[test]
fn actual_collector_exception_is_distinct_from_precondition_refusal() -> Result<(), Error> {
    let mut calc = Calculator::default();
    calc.new_calc(f64::NAN)?;
    assert_eq!(
        calc.calculate(),
        Err(Error::Logic("Empty solution collector"))
    );
    assert!(!calc.buffer_2r().is_empty());
    assert!(calc.get_results().iter().all(Option::is_none));
    Ok(())
}

#[test]
fn literal_series_and_resistance_default_contracts() {
    for (selector, length) in [1, 3, 6, 12, 24, 48, 96, 192].into_iter().enumerate() {
        assert_eq!(eseries::values(selector as i32).len(), length);
    }
    assert_eq!(eseries::values(i32::MIN), vec![100]);
    assert_eq!(eseries::values(i32::MAX), vec![100]);
    let r = Resistance::new(-0., "".into(), vec![]);
    assert_eq!(
        r.parts.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        vec![(-0f64).to_bits()]
    );
}

#[test]
fn private_builder_rejects_only_undefined_or_nonterminating_tables() {
    assert_eq!(
        Calculator::build_series_data(&[]),
        Err(Error::Domain("series table must be nonempty"))
    );
    assert_eq!(
        Calculator::build_series_data(&[0, 0]),
        Err(Error::Domain("all-zero series table does not terminate"))
    );
}
