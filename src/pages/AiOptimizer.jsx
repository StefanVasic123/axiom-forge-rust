import React, { useState, useEffect, useCallback } from 'react';
import {
  Activity,
  Brain,
  CheckCircle2,
  Cpu,
  Database,
  Download,
  RefreshCw,
  ShieldCheck,
  Sliders,
  Sparkles,
  TrendingUp,
  TrendingDown,
  Copy,
  Check,
  AlertTriangle,
  Layers,
  GitBranch,
  ChevronRight,
  Code2,
} from 'lucide-react';
import { useI18n } from '../i18n/I18nContext';

function formatDelta(val) {
  const num = Number(val || 0);
  if (num > 0) return `+${num.toFixed(1)}%`;
  if (num < 0) return `${num.toFixed(1)}%`;
  return '0.0%';
}

function AccuracyTrendSvg({ timeline }) {
  const measured = (timeline || []).filter((pt) => pt.telemetryStatus !== 'unmeasured');
  if (measured.length === 0) {
    return (
      <div className="h-52 flex items-center justify-center text-slate-500 text-sm border border-dashed border-slate-800 rounded-xl">
        No measured project cohorts yet. Generate a project to populate your accuracy trajectory.
      </div>
    );
  }

  const width = 760;
  const height = 220;
  const padL = 44;
  const padR = 20;
  const padT = 20;
  const padB = 34;
  const plotW = width - padL - padR;
  const plotH = height - padT - padB;

  const pts = measured.map((pt, idx) => {
    const x =
      measured.length === 1
        ? padL + plotW / 2
        : padL + (idx / (measured.length - 1)) * plotW;
    const yExp = padT + (1 - Math.min(100, Math.max(0, pt.exponentialIndexPct)) / 100) * plotH;
    const yRaw = padT + (1 - Math.min(100, Math.max(0, pt.rawAccuracyPct)) / 100) * plotH;
    return { ...pt, x, yExp, yRaw };
  });

  const expPath = pts.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x.toFixed(1)} ${p.yExp.toFixed(1)}`).join(' ');
  const rawPath = pts.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x.toFixed(1)} ${p.yRaw.toFixed(1)}`).join(' ');
  const areaPath =
    pts.length > 0
      ? `${expPath} L ${pts[pts.length - 1].x.toFixed(1)} ${(padT + plotH).toFixed(1)} L ${pts[0].x.toFixed(1)} ${(padT + plotH).toFixed(1)} Z`
      : '';

  return (
    <div className="w-full overflow-x-auto">
      <svg viewBox={`0 0 ${width} ${height}`} className="w-full h-56 select-none">
        <defs>
          <linearGradient id="axiomExpArea" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#6366f1" stopOpacity="0.35" />
            <stop offset="100%" stopColor="#6366f1" stopOpacity="0.02" />
          </linearGradient>
        </defs>

        {[0, 25, 50, 75, 100].map((tick) => {
          const y = padT + (1 - tick / 100) * plotH;
          return (
            <g key={tick}>
              <line
                x1={padL}
                y1={y}
                x2={width - padR}
                y2={y}
                stroke="#1e293b"
                strokeDasharray="4 4"
                strokeWidth="1"
              />
              <text
                x={padL - 8}
                y={y + 4}
                textAnchor="end"
                className="fill-slate-500 text-[10px] font-mono"
              >
                {tick}%
              </text>
            </g>
          );
        })}

        {areaPath && <path d={areaPath} fill="url(#axiomExpArea)" />}

        {pts.length > 1 && (
          <>
            <path
              d={rawPath}
              fill="none"
              stroke="#34d399"
              strokeWidth="2"
              strokeDasharray="5 4"
            />
            <path d={expPath} fill="none" stroke="#818cf8" strokeWidth="3" />
          </>
        )}

        {pts.map((p) => (
          <g key={p.version}>
            <circle cx={p.x} cy={p.yRaw} r="4" fill="#10b981" stroke="#0f172a" strokeWidth="1.5" />
            <circle cx={p.x} cy={p.yExp} r="5.5" fill="#6366f1" stroke="#e0e7ff" strokeWidth="2" />
            <text
              x={p.x}
              y={Math.max(12, p.yExp - 10)}
              textAnchor="middle"
              className="fill-indigo-200 text-[10px] font-mono font-semibold"
            >
              {p.exponentialIndexPct}%
            </text>
            <text
              x={p.x}
              y={height - 10}
              textAnchor="middle"
              className="fill-slate-400 text-[10px] font-mono"
            >
              {p.version}
            </text>
          </g>
        ))}
      </svg>

      <div className="flex items-center justify-end gap-6 mt-2 text-xs text-slate-400">
        <span className="flex items-center gap-2">
          <span className="w-3 h-0.5 bg-indigo-400 inline-block" />
          Exponential Index (I^exp)
        </span>
        <span className="flex items-center gap-2">
          <span className="w-3 h-0.5 bg-emerald-400 border-t border-dashed border-emerald-400 inline-block" />
          Raw Cohort Accuracy (A_t)
        </span>
      </div>
    </div>
  );
}

