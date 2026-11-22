// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_resistors::reader::DatafileParserReader;

#[test]
fn getter_preserves_nullable_borrowed_identity() {
    let readers = [String::from("same"), String::from("same")];
    let mut parser = DatafileParserReader::from_lexer_state(Some(&readers[0]));
    assert!(parser
        .get_reader()
        .is_some_and(|reader| std::ptr::eq(reader, &readers[0])));
    parser.bind_lexer_state(Some(&readers[1]));
    assert!(parser
        .get_reader()
        .is_some_and(|reader| std::ptr::eq(reader, &readers[1])));
    parser.bind_lexer_state(None);
    assert!(parser.get_reader().is_none());
}

#[test]
fn constructors_outside_owned_scope_remain_explicitly_unported() {
    let reader = String::new();
    assert!(DatafileParserReader::from_line_reader(Some(&reader))
        .is_err_and(|error| error.unit == "pcb_calculator.application.ui.p001"));
    assert!(DatafileParserReader::<String>::from_text(b"", "")
        .is_err_and(|error| error.unit == "pcb_calculator.application.ui.p001"));
}
