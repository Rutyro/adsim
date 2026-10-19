//! ADS/TCP server for the adsim gantry.

use std::sync::{Arc, Mutex};

use adsim_symbols::SymbolTable;
use ams::{
    AMS_HEADER_LEN, Command, Packet, read_device_info_response, read_state_response,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub const DEVICE_NAME: &str = "adsim";
pub const VERSION: (u8, u8, u16) = (0, 1, 0);
pub const ADS_STATE_RUN: u16 = 5;
pub const ADSERR_DEVICE_SERVICE_NOT_SUPPORTED: u32 = 0x0701;

const SYM_HNDBYNAME: u32 = 0xF003;
const SYM_VALBYHND: u32 = 0xF005;
const SYM_RELEASEHND: u32 = 0xF006;
const SYM_INFOBYNAMEEX: u32 = 0xF009;
const SYM_UPLOAD: u32 = 0xF00B;
const SYM_UPLOADINFO2: u32 = 0xF00F;

const MAX_FRAME_LEN: usize = 64 * 1024;

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    let symbols = Arc::new(Mutex::new(SymbolTable::gantry()));

    loop {
        let (stream, _) = listener.accept().await?;
        let symbols = Arc::clone(&symbols);
        tokio::spawn(async move {
            let _ = handle(stream, symbols).await;
        });
    }
}

async fn handle(mut stream: TcpStream, symbols: Arc<Mutex<SymbolTable>>) -> std::io::Result<()> {
    loop {
        let Some(frame) = read_frame(&mut stream).await? else {
            return Ok(());
        };
        let Ok(request) = Packet::decode(&frame) else {
            return Ok(());
        };
        trace(&request);

        let response = {
            let mut symbols = symbols.lock().expect("symbol table lock");
            request.response(answer(&request, &mut symbols))
        };
        stream.write_all(&response.encode()).await?;
    }
}

fn answer(request: &Packet, symbols: &mut SymbolTable) -> Vec<u8> {
    match request.header.command {
        Command::ReadState => read_state_response(0, ADS_STATE_RUN, 0),
        Command::ReadDeviceInfo => read_device_info_response(0, VERSION, DEVICE_NAME),
        Command::Read => answer_read(&request.data, symbols),
        Command::Write => answer_write(&request.data, symbols),
        Command::ReadWrite => answer_read_write(&request.data, symbols),
        Command::AddNotification => {
            let mut out = ADSERR_DEVICE_SERVICE_NOT_SUPPORTED.to_le_bytes().to_vec();
            out.extend_from_slice(&0u32.to_le_bytes());
            out
        }
        _ => ADSERR_DEVICE_SERVICE_NOT_SUPPORTED.to_le_bytes().to_vec(),
    }
}

fn answer_read(data: &[u8], symbols: &mut SymbolTable) -> Vec<u8> {
    let Some([index_group, index_offset, length]) = read_header(data) else {
        return result_with_data(ADSERR_DEVICE_SERVICE_NOT_SUPPORTED, &[]);
    };

    let value = match index_group {
        SYM_UPLOADINFO2 => Some(symbols.upload_info2()),
        SYM_UPLOAD => Some(symbols.upload()),
        SYM_VALBYHND => symbols.read_handle(index_offset),
        _ => symbols.read_by_index(index_group, index_offset, length),
    };

    match value {
        Some(value) => result_with_data(0, &value),
        None => result_with_data(ADSERR_DEVICE_SERVICE_NOT_SUPPORTED, &[]),
    }
}

fn answer_write(data: &[u8], symbols: &mut SymbolTable) -> Vec<u8> {
    let Some([index_group, index_offset, _length]) = read_header(data) else {
        return ADSERR_DEVICE_SERVICE_NOT_SUPPORTED.to_le_bytes().to_vec();
    };
    let value = &data[12..];

    let written = match index_group {
        SYM_VALBYHND => symbols.write_handle(index_offset, value).is_some(),
        SYM_RELEASEHND => {
            value.len() >= 4
                && symbols
                    .release_handle(u32::from_le_bytes(value[0..4].try_into().expect("handle")))
        }
        _ => false,
    };

    (if written {
        0
    } else {
        ADSERR_DEVICE_SERVICE_NOT_SUPPORTED
    })
    .to_le_bytes()
    .to_vec()
}

