import { spawn, execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import net from 'net';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '..', '..');
const STATE_DIR = path.join(__dirname, '.state');
const MANIFESTS_DIR = path.join(__dirname, 'manifests');
const STATE_FILE = path.join(STATE_DIR, 'state.json');
const PAUSE_FLAG = path.join(STATE_DIR, 'pause.flag');
const MONITOR_PORT = Number(process.env.AXIOM_E2E_PORT || 4789);
const PROJECTS_ROOT = path.join(process.env.LOCALAPPDATA || '', 'axiom-forge', 'projects');

fs.mkdirSync(STATE_DIR, { recursive: true });

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    manifest: 'web_1',
    iteration: 1,
    total: 5,
    track: 'Web',
    model: 'qwen2.5-coder:7b',
    cooldown: 0,
    skipBuild: false,
  };
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--manifest') out.manifest = args[++i];
    else if (args[i] === '--iteration') out.iteration = Number(args[++i]);
    else if (args[i] === '--total') out.total = Number(args[++i]);
    else if (args[i] === '--track') out.track = args[++i];
    else if (args[i] === '--model') out.model = args[++i];
    else if (args[i] === '--cooldown') out.cooldown = Number(args[++i]);
    else if (args[i] === '--skip-build') out.skipBuild = true;
  }
  return out;
}

function readState() {
  try {
    if (fs.existsSync(STATE_FILE)) return JSON.parse(fs.readFileSync(STATE_FILE, 'utf8'));
  } catch {}
  return { logs: [], history: [] };
}

function updateState(patch, logLine) {
  const cur = readState();
  const logs = Array.isArray(cur.logs) ? cur.logs.slice(-140) : [];
  if (logLine) {
    const ts = new Date().toLocaleTimeString('sr-RS', { hour12: false });
    logs.push(`[${ts}] ${logLine}`);
  }
  const next = { ...cur, ...patch, logs, updatedAt: new Date().toISOString() };
  fs.writeFileSync(STATE_FILE, JSON.stringify(next, null, 2));
  return next;
}

function sleep(ms) {
  return new Promise(r => setTimeout(r, ms));
}

async function waitWhilePaused(contextLabel) {
  let announced = false;
  while (fs.existsSync(PAUSE_FLAG)) {
    if (!announced) {
      updateState({ status: 'paused', message: `Pauzirano (${contextLabel}) — klikni Nastavi u Monitor UI.` }, `⏸ Pauzirano (${contextLabel})`);
      announced = true;
    }
    await sleep(1000);
  }
  if (announced) {
    updateState({ status: 'generating', message: `Nastavljeno (${contextLabel})...` }, `▶ Nastavljeno (${contextLabel})`);
  }
}

async function runCooldown(seconds) {
  if (!seconds || seconds <= 0) return;
  updateState(
    { status: 'cooldown', phase: 'cooldown', cooldownSecondsRemaining: seconds, message: `Termalno hlađenje CPU/GPU (${seconds}s)...` },
    `❄ Pokrenuta termalna pauza od ${seconds}s pre generisanja`
  );
  for (let rem = seconds; rem > 0; rem--) {
    await waitWhilePaused('cooldown');
    updateState({ status: 'cooldown', phase: 'cooldown', cooldownSecondsRemaining: rem, message: `Termalno hlađenje hardvera — preostalo ${rem}s...` });
    await sleep(1000);
  }
  updateState({ cooldownSecondsRemaining: 0 }, '✅ Termalno hlađenje završeno, krećemo sa sledećom iteracijom.');
}

function getFreePort(startPort = 3400) {
  return new Promise((resolve) => {
    const srv = net.createServer();
    srv.listen(startPort, '127.0.0.1', () => {
      const p = srv.address().port;
      srv.close(() => resolve(p));
    });
    srv.on('error', () => resolve(getFreePort(startPort + 1)));
  });
}

function killTree(pid) {
  if (!pid) return;
  try {
    if (process.platform === 'win32') {
      execSync(`taskkill /F /T /PID ${pid}`, { stdio: 'ignore' });
    } else {
      process.kill(-pid, 'SIGKILL');
    }
  } catch {}
}

