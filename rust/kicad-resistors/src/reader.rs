// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal parser/lexer reader state co-port for the owned GetReader accessor.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unported {
    pub upstream: &'static str,
    pub unit: &'static str,
}

/// The surrounding lexer owns its state; this context borrows the current reader.
/// It neither constructs a C++ parser nor reads a byte from the reader.
#[derive(Debug)]
pub struct DatafileParserReader<'reader, R> {
    reader: Option<&'reader R>,
}

impl<'reader, R> DatafileParserReader<'reader, R> {
    /// Bind already-provided lexer state, including a null current reader.
    pub fn from_lexer_state(reader: Option<&'reader R>) -> Self {
        Self { reader }
    }

    /// Mirrors PCB_CALCULATOR_DATAFILE_PARSER::GetReader without cloning or ownership transfer.
    pub fn get_reader(&self) -> Option<&'reader R> {
        self.reader
    }

    /// The caller supplies the next surrounding lexer state; this is not PushReader/PopReader.
    pub fn bind_lexer_state(&mut self, reader: Option<&'reader R>) {
        self.reader = reader;
    }

    pub fn from_line_reader(_reader: Option<&'reader R>) -> Result<Self, Unported> {
        // UNPORTED(pcb_calculator.application.ui.p001): PCB_CALCULATOR_DATAFILE_PARSER(LINE_READER*)
        Err(Unported {
            upstream:
                "PCB_CALCULATOR_DATAFILE_PARSER::PCB_CALCULATOR_DATAFILE_PARSER(LINE_READER*)",
            unit: "pcb_calculator.application.ui.p001",
        })
    }

    pub fn from_text(_line: &[u8], _source: &str) -> Result<Self, Unported> {
        // UNPORTED(pcb_calculator.application.ui.p001): PCB_CALCULATOR_DATAFILE_PARSER(char*, const wxString&)
        Err(Unported { upstream: "PCB_CALCULATOR_DATAFILE_PARSER::PCB_CALCULATOR_DATAFILE_PARSER(char*, const wxString&)", unit: "pcb_calculator.application.ui.p001" })
    }
}