fn answer_read_write(data: &[u8], symbols: &mut SymbolTable) -> Vec<u8> {
    if data.len() < 16 {
        return result_with_data(ADSERR_DEVICE_SERVICE_NOT_SUPPORTED, &[]);
    }

    let index_group = u32::from_le_bytes(data[0..4].try_into().expect("index group"));
    let index_offset = u32::from_le_bytes(data[4..8].try_into().expect("index offset"));
    let read_length = u32::from_le_bytes(data[8..12].try_into().expect("read length")) as usize;
    let write_length = u32::from_le_bytes(data[12..16].try_into().expect("write length")) as usize;
    let write_data = data.get(16..16 + write_length).unwrap_or(&[]);

    match index_group {
        SYM_HNDBYNAME => {
            let name = write_data.split(|byte| *byte == 0).next().unwrap_or(write_data);
            let handle = std::str::from_utf8(name)
                .ok()
                .and_then(|name| symbols.handle(name));
            match handle {
                Some(handle) => result_with_data(0, &handle.to_le_bytes()),
                None => result_with_data(ADSERR_DEVICE_SERVICE_NOT_SUPPORTED, &[]),
            }
        }
        SYM_INFOBYNAMEEX => {
            let name = write_data.split(|byte| *byte == 0).next().unwrap_or(write_data);
            match std::str::from_utf8(name)
                .ok()
                .and_then(|name| symbols.symbol(name))
            {
                Some(symbol) => result_with_data(0, &symbol.entry()),
                None => result_with_data(ADSERR_DEVICE_SERVICE_NOT_SUPPORTED, &[]),
            }
        }
        SYM_VALBYHND => {
            if symbols.write_handle(index_offset, write_data).is_none() {
                return result_with_data(ADSERR_DEVICE_SERVICE_NOT_SUPPORTED, &[]);
            }
            match symbols.read_handle(index_offset) {
                Some(value) => {
                    let length = read_length.min(value.len());
                    result_with_data(0, &value[..length])
                }
                None => result_with_data(ADSERR_DEVICE_SERVICE_NOT_SUPPORTED, &[]),
            }
        }
        _ => result_with_data(ADSERR_DEVICE_SERVICE_NOT_SUPPORTED, &[]),
    }
}

fn read_header(data: &[u8]) -> Option<[u32; 3]> {
    if data.len() < 12 {
        return None;
    }

    Some([
        u32::from_le_bytes(data[0..4].try_into().expect("index group")),
        u32::from_le_bytes(data[4..8].try_into().expect("index offset")),
        u32::from_le_bytes(data[8..12].try_into().expect("length")),
    ])
}

fn result_with_data(result: u32, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + data.len());
    out.extend_from_slice(&result.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    out
}

fn trace(request: &Packet) {
    if std::env::var_os("ADSIM_TRACE").is_none() {
        return;
    }

    let (index_group, index_offset) = match request.data.len() {
        8.. => (
            u32::from_le_bytes(request.data[0..4].try_into().expect("index group")),
            u32::from_le_bytes(request.data[4..8].try_into().expect("index offset")),
        ),
        _ => (0, 0),
    };
    let data: Vec<String> = request
        .data
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect();
    eprintln!(
        "[adsim] {:?} ig=0x{index_group:04X} io=0x{index_offset:08X} data={}",
        request.header.command,
        data.join(" ")
    );
}

async fn read_frame(stream: &mut TcpStream) -> std::io::Result<Option<Vec<u8>>> {
    let mut head = [0u8; 6];
    match stream.read_exact(&mut head).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }

    let len = u32::from_le_bytes(head[2..6].try_into().expect("4-byte length")) as usize;
    if len < AMS_HEADER_LEN || len > MAX_FRAME_LEN {
        return Ok(None);
    }

    let mut frame = vec![0u8; 6 + len];
    frame[..6].copy_from_slice(&head);
    stream.read_exact(&mut frame[6..]).await?;
    Ok(Some(frame))
}
