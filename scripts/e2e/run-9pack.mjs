#!/usr/bin/env node
/**
 * run-9pack.mjs
 * Sequential 9-iteration E2E benchmark campaign across all 3 platforms:
 *   1-3: Web (web_1, web_2, web_3)
 *   4-6: Desktop (desktop_1, desktop_2, desktop_3)
 *   7-9: Mobile-Expo (expo_1, expo_2, expo_3)
 * Enforces a 2-minute (120s) thermal cooldown before iterations 2..9.
 */
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { spawn } from 'child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const STATE_DIR = path.join(__dirname, '.state');
const STATE_FILE = path.join(STATE_DIR, 'state.json');
const PAUSE_FLAG = path.join(STATE_DIR, 'pause.flag');
const STOP_FLAG = path.join(STATE_DIR, 'stop.flag');
const RUN_ITERATION_SCRIPT = path.join(__dirname, 'run_iteration.mjs');

const PLAN = [
  { manifest: 'web_1',     track: 'Web',         skipBuild: false },
  { manifest: 'web_2',     track: 'Web',         skipBuild: true  },
  { manifest: 'web_3',     track: 'Web',         skipBuild: true  },
  { manifest: 'desktop_1', track: 'Desktop',     skipBuild: true  },
  { manifest: 'desktop_2', track: 'Desktop',     skipBuild: true  },
  { manifest: 'desktop_3', track: 'Desktop',     skipBuild: true  },
  { manifest: 'expo_1',    track: 'Mobile-Expo', skipBuild: true  },
  { manifest: 'expo_2',    track: 'Mobile-Expo', skipBuild: true  },
  { manifest: 'expo_3',    track: 'Mobile-Expo', skipBuild: true  },
];

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    model: 'qwen2.5-coder:7b',
    cooldown: 120,
  };
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--model' && args[i + 1]) out.model = args[++i];
    else if (args[i] === '--cooldown' && args[i + 1]) out.cooldown = Number(args[++i]);
  }
  return out;
}

function readStateSafe() {
  try {
    if (fs.existsSync(STATE_FILE)) return JSON.parse(fs.readFileSync(STATE_FILE, 'utf8'));
  } catch {}
  return { logs: [], history: [] };
}

function writeStatePatch(patch, logLine) {
  const cur = readStateSafe();
  const logs = Array.isArray(cur.logs) ? [...cur.logs] : [];
  if (logLine) {
    logs.push(`[${new Date().toLocaleTimeString()}] ${logLine}`);
  }
  const next = {
    ...cur,
    ...patch,
    logs: logs.slice(-140),
    updatedAt: new Date().toISOString(),
  };
  fs.writeFileSync(STATE_FILE, JSON.stringify(next, null, 2));
}

function snapshotReports(manifestId) {
  const map = new Map();
  if (!fs.existsSync(STATE_DIR)) return map;
  const prefix = `report-${manifestId}-`;
  for (const name of fs.readdirSync(STATE_DIR)) {
    if (name.startsWith(prefix) && name.endsWith('.json')) {
      try {
        const full = path.join(STATE_DIR, name);
        map.set(name, fs.statSync(full).mtimeMs);
      } catch {}
    }
  }
  return map;
}

function findNewReport(manifestId, beforeMap) {
  if (!fs.existsSync(STATE_DIR)) return null;
  const prefix = `report-${manifestId}-`;
  const candidates = [];
  for (const name of fs.readdirSync(STATE_DIR)) {
    if (name.startsWith(prefix) && name.endsWith('.json')) {
      try {
        const full = path.join(STATE_DIR, name);
        const mtimeMs = fs.statSync(full).mtimeMs;
        const prevMtime = beforeMap.get(name);
        if (prevMtime === undefined || mtimeMs > prevMtime) {
          candidates.push({ name, full, mtimeMs });
        }
      } catch {}
    }
  }
  candidates.sort((a, b) => b.mtimeMs - a.mtimeMs);
  return candidates[0] || null;
}

function runChildIteration(args) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [RUN_ITERATION_SCRIPT, ...args], {
      cwd: path.resolve(__dirname, '..', '..'),
      stdio: 'inherit',
      windowsHide: true,
    });
    child.on('exit', (code, signal) => {
      resolve({ code: code ?? 1, signal: signal || null });
    });
    child.on('error', (err) => {
      resolve({ code: 1, signal: null, spawnError: String(err.message || err) });
    });
  });
}

