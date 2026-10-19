//! Minimal ADS client for adsim and TwinCAT targets.
//!
//! Speaks AMS/TCP, resolves symbols by name and maps BOOL, INT32 and LREAL
//! values to JSON. Other types are read and written as byte arrays.

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use ams::{AMS_HEADER_LEN, AmsAddress, AmsHeader, Command, FLAG_REQUEST, Packet};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const SYM_HNDBYNAME: u32 = 0xF003;
const SYM_VALBYHND: u32 = 0xF005;
const SYM_UPLOAD: u32 = 0xF00B;
const SYM_UPLOADINFO2: u32 = 0xF00F;

const TYPE_INT32: u32 = 3;
const TYPE_LREAL: u32 = 5;
const TYPE_BOOL: u32 = 33;

const ADS_PORT: u16 = 851;
const SOURCE_NET_ID: [u8; 6] = [10, 0, 0, 1, 1, 1];
const SOURCE_PORT: u16 = 30000;

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub enum ClientError {
    Io(std::io::Error),
    Decode(&'static str),
    Ams(u32),
    Ads(u32),
    UnknownSymbol(String),
    UnsupportedType(u32),
    UnsupportedValue,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "io error: {error}"),
            Self::Decode(reason) => write!(f, "decode error: {reason}"),
            Self::Ams(code) => write!(f, "AMS error 0x{code:04X}"),
            Self::Ads(code) => write!(f, "ADS error 0x{code:04X}"),
            Self::UnknownSymbol(name) => write!(f, "unknown symbol: {name}"),
            Self::UnsupportedType(data_type) => write!(f, "unsupported ADS type: {data_type}"),
            Self::UnsupportedValue => write!(f, "value does not match the symbol type"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<std::io::Error> for ClientError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolInfo {
    pub name: String,
    pub type_name: String,
    pub data_type: u32,
    pub size: u32,
}

pub struct AdsClient {
    stream: TcpStream,
    target: AmsAddress,
    source: AmsAddress,
    invoke_id: u32,
    handles: HashMap<String, u32>,
    symbols: Vec<SymbolInfo>,
}

impl AdsClient {
    pub async fn connect(server: &str, target_net_id: [u8; 6]) -> Result<Self, ClientError> {
        let stream = TcpStream::connect(server).await?;
        let mut client = Self {
            stream,
            target: AmsAddress::new(target_net_id, ADS_PORT),
            source: AmsAddress::new(SOURCE_NET_ID, SOURCE_PORT),
            invoke_id: 1,
            handles: HashMap::new(),
            symbols: Vec::new(),
        };
        client.symbols = client.load_symbols().await?;
        Ok(client)
    }

    pub fn symbols(&self) -> &[SymbolInfo] {
        &self.symbols
    }

    pub async fn read_symbol(&mut self, name: &str) -> Result<Value, ClientError> {
        let info = self.symbol(name)?.clone();
        let handle = self.handle(name).await?;
        let bytes = self.read_index(SYM_VALBYHND, handle, info.size).await?;
        Ok(decode(&info, &bytes))
    }

    pub async fn write_symbol(&mut self, name: &str, value: &Value) -> Result<(), ClientError> {
        let info = self.symbol(name)?.clone();
        let bytes = encode(&info, value)?;
        let handle = self.handle(name).await?;
        self.write_index(SYM_VALBYHND, handle, &bytes).await
    }

    pub async fn wait_for_symbol(
        &mut self,
        name: &str,
        expected: &Value,
        timeout_ms: u64,
    ) -> Result<(Value, bool), ClientError> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let value = self.read_symbol(name).await?;
            if value == *expected {
                return Ok((value, true));
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok((value, false));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    fn symbol(&self, name: &str) -> Result<&SymbolInfo, ClientError> {
        self.symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .ok_or_else(|| ClientError::UnknownSymbol(name.to_string()))
    }

    async fn load_symbols(&mut self) -> Result<Vec<SymbolInfo>, ClientError> {
        let info = self.read_index(SYM_UPLOADINFO2, 0, 64).await?;
        if info.len() < 8 {
            return Err(ClientError::Decode("symbol upload info too short"));
        }
        let upload_size = u32::from_le_bytes(info[4..8].try_into().expect("4 bytes"));
        let blob = self.read_index(SYM_UPLOAD, 0, upload_size).await?;
        parse_symbols(&blob)
    }

    async fn handle(&mut self, name: &str) -> Result<u32, ClientError> {
        if let Some(handle) = self.handles.get(name) {
            return Ok(*handle);
        }

        let mut write = Vec::with_capacity(name.len() + 1);
        write.extend_from_slice(name.as_bytes());
        write.push(0);
        let mut data = Vec::with_capacity(16 + write.len());
        data.extend_from_slice(&SYM_HNDBYNAME.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(&(write.len() as u32).to_le_bytes());
        data.extend_from_slice(&write);

        let response = self.request(Command::ReadWrite, data).await?;
        let (result, payload) = parse_response(&response)?;
        if result != 0 {
            return Err(ClientError::Ads(result));
        }
        if payload.len() < 4 {
            return Err(ClientError::Decode("handle response too short"));
        }
        let handle = u32::from_le_bytes(payload[0..4].try_into().expect("4 bytes"));
        self.handles.insert(name.to_string(), handle);
        Ok(handle)
    }

    async fn read_index(
        &mut self,
        index_group: u32,
        index_offset: u32,
        length: u32,
    ) -> Result<Vec<u8>, ClientError> {
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&index_group.to_le_bytes());
        data.extend_from_slice(&index_offset.to_le_bytes());
        data.extend_from_slice(&length.to_le_bytes());

        let response = self.request(Command::Read, data).await?;
        let (result, payload) = parse_response(&response)?;
        if result != 0 {
            return Err(ClientError::Ads(result));
        }
        Ok(payload.to_vec())
    }

    async fn write_index(
        &mut self,
        index_group: u32,
        index_offset: u32,
        value: &[u8],
    ) -> Result<(), ClientError> {
        let mut data = Vec::with_capacity(12 + value.len());
        data.extend_from_slice(&index_group.to_le_bytes());
        data.extend_from_slice(&index_offset.to_le_bytes());
        data.extend_from_slice(&(value.len() as u32).to_le_bytes());
        data.extend_from_slice(value);

        let response = self.request(Command::Write, data).await?;
        if response.len() < 4 {
            return Err(ClientError::Decode("write response too short"));
        }
        let result = u32::from_le_bytes(response[0..4].try_into().expect("4 bytes"));
        if result != 0 {
            return Err(ClientError::Ads(result));
        }
        Ok(())
    }

    async fn request(&mut self, command: Command, data: Vec<u8>) -> Result<Vec<u8>, ClientError> {
        let invoke_id = self.invoke_id;
        self.invoke_id = self.invoke_id.wrapping_add(1);

        let packet = Packet {
            header: AmsHeader {
                target: self.target,
                source: self.source,
                command,
                flags: FLAG_REQUEST,
                error_code: 0,
                invoke_id,
            },
            data,
        };
        self.stream.write_all(&packet.encode()).await?;

        loop {
            let response = self.receive().await?;
            if response.header.command == Command::Notification {
                continue;
            }
            if response.header.error_code != 0 {
                return Err(ClientError::Ams(response.header.error_code));
            }
            if response.header.invoke_id != invoke_id {
                return Err(ClientError::Decode("unexpected invoke id"));
            }
            return Ok(response.data);
        }
    }

    async fn receive(&mut self) -> Result<Packet, ClientError> {
        let mut head = [0u8; 6];
        self.stream.read_exact(&mut head).await?;
        let len = u32::from_le_bytes(head[2..6].try_into().expect("4 bytes")) as usize;
        if !(AMS_HEADER_LEN..=MAX_FRAME_LEN).contains(&len) {
            return Err(ClientError::Decode("bad AMS frame length"));
        }

        let mut frame = vec![0u8; 6 + len];
        frame[..6].copy_from_slice(&head);
        self.stream.read_exact(&mut frame[6..]).await?;
        Packet::decode(&frame).map_err(|_| ClientError::Decode("bad AMS frame"))
    }
}

fn parse_response(data: &[u8]) -> Result<(u32, &[u8]), ClientError> {
    if data.len() < 8 {
        return Err(ClientError::Decode("ADS response too short"));
    }
    let result = u32::from_le_bytes(data[0..4].try_into().expect("4 bytes"));
    let length = u32::from_le_bytes(data[4..8].try_into().expect("4 bytes")) as usize;
    let payload = data
        .get(8..8 + length)
        .ok_or(ClientError::Decode("ADS response length mismatch"))?;
    Ok((result, payload))
}

fn parse_symbols(blob: &[u8]) -> Result<Vec<SymbolInfo>, ClientError> {
    const HEADER_LEN: usize = 30;
    let mut symbols = Vec::new();
    let mut position = 0;

    while position + HEADER_LEN <= blob.len() {
        let entry_len =
            u32::from_le_bytes(blob[position..position + 4].try_into().expect("4 bytes")) as usize;
        if entry_len < HEADER_LEN || position + entry_len > blob.len() {
            return Err(ClientError::Decode("bad symbol entry length"));
        }

        let size = u32::from_le_bytes(blob[position + 12..position + 16].try_into().expect("4"));
        let data_type =
            u32::from_le_bytes(blob[position + 16..position + 20].try_into().expect("4"));
        let name_len =
            u16::from_le_bytes(blob[position + 24..position + 26].try_into().expect("2")) as usize;
        let type_len =
            u16::from_le_bytes(blob[position + 26..position + 28].try_into().expect("2")) as usize;

        let name_start = position + HEADER_LEN;
        let name_end = name_start + name_len;
        let type_start = name_end + 1;
        let type_end = type_start + type_len;
        if type_end > position + entry_len {
            return Err(ClientError::Decode("symbol entry strings out of range"));
        }

        let name = std::str::from_utf8(&blob[name_start..name_end])
            .map_err(|_| ClientError::Decode("symbol name is not utf-8"))?;
        let type_name = std::str::from_utf8(&blob[type_start..type_end])
            .map_err(|_| ClientError::Decode("symbol type is not utf-8"))?;

        symbols.push(SymbolInfo {
            name: name.to_string(),
            type_name: type_name.to_string(),
            data_type,
            size,
        });
        position += entry_len;
    }

    Ok(symbols)
}

fn decode(info: &SymbolInfo, bytes: &[u8]) -> Value {
    match info.data_type {
        TYPE_BOOL => Value::Bool(bytes.first().is_some_and(|byte| *byte != 0)),
        TYPE_LREAL if bytes.len() >= 8 => {
            json!(f64::from_le_bytes(bytes[0..8].try_into().expect("8 bytes")))
        }
        TYPE_INT32 if bytes.len() >= 4 => {
            json!(i32::from_le_bytes(bytes[0..4].try_into().expect("4 bytes")))
        }
        _ => json!(bytes),
    }
}

fn encode(info: &SymbolInfo, value: &Value) -> Result<Vec<u8>, ClientError> {
    match info.data_type {
        TYPE_BOOL => Ok(vec![u8::from(
            value.as_bool().ok_or(ClientError::UnsupportedValue)?,
        )]),
        TYPE_LREAL => Ok(value
            .as_f64()
            .ok_or(ClientError::UnsupportedValue)?
            .to_le_bytes()
            .to_vec()),
        TYPE_INT32 => Ok(
            (value.as_i64().ok_or(ClientError::UnsupportedValue)? as i32)
                .to_le_bytes()
                .to_vec(),
        ),
        other => Err(ClientError::UnsupportedType(other)),
    }
}
