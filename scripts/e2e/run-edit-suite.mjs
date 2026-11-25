#!/usr/bin/env node
/**
 * run-edit-suite.mjs
 * Live AI Edit E2E orchestrator with identical CPU thermal guard (>95°C for 5s -> 30s pause),
 * isolated sandbox copies (never mutating original generated projects), tsc/build verification,
 * and live dashboard updates (scripts/e2e/.state/edit-state.json + state.json).
 */
import fs from 'fs';
import path from 'path';
import os from 'os';
import { fileURLToPath } from 'url';
import { spawn, spawnSync } from 'child_process';
import { createCpuThermalGuard } from './cpu-thermal-guard.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '../..');
const STATE_DIR = path.join(__dirname, '.state');
const STATE_FILE = path.join(STATE_DIR, 'state.json');
const EDIT_STATE_FILE = path.join(STATE_DIR, 'edit-state.json');
const SANDBOX_FILE = path.join(STATE_DIR, 'edit-sandboxes.json');
const SCREENSHOTS_DIR = path.join(STATE_DIR, 'screenshots');
const PAUSE_FLAG = path.join(STATE_DIR, 'pause.flag');
const PROJECTS_ROOT = path.join(process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local'), 'axiom-forge', 'projects');

fs.mkdirSync(STATE_DIR, { recursive: true });
fs.mkdirSync(SCREENSHOTS_DIR, { recursive: true });

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    cmd: null,
    test: null,
    title: '',
    platform: 'Web',
    projectId: '',
    mode: 'single',
    file: '',
    prompt: '',
    visualContextFile: '',
    model: 'qwen2.5-coder:7b',
    cooldown: 0,
    skipBuild: false,
    webSource: 'e2e-web_2-mwwagu5l',
    expoSource: 'e2e-expo_2-mwwe4u4u',
    uiStatus: 'pending',
    uiSummary: '',
    beforeSrc: '',
    afterSrc: '',
    visualJson: '',
  };
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === '--prepare') out.cmd = 'prepare';
    else if (a === '--run-edit') out.cmd = 'run-edit';
    else if (a === '--record-ui') out.cmd = 'record-ui';
    else if (a === '--finish') out.cmd = 'finish';
    else if (a === '--test' && args[i + 1]) out.test = Number(args[++i]);
    else if (a === '--title' && args[i + 1]) out.title = args[++i];
    else if (a === '--platform' && args[i + 1]) out.platform = args[++i];
    else if (a === '--project-id' && args[i + 1]) out.projectId = args[++i];
    else if (a === '--mode' && args[i + 1]) out.mode = args[++i];
    else if (a === '--file' && args[i + 1]) out.file = args[++i];
    else if (a === '--prompt' && args[i + 1]) out.prompt = args[++i];
    else if (a === '--visual-context-file' && args[i + 1]) out.visualContextFile = args[++i];
    else if (a === '--model' && args[i + 1]) out.model = args[++i];
    else if (a === '--cooldown' && args[i + 1]) out.cooldown = Number(args[++i]);
    else if (a === '--skip-build') out.skipBuild = true;
    else if (a === '--web-source' && args[i + 1]) out.webSource = args[++i];
    else if (a === '--expo-source' && args[i + 1]) out.expoSource = args[++i];
    else if (a === '--status' && args[i + 1]) out.uiStatus = args[++i];
    else if (a === '--summary' && args[i + 1]) out.uiSummary = args[++i];
    else if (a === '--before-src' && args[i + 1]) out.beforeSrc = args[++i];
    else if (a === '--after-src' && args[i + 1]) out.afterSrc = args[++i];
    else if (a === '--visual-json' && args[i + 1]) out.visualJson = args[++i];
  }
  return out;
}

