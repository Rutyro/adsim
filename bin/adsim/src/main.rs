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

const PAGE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>adsim · gantry</title>
<style>
  :root {
    --bg: #0b0d10;
    --panel: #14171c;
    --panel-2: #191d23;
    --border: #262b33;
    --text: #e8eaed;
    --muted: #9098a3;
    --accent: #4ade80;
    --danger: #f87171;
  }
  * { box-sizing: border-box; }
  html, body { margin: 0; }
  body {
    min-height: 100vh;
    background: radial-gradient(1100px 560px at 50% -10%, #161b22 0%, var(--bg) 62%);
    color: var(--text);
    font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
    -webkit-font-smoothing: antialiased;
  }
  .wrap { max-width: 1060px; margin: 0 auto; padding: 2rem 1.25rem 2.5rem; }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 1rem; }
  .brand h1 { margin: 0; font-size: 1.5rem; font-weight: 650; letter-spacing: -.01em; }
  .brand h1 span { color: var(--muted); font-weight: 400; }
  .brand p { margin: .35rem 0 0; color: var(--muted); font-size: .9rem; }
  .live {
    display: inline-flex; align-items: center; gap: .45rem; flex: none;
    font-size: .78rem; font-weight: 500; padding: .35rem .7rem;
    border-radius: 99px; border: 1px solid var(--border); color: var(--muted);
  }
  .live::before { content: ""; width: 7px; height: 7px; border-radius: 50%; background: var(--muted); }
  .live.on { color: #86efac; border-color: #14532d; background: #0f2418; }
  .live.on::before { background: var(--accent); box-shadow: 0 0 8px #4ade8088; }
  .live.off { color: #fecaca; border-color: #7f1d1d; background: #2a1414; }
  .live.off::before { background: var(--danger); }
  .alarm {
    display: none; margin: 1.1rem 0 0; padding: .8rem 1rem;
    border-radius: 10px; border: 1px solid #7f1d1d;
    background: linear-gradient(90deg, #2a1414, #1c1111);
    color: #fecaca; font-weight: 600; font-size: .92rem;
  }
  .alarm.show { display: block; }
  .grid { display: grid; grid-template-columns: 1.05fr .95fr; gap: 1rem; margin-top: 1.1rem; }
  .panel { background: var(--panel); border: 1px solid var(--border); border-radius: 14px; padding: 1rem; }
  .panel h2 {
    margin: 0 0 .75rem; font-size: .75rem; font-weight: 600;
    letter-spacing: .08em; text-transform: uppercase; color: var(--muted);
  }
  .plot { display: block; width: 100%; height: auto; }
  .axes { display: grid; gap: .75rem; }
  .axis { background: var(--panel); border: 1px solid var(--border); border-radius: 14px; padding: .9rem 1rem; }
  .axis-top { display: flex; align-items: baseline; justify-content: space-between; gap: .75rem; }
  .axis-name { display: flex; align-items: center; gap: .55rem; font-weight: 600; }
  .badge {
    display: inline-grid; place-items: center; width: 1.55rem; height: 1.55rem;
    border-radius: 7px; background: var(--panel-2); border: 1px solid var(--border); font-size: .8rem;
  }
  .value {
    font-family: ui-monospace, "Cascadia Mono", "SF Mono", monospace;
    font-size: 1.15rem; font-variant-numeric: tabular-nums;
  }
  .value small { color: var(--muted); font-size: .7rem; margin-left: .2rem; }
  .state { margin-top: .4rem; color: var(--muted); font-size: .78rem; display: flex; gap: .4rem; flex-wrap: wrap; }
  .chip { padding: .1rem .45rem; border-radius: 99px; border: 1px solid var(--border); background: var(--panel-2); }
  .chip.ok { color: #86efac; border-color: #14532d; }
  .chip.bad { color: #fecaca; border-color: #7f1d1d; background: #2a1414; }
  .bar { height: 8px; margin-top: .65rem; border-radius: 99px; background: var(--panel-2); overflow: hidden; }
  .fill {
    height: 100%; width: 0; border-radius: inherit;
    background: linear-gradient(90deg, #22c55e, #4ade80);
    transition: width .12s linear;
  }
  .fill.bad { background: linear-gradient(90deg, #b91c1c, #f87171); }
  footer {
    margin-top: 1.4rem; display: flex; justify-content: space-between; gap: 1rem;
    color: var(--muted); font-size: .78rem;
  }
  footer a { color: var(--muted); text-decoration: none; border-bottom: 1px solid var(--border); }
  footer a:hover { color: var(--text); }
  @media (max-width: 760px) {
    .grid { grid-template-columns: 1fr; }
    footer { flex-direction: column; }
  }
  @media (prefers-reduced-motion: reduce) {
    .fill { transition: none; }
    .live.on::before { box-shadow: none; }
  }
</style>
</head>
<body>
<div class="wrap">
  <header>
    <div class="brand">
      <h1>adsim <span>gantry</span></h1>
      <p>TwinCAT-compatible XYZ simulator over ADS/TCP · 0–300 mm</p>
    </div>
    <span class="live" id="live">connecting</span>
  </header>

  <div class="alarm" id="alarm" role="alert"></div>

  <div class="grid">
    <section class="panel">
      <h2>Workspace · top view</h2>
      <svg class="plot" viewBox="0 0 340 340" role="img" aria-label="Gantry top view with live X and Y position">
        <defs>
          <pattern id="grid" width="50" height="50" patternUnits="userSpaceOnUse" x="20" y="20">
            <path d="M 50 0 L 0 0 0 50" fill="none" stroke="#23272e" stroke-width="1"/>
          </pattern>
        </defs>
        <rect x="20" y="20" width="300" height="300" fill="url(#grid)" stroke="#2e3440" stroke-width="1.5"/>
        <text x="20" y="336" fill="#6b7280" font-size="10">0</text>
        <text x="286" y="336" fill="#6b7280" font-size="10">300 mm</text>
        <text x="6" y="24" fill="#6b7280" font-size="10">Y</text>
        <line id="gx" x1="20" y1="320" x2="20" y2="320" stroke="#4ade8033" stroke-width="1"/>
        <line id="gy" x1="20" y1="320" x2="20" y2="320" stroke="#4ade8033" stroke-width="1"/>
        <circle id="tool" cx="20" cy="320" r="6" fill="#4ade80" stroke="#0b0d10" stroke-width="2"/>
      </svg>
    </section>

    <div class="axes">
      <div class="axis">
        <div class="axis-top">
          <span class="axis-name"><span class="badge">X</span></span>
          <span class="value"><span id="x">0.000</span><small>mm</small></span>
        </div>
        <div class="state">
          <span class="chip" id="cx-enabled">disabled</span>
          <span class="chip" id="cx-homed">not homed</span>
          <span class="chip" id="cx-error">ok</span>
        </div>
        <div class="bar"><div class="fill" id="bx"></div></div>
      </div>
      <div class="axis">
        <div class="axis-top">
          <span class="axis-name"><span class="badge">Y</span></span>
          <span class="value"><span id="y">0.000</span><small>mm</small></span>
        </div>
        <div class="state">
          <span class="chip" id="cy-enabled">disabled</span>
          <span class="chip" id="cy-homed">not homed</span>
          <span class="chip" id="cy-error">ok</span>
        </div>
        <div class="bar"><div class="fill" id="by"></div></div>
      </div>
      <div class="axis">
        <div class="axis-top">
          <span class="axis-name"><span class="badge">Z</span></span>
          <span class="value"><span id="z">0.000</span><small>mm</small></span>
        </div>
        <div class="state">
          <span class="chip" id="cz-enabled">disabled</span>
          <span class="chip" id="cz-homed">not homed</span>
          <span class="chip" id="cz-error">ok</span>
        </div>
        <div class="bar"><div class="fill" id="bz"></div></div>
      </div>
    </div>
  </div>

  <footer>
    <span>ADS/TCP 48898 · trapezoid motion · software limits</span>
    <a href="https://github.com/Rutyro/adsim">source</a>
  </footer>
</div>
<script>
const source = new EventSource('/events');
const live = document.getElementById('live');
source.onopen = () => { live.textContent = 'live'; live.className = 'live on'; };
source.onerror = () => { live.textContent = 'disconnected'; live.className = 'live off'; };

function setChip(id, text, on) {
  const chip = document.getElementById(id);
  chip.textContent = text;
  chip.className = 'chip' + (on ? ' ok' : '');
}

source.onmessage = (event) => {
  const p = JSON.parse(event.data);
  for (const axis of ['x', 'y', 'z']) {
    document.getElementById(axis).textContent = p[axis].toFixed(3);
    const enabled = p[axis + 'e'];
    const homed = p[axis + 'h'];
    const error = p[axis + 'err'];
    const fill = document.getElementById('b' + axis);
    fill.style.width = Math.min(100, Math.max(0, p[axis] / 300 * 100)) + '%';
    fill.classList.toggle('bad', !!error);
    setChip('c' + axis + '-enabled', enabled ? 'enabled' : 'disabled', enabled);
    setChip('c' + axis + '-homed', homed ? 'homed' : 'not homed', homed);
    const errorChip = document.getElementById('c' + axis + '-error');
    errorChip.textContent = error ? 'error' : 'ok';
    errorChip.className = 'chip' + (error ? ' bad' : '');
  }
  const cx = 20 + Math.min(300, Math.max(0, p.x));
  const cy = 320 - Math.min(300, Math.max(0, p.y));
  document.getElementById('tool').setAttribute('cx', cx);
  document.getElementById('tool').setAttribute('cy', cy);
  const gx = document.getElementById('gx');
  gx.setAttribute('x2', cx);
  gx.setAttribute('y1', cy);
  gx.setAttribute('y2', cy);
  const gy = document.getElementById('gy');
  gy.setAttribute('x1', cx);
  gy.setAttribute('x2', cx);
  gy.setAttribute('y2', cy);
  const alarm = document.getElementById('alarm');
  const axis = {1001: 'X', 1002: 'Y', 1003: 'Z'}[p.alarm];
  alarm.textContent = axis ? 'ALARM ' + p.alarm + ' · axis ' + axis + ' soft limit' : '';
  alarm.className = 'alarm' + (axis ? ' show' : '');
};
</script>
</body>
</html>
"##;
