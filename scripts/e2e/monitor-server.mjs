import http from 'http';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const STATE_DIR = path.join(__dirname, '.state');
const MANIFESTS_DIR = path.join(__dirname, 'manifests');
const STATE_FILE = path.join(STATE_DIR, 'state.json');
const PAUSE_FLAG = path.join(STATE_DIR, 'pause.flag');
const STOP_FLAG = path.join(STATE_DIR, 'stop.flag');
const PORT = Number(process.env.AXIOM_E2E_PORT || 4789);

fs.mkdirSync(STATE_DIR, { recursive: true });
fs.mkdirSync(MANIFESTS_DIR, { recursive: true });

function defaultState() {
  return {
    track: 'Web',
    iteration: 0,
    totalIterations: 5,
    projectId: null,
    projectName: 'Čeka pokretanje...',
    manifestId: null,
    phase: 'idle',
    status: 'idle',
    progress: 0,
    currentFile: '',
    fileIndex: 0,
    totalFiles: 0,
    message: 'Sistem testiranja je spreman.',
    cooldownSecondsRemaining: 0,
    startedAt: null,
    updatedAt: new Date().toISOString(),
    logs: [],
    history: [],
  };
}

function readState() {
  try {
    if (fs.existsSync(STATE_FILE)) {
      return { ...defaultState(), ...JSON.parse(fs.readFileSync(STATE_FILE, 'utf8')) };
    }
  } catch {}
  return defaultState();
}

function writeState(patch) {
  const cur = readState();
  const next = { ...cur, ...patch, updatedAt: new Date().toISOString() };
  fs.writeFileSync(STATE_FILE, JSON.stringify(next, null, 2));
  return next;
}

if (!fs.existsSync(STATE_FILE)) {
  writeState(defaultState());
}