async function probeRoute(baseUrl, routePath, method = 'GET', bodyObj = null) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 15000);
  try {
    let url = baseUrl + routePath;
    let redirects = 0;
    let finalRes = null;
    const redirectChain = [];
    while (redirects < 6) {
      const opts = {
        method,
        redirect: 'manual',
        signal: controller.signal,
        headers: bodyObj ? { 'Content-Type': 'application/json' } : {},
        body: bodyObj ? JSON.stringify(bodyObj) : undefined,
      };
      const res = await fetch(url, opts);
      if (res.status >= 300 && res.status < 400 && res.headers.get('location')) {
        const loc = res.headers.get('location');
        redirectChain.push(`${res.status}->${loc}`);
        url = loc.startsWith('http') ? loc : new URL(loc, url).toString();
        redirects++;
        if (redirects >= 5) {
          return { route: routePath, method, ok: false, status: res.status, issue: `INFINITE_REDIRECT_LOOP (${redirectChain.join(', ')})` };
        }
        continue;
      }
      finalRes = res;
      break;
    }
    const text = await finalRes.text();
    const low = text.toLowerCase();
    const hasNextError =
      text.includes('__next_error__') ||
      text.includes('Unhandled Runtime Error') ||
      text.includes('Application error: a client-side exception') ||
      (low.includes('syntax error') && low.includes('globals.css')) ||
      text.includes('Module not found: Can\'t resolve');
    const hasInlineLoginLoop =
      routePath === '/' &&
      text.includes("window.location.href = '/login'") &&
      text.includes("const status = 'unauthenticated'");
    const ok = finalRes.status < 500 && !hasNextError && !hasInlineLoginLoop;
    let issue = null;
    if (finalRes.status >= 500) issue = `HTTP_${finalRes.status}`;
    else if (hasInlineLoginLoop) issue = 'ROOT_LAYOUT_LOGIN_REDIRECT_LOOP';
    else if (hasNextError) {
      const m = text.match(/(?:TypeError|ReferenceError|SyntaxError|Module not found)[^<\n]{0,140}/);
      issue = m ? m[0] : 'NEXT_RUNTIME_OR_COMPILE_ERROR';
    }
    return {
      route: routePath,
      method,
      ok,
      status: finalRes.status,
      redirects: redirectChain,
      issue,
    };
  } catch (err) {
    return { route: routePath, method, ok: false, status: 0, issue: String(err.message || err) };
  } finally {
    clearTimeout(timer);
  }
}

