#!/usr/bin/env node
/**
 * cpu-thermal-guard.mjs
 * Ultra-low-overhead CPU temperature monitor & automatic thermal throttle guard
 * for Axiom Forge E2E test campaigns on Windows (AMD Ryzen / Intel).
 *
 * Behavior:
 *   - Reads CPU temperature once per second via:
 *       1) LibreHardwareMonitor Remote Web Server (http://127.0.0.1:8085/data.json) if enabled
 *       2) Otherwise ONE persistent PowerShell WMI worker reading root\LibreHardwareMonitor
 *          or root\OpenHardwareMonitor (zero per-second process spawning churn).
 *   - If CPU temperature exceeds threshold (default 95 °C) continuously for longer than
 *     sustainMs (default 5000ms), acquires scripts/e2e/.state/pause.flag with prefix
 *     THERMAL_AUTO_PAUSE: for pauseMs (default 30000ms), then releases only its own flag.
 *   - Never touches a manual pause.flag created by the user in Monitor UI.
 *
 * CLI modes:
 *   node scripts/e2e/cpu-thermal-guard.mjs --probe
 *   node scripts/e2e/cpu-thermal-guard.mjs --self-test
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const STATE_DIR = path.join(__dirname, '.state');
const STATE_FILE = path.join(STATE_DIR, 'state.json');
const THERMAL_FILE = path.join(STATE_DIR, 'thermal.json');
const PAUSE_FLAG = path.join(STATE_DIR, 'pause.flag');
const LHM_HTTP_URL = process.env.AXIOM_LHM_URL || 'http://127.0.0.1:8085/data.json';

export const THERMAL_PAUSE_PREFIX = 'THERMAL_AUTO_PAUSE:';

export function selectCpuTemperatureFromSensors(sensors) {
  if (!Array.isArray(sensors) || sensors.length === 0) return null;

  const valid = sensors
    .map((s) => ({
      name: String(s?.name || '').trim(),
      identifier: String(s?.identifier || '').trim(),
      parent: String(s?.parent || '').trim(),
      value: typeof s?.value === 'number' ? s.value : Number(s?.value),
    }))
    .filter((s) => Number.isFinite(s.value) && s.value > 0 && s.value < 130);

  if (valid.length === 0) return null;

  const cpuHw = valid.filter((s) => {
    const id = s.identifier.toLowerCase();
    const par = s.parent.toLowerCase();
    return (
      id.startsWith('/amdcpu/') ||
      id.startsWith('/intelcpu/') ||
      par.startsWith('/amdcpu/') ||
      par.startsWith('/intelcpu/')
    );
  });

  const candidates =
    cpuHw.length > 0
      ? cpuHw
      : valid.filter((s) => {
          const nm = s.name;
          const par = s.parent;
          if (/gpu|hdd|ssd|nvme|motherboard|chipset|vrm|system|ambient/i.test(nm)) return false;
          if (/gpu|hdd|nvme/i.test(par)) return false;
          return /\b(cpu|tdie|tctl)\b/i.test(nm);
        });

  if (candidates.length === 0) return null;

  const pickMax = (list) => {
    let best = list[0];
    for (let i = 1; i < list.length; i++) {
      if (list[i].value > best.value) best = list[i];
    }
    return best;
  };

  const buckets = [
    (s) => /\btdie\b/i.test(s.name) && !/tctl/i.test(s.name),
    (s) => /tctl\/tdie|tctl/i.test(s.name),
    (s) => /cpu\s*package|^package$/i.test(s.name),
    (s) => /cpu\s*die|cpu\s*core|core\s*average|core\s*max/i.test(s.name),
  ];

  let chosen = null;
  for (const pred of buckets) {
    const matched = candidates.filter(pred);
    if (matched.length > 0) {
      chosen = pickMax(matched);
      break;
    }
  }
  if (!chosen) {
    chosen = pickMax(candidates);
  }

  return {
    tempC: Math.round(chosen.value * 10) / 10,
    sensor: chosen.name || chosen.identifier || 'CPU',
    identifier: chosen.identifier || null,
  };
}

function parseNumericTempString(raw) {
  if (typeof raw === 'number') return Number.isFinite(raw) ? raw : null;
  if (typeof raw !== 'string') return null;
  const trimmed = raw.trim();
  if (!trimmed || /\b(mhz|ghz|rpm|kb|mb|gb)\b/i.test(trimmed)) return null;
  if (/^\s*[-+]?\d+(?:[.,]\d+)?\s*(%|w|v)\s*$/i.test(trimmed)) return null;

  const m = trimmed.match(/([-+]?\d+(?:[.,]\d+)?)/);
  if (!m) return null;
  let num = Number(m[1].replace(',', '.'));
  if (!Number.isFinite(num)) return null;
  if (/°\s*f\b/i.test(trimmed)) {
    num = ((num - 32) * 5) / 9;
  }
  return num;
}

export function parseLhmHttpTree(rootNode) {
  if (!rootNode || typeof rootNode !== 'object') return null;
  const collected = [];

  function walk(node, ctx) {
    if (!node || typeof node !== 'object') return;
    const text = String(node.Text || '');
    const img = String(node.ImageURL || '').toLowerCase();
    const hwId = String(node.HardwareId || '');
    const sensorId = String(node.SensorId || '');
    const children = Array.isArray(node.Children) ? node.Children : [];

    let nextHwKind = ctx.hwKind;
    let nextHwName = ctx.hwName;

    const lowerId = hwId.toLowerCase();
    if (lowerId.startsWith('/amdcpu/') || img.includes('cpu.png')) {
      nextHwKind = 'cpu-amd';
      nextHwName = text || nextHwName;
    } else if (lowerId.startsWith('/intelcpu/')) {
      nextHwKind = 'cpu-intel';
      nextHwName = text || nextHwName;
    } else if (
      img.includes('nvidia.png') ||
      img.includes('ati.png') ||
      img.includes('gpu') ||
      img.includes('hdd.png') ||
      img.includes('mainboard.png') ||
      img.includes('ram.png') ||
      img.includes('nic.png') ||
      lowerId.startsWith('/gpu-') ||
      lowerId.startsWith('/nvme/') ||
      lowerId.startsWith('/hdd/') ||
      lowerId.startsWith('/lpc/')
    ) {
      nextHwKind = 'other';
      nextHwName = text || nextHwName;
    } else if (!nextHwKind && /\b(ryzen|epyc|threadripper|athlon)\b/i.test(text) && !/gpu|radeon|geforce|nvidia/i.test(text)) {
      nextHwKind = 'cpu-amd';
      nextHwName = text;
    } else if (!nextHwKind && /\bintel\b.*\b(core|xeon|pentium|celeron)\b/i.test(text) && !/graphics|arc|uhd|iris/i.test(text)) {
      nextHwKind = 'cpu-intel';
      nextHwName = text;
    }

    const inTempGroup =
      ctx.inTempGroup ||
      /^temperatures?$/i.test(text) ||
      img.includes('temperature.png') ||
      node.Type === 'Temperature';

    if (children.length === 0) {
      const valStr = typeof node.Value === 'string' ? node.Value : String(node.Value ?? '');
      const looksLikeTemp =
        node.Type === 'Temperature' ||
        ctx.inTempGroup ||
        /°\s*[CF]/i.test(valStr);
      if (looksLikeTemp) {
        const val = parseNumericTempString(valStr);
        if (val !== null) {
          const parentId =
            sensorId ||
            (nextHwKind === 'cpu-amd'
              ? '/amdcpu/http'
              : nextHwKind === 'cpu-intel'
              ? '/intelcpu/http'
              : nextHwName || hwId || 'other');
          collected.push({
            name: text,
            identifier: sensorId,
            parent: parentId,
            value: val,
          });
        }
      }
      return;
    }

    const childCtx = {
      hwKind: nextHwKind,
      hwName: nextHwName,
      inTempGroup,
    };
    for (const child of children) {
      walk(child, childCtx);
    }
  }

  walk(rootNode, { hwKind: '', hwName: '', inTempGroup: false });
  return selectCpuTemperatureFromSensors(collected);
}

export function createInitialThermalState() {
  return {
    overheatStartedAt: null,
    thermalPauseUntil: null,
    thermalPauseActive: false,
    thermalPauseCount: 0,
    maxObservedTempC: null,
  };
}

export function evaluateThermalStep(prevState, sample, cfg = {}) {
  const thresholdC = typeof cfg.thresholdC === 'number' ? cfg.thresholdC : 95;
  const sustainMs = typeof cfg.sustainMs === 'number' ? cfg.sustainMs : 5000;
  const pauseMs = typeof cfg.pauseMs === 'number' ? cfg.pauseMs : 30000;

  const nowMs = sample?.nowMs ?? Date.now();
  const tempC =
    typeof sample?.tempC === 'number' && Number.isFinite(sample.tempC) ? sample.tempC : null;

  const state = {
    overheatStartedAt: prevState?.overheatStartedAt ?? null,
    thermalPauseUntil: prevState?.thermalPauseUntil ?? null,
    thermalPauseActive: Boolean(prevState?.thermalPauseActive),
    thermalPauseCount: Number(prevState?.thermalPauseCount || 0),
    maxObservedTempC:
      typeof prevState?.maxObservedTempC === 'number' ? prevState.maxObservedTempC : null,
  };

  if (tempC !== null) {
    state.maxObservedTempC =
      state.maxObservedTempC === null ? tempC : Math.max(state.maxObservedTempC, tempC);
  }

  if (state.thermalPauseActive) {
    const until = state.thermalPauseUntil ?? nowMs;
    if (nowMs >= until) {
      state.thermalPauseActive = false;
      state.thermalPauseUntil = null;
      state.overheatStartedAt = null;
      return { state, action: 'resume', remainingSec: 0, overheatElapsedSec: 0 };
    }
    const remainingSec = Math.max(1, Math.ceil((until - nowMs) / 1000));
    return { state, action: 'holding_pause', remainingSec, overheatElapsedSec: 0 };
  }

  if (tempC !== null && tempC > thresholdC) {
    if (state.overheatStartedAt === null) {
      state.overheatStartedAt = nowMs;
      return { state, action: 'overheating', remainingSec: 0, overheatElapsedSec: 0 };
    }
    const elapsedMs = nowMs - state.overheatStartedAt;
    if (elapsedMs >= sustainMs) {
      state.thermalPauseActive = true;
      state.thermalPauseUntil = nowMs + pauseMs;
      state.thermalPauseCount += 1;
      state.overheatStartedAt = null;
      return {
        state,
        action: 'trigger_pause',
        remainingSec: Math.ceil(pauseMs / 1000),
        overheatElapsedSec: Math.round(elapsedMs / 100) / 10,
      };
    }
    return {
      state,
      action: 'overheating',
      remainingSec: 0,
      overheatElapsedSec: Math.round(elapsedMs / 100) / 10,
    };
  }

  state.overheatStartedAt = null;
  return { state, action: 'normal', remainingSec: 0, overheatElapsedSec: 0 };
}

export function applyPauseFlag(pauseFlagPath, detail = '') {
  if (fs.existsSync(pauseFlagPath)) {
    return { acquired: false, alreadyPaused: true };
  }
  fs.mkdirSync(path.dirname(pauseFlagPath), { recursive: true });
  const payload = `${THERMAL_PAUSE_PREFIX}${new Date().toISOString()}${detail ? `|${detail}` : ''}`;
  fs.writeFileSync(pauseFlagPath, payload, 'utf8');
  return { acquired: true, alreadyPaused: false };
}

export function releaseThermalPauseFlag(pauseFlagPath) {
  if (!fs.existsSync(pauseFlagPath)) {
    return { released: false, reason: 'missing' };
  }
  try {
    const content = fs.readFileSync(pauseFlagPath, 'utf8');
    if (!content.startsWith(THERMAL_PAUSE_PREFIX)) {
      return { released: false, reason: 'manual_pause_preserved' };
    }
    fs.unlinkSync(pauseFlagPath);
    return { released: true, reason: 'released' };
  } catch (err) {
    return { released: false, reason: String(err?.message || err) };
  }
}

function appendStateLog(logLine) {
  try {
    if (!logLine) return;
    fs.mkdirSync(STATE_DIR, { recursive: true });
    let cur = { logs: [], history: [] };
    if (fs.existsSync(STATE_FILE)) {
      cur = JSON.parse(fs.readFileSync(STATE_FILE, 'utf8'));
    }
    const logs = Array.isArray(cur.logs) ? cur.logs.slice(-140) : [];
    const ts = new Date().toLocaleTimeString('sr-RS', { hour12: false });
    logs.push(`[${ts}] ${logLine}`);
    cur.logs = logs;
    cur.updatedAt = new Date().toISOString();
    fs.writeFileSync(STATE_FILE, JSON.stringify(cur, null, 2));
  } catch {}
}

function writeThermalFile(payload) {
  try {
    fs.mkdirSync(STATE_DIR, { recursive: true });
    fs.writeFileSync(THERMAL_FILE, JSON.stringify(payload, null, 2));
  } catch {}
}

function encodePsCommand(script) {
  return Buffer.from(script, 'utf16le').toString('base64');
}

const SINGLE_PROBE_PS = `
$ErrorActionPreference = 'SilentlyContinue'
foreach ($ns in @('root\\LibreHardwareMonitor', 'root\\OpenHardwareMonitor')) {
  $sensors = Get-CimInstance -Namespace $ns -ClassName Sensor -Filter "SensorType='Temperature'"
  if ($sensors) {
    $arr = @($sensors | ForEach-Object {
      [pscustomobject]@{
        name = [string]$_.Name
        identifier = [string]$_.Identifier
        parent = [string]$_.Parent
        value = [double]$_.Value
      }
    })
    @{ ok = $true; source = $ns; sensors = $arr } | ConvertTo-Json -Compress -Depth 3
    exit 0
  }
}
Write-Output '{"ok":false,"source":"none","sensors":[]}'
`.trim();

const PERSISTENT_WMI_PS = `
$ErrorActionPreference = 'SilentlyContinue'
while ($true) {
  $found = $false
  foreach ($ns in @('root\\LibreHardwareMonitor', 'root\\OpenHardwareMonitor')) {
    $sensors = Get-CimInstance -Namespace $ns -ClassName Sensor -Filter "SensorType='Temperature'"
    if ($sensors) {
      $arr = @($sensors | ForEach-Object {
        [pscustomobject]@{
          name = [string]$_.Name
          identifier = [string]$_.Identifier
          parent = [string]$_.Parent
          value = [double]$_.Value
        }
      })
      $out = @{ ok = $true; source = $ns; sensors = $arr } | ConvertTo-Json -Compress -Depth 3
      [Console]::Out.WriteLine($out)
      [Console]::Out.Flush()
      $found = $true
      break
    }
  }
  if (-not $found) {
    [Console]::Out.WriteLine('{"ok":false,"source":"none","sensors":[]}')
    [Console]::Out.Flush()
    Start-Sleep -Milliseconds 3000
  } else {
    Start-Sleep -Milliseconds 1000
  }
}
`.trim();

async function readViaHttp(url = LHM_HTTP_URL, timeoutMs = 800) {
  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
    if (!res.ok) return null;
    const json = await res.json();
    const picked = parseLhmHttpTree(json);
    if (!picked) return null;
    return {
      ok: true,
      source: 'lhm-http:8085',
      ...picked,
    };
  } catch {
    return null;
  }
}

function readViaSingleWmiProbe(timeoutMs = 6000) {
  return new Promise((resolve) => {
    const child = spawn(
      'powershell.exe',
      ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', encodePsCommand(SINGLE_PROBE_PS)],
      { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true }
    );
    let out = '';
    const timer = setTimeout(() => {
      try { child.kill(); } catch {}
      resolve({ ok: false, source: 'wmi-timeout', tempC: null, sensor: null });
    }, timeoutMs);

    child.stdout.on('data', (chunk) => { out += chunk.toString(); });
    child.on('exit', () => {
      clearTimeout(timer);
      const lines = out.trim().split(/\r?\n/).filter(Boolean);
      const last = lines[lines.length - 1] || '';
      try {
        const parsed = JSON.parse(last);
        const rawSensors = Array.isArray(parsed.sensors)
          ? parsed.sensors
          : parsed.sensors
          ? [parsed.sensors]
          : [];
        if (parsed.ok && rawSensors.length > 0) {
          const picked = selectCpuTemperatureFromSensors(rawSensors);
          if (picked) {
            resolve({ ok: true, source: parsed.source, ...picked });
            return;
          }
        }
      } catch {}
      resolve({ ok: false, source: 'none', tempC: null, sensor: null });
    });
    child.on('error', () => {
      clearTimeout(timer);
      resolve({ ok: false, source: 'ps-error', tempC: null, sensor: null });
    });
  });
}

export async function probeCpuTemperature() {
  const httpRes = await readViaHttp();
  if (httpRes) return httpRes;
  if (process.platform === 'win32') {
    return await readViaSingleWmiProbe();
  }
  return { ok: false, source: 'unsupported-os', tempC: null, sensor: null };
}

export function createCpuThermalGuard(options = {}) {
  const thresholdC = Number(options.thresholdC ?? 95);
  const sustainMs = Number(options.sustainMs ?? 5000);
  const pauseMs = Number(options.pauseMs ?? 30000);
  const pollIntervalMs = Number(options.pollIntervalMs ?? 1000);
  const httpUrl = options.httpUrl || LHM_HTTP_URL;
  const pauseFlagPath = options.pauseFlagPath || PAUSE_FLAG;

  let active = false;
  let stopped = false;
  let state = createInitialThermalState();
  let lastTempC = null;
  let lastSensor = null;
  let lastSource = 'initializing';
  let lastReadAt = null;
  let sensorEverSeen = false;
  let wmiChild = null;
  let httpTimer = null;
  let tickBusy = false;

  function persistThermalSnapshot(extra = {}) {
    writeThermalFile({
      active,
      thresholdC,
      sustainSec: Math.round(sustainMs / 1000),
      pauseSec: Math.round(pauseMs / 1000),
      tempC: lastTempC,
      sensor: lastSensor,
      source: lastSource,
      maxObservedTempC: state.maxObservedTempC,
      thermalPaused: state.thermalPauseActive,
      thermalPauseRemainingSec:
        state.thermalPauseActive && state.thermalPauseUntil
          ? Math.max(0, Math.ceil((state.thermalPauseUntil - Date.now()) / 1000))
          : 0,
      thermalPauseCount: state.thermalPauseCount,
      overheatElapsedSec:
        state.overheatStartedAt !== null
          ? Math.round((Date.now() - state.overheatStartedAt) / 100) / 10
          : 0,
      lastReadAt,
      updatedAt: new Date().toISOString(),
      ...extra,
    });
  }

  function handleReading(reading) {
    if (stopped) return;
    const nowMs = Date.now();
    if (reading && typeof reading.tempC === 'number' && Number.isFinite(reading.tempC)) {
      lastTempC = reading.tempC;
      lastSensor = reading.sensor || 'CPU';
      lastSource = reading.source || lastSource;
      lastReadAt = new Date(nowMs).toISOString();
      if (!sensorEverSeen) {
        sensorEverSeen = true;
        const msg = `🌡 Povezan CPU senzor (${lastSource}): ${lastSensor} = ${lastTempC.toFixed(1)} °C | limit >${thresholdC} °C (${Math.round(sustainMs / 1000)}s -> pauza ${Math.round(pauseMs / 1000)}s)`;
        console.log(`[thermal-guard] ${msg}`);
        appendStateLog(msg);
      }
    } else {
      lastSource = reading?.source || 'none';
    }

    const res = evaluateThermalStep(
      state,
      { nowMs, tempC: reading?.tempC ?? null },
      { thresholdC, sustainMs, pauseMs }
    );
    state = res.state;

    if (res.action === 'trigger_pause') {
      const detail = `${(lastTempC ?? thresholdC).toFixed(1)}C`;
      const flagRes = applyPauseFlag(pauseFlagPath, detail);
      const msg = flagRes.acquired
        ? `🔥 CPU ${detail} > ${thresholdC} °C duže od ${Math.round(sustainMs / 1000)}s — automatska termalna pauza ${Math.round(pauseMs / 1000)}s!`
        : `🔥 CPU ${detail} > ${thresholdC} °C (već pauzirano ručno — čekam ${Math.round(pauseMs / 1000)}s hlađenja).`;
      console.log(`\n[thermal-guard] ${msg}`);
      appendStateLog(msg);
    } else if (res.action === 'resume') {
      const rel = releaseThermalPauseFlag(pauseFlagPath);
      const curStr = lastTempC !== null ? `${lastTempC.toFixed(1)} °C` : 'n/a';
      const msg = rel.released
        ? `❄ Termalna pauza (30s) završena — CPU sada ${curStr}, nastavljam testiranje.`
        : rel.reason === 'manual_pause_preserved'
        ? `❄ Termalni tajmer istekao (CPU ${curStr}), ali je ručna pauza aktivna — čekam korisnika.`
        : `❄ Termalni prozor završen (CPU ${curStr}).`;
      console.log(`[thermal-guard] ${msg}`);
      appendStateLog(msg);
    }

    persistThermalSnapshot();
  }

  function startPersistentWmiWorker() {
    if (stopped || wmiChild || process.platform !== 'win32') return;
    wmiChild = spawn(
      'powershell.exe',
      ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', encodePsCommand(PERSISTENT_WMI_PS)],
      { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true }
    );

    let buf = '';
    wmiChild.stdout.on('data', (chunk) => {
      if (stopped) return;
      buf += chunk.toString();
      const lines = buf.split(/\r?\n/);
      buf = lines.pop() || '';
      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed) continue;
        try {
          const parsed = JSON.parse(trimmed);
          const rawSensors = Array.isArray(parsed.sensors)
            ? parsed.sensors
            : parsed.sensors
            ? [parsed.sensors]
            : [];
          if (parsed.ok && rawSensors.length > 0) {
            const picked = selectCpuTemperatureFromSensors(rawSensors);
            if (picked) {
              handleReading({ source: parsed.source, ...picked });
              continue;
            }
          }
          handleReading({ source: parsed.source || 'none', tempC: null, sensor: null });
        } catch {}
      }
    });

    wmiChild.on('exit', () => {
      wmiChild = null;
      if (!stopped && !httpTimer) {
        setTimeout(() => {
          if (!stopped && !httpTimer) startPersistentWmiWorker();
        }, 3000);
      }
    });
  }

  function startHttpPolling() {
    if (stopped || httpTimer) return;
    let consecutiveFailures = 0;
    httpTimer = setInterval(async () => {
      if (stopped || tickBusy) return;
      tickBusy = true;
      try {
        const res = await readViaHttp(httpUrl, 800);
        if (res) {
          consecutiveFailures = 0;
          handleReading(res);
        } else {
          consecutiveFailures += 1;
          handleReading({ source: 'http-offline', tempC: null, sensor: null });
          if (consecutiveFailures >= 3) {
            clearInterval(httpTimer);
            httpTimer = null;
            startPersistentWmiWorker();
          }
        }
      } finally {
        tickBusy = false;
      }
    }, pollIntervalMs);
  }

  return {
    async start() {
      if (active) return await probeCpuTemperature();
      active = true;
      stopped = false;
      persistThermalSnapshot();

      const initial = await probeCpuTemperature();
      if (initial.ok) {
        handleReading(initial);
      } else {
        lastSource = initial.source || 'none';
        persistThermalSnapshot();
      }

      if (initial.source === 'lhm-http:8085') {
        startHttpPolling();
      } else {
        startPersistentWmiWorker();
      }

      return initial;
    },

    async stop() {
      if (stopped) return;
      stopped = true;
      active = false;
      if (httpTimer) {
        clearInterval(httpTimer);
        httpTimer = null;
      }
      if (wmiChild) {
        try { wmiChild.kill(); } catch {}
        wmiChild = null;
      }
      releaseThermalPauseFlag(pauseFlagPath);
      state.thermalPauseActive = false;
      state.thermalPauseUntil = null;
      state.overheatStartedAt = null;
      persistThermalSnapshot({ active: false });
    },

    getStats() {
      return {
        thresholdC,
        sustainSec: Math.round(sustainMs / 1000),
        pauseSec: Math.round(pauseMs / 1000),
        sensorDetected: sensorEverSeen,
        lastSource,
        lastSensor,
        lastTempC,
        maxObservedTempC: state.maxObservedTempC,
        thermalPauseCount: state.thermalPauseCount,
      };
    },
  };
}

function runSelfTest() {
  // 1. Sensor selection priority (Ryzen Tdie over Tctl and non-CPU sensors)
  const sampleSensors = [
    { name: 'GPU Core', identifier: '/gpu-nvidia/0/temperature/0', parent: '/gpu-nvidia/0', value: 42.0 },
    { name: 'Temperature #1', identifier: '/lpc/it8686e/temperature/0', parent: '/lpc/it8686e', value: 33.0 },
    { name: 'CPU', identifier: '/lpc/it8686e/temperature/2', parent: '/lpc/it8686e', value: 49.0 },
    { name: 'Core (Tctl/Tdie)', identifier: '/amdcpu/0/temperature/0', parent: '/amdcpu/0', value: 64.5 },
    { name: 'Core (Tdie)', identifier: '/amdcpu/0/temperature/1', parent: '/amdcpu/0', value: 54.5 },
  ];
  const picked = selectCpuTemperatureFromSensors(sampleSensors);
  assert.equal(picked?.tempC, 54.5);
  assert.equal(picked?.sensor, 'Core (Tdie)');

  // 2. Fallback to Tctl/Tdie when only that exists
  const onlyTctl = selectCpuTemperatureFromSensors([
    { name: 'GPU Core', identifier: '/gpu-nvidia/0/temperature/0', parent: '/gpu-nvidia/0', value: 60.0 },
    { name: 'Core (Tctl/Tdie)', identifier: '/amdcpu/0/temperature/0', parent: '/amdcpu/0', value: 78.2 },
  ]);
  assert.equal(onlyTctl?.tempC, 78.2);

  // 3. No CPU sensor -> returns null (never mistakes GPU/NVMe for CPU)
  const gpuOnly = selectCpuTemperatureFromSensors([
    { name: 'GPU Core', identifier: '/gpu-nvidia/0/temperature/0', parent: '/gpu-nvidia/0', value: 70.0 },
    { name: 'Assembly', identifier: '/hdd/0/temperature/0', parent: '/hdd/0', value: 40.0 },
  ]);
  assert.equal(gpuOnly, null);

  // 4. LHM HTTP data.json tree parser (comma decimal & GPU ignored)
  const mockLhmTree = {
    Text: 'Sensor',
    Children: [
      {
        Text: 'DESKTOP-PC',
        ImageURL: 'images_icon/computer.png',
        Children: [
          {
            Text: 'AMD Ryzen 5 2600X',
            ImageURL: 'images_icon/cpu.png',
            Children: [
              {
                Text: 'Temperatures',
                ImageURL: 'images_icon/temperature.png',
                Children: [
                  { Text: 'Core (Tctl/Tdie)', Value: '63,4 °C' },
                  { Text: 'Core (Tdie)', Value: '53,4 °C' },
                ],
              },
            ],
          },
          {
            Text: 'NVIDIA GeForce GTX 1060 6GB',
            ImageURL: 'images_icon/nvidia.png',
            Children: [
              {
                Text: 'Temperatures',
                ImageURL: 'images_icon/temperature.png',
                Children: [{ Text: 'GPU Core', Value: '98.0 °C' }],
              },
            ],
          },
        ],
      },
    ],
  };
  const lhmPicked = parseLhmHttpTree(mockLhmTree);
  assert.equal(lhmPicked?.tempC, 53.4);
  assert.equal(lhmPicked?.sensor, 'Core (Tdie)');

  // 5. Thermal state machine: <= 5s spike resets; > 5s (> threshold) triggers 30s pause, then resumes
  const cfg = { thresholdC: 95, sustainMs: 5000, pauseMs: 30000 };
  let st = createInitialThermalState();
  let step = evaluateThermalStep(st, { nowMs: 1000, tempC: 96.0 }, cfg);
  st = step.state;
  assert.equal(step.action, 'overheating');
  step = evaluateThermalStep(st, { nowMs: 4000, tempC: 97.2 }, cfg);
  st = step.state;
  assert.equal(step.action, 'overheating');
  // Brief drop below 95 resets sustained timer
  step = evaluateThermalStep(st, { nowMs: 4900, tempC: 94.8 }, cfg);
  st = step.state;
  assert.equal(step.action, 'normal');
  assert.equal(st.thermalPauseCount, 0);

  // Now sustained >= 5000ms (> 95°C)
  step = evaluateThermalStep(st, { nowMs: 10000, tempC: 95.5 }, cfg);
  st = step.state;
  assert.equal(step.action, 'overheating');
  step = evaluateThermalStep(st, { nowMs: 14900, tempC: 96.1 }, cfg);
  st = step.state;
  assert.equal(step.action, 'overheating');
  step = evaluateThermalStep(st, { nowMs: 15100, tempC: 96.3 }, cfg);
  st = step.state;
  assert.equal(step.action, 'trigger_pause');
  assert.equal(st.thermalPauseActive, true);
  assert.equal(st.thermalPauseCount, 1);
  assert.equal(st.maxObservedTempC, 97.2);

  // Holds during 30s window even if temp already dropped
  step = evaluateThermalStep(st, { nowMs: 30000, tempC: 72.0 }, cfg);
  st = step.state;
  assert.equal(step.action, 'holding_pause');
  assert.equal(step.remainingSec, 16);

  // Resumes cleanly at >= 15100 + 30000 = 45100
  step = evaluateThermalStep(st, { nowMs: 45100, tempC: 70.0 }, cfg);
  st = step.state;
  assert.equal(step.action, 'resume');
  assert.equal(st.thermalPauseActive, false);

  // 6. pause.flag ownership: preserves manual pause, releases only THERMAL_AUTO_PAUSE:
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'axiom-thermal-test-'));
  try {
    const testFlag = path.join(tmpDir, 'pause.flag');
    // Manual pause exists first -> guard must NOT overwrite or delete it
    fs.writeFileSync(testFlag, '2026-11-24T17:00:00Z', 'utf8');
    assert.deepEqual(applyPauseFlag(testFlag, '96.5C'), { acquired: false, alreadyPaused: true });
    assert.deepEqual(releaseThermalPauseFlag(testFlag), { released: false, reason: 'manual_pause_preserved' });
    assert.equal(fs.existsSync(testFlag), true);
    fs.unlinkSync(testFlag);

    // Thermal pause acquires and cleans up its own flag
    const acq = applyPauseFlag(testFlag, '96.5C');
    assert.equal(acq.acquired, true);
    assert.ok(fs.readFileSync(testFlag, 'utf8').startsWith(THERMAL_PAUSE_PREFIX));
    assert.deepEqual(releaseThermalPauseFlag(testFlag), { released: true, reason: 'released' });
    assert.equal(fs.existsSync(testFlag), false);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }

  console.log('cpu-thermal-guard --self-test: ALL 6 ASSERTION SUITES PASSED');
}

const isCli = process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href;
if (isCli) {
  const args = new Set(process.argv.slice(2));
  if (args.has('--self-test')) {
    runSelfTest();
    process.exit(0);
  } else if (args.has('--probe')) {
    probeCpuTemperature()
      .then((res) => {
        console.log(JSON.stringify(res, null, 2));
        process.exit(res.ok ? 0 : 2);
      })
      .catch((err) => {
        console.error(err);
        process.exit(1);
      });
  } else {
    console.log('Usage: node scripts/e2e/cpu-thermal-guard.mjs [--probe | --self-test]');
  }
}
