import React, { useEffect, useRef, useState } from 'react';
import {
  X,
  Sparkles,
  Palette,
  Wand2,
  CheckCircle2,
  AlertCircle,
  Loader2,
  Copy,
  Check,
  Download,
  Layers,
  Sun,
  Moon,
  Share2,
  Smartphone,
  Sliders
} from 'lucide-react';

const MOTIFS = [
  'pulse',
  'prism',
  'shield',
  'spark',
  'orbit',
  'grid',
  'wave',
  'hexagon',
  'bolt',
  'leaf',
  'layers',
  'monogram'
];

const PALETTE_PRESETS = [
  { name: 'Emerald', primary: '#10B981', secondary: '#0EA5E9', accent: '#34D399', bg: '#06151B' },
  { name: 'Indigo AI', primary: '#6366F1', secondary: '#A855F7', accent: '#38BDF8', bg: '#0B0F1F' },
  { name: 'Forge', primary: '#F97316', secondary: '#6366F1', accent: '#FBBF24', bg: '#12101B' },
  { name: 'Audio', primary: '#8B5CF6', secondary: '#EC4899', accent: '#F472B6', bg: '#110B22' },
  { name: 'Ocean', primary: '#3B82F6', secondary: '#14B8A6', accent: '#60A5FA', bg: '#081224' },
  { name: 'Crimson', primary: '#F43F5E', secondary: '#F59E0B', accent: '#FDA4AF', bg: '#180B12' }
];