async function verifyProject(projDir, projectId) {
  const result = {
    projectId,
    projDir,
    tscErrors: 0,
    tscOutput: '',
    buildIdBeforeVerify: fs.existsSync(path.join(projDir, '.next', 'BUILD_ID')),
    nextBuildOk: false,
    nextBuildError: '',
    generationWarnings: [],
    routes: [],
    routesPassed: 0,
    routesFailed: 0,
  };

  // 1. Read generation_state warnings
  const genStatePath = path.join(projDir, '.axiom', 'generation_state.json');
  if (fs.existsSync(genStatePath)) {
    try {
      const gs = JSON.parse(fs.readFileSync(genStatePath, 'utf8'));
      result.generationWarnings = gs.warnings || [];
    } catch {}
  }

  // 2. Run npx tsc --noEmit (read-only)
  updateState({ phase: 'verifying', status: 'verifying', progress: 99, message: 'Verifikacija: npx tsc --noEmit...' }, '🔍 Pokrećem tsc --noEmit...');
  try {
    execSync('npx tsc --noEmit', { cwd: projDir, encoding: 'utf8', stdio: 'pipe', timeout: 120000 });
    result.tscErrors = 0;
  } catch (e) {
    const out = ((e.stdout || '') + '\n' + (e.stderr || '')).trim();
    result.tscOutput = out.slice(0, 6000);
    const matches = out.match(/error TS\d+:/g) || [];
    result.tscErrors = matches.length || 1;
  }

  // 3. Check next build
  if (result.buildIdBeforeVerify) {
    result.nextBuildOk = true;
  } else {
    updateState({ message: 'Verifikacija: provera next build...' }, '🔍 BUILD_ID ne postoji, proveravam uzrok u next build...');
    try {
      execSync('npx next build', { cwd: projDir, encoding: 'utf8', stdio: 'pipe', timeout: 180000 });
      result.nextBuildOk = fs.existsSync(path.join(projDir, '.next', 'BUILD_ID'));
    } catch (e) {
      const out = ((e.stdout || '') + '\n' + (e.stderr || '')).trim();
      result.nextBuildError = out.slice(-3000);
      result.nextBuildOk = false;
    }
  }

  // 4. Discover page & API routes
  const pageRoutes = new Set(['/']);
  const appDir = path.join(projDir, 'src', 'app');
  if (fs.existsSync(appDir)) {
    for (const ent of fs.readdirSync(appDir, { withFileTypes: true })) {
      if (ent.isDirectory() && ent.name !== 'api' && !ent.name.startsWith('_') && !ent.name.startsWith('.')) {
        const sub = path.join(appDir, ent.name);
        if (fs.existsSync(path.join(sub, 'page.tsx')) || fs.existsSync(path.join(sub, 'page.jsx'))) {
          pageRoutes.add('/' + ent.name);
        }
      }
    }
  }
  const apiRoutes = [];
  const apiDir = path.join(appDir, 'api');
  if (fs.existsSync(apiDir)) {
    for (const ent of fs.readdirSync(apiDir, { withFileTypes: true })) {
      if (ent.isDirectory() && ent.name !== 'auth' && !ent.name.includes('webhook')) {
        const sub = path.join(apiDir, ent.name);
        if (fs.existsSync(path.join(sub, 'route.ts')) || fs.existsSync(path.join(sub, 'route.js'))) {
          apiRoutes.push('/api/' + ent.name);
        }
      }
    }
  }

  // 5. Start temporary Next.js server & smoke test routes
  const port = await getFreePort(3410);
  const mode = result.nextBuildOk ? 'start' : 'dev';
  updateState({ message: `Verifikacija: smoke test ruta na portu ${port} (${mode})...` }, `🌐 Pokrećem next ${mode} na portu ${port}...`);

  const nextBin = path.join(projDir, 'node_modules', 'next', 'dist', 'bin', 'next');
  const srv = spawn('node', [nextBin, mode, '-p', String(port)], {
    cwd: projDir,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, NO_COLOR: '1' },
  });

  let ready = false;
  const deadline = Date.now() + 45000;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/`, { redirect: 'manual' });
      if (r.status > 0) { ready = true; break; }
    } catch {}
    await sleep(800);
  }

  try {
    if (ready) {
      const baseUrl = `http://127.0.0.1:${port}`;
      for (const r of pageRoutes) {
        const probe = await probeRoute(baseUrl, r, 'GET');
        result.routes.push(probe);
      }
      for (const api of apiRoutes) {
        if (api === '/api/register') {
          const probe = await probeRoute(baseUrl, api, 'POST', {
            email: `e2e_${Date.now()}@test.local`,
            password: 'TestPassword123!',
          });
          result.routes.push(probe);
        } else {
          const probe = await probeRoute(baseUrl, api, 'GET');
          result.routes.push(probe);
        }
      }
    } else {
      result.routes.push({ route: '/', method: 'GET', ok: false, status: 0, issue: 'DEV_SERVER_FAILED_TO_START' });
    }
  } finally {
    killTree(srv.pid);
  }

  result.routesPassed = result.routes.filter(r => r.ok).length;
  result.routesFailed = result.routes.filter(r => !r.ok).length;

  // 6. Electron-specific syntax verification for main.js & preload.js when present
  result.electronFiles = [];
  for (const ef of ['main.js', 'preload.js', 'src/main/index.js']) {
    const efp = path.join(projDir, ef);
    if (fs.existsSync(efp)) {
      try {
        execSync(`node --check "${efp}"`, { stdio: 'pipe', timeout: 10000 });
        result.electronFiles.push({ file: ef, syntaxOk: true });
      } catch (e) {
        result.electronFiles.push({ file: ef, syntaxOk: false, error: ((e.stderr || '') + (e.stdout || '')).slice(0, 400) });
      }
    }
  }

  return result;
}

