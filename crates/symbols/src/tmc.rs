//! Minimal TwinCAT TMC import: flat PLC symbols with type and size.

use quick_xml::Reader;
use quick_xml::events::Event;

#[derive(Clone, Debug, PartialEq)]
pub struct ImportSymbol {
    pub name: String,
    pub type_name: String,
    pub data_type: u32,
    pub size: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TmcError(String);

impl std::fmt::Display for TmcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TMC error: {}", self.0)
    }
}

impl std::error::Error for TmcError {}

pub fn parse_tmc(xml: &str) -> Result<Vec<ImportSymbol>, TmcError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut symbols = Vec::new();
    let mut current: Option<Partial> = None;
    let mut tag = String::new();
    let mut depth = 0usize;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                depth += 1;
                let name = event.local_name();
                match name.as_ref() {
                    "Symbol" => current = Some(Partial::default()),
                    "Name" | "Type" | "BitSize" if current.is_some() => {
                        tag = name.as_ref().to_string();
                    }
                    _ => tag.clear(),
                }
            }
            Ok(Event::Text(text)) => {
                let Some(partial) = &mut current else {
                    continue;
                };
                if tag.is_empty() {
                    continue;
                }
                let value = text.as_ref().trim().to_string();
                match tag.as_str() {
                    "Name" => partial.name = value,
                    "Type" => partial.type_name = value,
                    "BitSize" => partial.bit_size = value.parse().unwrap_or(0),
                    _ => {}
                }
            }
            Ok(Event::End(event)) => {
                depth = depth.saturating_sub(1);
                if event.local_name().as_ref() != "Symbol" {
                    continue;
                }
                if let Some(symbol) = current.take().and_then(Partial::build) {
                    symbols.push(symbol);
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(TmcError(error.to_string())),
            _ => {}
        }
    }

    if depth != 0 || current.is_some() {
        return Err(TmcError("unclosed XML elements".to_string()));
    }
    Ok(symbols)
}

#[derive(Default)]
struct Partial {
    name: String,
    type_name: String,
    bit_size: u32,
}

impl Partial {
    fn build(self) -> Option<ImportSymbol> {
        if self.name.is_empty() || self.type_name.is_empty() || self.bit_size == 0 {
            return None;
        }
        Some(ImportSymbol {
            name: self.name,
            data_type: data_type(&self.type_name),
            size: self.bit_size.div_ceil(8).max(1),
            type_name: self.type_name,
        })
    }
}

fn data_type(type_name: &str) -> u32 {
    let base = type_name
        .split('(')
        .next()
        .unwrap_or(type_name)
        .trim()
        .to_uppercase();

    match base.as_str() {
        "BOOL" => 33,
        "SINT" => 16,
        "BYTE" | "USINT" => 17,
        "INT" | "INT16" => 2,
        "UINT" | "UINT16" | "WORD" => 18,
        "DINT" | "INT32" => 3,
        "UDINT" | "UINT32" | "DWORD" | "TIME" | "DATE" => 19,
        "LINT" | "INT64" => 20,
        "ULINT" | "UINT64" | "LTIME" => 21,
        "REAL" | "REAL32" => 4,
        "LREAL" | "REAL64" => 5,
        "STRING" => 30,
        "WSTRING" => 31,
        _ => 0,
    }
}
