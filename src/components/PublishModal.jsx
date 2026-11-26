import React, { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  X,
  Rocket,
  Github,
  Database,
  Globe,
  ShieldCheck,
  CheckCircle2,
  AlertCircle,
  Loader2,
  ExternalLink,
  Copy,
  Check,
  RefreshCw,
  Settings,
  Lock,
  Unlock,
  KeyRound,
  Cloud,
  Sparkles,
  ChevronDown,
  ChevronUp
} from 'lucide-react';

const NEON_REGIONS = [
  { id: 'aws-eu-central-1', label: 'Europe (Frankfurt) — aws-eu-central-1' },
  { id: 'aws-us-east-2', label: 'US East (Ohio) — aws-us-east-2' },
  { id: 'aws-us-west-2', label: 'US West (Oregon) — aws-us-west-2' },
  { id: 'aws-ap-southeast-1', label: 'Asia Pacific (Singapore) — aws-ap-southeast-1' }
];

const PIPELINE_STEPS = [
  { key: 'prepare', label: 'Prepare' },
  { key: 'github', label: 'GitHub' },
  { key: 'database', label: 'Neon DB' },
  { key: 'prisma', label: 'Prisma' },
  { key: 'vercel', label: 'Vercel' },
  { key: 'deploy', label: 'Build' },
  { key: 'domain', label: 'Domain' }
];

