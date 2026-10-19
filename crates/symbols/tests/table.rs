use adsim_symbols::{SymbolTable, TYPE_BOOL, TYPE_INT32, TYPE_LREAL};

#[test]
fn gantry_has_axis_symbols() {
    let table = SymbolTable::gantry();
    let symbol = table.symbol("MAIN.stAxisX.fActPos").expect("symbol");

    assert_eq!(symbol.size(), 8);
    assert_eq!(symbol.type_name, "LREAL");
    assert_eq!(symbol.data_type, TYPE_LREAL);
    assert_eq!(
        table
            .symbol("MAIN.stAxisX.nError")
            .expect("symbol")
            .data_type,
        TYPE_INT32
    );
}

#[test]
fn upload_info2_describes_the_upload_blob() {
    let table = SymbolTable::gantry();
    let info = table.upload_info2();

    assert_eq!(info.len(), 24);
    assert_eq!(
        u32::from_le_bytes(info[0..4].try_into().expect("symbol count")) as usize,
        table.len()
    );
    assert_eq!(
        u32::from_le_bytes(info[4..8].try_into().expect("symbol size")) as usize,
        table.upload().len()
    );
    assert_eq!(&info[8..24], &[0u8; 16]);
}

#[test]
fn upload_lists_names_and_types() {
    let table = SymbolTable::gantry();
    let blob = table.upload();
    let mut names = Vec::new();
    let mut position = 0;

    while position < blob.len() {
        let entry_len = u32::from_le_bytes(
            blob[position..position + 4]
                .try_into()
                .expect("entry length"),
        ) as usize;
        let name_len = u16::from_le_bytes(
            blob[position + 24..position + 26]
                .try_into()
                .expect("name length"),
        ) as usize;
        let type_len = u16::from_le_bytes(
            blob[position + 26..position + 28]
                .try_into()
                .expect("type length"),
        ) as usize;
        let comment_len = u16::from_le_bytes(
            blob[position + 28..position + 30]
                .try_into()
                .expect("comment length"),
        ) as usize;

        assert_eq!(
            entry_len,
            30 + name_len + 1 + type_len + 1 + comment_len + 1
        );
        let name = std::str::from_utf8(&blob[position + 30..position + 30 + name_len])
            .expect("utf-8 name");
        names.push(name.to_string());
        position += entry_len;
    }

    assert!(names.contains(&"MAIN.stAxisX.fActPos".to_string()));
    assert!(names.contains(&"MAIN.stAxisZ.nError".to_string()));
}

#[test]
fn entry_matches_the_upload_encoding() {
    let table = SymbolTable::gantry();
    let symbol = table.symbol("MAIN.stAxisX.fActPos").expect("symbol");
    let entry = symbol.entry();
    let entry_len = u32::from_le_bytes(entry[0..4].try_into().expect("entry length"));

    assert_eq!(entry_len as usize, entry.len());
    assert_eq!(&entry[4..8], &0x4020u32.to_le_bytes());
    assert_eq!(&entry[30..50], b"MAIN.stAxisX.fActPos");
    assert_eq!(entry[50], 0);
    assert_eq!(&entry[51..56], b"LREAL");
    assert_eq!(entry[56], 0);
    assert_eq!(entry[57], 0);
}

#[test]
fn handles_read_and_write_a_symbol() {
    let mut table = SymbolTable::gantry();
    let handle = table.handle("MAIN.stAxisX.fActPos").expect("handle");

    assert_ne!(handle, 0);
    assert_eq!(table.read_handle(handle), Some(vec![0u8; 8]));

    table
        .write_handle(handle, &1.5f64.to_le_bytes())
        .expect("write");
    assert_eq!(
        table.read_handle(handle),
        Some(1.5f64.to_le_bytes().to_vec())
    );
}

#[test]
fn releasing_a_handle_invalidates_it() {
    let mut table = SymbolTable::gantry();
    let handle = table.handle("MAIN.stAxisX.fActPos").expect("handle");

    assert!(table.release_handle(handle));
    assert_eq!(table.read_handle(handle), None);
    assert!(!table.release_handle(handle));
}

#[test]
fn index_group_access_reads_and_writes_a_symbol() {
    let mut table = SymbolTable::gantry();
    let symbol = table.symbol("MAIN.stAxisX.fActPos").expect("symbol");
    let (index_group, index_offset) = (symbol.index_group, symbol.index_offset);

    assert_eq!(
        table.read_by_index(index_group, index_offset, 8),
        Some(vec![0u8; 8])
    );
    table
        .write_by_index(index_group, index_offset, &2.5f64.to_le_bytes())
        .expect("write");
    assert_eq!(
        table.read_by_index(index_group, index_offset, 8),
        Some(2.5f64.to_le_bytes().to_vec())
    );
}

#[test]
fn typed_accessors_read_and_write_values() {
    let mut table = SymbolTable::gantry();

    assert_eq!(table.read_f64("MAIN.stAxisX.fActPos"), Some(0.0));
    table
        .write_f64("MAIN.stAxisX.fActPos", 1.25)
        .expect("write f64");
    assert_eq!(table.read_f64("MAIN.stAxisX.fActPos"), Some(1.25));

    assert_eq!(table.read_bool("MAIN.stAxisX.bEnabled"), Some(false));
    table
        .write_bool("MAIN.stAxisX.bEnabled", true)
        .expect("write bool");
    assert_eq!(table.read_bool("MAIN.stAxisX.bEnabled"), Some(true));

    assert_eq!(table.read_i32("MAIN.stAxisX.nError"), Some(0));
    table
        .write_i32("MAIN.stAxisX.nError", 7)
        .expect("write i32");
    assert_eq!(table.read_i32("MAIN.stAxisX.nError"), Some(7));

    assert_eq!(table.read_f64("MAIN.nope"), None);
    assert_eq!(table.write_bool("MAIN.nope", true), None);
}

#[test]
fn gantry_has_a_global_reset_symbol() {
    let table = SymbolTable::gantry();
    let reset = table.symbol("MAIN.bReset").expect("reset symbol");

    assert_eq!(reset.data_type, TYPE_BOOL);
}

#[test]
fn unknown_names_and_ids_are_rejected() {
    let mut table = SymbolTable::gantry();

    assert!(table.handle("MAIN.nope").is_none());
    assert_eq!(table.read_handle(999), None);
    assert_eq!(table.read_by_index(0x1234, 0, 4), None);
    assert!(table.write_by_index(0x1234, 0, &[0]).is_none());
}