export default function AiOptimizer() {
  const { t } = useI18n();
  const [telemetry, setTelemetry] = useState(null);
  const [selectedModel, setSelectedModel] = useState('all');
  const [loading, setLoading] = useState(true);
  const [errorMsg, setErrorMsg] = useState('');

  const [config, setConfig] = useState({ aggressiveness: 0.8, balance: 0.5 });
  const [customDirective, setCustomDirective] = useState('');
  const [saveNotice, setSaveNotice] = useState('');

  const [baseModelForSft, setBaseModelForSft] = useState('qwen2.5-coder:3b');
  const [exporting, setExporting] = useState(false);
  const [exportResult, setExportResult] = useState(null);
  const [copiedCmd, setCopiedCmd] = useState(false);

  const [policyRegistry, setPolicyRegistry] = useState(null);
  const [selectedAgentId, setSelectedAgentId] = useState('all');
  const [selectedRuleId, setSelectedRuleId] = useState('manifest.required-auth-routes');

  const loadAll = useCallback(async (modelFilter = selectedModel) => {
    setLoading(true);
    setErrorMsg('');
    try {
      const api = window.electronAPI?.aiOptimizer;
      if (!api) {
        throw new Error('AI Optimizer telemetry bridge is unavailable.');
      }
      const [telRes, cfgRes, regRes] = await Promise.all([
        api.getTelemetry(modelFilter === 'all' ? null : modelFilter),
        api.getConfig(),
        api.getPolicyRegistry ? api.getPolicyRegistry().catch(() => null) : Promise.resolve(null),
      ]);
      setTelemetry(telRes);
      if (regRes) {
        setPolicyRegistry(regRes);
      }
      if (cfgRes) {
        setConfig({
          aggressiveness: Number(cfgRes.aggressiveness ?? 0.8),
          balance: Number(cfgRes.balance ?? 0.5),
        });
        setCustomDirective(String(cfgRes.customDirective || ''));
      }
    } catch (err) {
      setErrorMsg(String(err?.message || err));
    } finally {
      setLoading(false);
    }
  }, [selectedModel]);

  useEffect(() => {
    loadAll(selectedModel);
  }, [selectedModel, loadAll]);

  const handleSaveOptimizerConfig = async (nextConfig, nextDirective) => {
    try {
      await window.electronAPI?.aiOptimizer?.saveConfig({
        ...nextConfig,
        customDirective: nextDirective,
      });
      setSaveNotice(t('aiOptimizer.saveDirectiveSuccess', 'Optimizer preferences saved locally.'));
      setTimeout(() => setSaveNotice(''), 2500);
    } catch (err) {
      setErrorMsg(String(err?.message || err));
    }
  };

  const handleExportTraining = async () => {
    setExporting(true);
    setErrorMsg('');
    try {
      const res = await window.electronAPI?.aiOptimizer?.exportTrainingDataset(null, baseModelForSft);
      setExportResult(res);
    } catch (err) {
      setErrorMsg(String(err?.message || err));
    } finally {
      setExporting(false);
    }
  };

  const handleCopyOllamaCommand = async () => {
    if (!exportResult?.ollamaCommand) return;
    try {
      await navigator.clipboard.writeText(exportResult.ollamaCommand);
      setCopiedCmd(true);
      setTimeout(() => setCopiedCmd(false), 2000);
    } catch {
      // Ignore clipboard failures on headless shells
    }
  };

  const summary = telemetry?.summary || {};
  const timeline = telemetry?.versionsTimeline || [];
  const domains = telemetry?.domainBreakdown || [];
  const projects = telemetry?.projects || [];
  const availableModels = telemetry?.availableModels || [];

  const deltaPrevNum = Number(summary.deltaVsPrevPct || 0);
  const deltaBestNum = Number(summary.deltaVsBestPct || 0);

  const registryAgents = policyRegistry?.agents || [];
  const registryRules = policyRegistry?.rules || [];
  const filteredRules =
    selectedAgentId === 'all'
      ? registryRules
      : registryRules.filter((r) => r.agentStage === selectedAgentId);
  const activeRule =
    filteredRules.find((r) => r.id === selectedRuleId) || filteredRules[0] || registryRules[0] || null;

  return (
    <div className="max-w-7xl mx-auto pb-16 space-y-6">
      {/* Header & Model Filter */}
      <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4 bg-slate-900/50 border border-slate-800 rounded-2xl p-6">
        <div>
          <div className="flex items-center gap-2.5">
            <Brain className="w-6 h-6 text-indigo-400" />
            <h1 className="text-2xl font-bold text-white">
              {t('aiOptimizer.title', 'AI Model Progress & Local Trainer')}
            </h1>
            {telemetry?.currentAppVersion && (
              <span className="px-2.5 py-0.5 text-xs font-mono rounded-full bg-indigo-500/20 text-indigo-300 border border-indigo-500/30">
                v{telemetry.currentAppVersion}
              </span>
            )}
          </div>
          <p className="text-slate-400 text-sm mt-1">
            {t(
              'aiOptimizer.subtitle',
              'Exponential accuracy tracking across Axiom versions, LLM domain strengths/weaknesses, and mini-LLM fine-tuning dataset export.'
            )}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-2 bg-slate-950/80 border border-slate-700 rounded-xl px-3 py-2">
            <Cpu className="w-4 h-4 text-slate-400" />
            <select
              value={selectedModel}
              onChange={(e) => setSelectedModel(e.target.value)}
              className="bg-transparent text-sm text-slate-200 focus:outline-none font-mono"
            >
              <option value="all" className="bg-slate-900">
                {t('aiOptimizer.allModels', 'All Local Models')}
              </option>
              {availableModels.map((m) => (
                <option key={m} value={m} className="bg-slate-900">
                  {m}
                </option>
              ))}
            </select>
          </div>

          <button
            onClick={() => loadAll(selectedModel)}
            disabled={loading}
            className="flex items-center gap-2 px-4 py-2 rounded-xl bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white text-sm font-medium transition-colors"
          >
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
            {t('common.refresh', 'Refresh')}
          </button>
        </div>
      </div>

      {errorMsg && (
        <div className="bg-rose-500/10 border border-rose-500/30 rounded-xl p-4 text-rose-300 text-sm flex items-center gap-3">
          <AlertTriangle className="w-5 h-5 shrink-0" />
          <span>{errorMsg}</span>
        </div>
      )}

      {/* KPI Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-5 gap-4">
        <div className="bg-slate-900/60 border border-indigo-500/30 rounded-2xl p-5">
          <p className="text-xs uppercase tracking-wider text-slate-400">
            {t('aiOptimizer.expIndex', 'Exponential Accuracy Index')}
          </p>
          <p className="text-3xl font-bold text-indigo-300 font-mono mt-2">
            {Number(summary.exponentialIndexPct || 0).toFixed(1)}%
          </p>
          <p className="text-xs text-slate-500 mt-1">
            Raw: {Number(summary.rawAccuracyPct || 0).toFixed(1)}% ({summary.version || '—'})
          </p>
        </div>

        <div className="bg-slate-900/60 border border-emerald-500/30 rounded-2xl p-5">
          <p className="text-xs uppercase tracking-wider text-slate-400">
            {t('aiOptimizer.cleanFeatures', '100%-Clean Feature Rate')}
          </p>
          <p className="text-3xl font-bold text-emerald-400 font-mono mt-2">
            {Number(summary.f100Pct || 0).toFixed(1)}%
          </p>
          <p className="text-xs text-slate-500 mt-1">
            Projects: {telemetry?.totalProjects ?? 0}
          </p>
        </div>

        <div className="bg-slate-900/60 border border-amber-500/30 rounded-2xl p-5">
          <p className="text-xs uppercase tracking-wider text-slate-400">
            {t('aiOptimizer.avgBugScope', 'Mean Bug Scope')}
          </p>
          <p className="text-3xl font-bold text-amber-300 font-mono mt-2">
            {Number(summary.bugScopePct || 0).toFixed(1)}%
          </p>
          <p className="text-xs text-slate-500 mt-1">
            Healthy files: {(100 - Number(summary.bugScopePct || 0)).toFixed(1)}%
          </p>
        </div>

        <div className="bg-slate-900/60 border border-slate-800 rounded-2xl p-5">
          <p className="text-xs uppercase tracking-wider text-slate-400">
            Growth Δ
          </p>
          <div className="mt-2 space-y-1">
            <div className="flex items-center justify-between text-sm font-mono">
              <span className="text-slate-400 text-xs">{t('aiOptimizer.vsPrev', 'vs Prev')}:</span>
              <span className={`flex items-center gap-1 font-semibold ${deltaPrevNum >= 0 ? 'text-emerald-400' : 'text-rose-400'}`}>
                {deltaPrevNum >= 0 ? <TrendingUp className="w-3.5 h-3.5" /> : <TrendingDown className="w-3.5 h-3.5" />}
                {formatDelta(deltaPrevNum)}
              </span>
            </div>
            <div className="flex items-center justify-between text-sm font-mono">
              <span className="text-slate-400 text-xs">{t('aiOptimizer.vsBest', 'vs Best')}:</span>
              <span className={`font-semibold ${deltaBestNum >= 0 ? 'text-indigo-300' : 'text-amber-400'}`}>
                {formatDelta(deltaBestNum)}
              </span>
            </div>
          </div>
        </div>

        <div className="bg-slate-900/60 border border-slate-800 rounded-2xl p-5">
          <p className="text-xs uppercase tracking-wider text-slate-400">
            {t('aiOptimizer.katastarVerified', 'Verified Katastar Rules')}
          </p>
          <p className="text-3xl font-bold text-cyan-300 font-mono mt-2">
            {telemetry?.katastarVerifiedEntries ?? 0}
          </p>
          <p className="text-xs text-slate-500 mt-1">
            Total learned: {telemetry?.katastarTotalEntries ?? 0}
          </p>
        </div>
      </div>

      {/* Accuracy Trajectory Chart + Mathematical Formula */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2 bg-slate-900/40 border border-slate-800 rounded-2xl p-6">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-lg font-semibold text-white flex items-center gap-2">
              <Activity className="w-5 h-5 text-indigo-400" />
              {t('aiOptimizer.chartTitle', 'Version-by-Version Accuracy Trajectory')}
            </h2>
          </div>
          <AccuracyTrendSvg timeline={timeline} />
        </div>

        <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-6 flex flex-col justify-between">
          <div>
            <h2 className="text-lg font-semibold text-white flex items-center gap-2 mb-2">
              <Sparkles className="w-5 h-5 text-amber-400" />
              {t('aiOptimizer.equationTitle', 'Exponential Accuracy Growth Equation')}
            </h2>
            <p className="text-xs text-slate-400 leading-relaxed mb-4">
              {t(
                'aiOptimizer.equationDesc',
                'Combines first-pass 100%-clean feature rate (F100), untouched file scope (1 - b_p), error density attenuation (phi_p), historical deltas, and recency-weighted exponential compounding.'
              )}
            </p>
            <div className="bg-slate-950/90 border border-slate-800 rounded-xl p-4 font-mono text-xs text-indigo-200 space-y-2 overflow-x-auto">
              <div>A_t = (0.55·F100 + 0.45·(1 - b_p)) · (1 / (1 + 0.25·μ_p))</div>
              <div>Δ_prev = A_t − A_(t-1)   |   Δ_best = A_t − max_(j&lt;t) A_j</div>
              <div>EMA_t = α·A_t + (1 - α)·EMA_(t-1)   (α = 0.45)</div>
              <div className="text-emerald-300 font-semibold">
                I^exp_t = min(1, EMA_t + 0.35 · max(0, Δ_best) · (1 − EMA_t)) · 100%
              </div>
            </div>
          </div>

          <div className="mt-4 pt-4 border-t border-slate-800 grid grid-cols-2 gap-3 text-xs">
            <div className="bg-slate-950/60 p-3 rounded-xl border border-slate-800/80">
              <span className="text-slate-500 block">First-Pass Clean Weight</span>
              <span className="text-white font-mono text-sm">w₁ = 0.55</span>
            </div>
            <div className="bg-slate-950/60 p-3 rounded-xl border border-slate-800/80">
              <span className="text-slate-500 block">Healthy File Scope</span>
              <span className="text-white font-mono text-sm">w₂ = 0.45</span>
            </div>
          </div>
        </div>
      </div>

      {/* Domain Strength & Weakness Map + Mini-LLM Trainer */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-6">
          <h2 className="text-lg font-semibold text-white flex items-center gap-2 mb-4">
            <Layers className="w-5 h-5 text-emerald-400" />
            {t('aiOptimizer.domainAnalysisTitle', 'Error Frequency (učestalost_greske) — LLM Strength & Weakness Map')}
          </h2>

          <div className="space-y-4">
            {domains.map((dom) => {
              const strength = Number(dom.strengthScore || 0);
              const barColor =
                strength >= 80
                  ? 'bg-emerald-500'
                  : strength >= 55
                  ? 'bg-amber-500'
                  : 'bg-rose-500';
              return (
                <div key={dom.domain} className="space-y-1.5">
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-slate-200 font-medium">{dom.domain}</span>
                    <div className="flex items-center gap-3 font-mono text-xs">
                      <span className="text-slate-400">
                        err: {dom.totalErrors} ({dom.errorSharePct}%)
                      </span>
                      <span className="text-cyan-300">
                        heal: {dom.healSuccessPct}%
                      </span>
                      <span className="text-white font-semibold w-14 text-right">
                        {strength.toFixed(0)}/100
                      </span>
                    </div>
                  </div>
                  <div className="w-full h-2.5 bg-slate-950 rounded-full overflow-hidden border border-slate-800">
                    <div
                      className={`h-full ${barColor} transition-all duration-300`}
                      style={{ width: `${Math.min(100, Math.max(4, strength))}%` }}
                    />
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Mini-LLM Trainer */}
        <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-6 flex flex-col justify-between">
          <div>
            <h2 className="text-lg font-semibold text-white flex items-center gap-2 mb-2">
              <Database className="w-5 h-5 text-cyan-400" />
              {t('aiOptimizer.trainerTitle', 'Mini-LLM SFT Dataset & Modelfile Trainer')}
            </h2>
            <p className="text-sm text-slate-400 mb-5">
              {t(
                'aiOptimizer.trainerDesc',
                'Export verified Katastar fixes, heal-ledger pairs, and Axiom Stack Rulepacks as scrubbed JSONL + Ollama Modelfile to specialize a fast local coding model.'
              )}
            </p>

            <div className="space-y-4">
              <div>
                <label className="block text-xs uppercase tracking-wider text-slate-400 mb-1.5">
                  Base Local Ollama Model (FROM)
                </label>
                <select
                  value={baseModelForSft}
                  onChange={(e) => setBaseModelForSft(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-700 rounded-xl px-3.5 py-2.5 text-sm text-slate-200 font-mono focus:outline-none focus:border-indigo-500"
                >
                  <option value="qwen2.5-coder:3b">qwen2.5-coder:3b (Recommended fast 3B)</option>
                  <option value="qwen2.5-coder:7b">qwen2.5-coder:7b (Balanced 7B)</option>
                  <option value="qwen2.5-coder:1.5b">qwen2.5-coder:1.5b (Ultra-light mini-LLM)</option>
                  <option value="llama3.2:3b">llama3.2:3b (General 3B)</option>
                  <option value="llama3.2:1b">llama3.2:1b (Edge 1B)</option>
                </select>
              </div>

              <button
                onClick={handleExportTraining}
                disabled={exporting}
                className="w-full flex items-center justify-center gap-2 px-4 py-3 rounded-xl bg-gradient-to-r from-indigo-600 to-cyan-600 hover:from-indigo-500 hover:to-cyan-500 disabled:opacity-50 text-white font-semibold text-sm transition-all shadow-lg shadow-indigo-900/30"
              >
                <Download className="w-4 h-4" />
                {exporting
                  ? t('aiOptimizer.exporting', 'Exporting...')
                  : t('aiOptimizer.exportBtn', 'Generate SFT Dataset & Modelfile')}
              </button>
            </div>
          </div>

          {exportResult && (
            <div className="mt-5 bg-slate-950/90 border border-emerald-500/30 rounded-xl p-4 space-y-2 text-xs">
              <div className="flex items-center justify-between text-emerald-300 font-semibold">
                <span className="flex items-center gap-1.5">
                  <ShieldCheck className="w-4 h-4" />
                  Exported {exportResult.sampleCount} Scrubbed Training Pairs
                </span>
              </div>
              <div className="font-mono text-slate-400 break-all">
                JSONL: {exportResult.datasetPath}
              </div>
              <div className="font-mono text-slate-400 break-all">
                Modelfile: {exportResult.modelfilePath}
              </div>
              <div className="flex items-center justify-between gap-2 bg-slate-900 border border-slate-800 rounded-lg p-2.5 mt-2">
                <code className="text-indigo-300 font-mono text-[11px] truncate">
                  {exportResult.ollamaCommand}
                </code>
                <button
                  onClick={handleCopyOllamaCommand}
                  className="shrink-0 p-1.5 rounded-md bg-slate-800 hover:bg-slate-700 text-slate-200"
                  title="Copy command"
                >
                  {copiedCmd ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Release Cohort Breakdown Table */}
      <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-6 overflow-x-auto">
        <h2 className="text-lg font-semibold text-white mb-4">
          {t('aiOptimizer.versionTableTitle', 'Release Cohort Breakdown')}
        </h2>
        <table className="w-full text-left border-collapse text-sm">
          <thead>
            <tr className="border-b border-slate-800 text-slate-400 text-xs uppercase">
              <th className="py-2.5 px-3">Version</th>
              <th className="py-2.5 px-3">Projects</th>
              <th className="py-2.5 px-3">Clean Builds</th>
              <th className="py-2.5 px-3">F100 Clean</th>
              <th className="py-2.5 px-3">Bug Scope</th>
              <th className="py-2.5 px-3">Raw Acc</th>
              <th className="py-2.5 px-3">Exp Index</th>
              <th className="py-2.5 px-3">Δ Prev</th>
              <th className="py-2.5 px-3">Δ Best</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800/60 font-mono text-xs">
            {timeline.length === 0 ? (
              <tr>
                <td colSpan={9} className="py-6 text-center text-slate-500 font-sans">
                  No release telemetry recorded yet.
                </td>
              </tr>
            ) : (
              timeline.map((row) => {
                const isUnmeasured = row.telemetryStatus === 'unmeasured';
                return (
                  <tr key={row.version} className="hover:bg-slate-800/30">
                    <td className="py-2.5 px-3 text-indigo-300 font-semibold">
                      <span>{row.version}</span>
                      {isUnmeasured && (
                        <span className="ml-2 px-1.5 py-0.5 text-[10px] rounded bg-slate-800 text-slate-400 font-normal">
                          unmeasured
                        </span>
                      )}
                    </td>
                    <td className="py-2.5 px-3 text-slate-300">{row.projectCount}</td>
                    <td className="py-2.5 px-3 text-slate-300">
                      {isUnmeasured ? '—' : `${row.cleanProjectsCount}/${row.projectCount}`}
                    </td>
                    <td className="py-2.5 px-3 text-emerald-300">{isUnmeasured ? '—' : `${row.f100Pct}%`}</td>
                    <td className="py-2.5 px-3 text-amber-300">{isUnmeasured ? '—' : `${row.bugScopePct}%`}</td>
                    <td className="py-2.5 px-3 text-slate-200">{isUnmeasured ? '—' : `${row.rawAccuracyPct}%`}</td>
                    <td className="py-2.5 px-3 text-white font-bold">{isUnmeasured ? '—' : `${row.exponentialIndexPct}%`}</td>
                    <td className={`py-2.5 px-3 ${isUnmeasured ? 'text-slate-500' : row.deltaVsPrevPct >= 0 ? 'text-emerald-400' : 'text-rose-400'}`}>
                      {isUnmeasured ? '—' : formatDelta(row.deltaVsPrevPct)}
                    </td>
                    <td className={`py-2.5 px-3 ${isUnmeasured ? 'text-slate-500' : row.deltaVsBestPct >= 0 ? 'text-cyan-300' : 'text-amber-400'}`}>
                      {isUnmeasured ? '—' : formatDelta(row.deltaVsBestPct)}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      {/* Generated Projects Table */}
      <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-6 overflow-x-auto">
        <h2 className="text-lg font-semibold text-white mb-4">
          {t('aiOptimizer.projectsTableTitle', 'Generated Projects Telemetry')}
        </h2>
        <table className="w-full text-left border-collapse text-sm">
          <thead>
            <tr className="border-b border-slate-800 text-slate-400 text-xs uppercase">
              <th className="py-2.5 px-3">Project</th>
              <th className="py-2.5 px-3">Version</th>
              <th className="py-2.5 px-3">Model</th>
              <th className="py-2.5 px-3">Files</th>
              <th className="py-2.5 px-3">1st-Pass Clean</th>
              <th className="py-2.5 px-3">Bug Scope</th>
              <th className="py-2.5 px-3">Healed</th>
              <th className="py-2.5 px-3">Accuracy</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800/60 text-xs">
            {projects.map((p) => {
              const isUnmeasured = p.telemetryStatus === 'unmeasured';
              const isInProgress = p.telemetryStatus === 'in_progress';
              return (
                <tr key={p.id} className="hover:bg-slate-800/30">
                  <td className="py-2.5 px-3 text-slate-200 font-medium">
                    <span>{p.name}</span>
                    {isUnmeasured && (
                      <span className="ml-2 px-1.5 py-0.5 text-[10px] rounded bg-slate-800 text-slate-400">
                        unmeasured
                      </span>
                    )}
                    {isInProgress && (
                      <span className="ml-2 px-1.5 py-0.5 text-[10px] rounded bg-indigo-500/20 text-indigo-300">
                        in progress
                      </span>
                    )}
                    {p.hasPrismaFailure && (
                      <span className="ml-2 px-1.5 py-0.5 text-[10px] rounded bg-rose-500/20 text-rose-300">
                        prisma failed
                      </span>
                    )}
                  </td>
                  <td className="py-2.5 px-3 font-mono text-indigo-300">v{p.appVersion}</td>
                  <td className="py-2.5 px-3 font-mono text-slate-400">{p.modelId}</td>
                  <td className="py-2.5 px-3 font-mono text-slate-300">{p.totalFiles}</td>
                  <td className="py-2.5 px-3 font-mono text-emerald-300">{isUnmeasured ? '—' : `${p.firstPassCleanRatePct}%`}</td>
                  <td className="py-2.5 px-3 font-mono text-amber-300">{isUnmeasured ? '—' : `${p.bugScopePct}%`}</td>
                  <td className="py-2.5 px-3 font-mono text-cyan-300">{isUnmeasured ? '—' : p.healedIssuesCount}</td>
                  <td className="py-2.5 px-3 font-mono text-white font-semibold">
                    {isUnmeasured || isInProgress ? '—' : `${p.accuracyPct}%`}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {/* Interactive Agent Pipeline & Policy Registry (ruleId) Inspector */}
      {registryAgents.length > 0 && (
        <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-6 space-y-5">
          <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
            <div>
              <h2 className="text-lg font-semibold text-white flex items-center gap-2">
                <GitBranch className="w-5 h-5 text-indigo-400" />
                {t('aiOptimizer.policyRegistryTitle', 'Pipeline Orchestrator & Policy Registry (ruleId) Inspector')}
              </h2>
              <p className="text-xs text-slate-400 mt-1">
                Click an execution stage or rule to inspect unified prompt directives, pre-save guards, Rust deterministic sanitizers, and Katastar verification stats.
              </p>
            </div>
            <div className="flex items-center gap-2 text-xs font-mono">
              <button
                onClick={() => setSelectedAgentId('all')}
                className={`px-3 py-1.5 rounded-lg border transition-colors ${
                  selectedAgentId === 'all'
                    ? 'bg-indigo-600/30 border-indigo-500 text-indigo-200'
                    : 'bg-slate-950/60 border-slate-800 text-slate-400 hover:text-slate-200'
                }`}
              >
                All Rules ({policyRegistry?.totalRules ?? registryRules.length})
              </button>
            </div>
          </div>

          {/* Clickable 6-Stage Pipeline Graph */}
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-6 gap-2.5">
            {registryAgents.map((agent) => {
              const isSelected = selectedAgentId === agent.id;
              return (
                <button
                  key={agent.id}
                  onClick={() => {
                    const nextId = isSelected ? 'all' : agent.id;
                    setSelectedAgentId(nextId);
                    if (nextId !== 'all' && agent.ruleIds?.length > 0) {
                      setSelectedRuleId(agent.ruleIds[0]);
                    }
                  }}
                  className={`text-left p-3.5 rounded-xl border transition-all ${
                    isSelected
                      ? 'bg-indigo-600/20 border-indigo-400 shadow-lg shadow-indigo-950/40'
                      : 'bg-slate-950/60 border-slate-800 hover:border-slate-700'
                  }`}
                >
                  <div className="flex items-center justify-between text-[10px] font-mono text-slate-400 mb-1">
                    <span>STAGE {agent.stageOrder}</span>
                    <span className="px-1.5 py-0.5 rounded bg-slate-900 text-indigo-300">
                      {agent.ruleCount} rules
                    </span>
                  </div>
                  <div className="text-xs font-semibold text-white leading-snug">{agent.name}</div>
                  <div className="mt-2 flex items-center justify-between text-[10px] font-mono text-slate-400">
                    <span>Katastar: {agent.katastarMatches}</span>
                    <span className="text-emerald-400">Verified: {agent.verifiedFixes}</span>
                  </div>
                </button>
              );
            })}
          </div>

          {/* Selected Agent Summary Banner when filtered */}
          {selectedAgentId !== 'all' && (() => {
            const ag = registryAgents.find((a) => a.id === selectedAgentId);
            if (!ag) return null;
            return (
              <div className="bg-slate-950/70 border border-indigo-500/30 rounded-xl px-4 py-3 flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2 text-xs">
                <div>
                  <span className="text-indigo-300 font-semibold">{ag.name}:</span>{' '}
                  <span className="text-slate-300">{ag.description}</span>
                </div>
                <span className="font-mono text-[11px] text-cyan-300 shrink-0">{ag.modulePath}</span>
              </div>
            );
          })()}

          {/* Rules List + Rule Detail Inspector */}
          <div className="grid grid-cols-1 lg:grid-cols-12 gap-4">
            <div className="lg:col-span-5 space-y-2 max-h-[420px] overflow-y-auto pr-1">
              {filteredRules.map((rule) => {
                const isRuleActive = activeRule?.id === rule.id;
                const sevBadge =
                  rule.severity === 'error'
                    ? 'bg-rose-500/20 text-rose-300 border-rose-500/30'
                    : rule.severity === 'warning'
                    ? 'bg-amber-500/20 text-amber-300 border-amber-500/30'
                    : 'bg-cyan-500/20 text-cyan-300 border-cyan-500/30';
                return (
                  <button
                    key={rule.id}
                    onClick={() => setSelectedRuleId(rule.id)}
                    className={`w-full text-left p-3 rounded-xl border transition-all flex items-start justify-between gap-2 ${
                      isRuleActive
                        ? 'bg-slate-900 border-indigo-500'
                        : 'bg-slate-950/60 border-slate-800/80 hover:border-slate-700'
                    }`}
                  >
                    <div className="min-w-0">
                      <div className="flex items-center gap-2 flex-wrap">
                        <span className="font-mono text-[11px] text-indigo-300 font-semibold truncate">
                          {rule.id}
                        </span>
                        <span className={`px-1.5 py-0.5 text-[10px] font-mono rounded border ${sevBadge}`}>
                          {rule.severity}
                        </span>
                      </div>
                      <div className="text-xs text-slate-200 font-medium mt-1 truncate">{rule.title}</div>
                      <div className="text-[10px] text-slate-500 mt-1 font-mono">
                        {rule.domain} · {rule.katastarMatches} Katastar hits
                      </div>
                    </div>
                    <ChevronRight className={`w-4 h-4 shrink-0 mt-1 ${isRuleActive ? 'text-indigo-400' : 'text-slate-600'}`} />
                  </button>
                );
              })}
            </div>

            {activeRule && (
              <div className="lg:col-span-7 bg-slate-950/80 border border-slate-800 rounded-xl p-5 space-y-4">
                <div className="flex flex-wrap items-start justify-between gap-3 border-b border-slate-800 pb-3">
                  <div>
                    <div className="flex items-center gap-2 flex-wrap">
                      <Code2 className="w-4 h-4 text-indigo-400" />
                      <span className="font-mono text-sm font-bold text-indigo-300">{activeRule.id}</span>
                      <span className="px-2 py-0.5 text-[10px] font-mono rounded bg-slate-800 text-slate-300">
                        {activeRule.agentStageLabel}
                      </span>
                    </div>
                    <h3 className="text-sm font-semibold text-white mt-1.5">{activeRule.title}</h3>
                  </div>
                  <div className="flex items-center gap-2 font-mono text-[11px]">
                    <span className="px-2 py-1 rounded bg-slate-900 border border-slate-800 text-slate-300">
                      Hits: {activeRule.katastarMatches}
                    </span>
                    <span className="px-2 py-1 rounded bg-emerald-500/10 border border-emerald-500/30 text-emerald-300">
                      Verified: {activeRule.verifiedFixes}
                    </span>
                  </div>
                </div>

                <div className="space-y-3 text-xs">
                  <div>
                    <span className="text-[10px] uppercase tracking-wider text-slate-400 block mb-1">
                      Target File Roles
                    </span>
                    <div className="flex flex-wrap gap-1.5">
                      {(activeRule.appliesToRoles || []).map((role) => (
                        <span key={role} className="px-2 py-0.5 rounded bg-slate-900 border border-slate-700 font-mono text-[11px] text-cyan-300">
                          {role}
                        </span>
                      ))}
                    </div>
                  </div>

                  <div>
                    <span className="text-[10px] uppercase tracking-wider text-slate-400 block mb-1">
                      1. Single-Source Prompt Directive
                    </span>
                    <div className="bg-slate-900/90 border border-slate-800 rounded-lg p-3 text-slate-200 leading-relaxed font-mono text-[11px]">
                      {activeRule.promptDirective}
                    </div>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                      <span className="text-[10px] uppercase tracking-wider text-slate-400 block mb-1">
                        2. Pre-Save Reviewer Check
                      </span>
                      <div className="bg-slate-900/70 border border-slate-800 rounded-lg p-3 text-slate-300 leading-relaxed">
                        {activeRule.validatorSummary}
                      </div>
                    </div>
                    <div>
                      <span className="text-[10px] uppercase tracking-wider text-slate-400 block mb-1">
                        3. Deterministic Sanitizer Function
                      </span>
                      <div className="bg-slate-900/70 border border-slate-800 rounded-lg p-3 font-mono text-[11px] text-emerald-300 break-all">
                        {activeRule.sanitizerFnName}
                      </div>
                    </div>
                  </div>

                  <div>
                    <span className="text-[10px] uppercase tracking-wider text-slate-400 block mb-1">
                      4. Auto-Heal Stagnation Directive
                    </span>
                    <div className="bg-amber-500/5 border border-amber-500/20 rounded-lg p-3 text-amber-200 leading-relaxed">
                      {activeRule.healDirective}
                    </div>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Local Inference Tuning & Directives */}
      <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-6 space-y-6">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold text-white flex items-center gap-2">
            <Sliders className="w-5 h-5 text-indigo-400" />
            {t('aiOptimizer.tuningTitle', 'Local AI Inference Directives & Tuning')}
          </h2>
          {saveNotice && (
            <span className="flex items-center gap-1.5 text-xs text-emerald-400 font-medium">
              <CheckCircle2 className="w-4 h-4" />
              {saveNotice}
            </span>
          )}
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div>
            <div className="flex justify-between items-center mb-2">
              <label className="text-slate-300 text-sm font-medium">Context Compression Aggressiveness</label>
              <span className="text-indigo-400 text-sm font-mono">{config.aggressiveness}</span>
            </div>
            <input
              type="range"
              min="0"
              max="1"
              step="0.1"
              value={config.aggressiveness}
              onChange={(e) => {
                const next = { ...config, aggressiveness: parseFloat(e.target.value) };
                setConfig(next);
                handleSaveOptimizerConfig(next, customDirective);
              }}
              className="w-full accent-indigo-500 bg-slate-800 rounded-lg appearance-none h-2"
            />
          </div>

          <div>
            <div className="flex justify-between items-center mb-2">
              <label className="text-slate-300 text-sm font-medium">Balance (Speed vs Strictness)</label>
              <span className="text-indigo-400 text-sm font-mono">{config.balance}</span>
            </div>
            <input
              type="range"
              min="0"
              max="1"
              step="0.1"
              value={config.balance}
              onChange={(e) => {
                const next = { ...config, balance: parseFloat(e.target.value) };
                setConfig(next);
                handleSaveOptimizerConfig(next, customDirective);
              }}
              className="w-full accent-indigo-500 bg-slate-800 rounded-lg appearance-none h-2"
            />
          </div>
        </div>

        <div>
          <label className="block text-slate-300 text-sm font-medium mb-2">Custom Architecture Directive</label>
          <textarea
            className="w-full h-28 bg-slate-950 text-slate-200 p-4 rounded-xl border border-slate-700 focus:border-indigo-500 focus:outline-none font-mono text-sm resize-y"
            placeholder="e.g. Always isolate Next-Intl locale routing before Auth middleware and keep RootLayout shell-only..."
            value={customDirective}
            onChange={(e) => setCustomDirective(e.target.value)}
            onBlur={() => handleSaveOptimizerConfig(config, customDirective)}
          />
          <div className="flex justify-end mt-3">
            <button
              onClick={() => handleSaveOptimizerConfig(config, customDirective)}
              className="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold transition-colors"
            >
              {t('common.save', 'Save')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