function svgToDataUri(svgStr) {
  if (!svgStr) return '';
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svgStr)}`;
}

export default function BrandLogoModal({ isOpen, onClose, project, onApplied }) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [isExpo, setIsExpo] = useState(false);
  const [brief, setBrief] = useState(null);
  const [variants, setVariants] = useState([]);
  const [selectedVariantId, setSelectedVariantId] = useState('gradient-badge');

  // Studio controls
  const [prompt, setPrompt] = useState('');
  const [isGeneratingAi, setIsGeneratingAi] = useState(false);
  const [aiNote, setAiNote] = useState(null);
  const [previewTab, setPreviewTab] = useState('dark'); // 'dark' | 'light' | 'icons' | 'og'
  const [writeBrandComponent, setWriteBrandComponent] = useState(true);
  const [patchLayoutMetadata, setPatchLayoutMetadata] = useState(true);

  // Apply & copy feedback
  const [isApplying, setIsApplying] = useState(false);
  const [applyResult, setApplyResult] = useState(null);
  const [copiedSvg, setCopiedSvg] = useState(false);

  const debounceRef = useRef(null);

  useEffect(() => {
    if (!isOpen || !project?.id) return;

    let cancelled = false;
    setLoading(true);
    setError(null);
    setAiNote(null);
    setApplyResult(null);

    window.electronAPI.brandLogo
      .getInit(project.id)
      .then((res) => {
        if (cancelled) return;
        setIsExpo(Boolean(res.isExpo));
        setBrief(res.brief);
        setVariants(res.variants || []);
        setSelectedVariantId(res.selectedVariantId || 'gradient-badge');
      })
      .catch((err) => {
        if (cancelled) return;
        setError(typeof err === 'string' ? err : err?.message || 'Failed to load brand studio');
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [isOpen, project?.id]);

  if (!isOpen || !project) return null;

  const selectedVariant =
    variants.find((v) => v.id === selectedVariantId) || variants[0] || null;

  const triggerParametricRefresh = (nextBrief) => {
    setBrief(nextBrief);
    setApplyResult(null);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(async () => {
      try {
        const res = await window.electronAPI.brandLogo.generate({
          projectId: project.id,
          brandName: nextBrief.brandName,
          prompt: null,
          useAi: false,
          modelId: null,
          briefOverride: nextBrief
        });
        setVariants(res.variants || []);
      } catch (e) {
        console.warn('Parametric refresh failed:', e);
      }
    }, 90);
  };

  const updateBriefField = (field, value) => {
    if (!brief) return;
    triggerParametricRefresh({ ...brief, [field]: value });
  };

  const applyPresetPalette = (preset) => {
    if (!brief) return;
    triggerParametricRefresh({
      ...brief,
      primaryColor: preset.primary,
      secondaryColor: preset.secondary,
      accentColor: preset.accent,
      bgColor: preset.bg
    });
  };

  const handleGenerateBrief = async (useAi) => {
    setIsGeneratingAi(true);
    setError(null);
    setAiNote(null);
    setApplyResult(null);
    try {
      const selectedModel =
        localStorage.getItem('axiom-forge-model') ||
        localStorage.getItem('axiom-forge-builder-model') ||
        null;
      const res = await window.electronAPI.brandLogo.generate({
        projectId: project.id,
        brandName: brief?.brandName || project.name,
        prompt: prompt.trim() || null,
        useAi,
        modelId: selectedModel,
        briefOverride: null
      });
      setBrief(res.brief);
      setVariants(res.variants || []);
      if (res.aiNote) setAiNote(res.aiNote);
    } catch (err) {
      setError(typeof err === 'string' ? err : err?.message || 'Generation failed');
    } finally {
      setIsGeneratingAi(false);
    }
  };

  const handleCopySvg = async () => {
    if (!selectedVariant) return;
    const svg =
      previewTab === 'light'
        ? selectedVariant.wordmarkLightSvg
        : previewTab === 'og'
        ? selectedVariant.ogImageSvg
        : previewTab === 'icons'
        ? selectedVariant.markSvg
        : selectedVariant.wordmarkDarkSvg;
    try {
      await navigator.clipboard.writeText(svg);
      setCopiedSvg(true);
      setTimeout(() => setCopiedSvg(false), 1600);
    } catch (e) {
      console.warn('Copy SVG failed:', e);
    }
  };

  const handleDownloadSvg = () => {
    if (!selectedVariant) return;
    const svg =
      previewTab === 'icons'
        ? selectedVariant.markSvg
        : previewTab === 'light'
        ? selectedVariant.wordmarkLightSvg
        : previewTab === 'og'
        ? selectedVariant.ogImageSvg
        : selectedVariant.wordmarkDarkSvg;
    const blob = new Blob([svg], { type: 'image/svg+xml;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${(brief?.brandName || 'brand').toLowerCase().replace(/\s+/g, '-')}-${selectedVariant.id}.svg`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  const handleApplyToProject = async () => {
    if (!brief || !selectedVariant) return;
    setIsApplying(true);
    setError(null);
    setApplyResult(null);
    try {
      const res = await window.electronAPI.brandLogo.apply({
        projectId: project.id,
        brief,
        variantId: selectedVariant.id,
        writeBrandComponent: !isExpo && writeBrandComponent,
        patchLayoutMetadata: !isExpo && patchLayoutMetadata
      });
      setApplyResult(res);
      if (typeof onApplied === 'function') {
        onApplied(res);
      }
    } catch (err) {
      setError(typeof err === 'string' ? err : err?.message || 'Failed to export brand assets');
    } finally {
      setIsApplying(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4">
      <div className="w-full max-w-5xl max-h-[92vh] flex flex-col rounded-2xl bg-slate-900 border border-slate-700/80 shadow-2xl overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-900/95">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-xl bg-violet-500/15 border border-violet-500/30 flex items-center justify-center">
              <Palette className="w-5 h-5 text-violet-400" />
            </div>
            <div>
              <h2 className="text-lg font-semibold text-white">Hybrid Brand SVG Logo Studio</h2>
              <p className="text-xs text-slate-400">
                Local AI Brand Brief + Parametric Vector Engine → SVG, Multi-Size ICO, Apple Touch Icon & OpenGraph PNG
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            disabled={isApplying}
            className="p-2 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
            title="Close"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-6">
          {loading ? (
            <div className="flex items-center justify-center py-16 gap-3 text-slate-400">
              <Loader2 className="w-5 h-5 animate-spin text-violet-400" />
              <span>Initializing Hybrid Brand Engine...</span>
            </div>
          ) : (
            <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
              {/* Left Column: Brief & Parametric Controls */}
              <div className="lg:col-span-5 space-y-4">
                {/* AI Prompt Bar */}
                <div className="p-4 rounded-xl bg-slate-800/50 border border-slate-700/70 space-y-2.5">
                  <label className="text-xs font-semibold uppercase tracking-wider text-violet-300 flex items-center gap-1.5">
                    <Sparkles className="w-3.5 h-3.5" />
                    Creative Direction (Hybrid AI + Parametric)
                  </label>
                  <textarea
                    rows={2}
                    value={prompt}
                    onChange={(e) => setPrompt(e.target.value)}
                    placeholder="e.g. futuristic emerald fintech pulse, bold geometric prism, minimalist violet audio studio..."
                    className="input w-full text-xs resize-none"
                  />
                  <div className="flex gap-2">
                    <button
                      type="button"
                      onClick={() => handleGenerateBrief(true)}
                      disabled={isGeneratingAi}
                      className="btn-primary flex-1 text-xs py-2 inline-flex items-center justify-center gap-1.5"
                    >
                      {isGeneratingAi ? (
                        <Loader2 className="w-3.5 h-3.5 animate-spin" />
                      ) : (
                        <Wand2 className="w-3.5 h-3.5" />
                      )}
                      AI Brand Brief (Local LLM)
                    </button>
                    <button
                      type="button"
                      onClick={() => handleGenerateBrief(false)}
                      disabled={isGeneratingAi}
                      className="btn-secondary text-xs py-2 px-3 inline-flex items-center gap-1"
                      title="Instant deterministic brief from keywords"
                    >
                      <Sliders className="w-3.5 h-3.5" />
                      Instant
                    </button>
                  </div>
                  {aiNote && (
                    <p className="text-[11px] text-indigo-300 bg-indigo-500/10 border border-indigo-500/20 rounded-lg px-2.5 py-1.5">
                      {aiNote}
                    </p>
                  )}
                </div>

                {/* Brand Identity Fields */}
                {brief && (
                  <div className="p-4 rounded-xl bg-slate-800/40 border border-slate-700/60 space-y-3">
                    <div className="grid grid-cols-3 gap-2">
                      <div className="col-span-2">
                        <label className="text-[11px] text-slate-400 block mb-1">Brand Name</label>
                        <input
                          type="text"
                          value={brief.brandName}
                          onChange={(e) => updateBriefField('brandName', e.target.value)}
                          className="input w-full text-xs"
                        />
                      </div>
                      <div>
                        <label className="text-[11px] text-slate-400 block mb-1">Monogram</label>
                        <input
                          type="text"
                          maxLength={2}
                          value={brief.monogram}
                          onChange={(e) => updateBriefField('monogram', e.target.value.toUpperCase())}
                          className="input w-full text-xs font-mono uppercase text-center"
                        />
                      </div>
                    </div>

                    <div>
                      <label className="text-[11px] text-slate-400 block mb-1">Tagline</label>
                      <input
                        type="text"
                        value={brief.tagline}
                        onChange={(e) => updateBriefField('tagline', e.target.value)}
                        className="input w-full text-xs"
                      />
                    </div>

                    {/* Motif Picker */}
                    <div>
                      <label className="text-[11px] text-slate-400 block mb-1.5">Geometric Motif</label>
                      <div className="grid grid-cols-4 gap-1.5">
                        {MOTIFS.map((m) => (
                          <button
                            key={m}
                            type="button"
                            onClick={() => updateBriefField('motif', m)}
                            className={`px-2 py-1.5 rounded-lg text-[11px] font-medium capitalize border transition-all ${
                              brief.motif === m
                                ? 'bg-violet-500/20 border-violet-400/70 text-violet-200'
                                : 'bg-slate-900/60 border-slate-800 text-slate-400 hover:border-slate-700 hover:text-slate-200'
                            }`}
                          >
                            {m}
                          </button>
                        ))}
                      </div>
                    </div>

                    {/* Color Palette */}
                    <div className="space-y-2">
                      <div className="flex items-center justify-between">
                        <label className="text-[11px] text-slate-400">Colors & Quick Palettes</label>
                        <div className="flex gap-1">
                          {PALETTE_PRESETS.map((pr) => (
                            <button
                              key={pr.name}
                              type="button"
                              onClick={() => applyPresetPalette(pr)}
                              title={pr.name}
                              className="w-4 h-4 rounded-full border border-white/20 hover:scale-110 transition-transform"
                              style={{
                                background: `linear-gradient(135deg, ${pr.primary}, ${pr.secondary})`
                              }}
                            />
                          ))}
                        </div>
                      </div>

                      <div className="grid grid-cols-4 gap-2">
                        {[
                          { key: 'primaryColor', label: 'Primary' },
                          { key: 'secondaryColor', label: 'Second' },
                          { key: 'accentColor', label: 'Accent' },
                          { key: 'bgColor', label: 'Surface' }
                        ].map((c) => (
                          <label
                            key={c.key}
                            className="flex flex-col items-center gap-1 p-1.5 rounded-lg bg-slate-900/70 border border-slate-800 cursor-pointer hover:border-slate-700"
                          >
                            <input
                              type="color"
                              value={brief[c.key] || '#6366F1'}
                              onChange={(e) => updateBriefField(c.key, e.target.value.toUpperCase())}
                              className="w-full h-6 rounded cursor-pointer bg-transparent border-0"
                            />
                            <span className="text-[10px] text-slate-400">{c.label}</span>
                          </label>
                        ))}
                      </div>
                    </div>

                    {/* Corner Radius */}
                    <div>
                      <div className="flex items-center justify-between text-[11px] text-slate-400 mb-1">
                        <span>Badge Corner Radius</span>
                        <span className="font-mono text-slate-300">{brief.cornerRadius}px</span>
                      </div>
                      <input
                        type="range"
                        min={0}
                        max={48}
                        value={brief.cornerRadius}
                        onChange={(e) => updateBriefField('cornerRadius', Number(e.target.value))}
                        className="w-full accent-violet-500"
                      />
                    </div>
                  </div>
                )}
              </div>

              {/* Right Column: 4 Variants + Live Preview Stage */}
              <div className="lg:col-span-7 space-y-4">
                {error && (
                  <div className="p-3.5 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs flex items-start gap-2.5">
                    <AlertCircle className="w-4 h-4 shrink-0 mt-0.5" />
                    <span>{error}</span>
                  </div>
                )}

                {/* 4 Variant Selector Cards */}
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5">
                  {variants.map((v) => {
                    const active = v.id === selectedVariant?.id;
                    return (
                      <button
                        key={v.id}
                        type="button"
                        onClick={() => {
                          setSelectedVariantId(v.id);
                          setApplyResult(null);
                        }}
                        className={`p-3 rounded-xl border text-center transition-all flex flex-col items-center gap-2 ${
                          active
                            ? 'bg-violet-500/15 border-violet-400/70 ring-1 ring-violet-400/40'
                            : 'bg-slate-800/40 border-slate-800 hover:border-slate-700'
                        }`}
                      >
                        <div className="w-16 h-16 flex items-center justify-center rounded-xl bg-slate-950/60 p-2">
                          <img
                            src={svgToDataUri(v.markSvg)}
                            alt={v.name}
                            className="w-full h-full object-contain"
                          />
                        </div>
                        <span className="text-[11px] font-medium text-slate-200 leading-tight">
                          {v.name}
                        </span>
                      </button>
                    );
                  })}
                </div>

                {/* Live Preview Tabs */}
                {selectedVariant && (
                  <div className="rounded-xl border border-slate-700/70 overflow-hidden bg-slate-950/70">
                    <div className="flex items-center justify-between px-3 py-2 border-b border-slate-800 bg-slate-900/80 flex-wrap gap-2">
                      <div className="flex items-center gap-1">
                        {[
                          { id: 'dark', label: 'Dark UI', icon: Moon },
                          { id: 'light', label: 'Light UI', icon: Sun },
                          { id: 'icons', label: 'Favicon / App', icon: Smartphone },
                          { id: 'og', label: 'OG Card', icon: Share2 }
                        ].map((tab) => {
                          const Icon = tab.icon;
                          return (
                            <button
                              key={tab.id}
                              type="button"
                              onClick={() => setPreviewTab(tab.id)}
                              className={`px-2.5 py-1 rounded-lg text-xs font-medium inline-flex items-center gap-1.5 transition-colors ${
                                previewTab === tab.id
                                  ? 'bg-violet-500/20 text-violet-200 border border-violet-500/40'
                                  : 'text-slate-400 hover:text-white hover:bg-slate-800'
                              }`}
                            >
                              <Icon className="w-3.5 h-3.5" />
                              {tab.label}
                            </button>
                          );
                        })}
                      </div>

                      <div className="flex items-center gap-1.5">
                        <button
                          type="button"
                          onClick={handleCopySvg}
                          className="px-2.5 py-1 rounded-lg text-xs bg-slate-800 hover:bg-slate-700 text-slate-200 inline-flex items-center gap-1"
                        >
                          {copiedSvg ? (
                            <Check className="w-3.5 h-3.5 text-emerald-400" />
                          ) : (
                            <Copy className="w-3.5 h-3.5" />
                          )}
                          {copiedSvg ? 'Copied' : 'Copy SVG'}
                        </button>
                        <button
                          type="button"
                          onClick={handleDownloadSvg}
                          className="px-2.5 py-1 rounded-lg text-xs bg-slate-800 hover:bg-slate-700 text-slate-200 inline-flex items-center gap-1"
                        >
                          <Download className="w-3.5 h-3.5" />
                          SVG
                        </button>
                      </div>
                    </div>

                    {/* Preview Canvas */}
                    <div
                      className={`p-6 flex items-center justify-center min-h-[210px] ${
                        previewTab === 'light'
                          ? 'bg-slate-100'
                          : 'bg-[radial-gradient(ellipse_at_top,_var(--tw-gradient-stops))] from-slate-900 via-slate-950 to-slate-950'
                      }`}
                    >
                      {previewTab === 'dark' && (
                        <img
                          src={svgToDataUri(selectedVariant.wordmarkDarkSvg)}
                          alt="Dark Wordmark"
                          className="max-w-full h-24 object-contain"
                        />
                      )}

                      {previewTab === 'light' && (
                        <img
                          src={svgToDataUri(selectedVariant.wordmarkLightSvg)}
                          alt="Light Wordmark"
                          className="max-w-full h-24 object-contain"
                        />
                      )}

                      {previewTab === 'icons' && (
                        <div className="flex items-end justify-center gap-6 flex-wrap">
                          {[
                            { sz: 96, label: 'App Icon (512/1024)' },
                            { sz: 56, label: 'Apple Touch (180)' },
                            { sz: 32, label: 'Favicon (32px)' },
                            { sz: 16, label: 'Tab (16px)' }
                          ].map((item) => (
                            <div key={item.sz} className="flex flex-col items-center gap-2">
                              <div className="p-2 rounded-xl bg-slate-900 border border-slate-800 flex items-center justify-center">
                                <img
                                  src={svgToDataUri(selectedVariant.markSvg)}
                                  alt={item.label}
                                  style={{ width: item.sz, height: item.sz }}
                                />
                              </div>
                              <span className="text-[11px] text-slate-400">{item.label}</span>
                            </div>
                          ))}
                        </div>
                      )}

                      {previewTab === 'og' && (
                        <img
                          src={svgToDataUri(selectedVariant.ogImageSvg)}
                          alt="OpenGraph Social Preview"
                          className="w-full max-w-lg rounded-xl border border-slate-700/80 shadow-xl"
                        />
                      )}
                    </div>
                  </div>
                )}

                {/* Export Options */}
                {!isExpo && (
                  <div className="p-3.5 rounded-xl bg-slate-800/40 border border-slate-700/60 flex flex-wrap gap-4 text-xs text-slate-300">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={writeBrandComponent}
                        onChange={(e) => setWriteBrandComponent(e.target.checked)}
                        className="rounded border-slate-600 bg-slate-900 text-violet-500"
                      />
                      Generate <code className="text-violet-300">src/components/BrandLogo.tsx</code>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={patchLayoutMetadata}
                        onChange={(e) => setPatchLayoutMetadata(e.target.checked)}
                        className="rounded border-slate-600 bg-slate-900 text-violet-500"
                      />
                      Wire <code className="text-violet-300">layout.tsx</code> icons & OG metadata
                    </label>
                  </div>
                )}

                {/* Success Banner */}
                {applyResult?.success && (
                  <div className="p-4 rounded-xl bg-emerald-500/10 border border-emerald-500/30 space-y-2">
                    <div className="flex items-center gap-2 text-emerald-300 text-sm font-semibold">
                      <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                      Brand Assets Exported ({applyResult.filesWritten.length} files)
                    </div>
                    <div className="flex flex-wrap gap-1.5">
                      {applyResult.filesWritten.map((f) => (
                        <span
                          key={f}
                          className="px-2 py-0.5 rounded bg-slate-900/80 border border-emerald-500/25 font-mono text-[11px] text-emerald-200"
                        >
                          {f}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between px-6 py-4 border-t border-slate-800 bg-slate-900/95">
          <button
            onClick={onClose}
            disabled={isApplying}
            className="btn-secondary text-sm"
          >
            Close
          </button>

          <button
            onClick={handleApplyToProject}
            disabled={loading || isApplying || !selectedVariant}
            className="btn-primary text-sm inline-flex items-center gap-2 disabled:opacity-40"
          >
            {isApplying ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" />
                Exporting SVG + PNG + ICO...
              </>
            ) : (
              <>
                <Layers className="w-4 h-4" />
                {isExpo
                  ? 'Apply to Expo (SVG + 1024px PNG Icons)'
                  : 'Apply to Project (SVG + ICO + PNG + OG)'}
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
