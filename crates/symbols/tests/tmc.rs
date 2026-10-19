use adsim_symbols::{SymbolTable, parse_tmc};

const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<TcModuleClass>
  <Modules>
    <Module>
      <DataAreas>
        <DataArea>
          <Symbols>
            <Symbol>
              <Name>MAIN.bReady</Name>
              <Type>BOOL</Type>
              <BitSize>8</BitSize>
              <BitOffs>0</BitOffs>
            </Symbol>
            <Symbol>
              <Name>MAIN.fLevel</Name>
              <Type>LREAL</Type>
              <BitSize>64</BitSize>
              <BitOffs>64</BitOffs>
            </Symbol>
            <Symbol>
              <Name>MAIN.sLabel</Name>
              <Type>STRING(80)</Type>
              <BitSize>648</BitSize>
              <BitOffs>128</BitOffs>
            </Symbol>
          </Symbols>
        </DataArea>
      </DataAreas>
    </Module>
  </Modules>
</TcModuleClass>"#;

#[test]
fn parses_symbols_from_a_tmc() {
    let symbols = parse_tmc(SAMPLE).expect("parse");

    assert_eq!(symbols.len(), 3);
    assert_eq!(symbols[0].name, "MAIN.bReady");
    assert_eq!(symbols[0].data_type, 33);
    assert_eq!(symbols[0].size, 1);
    assert_eq!(symbols[1].size, 8);
    assert_eq!(symbols[2].type_name, "STRING(80)");
    assert_eq!(symbols[2].size, 81);
}

#[test]
fn imported_symbols_are_readable_and_writable() {
    let mut table = SymbolTable::gantry();
    let symbols = parse_tmc(SAMPLE).expect("parse");
    table.add_imported(&symbols);

    assert_eq!(table.len(), 21);
    assert_eq!(table.read_bool("MAIN.bReady"), Some(false));
    table.write_bool("MAIN.bReady", true).expect("write bool");
    assert_eq!(table.read_bool("MAIN.bReady"), Some(true));
    table.write_f64("MAIN.fLevel", 2.5).expect("write real");
    assert_eq!(table.read_f64("MAIN.fLevel"), Some(2.5));
    assert_eq!(table.symbol("MAIN.sLabel").expect("string").size(), 81);
}

#[test]
fn rejects_malformed_xml() {
    assert!(parse_tmc("<TcModuleClass><Modules>").is_err());
}