function TokenChip({ label, ok, required = false }) {
  return (
    <span
      className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border ${
        ok
          ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300'
          : required
          ? 'bg-rose-500/10 border-rose-500/30 text-rose-300'
          : 'bg-slate-800/80 border-slate-700 text-slate-400'
      }`}
    >
      {ok ? (
        <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
      ) : (
        <AlertCircle className={`w-3.5 h-3.5 ${required ? 'text-rose-400' : 'text-slate-500'}`} />
      )}
      {label}
      {!ok && required && <span className="opacity-75">(Required)</span>}
    </span>
  );
}

export default function PublishModal({ isOpen, onClose, project, onPublished }) {
  const navigate = useNavigate();

  const [preflight, setPreflight] = useState(null);
  const [loadingPreflight, setLoadingPreflight] = useState(false);
  const [preflightError, setPreflightError] = useState(null);

  // Form state
  const [repoName, setRepoName] = useState('');
  const [isPrivateRepo, setIsPrivateRepo] = useState(false);
  const [dbMode, setDbMode] = useState('neon'); // 'neon' | 'custom_url' | 'skip'
  const [neonRegion, setNeonRegion] = useState('aws-eu-central-1');
  const [customDatabaseUrl, setCustomDatabaseUrl] = useState('');
  const [runPrismaPush, setRunPrismaPush] = useState(true);
  const [syncLocalEnvKeys, setSyncLocalEnvKeys] = useState(true);
  const [customDomain, setCustomDomain] = useState('');
  const [autoCloudflareDns, setAutoCloudflareDns] = useState(false);
  const [vercelTeamId, setVercelTeamId] = useState('');
  const [showAdvanced, setShowAdvanced] = useState(false);

  // Execution state
  const [isPublishing, setIsPublishing] = useState(false);
  const [progress, setProgress] = useState({ step: 'prepare', percent: 0, message: '' });
  const [publishResult, setPublishResult] = useState(null);
  const [publishError, setPublishError] = useState(null);

  // Domain verification state
  const [isVerifyingDomain, setIsVerifyingDomain] = useState(false);
  const [domainVerifyInfo, setDomainVerifyInfo] = useState(null);
  const [copiedKey, setCopiedKey] = useState(null);

  useEffect(() => {
    if (!isOpen || !project?.id) return;

    let cancelled = false;
    setLoadingPreflight(true);
    setPreflightError(null);
    setPublishError(null);
    setPublishResult(null);
    setDomainVerifyInfo(null);

    window.electronAPI.publish
      .getPreflight(project.id)
      .then((data) => {
        if (cancelled) return;
        setPreflight(data);
        setRepoName(data.suggestedRepoName || '');
        if (!data.hasPrisma) {
          setDbMode('skip');
        } else if (data.hasNeonToken) {
          setDbMode('neon');
        } else {
          setDbMode('custom_url');
        }
        if (data.existingState?.customDomain) {
          setCustomDomain(data.existingState.customDomain);
        } else {
          setCustomDomain('');
        }
        setAutoCloudflareDns(Boolean(data.hasCloudflareToken && data.existingState?.customDomain));
      })
      .catch((err) => {
        if (cancelled) return;
        setPreflightError(typeof err === 'string' ? err : err?.message || 'Preflight check failed');
      })
      .finally(() => {
        if (!cancelled) setLoadingPreflight(false);
      });

    return () => {
      cancelled = true;
    };
  }, [isOpen, project?.id]);

  useEffect(() => {
    if (!isOpen || !project?.id) return;
    const unsubscribe = window.electronAPI.publish.onProgress((payload) => {
      if (payload?.projectId === project.id) {
        setProgress({
          step: payload.step || 'prepare',
          percent: typeof payload.percent === 'number' ? payload.percent : 0,
          message: payload.message || ''
        });
      }
    });
    return () => {
      if (typeof unsubscribe === 'function') unsubscribe();
    };
  }, [isOpen, project?.id]);

  if (!isOpen || !project) return null;

  const missingRequiredTokens = preflight && (!preflight.hasGithubToken || !preflight.hasVercelToken);
  const needsNeonToken = preflight?.hasPrisma && dbMode === 'neon' && !preflight?.hasNeonToken;
  const invalidCustomDb =
    preflight?.hasPrisma &&
    dbMode === 'custom_url' &&
    !customDatabaseUrl.trim().startsWith('postgres://') &&
    !customDatabaseUrl.trim().startsWith('postgresql://');

  const canStart =
    !loadingPreflight &&
    !isPublishing &&
    preflight &&
    !missingRequiredTokens &&
    !needsNeonToken &&
    !invalidCustomDb &&
    repoName.trim().length > 0;

  const handleCopy = async (text, key) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedKey(key);
      setTimeout(() => setCopiedKey(null), 1600);
    } catch (e) {
      console.warn('Clipboard copy failed:', e);
    }
  };

  const handlePublish = async () => {
    if (!canStart) return;
    setIsPublishing(true);
    setPublishError(null);
    setPublishResult(null);
    setDomainVerifyInfo(null);
    setProgress({ step: 'prepare', percent: 5, message: 'Starting 1-Click Full-Stack Publish...' });

    try {
      const result = await window.electronAPI.publish.runFullStack({
        projectId: project.id,
        repoName: repoName.trim(),
        isPrivateRepo,
        dbMode: preflight.hasPrisma ? dbMode : 'skip',
        customDatabaseUrl: dbMode === 'custom_url' ? customDatabaseUrl.trim() : null,
        neonRegion,
        runPrismaPush: preflight.hasPrisma && dbMode !== 'skip' && runPrismaPush,
        customDomain: customDomain.trim() || null,
        autoCloudflareDns: Boolean(customDomain.trim() && autoCloudflareDns && preflight.hasCloudflareToken),
        syncLocalEnvKeys,
        vercelTeamId: vercelTeamId.trim() || null
      });
      setPublishResult(result);
      if (typeof onPublished === 'function') {
        onPublished(result);
      }
    } catch (err) {
      const msg = typeof err === 'string' ? err : err?.message || String(err);
      setPublishError(msg);
    } finally {
      setIsPublishing(false);
    }
  };

  const handleVerifyDomain = async (domainOverride = null) => {
    const targetDomain = (domainOverride || customDomain || preflight?.existingState?.customDomain || '').trim();
    if (!targetDomain) return;
    setIsVerifyingDomain(true);
    try {
      const res = await window.electronAPI.publish.verifyDomain(
        project.id,
        targetDomain,
        vercelTeamId.trim() || null
      );
      setDomainVerifyInfo(res);
      if (publishResult) {
        setPublishResult((prev) =>
          prev
            ? {
                ...prev,
                domainVerified: res.verified,
                dnsRecords: res.dnsRecords?.length ? res.dnsRecords : prev.dnsRecords
              }
            : prev
        );
      }
      if (typeof onPublished === 'function') {
        onPublished(res);
      }
    } catch (err) {
      setDomainVerifyInfo({
        success: false,
        verified: false,
        misconfigured: true,
        domain: targetDomain,
        dnsRecords: [],
        message: typeof err === 'string' ? err : err?.message || String(err)
      });
    } finally {
      setIsVerifyingDomain(false);
    }
  };

  const openExternal = (url) => {
    if (!url) return;
    window.electronAPI.shell.openExternal(url);
  };

  const currentStepIdx = PIPELINE_STEPS.findIndex((s) => s.key === progress.step);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4">
      <div className="w-full max-w-2xl max-h-[90vh] flex flex-col rounded-2xl bg-slate-900 border border-slate-700/80 shadow-2xl overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-900/90">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-xl bg-indigo-500/15 border border-indigo-500/30 flex items-center justify-center">
              <Rocket className="w-5 h-5 text-indigo-400" />
            </div>
            <div>
              <h2 className="text-lg font-semibold text-white">1-Click Full-Stack Publish</h2>
              <p className="text-xs text-slate-400">
                GitHub + PostgreSQL (Neon) + Prisma Push + Vercel + Custom Domain
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            disabled={isPublishing}
            className="p-2 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors disabled:opacity-40"
            title="Close"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-6 py-5 space-y-5">
          {loadingPreflight ? (
            <div className="flex items-center justify-center py-12 text-slate-400 gap-3">
              <Loader2 className="w-5 h-5 animate-spin text-indigo-400" />
              <span>Checking tokens, Prisma schema, and local environment...</span>
            </div>
          ) : preflightError ? (
            <div className="p-4 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-300 text-sm flex items-start gap-3">
              <AlertCircle className="w-5 h-5 shrink-0 mt-0.5" />
              <div>
                <p className="font-medium">Preflight failed</p>
                <p className="text-xs opacity-90 mt-1">{preflightError}</p>
              </div>
            </div>
          ) : (
            <>
              {/* Token Readiness Strip */}
              {preflight && (
                <div className="p-4 rounded-xl bg-slate-800/50 border border-slate-700/60 space-y-3">
                  <div className="flex items-center justify-between gap-2 flex-wrap">
                    <span className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-1.5">
                      <ShieldCheck className="w-4 h-4 text-indigo-400" />
                      Connected Integrations
                    </span>
                    <button
                      onClick={() => {
                        onClose();
                        navigate('/settings');
                      }}
                      className="text-xs text-indigo-400 hover:text-indigo-300 inline-flex items-center gap-1"
                    >
                      <Settings className="w-3.5 h-3.5" />
                      Configure Tokens
                    </button>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <TokenChip label="GitHub" ok={preflight.hasGithubToken} required />
                    <TokenChip label="Vercel" ok={preflight.hasVercelToken} required />
                    {preflight.hasPrisma && (
                      <TokenChip label="Neon PostgreSQL" ok={preflight.hasNeonToken} />
                    )}
                    <TokenChip label="Cloudflare Auto-DNS" ok={preflight.hasCloudflareToken} />
                  </div>
                  {preflight.isExpo && (
                    <p className="text-xs text-amber-300 bg-amber-500/10 border border-amber-500/20 rounded-lg px-3 py-2">
                      Expo project detected — Vercel will deploy the web bundle (`expo export`) if configured. Native iOS/Android builds use EAS.
                    </p>
                  )}
                </div>
              )}

              {/* Live Progress View */}
              {isPublishing && (
                <div className="p-5 rounded-xl bg-indigo-950/35 border border-indigo-500/30 space-y-4">
                  <div className="flex items-center justify-between">
                    <span className="text-sm font-semibold text-white flex items-center gap-2">
                      <Loader2 className="w-4 h-4 animate-spin text-indigo-400" />
                      Publishing to Production...
                    </span>
                    <span className="text-sm font-mono font-semibold text-indigo-300">
                      {progress.percent}%
                    </span>
                  </div>

                  <div className="w-full h-2.5 bg-slate-900 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-gradient-to-r from-indigo-500 to-emerald-400 transition-all duration-500"
                      style={{ width: `${Math.max(4, progress.percent)}%` }}
                    />
                  </div>

                  <div className="grid grid-cols-7 gap-1">
                    {PIPELINE_STEPS.map((st, idx) => {
                      const done = progress.step === 'complete' || (currentStepIdx >= 0 && idx < currentStepIdx);
                      const active = idx === currentStepIdx;
                      return (
                        <div
                          key={st.key}
                          className={`text-center py-1 rounded text-[11px] font-medium border ${
                            done
                              ? 'bg-emerald-500/15 border-emerald-500/30 text-emerald-300'
                              : active
                              ? 'bg-indigo-500/25 border-indigo-400/50 text-indigo-200 animate-pulse'
                              : 'bg-slate-900/60 border-slate-800 text-slate-500'
                          }`}
                        >
                          {st.label}
                        </div>
                      );
                    })}
                  </div>

                  <p className="text-xs text-slate-300 font-mono">
                    {progress.message || 'Working...'}
                  </p>
                </div>
              )}

              {/* Publish Error */}
              {publishError && !isPublishing && (
                <div className="p-4 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-300 text-sm flex items-start gap-3">
                  <AlertCircle className="w-5 h-5 shrink-0 mt-0.5 text-rose-400" />
                  <div className="space-y-1">
                    <p className="font-semibold">Publish Failed</p>
                    <p className="text-xs leading-relaxed break-words">{publishError}</p>
                  </div>
                </div>
              )}

              {/* Publish Success Result */}
              {publishResult && !isPublishing && (
                <div className="space-y-4">
                  <div className="p-5 rounded-xl bg-emerald-500/10 border border-emerald-500/30 space-y-4">
                    <div className="flex items-center gap-2.5 text-emerald-300">
                      <CheckCircle2 className="w-6 h-6 text-emerald-400" />
                      <div>
                        <h3 className="font-semibold text-white">Published Successfully!</h3>
                        <p className="text-xs text-emerald-200/80">
                          Your full-stack app is live on Vercel{publishResult.neonProjectId ? ' with Neon PostgreSQL' : ''}.
                        </p>
                      </div>
                    </div>

                    <div className="flex flex-wrap gap-2">
                      {publishResult.deployUrl && (
                        <button
                          onClick={() => openExternal(publishResult.deployUrl)}
                          className="btn-success text-xs py-2 px-3 inline-flex items-center gap-1.5"
                        >
                          <Globe className="w-4 h-4" />
                          Open Live Site
                          <ExternalLink className="w-3.5 h-3.5" />
                        </button>
                      )}
                      {publishResult.customDomainUrl && (
                        <button
                          onClick={() => openExternal(publishResult.customDomainUrl)}
                          className="btn-primary text-xs py-2 px-3 inline-flex items-center gap-1.5"
                        >
                          <Sparkles className="w-4 h-4" />
                          {publishResult.customDomainUrl.replace(/^https?:\/\//, '')}
                          <ExternalLink className="w-3.5 h-3.5" />
                        </button>
                      )}
                      {publishResult.githubUrl && (
                        <button
                          onClick={() => openExternal(publishResult.githubUrl)}
                          className="btn-secondary text-xs py-2 px-3 inline-flex items-center gap-1.5"
                        >
                          <Github className="w-4 h-4" />
                          GitHub Repo
                          <ExternalLink className="w-3.5 h-3.5" />
                        </button>
                      )}
                    </div>

                    {/* Metadata badges */}
                    <div className="flex flex-wrap gap-2 pt-1 text-xs">
                      {publishResult.neonProjectId && (
                        <span className="px-2.5 py-1 rounded-md bg-slate-900/70 border border-slate-700 text-slate-300">
                          Neon DB: <span className="font-mono text-emerald-300">{publishResult.neonProjectId}</span>
                        </span>
                      )}
                      {publishResult.prismaPushed && (
                        <span className="px-2.5 py-1 rounded-md bg-emerald-500/15 border border-emerald-500/30 text-emerald-300">
                          Prisma Schema Synced
                        </span>
                      )}
                      {publishResult.cloudflareDnsConfigured && (
                        <span className="px-2.5 py-1 rounded-md bg-sky-500/15 border border-sky-500/30 text-sky-300">
                          Cloudflare DNS Auto-Configured
                        </span>
                      )}
                    </div>

                    {publishResult.envKeysSynced?.length > 0 && (
                      <div className="pt-1">
                        <p className="text-[11px] text-slate-400 mb-1.5">
                          Encrypted Vercel Environment Variables Synced:
                        </p>
                        <div className="flex flex-wrap gap-1.5">
                          {publishResult.envKeysSynced.map((k) => (
                            <span
                              key={k}
                              className="px-2 py-0.5 rounded bg-slate-900/80 border border-slate-700 font-mono text-[11px] text-indigo-300"
                            >
                              {k}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>

                  {/* DNS Records & Verification Table */}
                  {(publishResult.dnsRecords?.length > 0 || publishResult.customDomainUrl) && (
                    <div className="p-4 rounded-xl bg-slate-800/60 border border-slate-700 space-y-3">
                      <div className="flex items-center justify-between gap-2 flex-wrap">
                        <div className="flex items-center gap-2">
                          <Globe className="w-4 h-4 text-indigo-400" />
                          <span className="text-sm font-semibold text-white">Custom Domain DNS & SSL</span>
                          {publishResult.domainVerified ? (
                            <span className="px-2 py-0.5 rounded-full text-[11px] bg-emerald-500/15 text-emerald-300 border border-emerald-500/30">
                              Verified
                            </span>
                          ) : (
                            <span className="px-2 py-0.5 rounded-full text-[11px] bg-amber-500/15 text-amber-300 border border-amber-500/30">
                              Pending DNS
                            </span>
                          )}
                        </div>
                        <button
                          onClick={() => handleVerifyDomain()}
                          disabled={isVerifyingDomain}
                          className="btn-secondary text-xs py-1.5 px-3 inline-flex items-center gap-1.5"
                        >
                          <RefreshCw className={`w-3.5 h-3.5 ${isVerifyingDomain ? 'animate-spin' : ''}`} />
                          Verify DNS & SSL
                        </button>
                      </div>

                      {domainVerifyInfo && (
                        <div
                          className={`p-2.5 rounded-lg text-xs border ${
                            domainVerifyInfo.verified && !domainVerifyInfo.misconfigured
                              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300'
                              : 'bg-amber-500/10 border-amber-500/30 text-amber-300'
                          }`}
                        >
                          {domainVerifyInfo.message}
                        </div>
                      )}

                      {publishResult.dnsRecords?.length > 0 && (
                        <div className="overflow-x-auto">
                          <table className="w-full text-left text-xs border-collapse">
                            <thead>
                              <tr className="border-b border-slate-700 text-slate-400">
                                <th className="py-1.5 pr-3">Type</th>
                                <th className="py-1.5 pr-3">Name / Host</th>
                                <th className="py-1.5 pr-3">Value / Target</th>
                                <th className="py-1.5 text-right">Copy</th>
                              </tr>
                            </thead>
                            <tbody>
                              {publishResult.dnsRecords.map((rec, i) => {
                                const cKey = `${rec.recordType}-${rec.name}-${i}`;
                                return (
                                  <tr key={cKey} className="border-b border-slate-800/80 last:border-0">
                                    <td className="py-2 pr-3 font-mono font-semibold text-indigo-300">
                                      {rec.recordType}
                                    </td>
                                    <td className="py-2 pr-3 font-mono text-slate-200">{rec.name}</td>
                                    <td className="py-2 pr-3 font-mono text-emerald-300 break-all">
                                      {rec.value}
                                    </td>
                                    <td className="py-2 text-right">
                                      <button
                                        onClick={() => handleCopy(rec.value, cKey)}
                                        className="p-1.5 rounded bg-slate-900 hover:bg-slate-700 text-slate-300"
                                        title="Copy value"
                                      >
                                        {copiedKey === cKey ? (
                                          <Check className="w-3.5 h-3.5 text-emerald-400" />
                                        ) : (
                                          <Copy className="w-3.5 h-3.5" />
                                        )}
                                      </button>
                                    </td>
                                  </tr>
                                );
                              })}
                            </tbody>
                          </table>
                        </div>
                      )}
                    </div>
                  )}

                  {/* Warnings */}
                  {publishResult.warnings?.length > 0 && (
                    <div className="p-3.5 rounded-xl bg-amber-500/10 border border-amber-500/20 space-y-1">
                      <p className="text-xs font-semibold text-amber-300">Preparation & Publish Notes</p>
                      <ul className="list-disc list-inside text-xs text-amber-200/85 space-y-0.5">
                        {publishResult.warnings.map((w, idx) => (
                          <li key={idx}>{w}</li>
                        ))}
                      </ul>
                    </div>
                  )}
                </div>
              )}

              {/* Configuration Form (visible before publish or for re-publish) */}
              {preflight && !isPublishing && !publishResult && (
                <div className="space-y-5">
                  {/* 1. GitHub Repository */}
                  <div className="space-y-2.5">
                    <label className="text-sm font-medium text-white flex items-center gap-2">
                      <Github className="w-4 h-4 text-slate-300" />
                      GitHub Repository
                    </label>
                    <div className="flex gap-2">
                      <input
                        type="text"
                        value={repoName}
                        onChange={(e) => setRepoName(e.target.value)}
                        placeholder="my-awesome-app"
                        className="input flex-1 font-mono text-sm"
                      />
                      <button
                        type="button"
                        onClick={() => setIsPrivateRepo((v) => !v)}
                        className={`px-3.5 py-2 rounded-lg text-xs font-medium border inline-flex items-center gap-1.5 transition-colors ${
                          isPrivateRepo
                            ? 'bg-indigo-500/15 border-indigo-500/40 text-indigo-300'
                            : 'bg-slate-800 border-slate-700 text-slate-300 hover:bg-slate-750'
                        }`}
                      >
                        {isPrivateRepo ? <Lock className="w-3.5 h-3.5" /> : <Unlock className="w-3.5 h-3.5" />}
                        {isPrivateRepo ? 'Private' : 'Public'}
                      </button>
                    </div>
                    <p className="text-xs text-slate-500">
                      Automatically prepares `.gitignore`, Prisma build scripts, `force-dynamic` API routes, and pushes to `main`.
                    </p>
                  </div>

                  {/* 2. Database Provisioning */}
                  {preflight.hasPrisma && (
                    <div className="p-4 rounded-xl bg-slate-800/40 border border-slate-700/70 space-y-3">
                      <div className="flex items-center justify-between">
                        <label className="text-sm font-medium text-white flex items-center gap-2">
                          <Database className="w-4 h-4 text-emerald-400" />
                          PostgreSQL Database (Prisma Detected)
                        </label>
                        {preflight.existingState?.neonProjectId && (
                          <span className="text-[11px] px-2 py-0.5 rounded bg-emerald-500/15 border border-emerald-500/30 text-emerald-300 font-mono">
                            Reusing Neon: {preflight.existingState.neonProjectId}
                          </span>
                        )}
                      </div>

                      <div className="grid grid-cols-3 gap-2">
                        {[
                          { id: 'neon', title: 'Auto Neon DB', desc: '1-click serverless Postgres' },
                          { id: 'custom_url', title: 'Custom URL', desc: 'Bring your own Postgres' },
                          { id: 'skip', title: 'Skip DB', desc: 'Configure manually later' }
                        ].map((opt) => (
                          <button
                            key={opt.id}
                            type="button"
                            onClick={() => setDbMode(opt.id)}
                            className={`p-3 rounded-lg border text-left transition-all ${
                              dbMode === opt.id
                                ? 'bg-indigo-500/15 border-indigo-400/60 text-white'
                                : 'bg-slate-900/60 border-slate-800 text-slate-400 hover:border-slate-700'
                            }`}
                          >
                            <div className="text-xs font-semibold">{opt.title}</div>
                            <div className="text-[11px] opacity-75 mt-0.5">{opt.desc}</div>
                          </button>
                        ))}
                      </div>

                      {dbMode === 'neon' && (
                        <div className="space-y-2 pt-1">
                          {!preflight.hasNeonToken ? (
                            <p className="text-xs text-amber-300 bg-amber-500/10 border border-amber-500/20 rounded-lg px-3 py-2">
                              Neon API Key is not configured yet. Add it in Settings or switch to Custom PostgreSQL URL.
                            </p>
                          ) : (
                            <div>
                              <label className="text-xs text-slate-400 block mb-1">Neon Region</label>
                              <select
                                value={neonRegion}
                                onChange={(e) => setNeonRegion(e.target.value)}
                                className="input w-full text-xs"
                              >
                                {NEON_REGIONS.map((r) => (
                                  <option key={r.id} value={r.id}>
                                    {r.label}
                                  </option>
                                ))}
                              </select>
                            </div>
                          )}
                        </div>
                      )}

                      {dbMode === 'custom_url' && (
                        <div className="pt-1">
                          <label className="text-xs text-slate-400 block mb-1">PostgreSQL Connection URI</label>
                          <input
                            type="password"
                            value={customDatabaseUrl}
                            onChange={(e) => setCustomDatabaseUrl(e.target.value)}
                            placeholder="postgresql://user:password@host/dbname?sslmode=require"
                            className="input w-full font-mono text-xs"
                          />
                        </div>
                      )}

                      {dbMode !== 'skip' && (
                        <label className="flex items-center gap-2 text-xs text-slate-300 cursor-pointer pt-1">
                          <input
                            type="checkbox"
                            checked={runPrismaPush}
                            onChange={(e) => setRunPrismaPush(e.target.checked)}
                            className="rounded border-slate-600 bg-slate-900 text-indigo-500 focus:ring-indigo-500"
                          />
                          Run <code className="text-emerald-300">prisma db push</code> automatically to create tables
                        </label>
                      )}
                    </div>
                  )}

                  {/* 3. Environment Variables */}
                  <div className="p-4 rounded-xl bg-slate-800/40 border border-slate-700/70 space-y-2.5">
                    <div className="flex items-center justify-between">
                      <span className="text-sm font-medium text-white flex items-center gap-2">
                        <KeyRound className="w-4 h-4 text-indigo-400" />
                        Vercel Environment Variables
                      </span>
                      <label className="flex items-center gap-2 text-xs text-slate-300 cursor-pointer">
                        <input
                          type="checkbox"
                          checked={syncLocalEnvKeys}
                          onChange={(e) => setSyncLocalEnvKeys(e.target.checked)}
                          className="rounded border-slate-600 bg-slate-900 text-indigo-500"
                        />
                        Sync local `.env.local` keys
                      </label>
                    </div>
                    <p className="text-xs text-slate-400">
                      <code className="text-indigo-300">DATABASE_URL</code>,{' '}
                      <code className="text-indigo-300">DIRECT_URL</code>,{' '}
                      <code className="text-indigo-300">NEXTAUTH_SECRET</code>, and{' '}
                      <code className="text-indigo-300">NEXTAUTH_URL</code> are automatically injected and encrypted on Vercel.
                    </p>
                    {syncLocalEnvKeys && preflight.detectedEnvKeys?.length > 0 && (
                      <div className="flex flex-wrap gap-1.5 pt-1">
                        {preflight.detectedEnvKeys.map((k) => (
                          <span
                            key={k}
                            className="px-2 py-0.5 rounded bg-slate-900 border border-slate-700 font-mono text-[11px] text-slate-300"
                          >
                            {k}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>

                  {/* 4. Custom Domain & Optional Cloudflare Auto-DNS */}
                  <div className="p-4 rounded-xl bg-slate-800/40 border border-slate-700/70 space-y-3">
                    <label className="text-sm font-medium text-white flex items-center gap-2">
                      <Globe className="w-4 h-4 text-sky-400" />
                      Custom Domain <span className="text-xs font-normal text-slate-500">(Optional)</span>
                    </label>
                    <input
                      type="text"
                      value={customDomain}
                      onChange={(e) => setCustomDomain(e.target.value)}
                      placeholder="app.yourdomain.com or yourdomain.com"
                      className="input w-full font-mono text-sm"
                    />
                    <div className="flex items-center justify-between gap-2 flex-wrap">
                      <label
                        className={`flex items-center gap-2 text-xs ${
                          preflight.hasCloudflareToken && customDomain.trim()
                            ? 'text-slate-300 cursor-pointer'
                            : 'text-slate-500 cursor-not-allowed'
                        }`}
                      >
                        <input
                          type="checkbox"
                          disabled={!preflight.hasCloudflareToken || !customDomain.trim()}
                          checked={autoCloudflareDns}
                          onChange={(e) => setAutoCloudflareDns(e.target.checked)}
                          className="rounded border-slate-600 bg-slate-900 text-sky-500"
                        />
                        <Cloud className="w-3.5 h-3.5 text-sky-400" />
                        Auto-configure DNS via Cloudflare
                      </label>
                      {preflight.existingState?.customDomain && (
                        <button
                          type="button"
                          onClick={() => handleVerifyDomain(preflight.existingState.customDomain)}
                          disabled={isVerifyingDomain}
                          className="text-xs text-indigo-400 hover:text-indigo-300 inline-flex items-center gap-1"
                        >
                          <RefreshCw className={`w-3 h-3 ${isVerifyingDomain ? 'animate-spin' : ''}`} />
                          Re-check Existing Domain
                        </button>
                      )}
                    </div>
                    {domainVerifyInfo && (
                      <div
                        className={`p-2.5 rounded-lg text-xs border ${
                          domainVerifyInfo.verified && !domainVerifyInfo.misconfigured
                            ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300'
                            : 'bg-amber-500/10 border-amber-500/30 text-amber-300'
                        }`}
                      >
                        {domainVerifyInfo.message}
                      </div>
                    )}
                  </div>

                  {/* Advanced (Vercel Team ID) */}
                  <div>
                    <button
                      type="button"
                      onClick={() => setShowAdvanced((v) => !v)}
                      className="text-xs text-slate-400 hover:text-slate-200 inline-flex items-center gap-1"
                    >
                      {showAdvanced ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
                      Advanced Options (Vercel Team ID)
                    </button>
                    {showAdvanced && (
                      <div className="mt-2">
                        <input
                          type="text"
                          value={vercelTeamId}
                          onChange={(e) => setVercelTeamId(e.target.value)}
                          placeholder="team_xxxxxxxxxxxx (leave blank for personal account)"
                          className="input w-full font-mono text-xs"
                        />
                      </div>
                    )}
                  </div>
                </div>
              )}
            </>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between px-6 py-4 border-t border-slate-800 bg-slate-900/90">
          <button
            onClick={onClose}
            disabled={isPublishing}
            className="btn-secondary text-sm disabled:opacity-40"
          >
            {publishResult ? 'Done' : 'Cancel'}
          </button>

          {publishResult ? (
            <button
              onClick={() => setPublishResult(null)}
              className="btn-primary text-sm inline-flex items-center gap-2"
            >
              <RefreshCw className="w-4 h-4" />
              Publish Update / Domain
            </button>
          ) : (
            <button
              onClick={handlePublish}
              disabled={!canStart}
              className="btn-primary text-sm inline-flex items-center gap-2 disabled:opacity-40 disabled:cursor-not-allowed"
            >
              {isPublishing ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  Publishing...
                </>
              ) : (
                <>
                  <Rocket className="w-4 h-4" />
                  {preflight?.existingState?.deployUrl ? 'Republish Full-Stack' : 'Publish Full-Stack'}
                </>
              )}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