function readJsonSafe(filePath, fallback) {
  try {
    if (fs.existsSync(filePath)) return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch {}
  return fallback;
}

function writeMainState(patch, logLine) {
  const cur = readJsonSafe(STATE_FILE, { logs: [], history: [] });
  const logs = Array.isArray(cur.logs) ? [...cur.logs] : [];
  if (logLine) logs.push(`[${new Date().toLocaleTimeString()}] ${logLine}`);
  const next = {
    ...cur,
    ...patch,
    logs: logs.slice(-160),
    updatedAt: new Date().toISOString(),
  };
  fs.writeFileSync(STATE_FILE, JSON.stringify(next, null, 2));
}

function readEditState() {
  return readJsonSafe(EDIT_STATE_FILE, {
    status: 'idle',
    activeTest: null,
    note: '',
    tests: [],
    updatedAt: new Date().toISOString(),
  });
}

function writeEditState(mutator) {
  const cur = readEditState();
  const next = mutator(cur) || cur;
  next.updatedAt = new Date().toISOString();
  fs.writeFileSync(EDIT_STATE_FILE, JSON.stringify(next, null, 2));
  return next;
}

function upsertTestEntry(testId, patch) {
  return writeEditState((st) => {
    const tests = Array.isArray(st.tests) ? [...st.tests] : [];
    const idx = tests.findIndex((t) => Number(t.id) === Number(testId));
    if (idx >= 0) {
      tests[idx] = { ...tests[idx], ...patch, id: Number(testId) };
    } else {
      tests.push({ id: Number(testId), ...patch });
      tests.sort((a, b) => Number(a.id) - Number(b.id));
    }
    return { ...st, tests };
  });
}

function copyProjectSandbox(srcProjectId, prefix) {
  const srcDir = path.join(PROJECTS_ROOT, srcProjectId);
  if (!fs.existsSync(srcDir)) {
    throw new Error(`Source project not found: ${srcDir}`);
  }
  const stamp = Date.now().toString(36);
  const dstId = `${prefix}-${stamp}`;
  const dstDir = path.join(PROJECTS_ROOT, dstId);
  fs.mkdirSync(dstDir, { recursive: true });

  const skipNames = new Set(['node_modules', '.next', '.expo', '.expo-verify', '.expo-e2e-dist', '.git']);
  function copyRec(from, to) {
    for (const ent of fs.readdirSync(from, { withFileTypes: true })) {
      if (skipNames.has(ent.name)) continue;
      const s = path.join(from, ent.name);
      const d = path.join(to, ent.name);
      if (ent.isDirectory()) {
        fs.mkdirSync(d, { recursive: true });
        copyRec(s, d);
      } else if (ent.isFile()) {
        fs.copyFileSync(s, d);
      }
    }
  }
  copyRec(srcDir, dstDir);

  const srcNm = path.join(srcDir, 'node_modules');
  const dstNm = path.join(dstDir, 'node_modules');
  if (fs.existsSync(srcNm) && !fs.existsSync(dstNm)) {
    fs.symlinkSync(srcNm, dstNm, 'junction');
  }
  return { projectId: dstId, projDir: dstDir, sourceProjectId: srcProjectId };
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function waitWhilePaused(stage) {
  let announced = false;
  while (fs.existsSync(PAUSE_FLAG)) {
    let reason = '';
    try {
      reason = fs.readFileSync(PAUSE_FLAG, 'utf8').trim();
    } catch {}
    const isThermal = reason.startsWith('THERMAL_AUTO_PAUSE:');
    if (!announced) {
      announced = true;
      writeMainState(
        {
          status: isThermal ? 'cooldown' : 'paused',
          phase: stage,
          message: isThermal
            ? `🌡 Automatska termalna pauza CPU (${stage}) — čekanje hlađenja 30s...`
            : `⏸ Pauzirano (${stage}) — čeka nastavak...`,
        },
        isThermal ? `🌡 Thermal pause active (${stage})` : `⏸ Manual pause active (${stage})`,
      );
    }
    await sleep(1000);
  }
}

async function runCooldown(seconds, label) {
  if (!seconds || seconds <= 0) return;
  for (let rem = seconds; rem > 0; rem--) {
    await waitWhilePaused('cooldown');
    writeMainState({
      status: 'cooldown',
      phase: 'cooldown',
      cooldownSecondsRemaining: rem,
      message: `❄ Termalno hlađenje pre ${label} (${rem}s)...`,
    });
    await sleep(1000);
  }
  writeMainState({ cooldownSecondsRemaining: 0 });
}

function fileMtimeMs(p) {
  try {
    return fs.statSync(p).mtimeMs;
  } catch {
    return 0;
  }
}

function runShellCapture(cmd, args, cwd, timeoutMs = 180000) {
  const res = spawnSync(cmd, args, {
    cwd,
    shell: true,
    encoding: 'utf8',
    timeout: timeoutMs,
    maxBuffer: 12 * 1024 * 1024,
  });
  return {
    code: typeof res.status === 'number' ? res.status : 1,
    stdout: (res.stdout || '').trim(),
    stderr: (res.stderr || '').trim(),
  };
}

function countTscErrors(output) {
  const matches = output.match(/error TS\d+:/g);
  return matches ? matches.length : 0;
}

async function cmdPrepare(opts) {
  const webSandbox = copyProjectSandbox(opts.webSource, 'edit-sandbox-web2');
  const expoSandbox = copyProjectSandbox(opts.expoSource, 'edit-sandbox-expo2');

  const sandboxes = {
    createdAt: new Date().toISOString(),
    web: webSandbox,
    expo: expoSandbox,
  };
  fs.writeFileSync(SANDBOX_FILE, JSON.stringify(sandboxes, null, 2));

  writeEditState(() => ({
    status: 'running',
    activeTest: null,
    note: 'Sandbox kopije pripremljene (originali netaknuti)',
    tests: [
      {
        id: 1,
        title: 'Web Single-File Visual Click Edit',
        platform: 'Web',
        mode: 'single',
        projectId: webSandbox.projectId,
        uiStatus: 'pending',
        uiSummary: 'Čeka klik i izvršenje',
      },
      {
        id: 2,
        title: 'Web Multi-File Project Edit (verticalFeatures)',
        platform: 'Web',
        mode: 'project',
        projectId: webSandbox.projectId,
        uiStatus: 'pending',
        uiSummary: 'Čeka izvršenje',
      },
      {
        id: 3,
        title: 'Mobile-Expo Edit (bez Next.js kontaminacije)',
        platform: 'Mobile-Expo',
        mode: 'single',
        projectId: expoSandbox.projectId,
        uiStatus: 'pending',
        uiSummary: 'Čeka izvršenje',
      },
    ],
  }));

  writeMainState(
    {
      track: 'AI Edit Suite',
      iteration: 0,
      totalIterations: 3,
      projectId: webSandbox.projectId,
      projectName: 'AI Edit Live Validation',
      phase: 'prepare',
      status: 'generating',
      progress: 5,
      message: 'Kreirane izolovane sandbox kopije za 3 AI Edit testa.',
      startedAt: new Date().toISOString(),
    },
    `[edit-suite] Prepared sandboxes: web=${webSandbox.projectId}, expo=${expoSandbox.projectId}`,
  );

  console.log(JSON.stringify(sandboxes, null, 2));
}

async function cmdRunEdit(opts) {
  const thermalGuard = createCpuThermalGuard({
    maxTempC: 95,
    sustainSec: 5,
    pauseSec: 30,
    onLog: (line) => {
      console.log(line);
      writeMainState({}, line);
    },
  });
  const guardStatus = await thermalGuard.start();
  if (!guardStatus.ok) {
    thermalGuard.stop();
    throw new Error(`Thermal guard unavailable: ${guardStatus.error || 'unknown'}`);
  }

  try {
    await runCooldown(opts.cooldown, `AI Edit Test #${opts.test}`);
    await waitWhilePaused(`pre-test-${opts.test}`);

    const projDir = path.join(PROJECTS_ROOT, opts.projectId);
    if (!fs.existsSync(projDir)) {
      throw new Error(`Sandbox project not found: ${projDir}`);
    }

    const visualContext = opts.visualContextFile && fs.existsSync(opts.visualContextFile)
      ? readJsonSafe(opts.visualContextFile, null)
      : null;

    writeEditState((st) => ({ ...st, status: 'running', activeTest: opts.test }));
    upsertTestEntry(opts.test, {
      title: opts.title || undefined,
      platform: opts.platform,
      mode: opts.mode,
      projectId: opts.projectId,
      filePath: opts.file || undefined,
      prompt: opts.prompt,
      visualContext,
      error: null,
    });

    writeMainState(
      {
        track: `AI Edit (${opts.platform})`,
        iteration: opts.test,
        totalIterations: 3,
        projectId: opts.projectId,
        projectName: opts.title || `AI Edit Test #${opts.test}`,
        phase: 'ai-edit',
        status: 'generating',
        currentFile: opts.file || '(Project Router)',
        progress: 15,
        message: `Pokretanje AI Edit Test #${opts.test} (${opts.mode})...`,
        startedAt: new Date().toISOString(),
      },
      `[edit-test #${opts.test}] Start (${opts.mode}) on ${opts.projectId}: "${opts.prompt}"`,
    );

    const runnerExe = path.join(REPO_ROOT, 'src-tauri', 'target', 'debug', 'e2e_edit_runner.exe');
    if (!opts.skipBuild || !fs.existsSync(runnerExe)) {
      writeMainState({ phase: 'build-runner', message: 'Kompajliranje e2e_edit_runner...' }, '[edit-suite] Building e2e_edit_runner...');
      const bRes = runShellCapture('cargo', ['build', '--manifest-path', 'src-tauri/Cargo.toml', '--bin', 'e2e_edit_runner'], REPO_ROOT, 300000);
      if (bRes.code !== 0) {
        throw new Error(`Failed to build e2e_edit_runner:\n${bRes.stderr || bRes.stdout}`);
      }
    }

    const contractsPath = path.join(projDir, '.axiom', 'axiom-contracts.json');
    const featuresPath = path.join(projDir, 'axiom-features.json');
    const preContractsMtime = fileMtimeMs(contractsPath);
    const preFeaturesMtime = fileMtimeMs(featuresPath);

    await waitWhilePaused(`pre-ollama-test-${opts.test}`);

    const runnerArgs = [
      '--mode', opts.mode,
      '--project-id', opts.projectId,
      '--prompt', opts.prompt,
      '--model', opts.model,
      '--apply',
    ];
    if (opts.file) runnerArgs.push('--file', opts.file);
    if (opts.visualContextFile) runnerArgs.push('--visual-context-file', opts.visualContextFile);

    let editResult = null;
    const rawLogs = [];

    await new Promise((resolve) => {
      const child = spawn(runnerExe, runnerArgs, { cwd: REPO_ROOT, stdio: ['ignore', 'pipe', 'pipe'] });
      let buf = '';
      const handleLine = (line) => {
        const trimmed = line.trim();
        if (!trimmed) return;
        rawLogs.push(trimmed);
        if (trimmed.startsWith('__AXIOM_EDIT_PROGRESS__:')) {
          try {
            const p = JSON.parse(trimmed.slice('__AXIOM_EDIT_PROGRESS__:'.length));
            writeMainState(
              {
                phase: 'ai-edit',
                progress: Math.min(75, Math.max(15, Math.round(15 + (p.percent || 0) * 0.55))),
                message: p.status || 'AI Edit u toku...',
              },
              `[edit-test #${opts.test}] ${p.status}`,
            );
          } catch {}
        } else if (trimmed.startsWith('__AXIOM_EDIT_RESULT__:')) {
          try {
            editResult = JSON.parse(trimmed.slice('__AXIOM_EDIT_RESULT__:'.length));
          } catch {}
        }
      };
      child.stdout.on('data', (chunk) => {
        buf += chunk.toString('utf8');
        const lines = buf.split(/\r?\n/);
        buf = lines.pop() || '';
        lines.forEach(handleLine);
      });
      child.stderr.on('data', (chunk) => {
        const txt = chunk.toString('utf8').trim();
        if (txt) rawLogs.push(`[stderr] ${txt}`);
      });
      child.on('close', () => {
        if (buf.trim()) handleLine(buf);
        resolve();
      });
    });

    const postContractsMtime = fileMtimeMs(contractsPath);
    const postFeaturesMtime = fileMtimeMs(featuresPath);
    const mapsRefreshed = postContractsMtime > preContractsMtime || postFeaturesMtime > preFeaturesMtime;

    await waitWhilePaused(`pre-verify-test-${opts.test}`);

    let prismaValidateOk = null;
    let prismaGenerateOk = null;
    let prismaLog = '';
    if (editResult && editResult.touchedSchema && opts.platform !== 'Mobile-Expo') {
      writeMainState(
        { phase: 'verifying', status: 'verifying', progress: 78, message: `Prisma validate & generate za Test #${opts.test}...` },
        `[edit-test #${opts.test}] Schema changed -> running prisma validate & prisma generate...`,
      );
      const pv = runShellCapture('npx', ['prisma', 'validate'], projDir, 120000);
      prismaValidateOk = pv.code === 0;
      if (prismaValidateOk) {
        const pg = runShellCapture('npx', ['prisma', 'generate'], projDir, 180000);
        prismaGenerateOk = pg.code === 0;
        prismaLog = `${pg.stdout}\n${pg.stderr}`.trim().slice(-800);
      } else {
        prismaGenerateOk = false;
        prismaLog = `${pv.stdout}\n${pv.stderr}`.trim().slice(-800);
      }
    }

    writeMainState(
      { phase: 'verifying', status: 'verifying', progress: 82, message: `Verifikacija tsc & build/export za Test #${opts.test}...` },
      `[edit-test #${opts.test}] Verifying tsc & build/export (mapsRefreshed=${mapsRefreshed}, prismaGen=${prismaGenerateOk})...`,
    );

    const tscRes = runShellCapture('npx', ['tsc', '--noEmit'], projDir, 180000);
    const tscCombined = `${tscRes.stdout}\n${tscRes.stderr}`.trim();
    const tscErrors = tscRes.code === 0 ? 0 : Math.max(1, countTscErrors(tscCombined));

    let buildOk = false;
    let buildLog = '';
    if (opts.platform === 'Mobile-Expo') {
      const expRes = runShellCapture('npx', ['expo', 'export', '--platform', 'web', '--output-dir', '.expo-verify'], projDir, 240000);
      buildOk = expRes.code === 0;
      buildLog = `${expRes.stdout}\n${expRes.stderr}`.trim().slice(-1200);
    } else {
      const nextRes = runShellCapture('npx', ['next', 'build'], projDir, 240000);
      buildOk = nextRes.code === 0;
      buildLog = `${nextRes.stdout}\n${nextRes.stderr}`.trim().slice(-1200);
    }

    const thSnap = thermalGuard.getStats();
    const summaryObj = {
      testId: opts.test,
      projectId: opts.projectId,
      mode: opts.mode,
      platform: opts.platform,
      editResult,
      mapsRefreshed,
      prismaValidateOk,
      prismaGenerateOk,
      prismaLog,
      tscErrors,
      tscOutput: tscCombined.slice(0, 1500),
      buildOk,
      buildLog,
      thermal: thSnap,
      logs: rawLogs.slice(-30),
    };

    upsertTestEntry(opts.test, {
      success: Boolean(editResult && editResult.success),
      durationSec: editResult?.durationSec || 0,
      filePath: editResult?.filePath || opts.file || undefined,
      modifiedFiles: editResult?.modifiedFiles || (opts.file ? [opts.file] : []),
      touchedSchema: Boolean(editResult?.touchedSchema),
      touchedApi: Boolean(editResult?.touchedApi),
      hasUseClient: Boolean(editResult?.hasUseClient || editResult?.hasUseClientOnAny),
      applied: Boolean(editResult?.applied),
      error: editResult?.error || null,
      mapsRefreshed,
      prismaValidateOk,
      prismaGenerateOk,
      tscErrors,
      buildOk,
    });

    writeMainState(
      {
        status: 'verifying',
        phase: 'ui-check',
        progress: 92,
        message: `Test #${opts.test} AI Edit gotov (TS=${tscErrors}, build=${buildOk ? 'PASS' : 'FAIL'}, maps=${mapsRefreshed}). Čeka UI browser proveru.`,
      },
      `[edit-test #${opts.test}] Done: success=${Boolean(editResult?.success)} TS=${tscErrors} build=${buildOk} mapsRefreshed=${mapsRefreshed}`,
    );

    const outPath = path.join(STATE_DIR, `edit-report-test${opts.test}.json`);
    fs.writeFileSync(outPath, JSON.stringify(summaryObj, null, 2));
    console.log(JSON.stringify(summaryObj, null, 2));
  } finally {
    await thermalGuard.stop();
  }
}

function cmdRecordUi(opts) {
  const patch = {
    uiStatus: opts.uiStatus,
    uiSummary: opts.uiSummary,
  };
  if (opts.visualJson) {
    try {
      patch.visualContext = JSON.parse(opts.visualJson);
    } catch {}
  }
  if (opts.beforeSrc && fs.existsSync(opts.beforeSrc)) {
    const name = `test${opts.test}-before.png`;
    fs.copyFileSync(opts.beforeSrc, path.join(SCREENSHOTS_DIR, name));
    patch.beforeScreenshot = name;
  }
  if (opts.afterSrc && fs.existsSync(opts.afterSrc)) {
    const name = `test${opts.test}-after.png`;
    fs.copyFileSync(opts.afterSrc, path.join(SCREENSHOTS_DIR, name));
    patch.afterScreenshot = name;
  }
  upsertTestEntry(opts.test, patch);
  writeMainState(
    {
      progress: opts.uiStatus === 'pending' ? 10 : 100,
      message: `UI Provera Test #${opts.test}: ${opts.uiStatus.toUpperCase()} — ${opts.uiSummary}`,
    },
    `[edit-test #${opts.test}] UI check (${opts.uiStatus}): ${opts.uiSummary}`,
  );
  console.log(JSON.stringify({ ok: true, test: opts.test, ...patch }));
}

function cmdFinish(opts) {
  writeEditState((st) => ({
    ...st,
    status: 'done',
    activeTest: null,
    note: opts.uiSummary || 'Sva 3 živa AI Edit testa završena.',
  }));
  writeMainState(
    {
      status: 'completed',
      phase: 'done',
      progress: 100,
      cooldownSecondsRemaining: 0,
      message: opts.uiSummary || 'Sva 3 živa AI Edit testa završena.',
    },
    '[edit-suite] All 3 live AI Edit tests completed.',
  );
  console.log(JSON.stringify({ ok: true, finished: true }));
}

const opts = parseArgs();
if (opts.cmd === 'prepare') {
  cmdPrepare(opts).catch((e) => { console.error(e); process.exit(1); });
} else if (opts.cmd === 'run-edit') {
  cmdRunEdit(opts).catch((e) => { console.error(e); process.exit(1); });
} else if (opts.cmd === 'record-ui') {
  cmdRecordUi(opts);
} else if (opts.cmd === 'finish') {
  cmdFinish(opts);
} else {
  console.error('Usage: run-edit-suite.mjs --prepare | --run-edit ... | --record-ui ... | --finish');
  process.exit(2);
}
