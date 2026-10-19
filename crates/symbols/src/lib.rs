//! Symbol table and ADS symbol upload encoding for the built-in gantry.

mod tmc;

use std::collections::HashMap;

pub use tmc::{ImportSymbol, TmcError, parse_tmc};

pub const INDEX_GROUP: u32 = 0x4020;
pub const TYPE_BOOL: u32 = 33;
pub const TYPE_INT32: u32 = 3;
pub const TYPE_LREAL: u32 = 5;
pub const FLAG_BITVALUE: u32 = 0x0002;

const ENTRY_HEADER_LEN: usize = 30;

pub struct Symbol {
    pub name: String,
    pub index_group: u32,
    pub index_offset: u32,
    pub data_type: u32,
    pub type_name: String,
    data: Vec<u8>,
}

impl Symbol {
    pub fn size(&self) -> u32 {
        self.data.len() as u32
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn entry(&self) -> Vec<u8> {
        let name_len = self.name.len();
        let type_len = self.type_name.len();
        let comment_len = 0;
        let entry_len = ENTRY_HEADER_LEN + name_len + 1 + type_len + 1 + comment_len + 1;
        let flags = if self.data_type == TYPE_BOOL {
            FLAG_BITVALUE
        } else {
            0
        };

        let mut out = Vec::with_capacity(entry_len);
        out.extend_from_slice(&(entry_len as u32).to_le_bytes());
        out.extend_from_slice(&self.index_group.to_le_bytes());
        out.extend_from_slice(&self.index_offset.to_le_bytes());
        out.extend_from_slice(&self.size().to_le_bytes());
        out.extend_from_slice(&self.data_type.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&(name_len as u16).to_le_bytes());
        out.extend_from_slice(&(type_len as u16).to_le_bytes());
        out.extend_from_slice(&(comment_len as u16).to_le_bytes());
        out.extend_from_slice(self.name.as_bytes());
        out.push(0);
        out.extend_from_slice(self.type_name.as_bytes());
        out.push(0);
        out.push(0);
        out
    }
}

pub struct SymbolTable {
    symbols: Vec<Symbol>,
    handles: HashMap<u32, usize>,
    next_handle: u32,
}

impl SymbolTable {
    pub fn gantry() -> Self {
        let mut table = Self {
            symbols: Vec::new(),
            handles: HashMap::new(),
            next_handle: 1,
        };
        let mut offset = 0;

        for axis in ["X", "Y", "Z"] {
            let prefix = format!("MAIN.stAxis{axis}");
            table.add(
                format!("{prefix}.fActPos"),
                "LREAL",
                TYPE_LREAL,
                0.0f64.to_le_bytes().to_vec(),
                offset,
            );
            offset += 8;
            table.add(
                format!("{prefix}.fSetPos"),
                "LREAL",
                TYPE_LREAL,
                0.0f64.to_le_bytes().to_vec(),
                offset,
            );
            offset += 8;
            table.add(
                format!("{prefix}.bEnabled"),
                "BOOL",
                TYPE_BOOL,
                vec![0],
                offset,
            );
            offset += 8;
            table.add(
                format!("{prefix}.bHomed"),
                "BOOL",
                TYPE_BOOL,
                vec![0],
                offset,
            );
            offset += 8;
            table.add(
                format!("{prefix}.nError"),
                "INT32",
                TYPE_INT32,
                0i32.to_le_bytes().to_vec(),
                offset,
            );
            offset += 8;
        }

        table.add(
            "MAIN.bReset".to_string(),
            "BOOL",
            TYPE_BOOL,
            vec![0],
            offset,
        );
        offset += 8;
        table.add(
            "MAIN.bAlarm".to_string(),
            "BOOL",
            TYPE_BOOL,
            vec![0],
            offset,
        );
        offset += 8;
        table.add(
            "MAIN.nAlarmCode".to_string(),
            "INT32",
            TYPE_INT32,
            0i32.to_le_bytes().to_vec(),
            offset,
        );

        table
    }

