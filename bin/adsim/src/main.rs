use std::sync::{Arc, Mutex};
use std::time::Duration;

use adsim_symbols::{SymbolTable, parse_tmc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut positional = Vec::new();
    let mut tmc_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tmc" => match args.next() {
                Some(path) => tmc_path = Some(path),
                None => {
                    eprintln!("--tmc needs a path");
                    std::process::exit(2);
                }
            },
            other => positional.push(other.to_string()),
        }
    }
    let ads_addr = positional
        .first()
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:48898".to_string());
    let view_addr = positional
        .get(1)
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let mut table = SymbolTable::gantry();
    if let Some(path) = tmc_path {
        let xml = std::fs::read_to_string(&path)?;
        let imported = parse_tmc(&xml)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        let count = imported.len();
        table.add_imported(&imported);
        println!("loaded {count} symbols from {path}");
    }
    let symbols = Arc::new(Mutex::new(table));

    let ads_listener = TcpListener::bind(&ads_addr).await?;
    println!("adsim ADS on {}", ads_listener.local_addr()?);
    let ads_symbols = Arc::clone(&symbols);
    tokio::spawn(async move {
        let _ = adsim_server::serve_with_symbols(ads_listener, ads_symbols).await;
    });

    let view_listener = TcpListener::bind(&view_addr).await?;
    println!("adsim view on http://{}/", view_listener.local_addr()?);
    loop {
        let (stream, _) = view_listener.accept().await?;
        let symbols = Arc::clone(&symbols);
        tokio::spawn(async move {
            let _ = handle_http(stream, symbols).await;
        });
    }
}

async fn handle_http(
    mut stream: TcpStream,
    symbols: Arc<Mutex<SymbolTable>>,
) -> std::io::Result<()> {
    let mut buffer = [0u8; 1024];
    let read = stream.read(&mut buffer).await?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    match path {
        "/" => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                PAGE.len(),
                PAGE
            );
            stream.write_all(response.as_bytes()).await?;
        }
        "/events" => {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n",
                )
                .await?;
            loop {
                if stream
                    .write_all(snapshot(&symbols).as_bytes())
                    .await
                    .is_err()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        _ => {
            stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await?;
        }
    }

    Ok(())
}

fn snapshot(symbols: &Mutex<SymbolTable>) -> String {
    let symbols = symbols.lock().expect("symbol table lock");
    let x = symbols.read_f64("MAIN.stAxisX.fActPos").unwrap_or(0.0);
    let y = symbols.read_f64("MAIN.stAxisY.fActPos").unwrap_or(0.0);
    let z = symbols.read_f64("MAIN.stAxisZ.fActPos").unwrap_or(0.0);
    let alarm = symbols.read_i32("MAIN.nAlarmCode").unwrap_or(0);

    let mut state = String::new();
    for (key, name) in [("x", "X"), ("y", "Y"), ("z", "Z")] {
        let enabled = symbols
            .read_bool(&format!("MAIN.stAxis{name}.bEnabled"))
            .unwrap_or(false);
        let homed = symbols
            .read_bool(&format!("MAIN.stAxis{name}.bHomed"))
            .unwrap_or(false);
        let error = symbols
            .read_i32(&format!("MAIN.stAxis{name}.nError"))
            .unwrap_or(0);
        state.push_str(&format!(
            ",\"{key}e\":{enabled},\"{key}h\":{homed},\"{key}err\":{error}"
        ));
    }

    format!("data: {{\"x\":{x:.3},\"y\":{y:.3},\"z\":{z:.3},\"alarm\":{alarm}{state}}}\n\n")
}

const PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>adsim gantry</title>
<style>
  body { font-family: system-ui, sans-serif; background: #111; color: #eee; margin: 2rem; }
  h1 { font-size: 1.4rem; font-weight: 600; }
  .axis { margin: 1.1rem 0; }
  .label { display: flex; justify-content: space-between; font-variant-numeric: tabular-nums; }
  .bar { height: 18px; background: #333; border-radius: 9px; overflow: hidden; margin-top: 6px; }
  .fill { height: 100%; width: 0; background: #4ade80; transition: width .1s linear; }
  .note { color: #888; font-size: .9rem; }
  .alarm { display: none; background: #7f1d1d; color: #fecaca; padding: .6rem 1rem; border-radius: 6px; margin: 1rem 0; font-weight: 600; }
  .state { color: #888; font-size: .85rem; }
  .live { float: right; font-size: .75rem; font-weight: 500; padding: .2rem .6rem; border-radius: 99px; background: #333; color: #aaa; }
  .live.on { background: #14532d; color: #86efac; }
  .live.off { background: #7f1d1d; color: #fecaca; }
</style>
</head>
<body>
<h1>adsim gantry <span class="live" id="live">connecting</span></h1>
<p class="note">Live ADS simulation. Positions in mm, range 0-300.</p>
<div class="alarm" id="alarm"></div>
<div class="axis"><div class="label"><span>X <span class="state" id="sx"></span></span><span id="x">0.000</span></div><div class="bar"><div class="fill" id="bx"></div></div></div>
<div class="axis"><div class="label"><span>Y <span class="state" id="sy"></span></span><span id="y">0.000</span></div><div class="bar"><div class="fill" id="by"></div></div></div>
<div class="axis"><div class="label"><span>Z <span class="state" id="sz"></span></span><span id="z">0.000</span></div><div class="bar"><div class="fill" id="bz"></div></div></div>
<script>
const source = new EventSource('/events');
const live = document.getElementById('live');
source.onopen = () => { live.textContent = 'live'; live.className = 'live on'; };
source.onerror = () => { live.textContent = 'disconnected'; live.className = 'live off'; };
source.onmessage = (event) => {
  const p = JSON.parse(event.data);
  for (const axis of ['x', 'y', 'z']) {
    document.getElementById(axis).textContent = p[axis].toFixed(3);
    document.getElementById('b' + axis).style.width = Math.min(100, p[axis] / 300 * 100) + '%';
    const parts = [p[axis + 'e'] ? 'enabled' : 'disabled'];
    if (p[axis + 'h']) parts.push('homed');
    if (p[axis + 'err']) parts.push('error');
    document.getElementById('s' + axis).textContent = parts.join(' · ');
  }
  const alarm = document.getElementById('alarm');
  const axis = {1001: 'X', 1002: 'Y', 1003: 'Z'}[p.alarm];
  alarm.textContent = axis ? 'ALARM ' + p.alarm + ' · axis ' + axis + ' soft limit' : '';
  alarm.style.display = p.alarm ? 'block' : 'none';
};
</script>
</body>
</html>
"#;