const DASHBOARD_HTML = `<!doctype html>
<html lang="sr">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width,initial-scale=1" />
  <title>Axiom Forge — E2E Test Monitor</title>
  <style>
    :root { --bg:#0b1020; --card:#131b31; --border:#253256; --text:#e6edf8; --muted:#93a4c7; --accent:#6366f1; --ok:#10b981; --warn:#f59e0b; --err:#ef4444; }
    * { box-sizing:border-box; }
    body { margin:0; font-family:Inter,Segoe UI,system-ui,sans-serif; background:var(--bg); color:var(--text); padding:20px; }
    .wrap { max-width:1180px; margin:0 auto; display:grid; gap:16px; }
    .top { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:12px; background:var(--card); border:1px solid var(--border); border-radius:14px; padding:16px 20px; }
    h1 { margin:0; font-size:20px; letter-spacing:-0.02em; }
    .sub { color:var(--muted); font-size:13px; margin-top:4px; }
    .controls { display:flex; gap:10px; align-items:center; flex-wrap:wrap; }
    button { border:0; border-radius:10px; padding:10px 16px; font-weight:600; font-size:13px; cursor:pointer; transition:opacity .15s, transform .1s; }
    button:hover { opacity:.92; }
    button:active { transform:scale(.98); }
    .btn-pause { background:var(--warn); color:#111827; }
    .btn-resume { background:var(--ok); color:#052e16; }
    .btn-stop { background:#334155; color:#f8fafc; }
    .btn-stop.active { background:var(--err); color:#fff; }
    .badge { display:inline-flex; align-items:center; gap:6px; padding:5px 12px; border-radius:999px; font-size:12px; font-weight:700; text-transform:uppercase; }
    .b-generating { background:rgba(99,102,241,.2); color:#a5b4fc; border:1px solid rgba(99,102,241,.4); }
    .b-paused { background:rgba(245,158,11,.2); color:#fcd34d; border:1px solid rgba(245,158,11,.4); }
    .b-cooldown { background:rgba(56,189,248,.18); color:#7dd3fc; border:1px solid rgba(56,189,248,.4); }
    .b-verifying, .b-analyzing { background:rgba(168,85,247,.2); color:#d8b4fe; border:1px solid rgba(168,85,247,.4); }
    .b-completed, .b-idle { background:rgba(16,185,129,.18); color:#6ee7b7; border:1px solid rgba(16,185,129,.4); }
    .b-error { background:rgba(239,68,68,.2); color:#fca5a5; border:1px solid rgba(239,68,68,.4); }
    .grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(210px,1fr)); gap:12px; }
    .card { background:var(--card); border:1px solid var(--border); border-radius:14px; padding:16px; }
    .label { font-size:11px; text-transform:uppercase; color:var(--muted); letter-spacing:.06em; }
    .val { font-size:20px; font-weight:700; margin-top:6px; word-break:break-word; }
    .val.sm { font-size:14px; font-weight:600; }
    .bar-wrap { height:12px; background:#090d19; border-radius:999px; overflow:hidden; border:1px solid var(--border); margin-top:10px; }
    .bar { height:100%; background:linear-gradient(90deg,#6366f1,#22d3ee); transition:width .35s ease; }
    .cooldown-banner { display:none; background:rgba(56,189,248,.12); border:1px solid rgba(56,189,248,.4); border-radius:12px; padding:14px 18px; align-items:center; justify-content:space-between; }
    .cooldown-banner.show { display:flex; }
    table { width:100%; border-collapse:collapse; font-size:13px; margin-top:10px; }
    th, td { text-align:left; padding:10px 10px; border-bottom:1px solid var(--border); vertical-align:top; }
    th { color:var(--muted); font-size:11px; text-transform:uppercase; letter-spacing:.05em; }
    .ok { color:var(--ok); font-weight:700; }
    .err { color:var(--err); font-weight:700; }
    .warn { color:var(--warn); font-weight:700; }
    pre { margin:10px 0 0; background:#070b15; border:1px solid var(--border); border-radius:10px; padding:12px; height:250px; overflow:auto; font-size:12px; line-height:1.45; color:#cbd5e1; white-space:pre-wrap; }
  </style>
</head>
<body>
  <div class="wrap">
    <div class="top">
      <div>
        <h1>Axiom Forge — E2E Automated Test Monitor</h1>
        <div class="sub">Najsvežiji lokalni Rust kod + Ollama (qwen2.5-coder:7b) • Projekti u redovnoj aplikaciji • Termalna pauza 2 min</div>
      </div>
      <div class="controls">
        <span id="statusBadge" class="badge b-idle">IDLE</span>
        <button id="pauseBtn" class="btn-pause" onclick="togglePause()">⏸ Pauziraj odmah</button>
        <button id="stopBtn" class="btn-stop" onclick="toggleStop()">⏹ Zaustavi posle iteracije</button>
      </div>
    </div>

    <div id="cooldownBanner" class="cooldown-banner">
      <div>
        <strong>❄ Termalna pauza za hlađenje CPU/GPU komponenti</strong>
        <div class="sub">Ollama miruje da se temperatura procesora spusti pre sledećeg generisanja.</div>
      </div>
      <div id="cooldownTimer" style="font-size:26px;font-weight:800;color:#7dd3fc;">02:00</div>
    </div>

    <div class="grid">
      <div class="card">
        <div class="label">Oblast &amp; Iteracija</div>
        <div class="val" id="iterVal">Web • 0 / 5</div>
      </div>
      <div class="card">
        <div class="label">Aktivni projekat</div>
        <div class="val sm" id="projVal">—</div>
        <div class="sub" id="projIdVal"></div>
      </div>
      <div class="card">
        <div class="label">Faza &amp; Fajl</div>
        <div class="val sm" id="phaseVal">idle</div>
        <div class="sub" id="fileVal">—</div>
      </div>
      <div class="card">
        <div class="label">Proteklo vreme</div>
        <div class="val" id="elapsedVal">00:00</div>
      </div>
    </div>

    <div class="card">
      <div style="display:flex;justify-content:space-between;align-items:center;">
        <div class="label">Ukupan progres aktivne iteracije</div>
        <strong id="pctVal">0%</strong>
      </div>
      <div class="bar-wrap"><div id="progressBar" class="bar" style="width:0%"></div></div>
      <div class="sub" id="msgVal" style="margin-top:8px;">—</div>
    </div>

    <div class="card">
      <div class="label">Rezultati iteracija (objektivna verifikacija)</div>
      <table>
        <thead>
          <tr>
            <th>#</th>
            <th>Projekat</th>
            <th>TS greške</th>
            <th>next build</th>
            <th>Runtime rute</th>
            <th>Trajanje</th>
            <th>Napomena / Sistemski fix</th>
          </tr>
        </thead>
        <tbody id="histBody">
          <tr><td colspan="7" style="color:var(--muted)">Još nema završenih iteracija.</td></tr>
        </tbody>
      </table>
    </div>

    <div class="card">
      <div class="label">Live log</div>
      <pre id="logBox">Učitavanje...</pre>
    </div>
  </div>

  <script>
    let isPaused = false;
    let isStopPending = false;

    function fmtTime(sec) {
      sec = Math.max(0, Math.floor(sec || 0));
      const m = String(Math.floor(sec / 60)).padStart(2, '0');
      const s = String(sec % 60).padStart(2, '0');
      return m + ':' + s;
    }

    async function togglePause() {
      await fetch(isPaused ? '/api/resume' : '/api/pause', { method: 'POST' });
      poll();
    }

    async function toggleStop() {
      await fetch(isStopPending ? '/api/clear-stop' : '/api/stop-after', { method: 'POST' });
      poll();
    }

    async function poll() {
      try {
        const res = await fetch('/api/state');
        const data = await res.json();
        const st = data.state;
        isPaused = !!data.paused;
        isStopPending = !!data.stopAfterCurrent;

        const badge = document.getElementById('statusBadge');
        const effStatus = isPaused ? 'paused' : (st.cooldownSecondsRemaining > 0 ? 'cooldown' : (st.status || 'idle'));
        badge.className = 'badge b-' + effStatus;
        badge.textContent = isPaused ? 'PAUZIRANO' : (st.cooldownSecondsRemaining > 0 ? 'HLAĐENJE' : effStatus.toUpperCase());

        const pBtn = document.getElementById('pauseBtn');
        pBtn.className = isPaused ? 'btn-resume' : 'btn-pause';
        pBtn.textContent = isPaused ? '▶ Nastavi testiranje' : '⏸ Pauziraj odmah';

        const sBtn = document.getElementById('stopBtn');
        sBtn.className = isStopPending ? 'btn-stop active' : 'btn-stop';
        sBtn.textContent = isStopPending ? '⏹ Zaustavljanje zakazano (poništi)' : '⏹ Zaustavi posle iteracije';

        const cdBanner = document.getElementById('cooldownBanner');
        if (st.cooldownSecondsRemaining > 0) {
          cdBanner.classList.add('show');
          document.getElementById('cooldownTimer').textContent = fmtTime(st.cooldownSecondsRemaining);
        } else {
          cdBanner.classList.remove('show');
        }

        document.getElementById('iterVal').textContent = (st.track || 'Web') + ' • ' + (st.iteration || 0) + ' / ' + (st.totalIterations || 5);
        document.getElementById('projVal').textContent = st.projectName || '—';
        document.getElementById('projIdVal').textContent = st.projectId || '';
        document.getElementById('phaseVal').textContent = st.phase || 'idle';
        document.getElementById('fileVal').textContent = st.currentFile
          ? (st.fileIndex && st.totalFiles ? '[' + st.fileIndex + '/' + st.totalFiles + '] ' : '') + st.currentFile
          : '—';

        if (st.startedAt && !['idle','completed'].includes(st.status)) {
          const elapsed = (Date.now() - new Date(st.startedAt).getTime()) / 1000;
          document.getElementById('elapsedVal').textContent = fmtTime(elapsed);
        } else if (st.lastDurationSec) {
          document.getElementById('elapsedVal').textContent = fmtTime(st.lastDurationSec);
        }

        const pct = Math.min(100, Math.max(0, Math.round(st.progress || 0)));
        document.getElementById('pctVal').textContent = pct + '%';
        document.getElementById('progressBar').style.width = pct + '%';
        document.getElementById('msgVal').textContent = st.message || '—';

        const tbody = document.getElementById('histBody');
        const hist = st.history || [];
        if (!hist.length) {
          tbody.innerHTML = '<tr><td colspan="7" style="color:var(--muted)">Još nema završenih iteracija.</td></tr>';
        } else {
          tbody.innerHTML = hist.map(h => \`
            <tr>
              <td><strong>#\${h.iteration}</strong></td>
              <td><strong>\${h.projectName || h.manifestId}</strong><div class="sub">\${h.projectId || ''}</div></td>
              <td class="\${h.tscErrors === 0 ? 'ok' : 'err'}">\${h.tscErrors}</td>
              <td class="\${h.nextBuildOk ? 'ok' : 'err'}">\${h.nextBuildOk ? '✓ PROŠAO' : '✗ PAO'}</td>
              <td class="\${(h.routesFailed || 0) === 0 ? 'ok' : 'warn'}">\${h.routesPassed || 0} OK / \${h.routesFailed || 0} FAIL</td>
              <td>\${fmtTime(h.durationSec)}</td>
              <td style="font-size:12px;color:var(--muted)">\${h.note || '—'}</td>
            </tr>
          \`).join('');
        }

        const logBox = document.getElementById('logBox');
        const atBottom = logBox.scrollTop + logBox.clientHeight >= logBox.scrollHeight - 25;
        logBox.textContent = (st.logs || []).join('\\n') || 'Nema logova još.';
        if (atBottom) logBox.scrollTop = logBox.scrollHeight;
      } catch (e) {
        console.error(e);
      }
    }

    poll();
    setInterval(poll, 1500);
  </script>
</body>
</html>`;

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://127.0.0.1:${PORT}`);

  // 1. Live Monitor UI
  if (req.method === 'GET' && url.pathname === '/') {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    res.end(DASHBOARD_HTML);
    return;
  }

  // 2. Monitor API: Get state
  if (req.method === 'GET' && url.pathname === '/api/state') {
    res.writeHead(200, { 'Content-Type': 'application/json; charset=utf-8' });
    res.end(JSON.stringify({
      state: readState(),
      paused: fs.existsSync(PAUSE_FLAG),
      stopAfterCurrent: fs.existsSync(STOP_FLAG),
    }));
    return;
  }

  // 3. Monitor API: Pause / Resume / Stop-after
  if (req.method === 'POST' && url.pathname === '/api/pause') {
    fs.writeFileSync(PAUSE_FLAG, new Date().toISOString());
    const st = readState();
    const logs = [...(st.logs || []).slice(-140), `[${new Date().toLocaleTimeString()}] ⏸ PAUZIRANO od strane korisnika (čekanje hlađenja / nastavka)`];
    writeState({ logs });
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ ok: true, paused: true }));
    return;
  }
  if (req.method === 'POST' && url.pathname === '/api/resume') {
    if (fs.existsSync(PAUSE_FLAG)) fs.unlinkSync(PAUSE_FLAG);
    const st = readState();
    const logs = [...(st.logs || []).slice(-140), `[${new Date().toLocaleTimeString()}] ▶ NASTAVLJENO od strane korisnika`];
    writeState({ logs });
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ ok: true, paused: false }));
    return;
  }
  if (req.method === 'POST' && url.pathname === '/api/stop-after') {
    fs.writeFileSync(STOP_FLAG, new Date().toISOString());
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ ok: true, stopAfterCurrent: true }));
    return;
  }
  if (req.method === 'POST' && url.pathname === '/api/clear-stop') {
    if (fs.existsSync(STOP_FLAG)) fs.unlinkSync(STOP_FLAG);
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ ok: true, stopAfterCurrent: false }));
    return;
  }

  // 4. Mock Web-App Manifest Endpoint (mimics https://idea-analyzer-ten.vercel.app/api/axiom/manifest/:id)
  const manifestMatch = url.pathname.match(/^\/api\/axiom\/manifest\/([A-Za-z0-9_-]+)$/);
  if (req.method === 'GET' && manifestMatch) {
    const id = manifestMatch[1];
    const mPath = path.join(MANIFESTS_DIR, `${id}.json`);
    if (fs.existsSync(mPath)) {
      const manifestData = JSON.parse(fs.readFileSync(mPath, 'utf8'));
      res.writeHead(200, { 'Content-Type': 'application/json; charset=utf-8' });
      res.end(JSON.stringify({ data: manifestData }));
    } else {
      res.writeHead(404, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: `Manifest '${id}' not found` }));
    }
    return;
  }

  res.writeHead(404, { 'Content-Type': 'text/plain' });
  res.end('Not found');
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`[E2E Monitor] Live Dashboard & Mock Manifest Server running at http://127.0.0.1:${PORT}`);
});
