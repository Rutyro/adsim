//! ADS/TCP server for the adsim gantry.

use ams::{
    AMS_HEADER_LEN, Command, Packet, read_device_info_response, read_state_response,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub const DEVICE_NAME: &str = "adsim";
pub const VERSION: (u8, u8, u16) = (0, 1, 0);
pub const ADS_STATE_RUN: u16 = 5;
pub const ADSERR_DEVICE_SERVICE_NOT_SUPPORTED: u32 = 0x0701;

const MAX_FRAME_LEN: usize = 64 * 1024;

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let _ = handle(stream).await;
        });
    }
}

async fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    loop {
        let Some(frame) = read_frame(&mut stream).await? else {
            return Ok(());
        };
        let Ok(request) = Packet::decode(&frame) else {
            return Ok(());
        };
        let response = request.response(answer(&request));
        stream.write_all(&response.encode()).await?;
    }
}

fn answer(request: &Packet) -> Vec<u8> {
    match request.header.command {
        Command::ReadState => read_state_response(0, ADS_STATE_RUN, 0),
        Command::ReadDeviceInfo => read_device_info_response(0, VERSION, DEVICE_NAME),
        _ => ADSERR_DEVICE_SERVICE_NOT_SUPPORTED.to_le_bytes().to_vec(),
    }
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