async function main() {
  const cfg = parseArgs();
  const manifestPath = path.join(MANIFESTS_DIR, `${cfg.manifest}.json`);
  if (!fs.existsSync(manifestPath)) {
    console.error(`Manifest not found: ${manifestPath}`);
    process.exit(2);
  }
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const projectName = manifest.name || manifest.metadata?.name || cfg.manifest;
  const projectId = `e2e-${cfg.manifest}-${Date.now().toString(36)}`;
  const startTime = Date.now();

  updateState({
    track: cfg.track,
    iteration: cfg.iteration,
    totalIterations: cfg.total,
    manifestId: cfg.manifest,
    projectId,
    projectName,
    startedAt: new Date().toISOString(),
    progress: 0,
    currentFile: '',
    fileIndex: 0,
    totalFiles: 0,
  }, `=== POČETAK ITERACIJE #${cfg.iteration}/${cfg.total}: ${projectName} (${cfg.manifest}) ===`);

  await runCooldown(cfg.cooldown);
  await waitWhilePaused('pre-build');

  // 1. Compile freshest local Rust e2e_runner
  if (!cfg.skipBuild) {
    updateState({ phase: 'compiling-runner', status: 'generating', progress: 1, message: 'Kompajliranje najsvežijeg lokalnog Rust koda (e2e_runner)...' }, '🔧 Kompajliram e2e_runner...');
    execSync('cargo build --manifest-path src-tauri/Cargo.toml --bin e2e_runner', {
      cwd: REPO_ROOT,
      stdio: 'inherit',
    });
  }

  await waitWhilePaused('pre-generation');

  // 2. Run headless e2e_runner pointing at local Mock Manifest Server
  const runnerExe = path.join(REPO_ROOT, 'src-tauri', 'target', 'debug', process.platform === 'win32' ? 'e2e_runner.exe' : 'e2e_runner');
  updateState({ phase: 'generate-files', status: 'generating', progress: 2, message: `Generisanje projekta '${projectName}'...` }, `🚀 Pokrenut e2e_runner za ${projectId}`);

  const exitCode = await new Promise((resolve) => {
    const child = spawn(
      runnerExe,
      [
        '--manifest-id', cfg.manifest,
        '--project-id', projectId,
        '--host', `http://127.0.0.1:${MONITOR_PORT}`,
        '--model', cfg.model,
      ],
      {
        cwd: REPO_ROOT,
        env: {
          ...process.env,
          AXIOM_E2E_HEADLESS: '1',
          AXIOM_E2E_PAUSE_FILE: PAUSE_FLAG,
        },
        stdio: ['ignore', 'pipe', 'pipe'],
      }
    );

    let buf = '';
    child.stdout.on('data', (chunk) => {
      buf += chunk.toString();
      const lines = buf.split(/\r?\n/);
      buf = lines.pop() || '';
      for (const line of lines) {
        if (line.startsWith('__AXIOM_PROGRESS__:')) {
          try {
            const p = JSON.parse(line.slice('__AXIOM_PROGRESS__:'.length));
            updateState({
              phase: p.phase || 'generating',
              status: p.status || 'generating',
              progress: typeof p.progress === 'number' ? p.progress : undefined,
              currentFile: p.currentFile || undefined,
              fileIndex: p.fileIndex || undefined,
              totalFiles: p.totalFiles || undefined,
              message: p.message || '',
            }, p.message ? `[${p.phase}] ${p.message}` : null);
          } catch {}
        } else if (line.trim()) {
          updateState({}, line.trim());
        }
      }
    });

    child.stderr.on('data', (chunk) => {
      const txt = chunk.toString().trim();
      if (txt) updateState({}, `[stderr] ${txt}`);
    });

    child.on('close', (code) => resolve(code ?? 1));
  });

  const projDir = path.join(PROJECTS_ROOT, projectId);
  let verify = null;
  if (fs.existsSync(projDir)) {
    verify = await verifyProject(projDir, projectId);
  }

  const durationSec = Math.round((Date.now() - startTime) / 1000);
  const report = {
    track: cfg.track,
    iteration: cfg.iteration,
    manifestId: cfg.manifest,
    projectId,
    projectName,
    model: cfg.model,
    runnerExitCode: exitCode,
    durationSec,
    verify,
    completedAt: new Date().toISOString(),
  };

  const reportPath = path.join(STATE_DIR, `report-${cfg.manifest}-${projectId}.json`);
  fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));

  const cur = readState();
  const history = Array.isArray(cur.history) ? [...cur.history] : [];
  history.push({
    iteration: cfg.iteration,
    manifestId: cfg.manifest,
    projectId,
    projectName,
    tscErrors: verify ? verify.tscErrors : -1,
    nextBuildOk: verify ? verify.nextBuildOk : false,
    routesPassed: verify ? verify.routesPassed : 0,
    routesFailed: verify ? verify.routesFailed : 0,
    durationSec,
    note: verify && verify.nextBuildOk && verify.tscErrors === 0 && verify.routesFailed === 0
      ? 'ČIST PROLAZ (0 grešaka)'
      : 'Čeka analizu & sistemski fix',
  });

  updateState({
    phase: 'analyzing',
    status: 'analyzing',
    progress: 100,
    lastDurationSec: durationSec,
    message: `Iteracija #${cfg.iteration} završena (${durationSec}s) — TS=${verify?.tscErrors}, build=${verify?.nextBuildOk ? 'OK' : 'FAIL'}, rute=${verify?.routesPassed} OK / ${verify?.routesFailed} FAIL.`,
    history,
  }, `📊 Izveštaj sačuvan: ${reportPath}`);

  console.log('\n========== E2E ITERATION SUMMARY ==========');
  console.log(JSON.stringify({
    iteration: cfg.iteration,
    projectId,
    projectName,
    durationSec,
    runnerExitCode: exitCode,
    tscErrors: verify?.tscErrors,
    buildIdBeforeVerify: verify?.buildIdBeforeVerify,
    nextBuildOk: verify?.nextBuildOk,
    routesPassed: verify?.routesPassed,
    routesFailed: verify?.routesFailed,
    failedRoutes: (verify?.routes || []).filter(r => !r.ok),
    electronFiles: verify?.electronFiles || [],
    generationWarnings: verify?.generationWarnings,
    reportPath,
  }, null, 2));
}

main().catch((err) => {
  console.error(err);
  updateState({ status: 'error', phase: 'error', message: String(err) }, `❌ Greška: ${err}`);
  process.exit(1);
});