async function main() {
  const cfg = parseArgs();
  fs.mkdirSync(STATE_DIR, { recursive: true });

  if (fs.existsSync(PAUSE_FLAG)) fs.unlinkSync(PAUSE_FLAG);
  if (fs.existsSync(STOP_FLAG)) fs.unlinkSync(STOP_FLAG);

  const campaignStartedAt = new Date().toISOString();
  const stamp = campaignStartedAt.replace(/[:.]/g, '-').slice(0, 19);

  const initState = {
    track: PLAN[0].track,
    iteration: 0,
    totalIterations: PLAN.length,
    projectId: '',
    projectName: 'Priprema 9-Pack E2E kampanje...',
    manifestId: '',
    phase: 'starting',
    status: 'starting',
    progress: 0,
    message: `Pokretanje 9-Pack E2E kampanje (${PLAN.length} aplikacija, pauza ${cfg.cooldown}s)...`,
    cooldownSecondsRemaining: 0,
    startedAt: campaignStartedAt,
    updatedAt: campaignStartedAt,
    logs: [
      `[${new Date().toLocaleTimeString()}] 🚀 Start 9-Pack E2E kampanje (3 Web, 3 Desktop, 3 Mobile-Expo | model=${cfg.model} | cooldown=${cfg.cooldown}s)`,
    ],
    history: [],
  };
  fs.writeFileSync(STATE_FILE, JSON.stringify(initState, null, 2));

  console.log(`\n=============================================================`);
  console.log(`  AXIOM FORGE 9-PACK E2E CAMPAIGN`);
  console.log(`  Model: ${cfg.model} | Inter-run cooldown: ${cfg.cooldown}s`);
  console.log(`  Order: ${PLAN.map((p) => p.manifest).join(' -> ')}`);
  console.log(`=============================================================\n`);

  const results = [];
  let stoppedByFlag = false;

  for (let i = 0; i < PLAN.length; i++) {
    if (fs.existsSync(STOP_FLAG)) {
      stoppedByFlag = true;
      console.log(`\n[9-pack] ⏹ Detektovan stop.flag pre iteracije #${i + 1} — prekidam kampanju.`);
      writeStatePatch(
        { status: 'idle', phase: 'stopped', cooldownSecondsRemaining: 0, message: `Kampanja zaustavljena posle ${i}/${PLAN.length} iteracija.` },
        `⏹ Kampanja zaustavljena od strane korisnika pre iteracije #${i + 1}.`
      );
      fs.unlinkSync(STOP_FLAG);
      break;
    }

    const item = PLAN[i];
    const iterationNum = i + 1;
    const cooldownForRun = i === 0 ? 0 : cfg.cooldown;
    const beforeReports = snapshotReports(item.manifest);

    const iterArgs = [
      '--manifest', item.manifest,
      '--iteration', String(iterationNum),
      '--total', String(PLAN.length),
      '--track', item.track,
      '--model', cfg.model,
      '--cooldown', String(cooldownForRun),
    ];
    if (item.skipBuild) iterArgs.push('--skip-build');

    console.log(`\n>>> [${iterationNum}/${PLAN.length}] ${item.track} / ${item.manifest} (cooldown=${cooldownForRun}s, skipBuild=${item.skipBuild})`);
    const t0 = Date.now();
    const exitInfo = await runChildIteration(iterArgs);
    const wallSec = Math.round((Date.now() - t0) / 1000);

    const foundReport = findNewReport(item.manifest, beforeReports);
    let parsedReport = null;
    if (foundReport) {
      try {
        parsedReport = JSON.parse(fs.readFileSync(foundReport.full, 'utf8'));
      } catch {}
    }

    const entry = {
      iteration: iterationNum,
      manifestId: item.manifest,
      track: item.track,
      cooldownSec: cooldownForRun,
      exitCode: exitInfo.code,
      signal: exitInfo.signal,
      wallSec,
      reportPath: foundReport ? foundReport.full : null,
      projectId: parsedReport?.projectId || null,
      projectName: parsedReport?.projectName || null,
      durationSec: parsedReport?.durationSec ?? null,
      totalFiles: parsedReport?.totalFiles ?? null,
      verify: parsedReport?.verify || null,
      runnerError: parsedReport?.runnerError || exitInfo.spawnError || null,
    };
    results.push(entry);

    console.log(
      `<<< [${iterationNum}/${PLAN.length}] ${item.manifest} exit=${exitInfo.code} wall=${wallSec}s` +
      (parsedReport
        ? ` | project=${parsedReport.projectId} | TS=${parsedReport.verify?.tscErrors} | nextBuild=${parsedReport.verify?.nextBuildOk} | expoExport=${parsedReport.verify?.expoExportOk} | routes=${parsedReport.verify?.routesPassed}/${(parsedReport.verify?.routesPassed || 0) + (parsedReport.verify?.routesFailed || 0)}`
        : ' | NO REPORT GENERATED')
    );

    if (fs.existsSync(STOP_FLAG)) {
      stoppedByFlag = true;
      console.log(`\n[9-pack] ⏹ Detektovan stop.flag nakon iteracije #${iterationNum} — prekidam kampanju.`);
      writeStatePatch(
        { status: 'idle', phase: 'stopped', cooldownSecondsRemaining: 0, message: `Kampanja zaustavljena nakon iteracije #${iterationNum}/${PLAN.length}.` },
        `⏹ Kampanja zaustavljena od strane korisnika nakon iteracije #${iterationNum}.`
      );
      fs.unlinkSync(STOP_FLAG);
      break;
    }
  }

  const campaignFinishedAt = new Date().toISOString();
  const summaryPath = path.join(STATE_DIR, `summary-9pack-${stamp}.json`);
  const summary = {
    campaignStartedAt,
    campaignFinishedAt,
    model: cfg.model,
    interRunCooldownSec: cfg.cooldown,
    plannedCount: PLAN.length,
    completedCount: results.length,
    stoppedByFlag,
    results,
  };
  fs.writeFileSync(summaryPath, JSON.stringify(summary, null, 2));

  if (!stoppedByFlag) {
    const okBuilds = results.filter((r) => {
      if (!r.verify) return false;
      return r.track === 'Mobile-Expo' ? Boolean(r.verify.expoExportOk) : Boolean(r.verify.nextBuildOk);
    }).length;
    writeStatePatch(
      {
        status: 'completed',
        phase: 'completed',
        progress: 100,
        cooldownSecondsRemaining: 0,
        message: `9-Pack kampanja završena (${results.length}/${PLAN.length}) — Build OK: ${okBuilds}/${results.length}.`,
      },
      `🏁 9-Pack kampanja završena! Summary: ${summaryPath}`
    );
  }

  console.log(`\n=============================================================`);
  console.log(`  9-PACK CAMPAIGN COMPLETE (${results.length}/${PLAN.length})`);
  console.log(`  Summary saved: ${summaryPath}`);
  console.log(`=============================================================\n`);
}

main().catch((err) => {
  console.error('[9-pack fatal]', err);
  process.exit(1);
});