    /// Append imported symbols as writable memory. Names that already exist
    /// are skipped, so the built-in gantry symbols always win.
    pub fn add_imported(&mut self, imported: &[ImportSymbol]) {
        let mut offset = self
            .symbols
            .iter()
            .map(|symbol| symbol.index_offset + 8)
            .max()
            .unwrap_or(0);

        for symbol in imported {
            if self.symbol(&symbol.name).is_some() {
                continue;
            }
            self.add(
                symbol.name.clone(),
                &symbol.type_name,
                symbol.data_type,
                vec![0u8; symbol.size as usize],
                offset,
            );
            offset += 8;
        }
    }

    fn add(
        &mut self,
        name: String,
        type_name: &str,
        data_type: u32,
        data: Vec<u8>,
        index_offset: u32,
    ) {
        self.symbols.push(Symbol {
            name,
            index_group: INDEX_GROUP,
            index_offset,
            data_type,
            type_name: type_name.to_string(),
            data,
        });
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    pub fn symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|symbol| symbol.name == name)
    }

    pub fn handle(&mut self, name: &str) -> Option<u32> {
        let index = self.symbols.iter().position(|symbol| symbol.name == name)?;
        if let Some((handle, _)) = self
            .handles
            .iter()
            .find(|(_, stored_index)| **stored_index == index)
        {
            return Some(*handle);
        }

        let handle = self.next_handle;
        self.next_handle += 1;
        self.handles.insert(handle, index);
        Some(handle)
    }

    pub fn release_handle(&mut self, handle: u32) -> bool {
        self.handles.remove(&handle).is_some()
    }

    pub fn read_handle(&self, handle: u32) -> Option<Vec<u8>> {
        let &index = self.handles.get(&handle)?;
        Some(self.symbols[index].data.clone())
    }

    pub fn write_handle(&mut self, handle: u32, data: &[u8]) -> Option<()> {
        let &index = self.handles.get(&handle)?;
        self.write_symbol(index, data)
    }

    pub fn read_by_index(
        &self,
        index_group: u32,
        index_offset: u32,
        length: u32,
    ) -> Option<Vec<u8>> {
        let symbol = self.symbols.iter().find(|symbol| {
            symbol.index_group == index_group && symbol.index_offset == index_offset
        })?;
        let length = (length as usize).min(symbol.data.len());
        Some(symbol.data[..length].to_vec())
    }

    pub fn write_by_index(
        &mut self,
        index_group: u32,
        index_offset: u32,
        data: &[u8],
    ) -> Option<()> {
        let index = self.symbols.iter().position(|symbol| {
            symbol.index_group == index_group && symbol.index_offset == index_offset
        })?;
        self.write_symbol(index, data)
    }

    fn write_symbol(&mut self, index: usize, data: &[u8]) -> Option<()> {
        let symbol = &mut self.symbols[index];
        let length = data.len().min(symbol.data.len());
        symbol.data[..length].copy_from_slice(&data[..length]);
        Some(())
    }

    pub fn read_f64(&self, name: &str) -> Option<f64> {
        let bytes: [u8; 8] = self.symbol(name)?.data.get(..8)?.try_into().ok()?;
        Some(f64::from_le_bytes(bytes))
    }

    pub fn write_f64(&mut self, name: &str, value: f64) -> Option<()> {
        self.write_named(name, &value.to_le_bytes())
    }

    pub fn read_bool(&self, name: &str) -> Option<bool> {
        Some(self.symbol(name)?.data.first().copied()? != 0)
    }

    pub fn write_bool(&mut self, name: &str, value: bool) -> Option<()> {
        self.write_named(name, &[u8::from(value)])
    }

    pub fn read_i32(&self, name: &str) -> Option<i32> {
        let bytes: [u8; 4] = self.symbol(name)?.data.get(..4)?.try_into().ok()?;
        Some(i32::from_le_bytes(bytes))
    }

    pub fn write_i32(&mut self, name: &str, value: i32) -> Option<()> {
        self.write_named(name, &value.to_le_bytes())
    }

    fn write_named(&mut self, name: &str, data: &[u8]) -> Option<()> {
        let index = self.symbols.iter().position(|symbol| symbol.name == name)?;
        self.write_symbol(index, data)
    }

    pub fn upload_info2(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(24);
        out.extend_from_slice(&(self.symbols.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.upload().len() as u32).to_le_bytes());
        out.extend_from_slice(&[0u8; 16]);
        out
    }

    pub fn upload(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for symbol in &self.symbols {
            out.extend_from_slice(&symbol.entry());
        }
        out
    }
}
