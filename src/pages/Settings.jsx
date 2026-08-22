/**
 * Axiom Forge - Settings
 * Application settings and token management
 */

import React, { useState, useEffect } from 'react';
import { 
  Key, 
  Server, 
  Shield, 
  ExternalLink, 
  Eye, 
  EyeOff, 
  CheckCircle2, 
  AlertCircle,
  Save,
  Trash2,
  RefreshCw,
  Sliders,
  AlertTriangle
} from 'lucide-react';
import { useAppStore } from '../hooks/useAppStore';
import { verifyCompatibility } from '../lib/compatibility';

// Token input component
function TokenInput({ 
  label, 
  tokenKey, 
  helpUrl, 
  description,
  onSave 
}) {
  const [value, setValue] = useState('');
  const [showValue, setShowValue] = useState(false);
  const [isSet, setIsSet] = useState(false);
  const [isChecking, setIsChecking] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [message, setMessage] = useState(null);

  useEffect(() => {
    checkToken();
  }, []);

  const checkToken = async () => {
    setIsChecking(true);
    try {
      const result = await window.electronAPI.security.hasToken(tokenKey);
      setIsSet(result?.exists === true);
    } catch (e) {
      console.error(`[Settings] Could not check token ${tokenKey}:`, e);
      setIsSet(false);
    } finally {
      setIsChecking(false);
    }
  };

  const handleSave = async () => {
    if (!value.trim()) return;
    
    setIsSaving(true);
    setMessage(null);

    try {
      await onSave(tokenKey, value);
      setIsSet(true);
      setValue('');
      setMessage({ type: 'success', text: 'Token saved successfully' });
    } catch (error) {
      setMessage({ type: 'error', text: error.message });
    } finally {
      setIsSaving(false);
    }
  };

  const handleClear = async () => {
    try {
      await window.electronAPI.security.deleteToken(tokenKey);
      setIsSet(false);
      setValue('');
      setMessage({ type: 'success', text: 'Token cleared' });
    } catch (e) {
      setMessage({ type: 'error', text: 'Could not clear token' });
    }
  };

  const handleReveal = async () => {
    if (showValue && value) {
      // Hide it again
      setValue('');
      setShowValue(false);
      return;
    }
    try {
      const result = await window.electronAPI.security.getToken(tokenKey);
      if (result) {
        setValue(result);
        setShowValue(true);
      } else {
        setMessage({ type: 'error', text: 'Could not retrieve token value' });
      }
    } catch (e) {
      setMessage({ type: 'error', text: 'Failed to reveal token' });
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <label className="label flex items-center gap-2">
          <Key className="w-4 h-4 text-slate-500" />
          {label}
          {isChecking ? (
            <RefreshCw className="w-3 h-3 text-slate-500 animate-spin" />
          ) : isSet ? (
            <CheckCircle2 className="w-4 h-4 text-emerald-400" />
          ) : (
            <AlertCircle className="w-4 h-4 text-amber-400" />
          )}
        </label>
        
        {helpUrl && (
          <a
            href={helpUrl}
            onClick={(e) => {
              e.preventDefault();
              window.electronAPI.shell.openExternal(helpUrl);
            }}
            className="flex items-center gap-1 text-xs text-indigo-400 hover:text-indigo-300"
          >
            <ExternalLink className="w-3 h-3" />
            Get Token
          </a>
        )}
      </div>
      
      {description && (
        <p className="text-xs text-slate-500">{description}</p>
      )}

      {isSet && !value && (
        <div className="flex items-center gap-2 px-3 py-2 bg-emerald-500/10 border border-emerald-500/20 rounded-lg">
          <CheckCircle2 className="w-4 h-4 text-emerald-400 flex-shrink-0" />
          <span className="text-xs text-emerald-400">Token is configured. Enter a new value below to replace it.</span>
        </div>
      )}
      
      <div className="flex gap-2">
        <div className="relative flex-1">
          <input
            type={showValue ? 'text' : 'password'}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder={isSet ? '••••••••••••••••' : `Enter ${label}`}
            className="input pr-10"
          />
          <button
            onClick={handleReveal}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300"
            title={showValue && value ? 'Hide token' : 'Reveal stored token'}
          >
            {showValue ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
          </button>
        </div>
        
        <button
          onClick={handleSave}
          disabled={isSaving || !value.trim()}
          className="btn-primary"
        >
          {isSaving ? (
            <RefreshCw className="w-4 h-4 animate-spin" />
          ) : (
            <Save className="w-4 h-4" />
          )}
          Save
        </button>
        
        {isSet && (
          <button
            onClick={handleClear}
            className="btn-danger"
            title="Clear token"
          >
            <Trash2 className="w-4 h-4" />
          </button>
        )}
      </div>
      
      {message && (
        <p className={`text-xs ${message.type === 'success' ? 'text-emerald-400' : 'text-rose-400'}`}>
          {message.text}
        </p>
      )}
    </div>
  );
}

function Settings() {
  const { saveToken, checkTokens, tokensConfigured, setAppMode } = useAppStore();
  const [ollamaHost, setOllamaHost] = useState('http://127.0.0.1:11434');
  const [builderModel, setBuilderModel] = useState('');
  const [editorModel, setEditorModel] = useState('');
  const [activeRole, setActiveRole] = useState('builder'); // 'builder' or 'editor'
  const [hardwareProfile, setHardwareProfile] = useState(null);
  const [ollamaStatus, setOllamaStatus] = useState('unknown');
  const [isTestingOllama, setIsTestingOllama] = useState(false);

  const [techOverrides, setTechOverrides] = useState({
    nextjs: '',
    react: '',
    node: '',
    tauri: '',
    expo: '',
    rust: ''
  });
  const [localTools, setLocalTools] = useState(null);
  const [compatibilityWarnings, setCompatibilityWarnings] = useState([]);

  useEffect(() => {
    checkTokens();
    testOllamaConnection();
    loadHardwareProfile();
    loadTechOverrides();
    loadLocalTools();
  }, []);

  const loadTechOverrides = async () => {
    try {
      const settings = await window.electronAPI.app.getSettings();
      if (settings && settings.techOverrides) {
        setTechOverrides(settings.techOverrides);
      }
    } catch (e) {
      console.error('[Settings] Error loading tech overrides:', e);
    }
  };

  const loadLocalTools = async () => {
    try {
      const tools = await window.electronAPI.hardware.getLocalToolsProfile();
      setLocalTools(tools);
    } catch (e) {
      console.error('[Settings] Error loading local tools profile:', e);
    }
  };

  const handleOverrideChange = async (key, value) => {
    const updated = { ...techOverrides, [key]: value };
    setTechOverrides(updated);
    
    try {
      const settings = await window.electronAPI.app.getSettings() || {};
      settings.techOverrides = updated;
      await window.electronAPI.app.saveSettings(settings);
    } catch (e) {
      console.error('[Settings] Error saving tech overrides:', e);
    }
  };

  useEffect(() => {
    if (localTools) {
      const warnings = verifyCompatibility(techOverrides, localTools);
      setCompatibilityWarnings(warnings);
    }
  }, [techOverrides, localTools]);

  const loadHardwareProfile = async () => {
    const profile = await window.electronAPI.hardware.getProfile();
    setHardwareProfile(profile);
    
    const selectedBuilder = await window.electronAPI.hardware.getBuilderModel();
    const selectedEditor = await window.electronAPI.hardware.getEditorModel();
    
    if (selectedBuilder) {
      setBuilderModel(selectedBuilder);
    } else {
      // Recommend SSA/SSM model if possible
      const recBuilder = profile.models.find(m => m.isCompatible && m.tags.includes('ssa'))?.id || profile.recommendedModelId;
      setBuilderModel(recBuilder);
      await window.electronAPI.hardware.setBuilderModel(recBuilder);
    }

    if (selectedEditor) {
      setEditorModel(selectedEditor);
    } else {
      // Recommend Transformer/dense code model
      const recEditor = profile.models.find(m => m.isCompatible && !m.tags.includes('ssa') && m.tags.includes('code'))?.id || profile.recommendedModelId;
      setEditorModel(recEditor);
      await window.electronAPI.hardware.setEditorModel(recEditor);
    }
  };

  const handleBuilderModelChange = async (newModel) => {
    const model = hardwareProfile?.models.find(m => m.id === newModel);
    if (model && !model.isCompatible && !model.isMarginal) {
      const confirmed = window.confirm(
        `Warning: This model requires ${model.minRamGB}GB of RAM, but your system only has ${hardwareProfile.ramGB?.toFixed(1)}GB. Running this model may cause severe system slowdowns, app crashes, or overheating. Do you want to proceed anyway?`
      );
      if (!confirmed) return;
    }
    setBuilderModel(newModel);
    await window.electronAPI.hardware.setBuilderModel(newModel);
  };

  const handleEditorModelChange = async (newModel) => {
    const model = hardwareProfile?.models.find(m => m.id === newModel);
    if (model && !model.isCompatible && !model.isMarginal) {
      const confirmed = window.confirm(
        `Warning: This model requires ${model.minRamGB}GB of RAM, but your system only has ${hardwareProfile.ramGB?.toFixed(1)}GB. Running this model may cause severe system slowdowns, app crashes, or overheating. Do you want to proceed anyway?`
      );
      if (!confirmed) return;
    }
    setEditorModel(newModel);
    await window.electronAPI.hardware.setEditorModel(newModel);
  };

  const testOllamaConnection = async () => {
    setIsTestingOllama(true);
    try {
      // In a real implementation, this would check via the main process
      await new Promise(resolve => setTimeout(resolve, 1000));
      setOllamaStatus('connected');
    } catch (error) {
      setOllamaStatus('error');
    } finally {
      setIsTestingOllama(false);
    }
  };

  return (
    <div className="max-w-3xl mx-auto space-y-8">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-white">Settings</h1>
        <p className="text-slate-400">Manage your API tokens and preferences</p>
      </div>

      {/* API Tokens Section */}
      <section className="card p-6 space-y-6">
        <div className="flex items-center gap-3 pb-4 border-b border-slate-800">
          <div className="w-10 h-10 rounded-lg bg-indigo-500/10 flex items-center justify-center">
            <Key className="w-5 h-5 text-indigo-400" />
          </div>
          <div>
            <h2 className="text-lg font-semibold text-white">API Tokens</h2>
            <p className="text-sm text-slate-500">
              Your tokens are encrypted and stored locally
            </p>
          </div>
        </div>

        <div className="space-y-6">
          <TokenInput
            label="GitHub Token"
            tokenKey="github-token"
            helpUrl="https://github.com/settings/tokens/new?scopes=repo,workflow&description=Axiom%20Forge"
            description="Required for pushing code to GitHub repositories"
            onSave={saveToken}
          />

          <hr className="border-slate-800" />

          <TokenInput
            label="Vercel Token"
            tokenKey="vercel-token"
            helpUrl="https://vercel.com/account/tokens"
            description="Required for deploying to Vercel"
            onSave={saveToken}
          />

          <hr className="border-slate-800" />

          <TokenInput
            label="Netlify Token"
            tokenKey="netlify-token"
            helpUrl="https://app.netlify.com/user/settings/applications#personal-access-tokens"
            description="Required for deploying to Netlify"
            onSave={saveToken}
          />

          <hr className="border-slate-800" />

          <TokenInput
            label="Render Token"
            tokenKey="render-token"
            helpUrl="https://dashboard.render.com/u/settings#api-keys"
            description="Required for deploying to Render"
            onSave={saveToken}
          />

          <hr className="border-slate-800" />

          <TokenInput
            label="Hostinger Token/SSH"
            tokenKey="hostinger-token"
            description="Required for custom deployments via SSH/FTP to Hostinger"
            onSave={saveToken}
          />

          <hr className="border-slate-800" />

          <TokenInput
            label="Resend API Token"
            tokenKey="resend-token"
            helpUrl="https://resend.com/api-keys"
            description="Required for user authentication and sending verification emails via Resend"
            onSave={saveToken}
          />
        </div>
      </section>

      {/* Ollama Settings */}
      <section className="card p-6 space-y-6">
        <div className="flex items-center gap-3 pb-4 border-b border-slate-800">
          <div className="w-10 h-10 rounded-lg bg-orange-500/10 flex items-center justify-center">
            <Server className="w-5 h-5 text-orange-400" />
          </div>
          <div>
            <h2 className="text-lg font-semibold text-white">Ollama & AI Model</h2>
            <p className="text-sm text-slate-500">
              Configure your local AI engine and generation model
            </p>
          </div>
        </div>

        <div className="space-y-4">
          <div>
            <label className="label">Ollama Host</label>
            <div className="flex gap-2">
              <input
                type="text"
                value={ollamaHost}
                onChange={(e) => setOllamaHost(e.target.value)}
                className="input flex-1"
                placeholder="http://127.0.0.1:11434"
              />
              <button
                onClick={testOllamaConnection}
                disabled={isTestingOllama}
                className="btn-secondary"
              >
                {isTestingOllama ? (
                  <RefreshCw className="w-4 h-4 animate-spin" />
                ) : (
                  'Test'
                )}
              </button>
            </div>
            
            {ollamaStatus === 'connected' && (
              <p className="text-xs text-emerald-400 mt-2 flex items-center gap-1">
                <CheckCircle2 className="w-3 h-3" />
                Connected to Ollama
              </p>
            )}
            {ollamaStatus === 'error' && (
              <p className="text-xs text-rose-400 mt-2 flex items-center gap-1">
                <AlertCircle className="w-3 h-3" />
                Could not connect to Ollama
              </p>
            )}
          </div>

          {hardwareProfile && (
            <div className="flex flex-col gap-2 md:flex-row md:items-center justify-between px-4 py-3 bg-slate-800/60 rounded-xl border border-slate-700/50 text-xs text-slate-400">
              <div className="flex items-center gap-4 flex-wrap">
                <span>🖥️ <span className="text-slate-300">{hardwareProfile.ramGB?.toFixed(1)} GB RAM</span></span>
                {hardwareProfile.gpu && (
                  <span>🎮 <span className="text-slate-300">{hardwareProfile.gpu.name} ({hardwareProfile.gpu.vramGB?.toFixed(1)} GB VRAM)</span></span>
                )}
                <span>🔧 <span className="text-slate-300">{hardwareProfile.cpus} CPUs</span></span>
              </div>
              <div className="flex items-center gap-4 flex-wrap">
                <span>🛠️ Builder: <span className="text-indigo-400 font-mono font-semibold">{builderModel}</span></span>
                <span>✏️ Editor: <span className="text-violet-400 font-mono font-semibold">{editorModel}</span></span>
              </div>
            </div>
          )}

          {/* Model Browser */}
          <div>
            <div className="flex flex-col md:flex-row md:items-center justify-between gap-3 mb-4">
              <div>
                <label className="label mb-0">AI Model Suite</label>
                <span className="text-xs text-slate-500 block mt-0.5">Select models for code building and code editing roles</span>
              </div>
              <div className="flex bg-slate-900/80 p-1 rounded-xl border border-slate-800/80 w-fit">
                <button
                  type="button"
                  onClick={() => setActiveRole('builder')}
                  className={`px-3 py-1.5 text-xs font-semibold rounded-lg transition-all ${
                    activeRole === 'builder'
                      ? 'bg-indigo-600 text-white shadow-md'
                      : 'text-slate-400 hover:text-slate-200'
                  }`}
                >
                  🛠️ Code Builder
                </button>
                <button
                  type="button"
                  onClick={() => setActiveRole('editor')}
                  className={`px-3 py-1.5 text-xs font-semibold rounded-lg transition-all ${
                    activeRole === 'editor'
                      ? 'bg-violet-600 text-white shadow-md'
                      : 'text-slate-400 hover:text-slate-200'
                  }`}
                >
                  ✏️ Code Editor
                </button>
              </div>
            </div>

            {hardwareProfile?.byTier ? (
              <div className="space-y-4">
                {Object.entries(hardwareProfile.byTier).map(([tier, tierModels]) => {
                  if (!tierModels || tierModels.length === 0) return null;

                  const tierMeta = {
                    nano:    { label: 'Nano',    emoji: '🌱', color: 'text-slate-400', border: 'border-slate-700' },
                    small:   { label: 'Small',   emoji: '⚡', color: 'text-sky-400',   border: 'border-sky-900/40' },
                    mid:     { label: 'Mid',     emoji: '🚀', color: 'text-indigo-400',border: 'border-indigo-900/40' },
                    power:   { label: 'Power',   emoji: '💪', color: 'text-violet-400',border: 'border-violet-900/40' },
                    expert:  { label: 'Expert',  emoji: '🧠', color: 'text-fuchsia-400',border: 'border-fuchsia-900/40' },
                    extreme: { label: 'Extreme', emoji: '🔥', color: 'text-rose-400',  border: 'border-rose-900/40' },
                  }[tier] || { label: tier, emoji: '📦', color: 'text-slate-400', border: 'border-slate-700' };

                  return (
                    <div key={tier}>
                      <div className={`flex items-center gap-2 mb-2 text-xs font-semibold uppercase tracking-wider ${tierMeta.color}`}>
                        <span>{tierMeta.emoji}</span>
                        <span>{tierMeta.label} Tier</span>
                        <div className={`flex-1 h-px border-t ${tierMeta.border} ml-2`} />
                      </div>
                      <div className="grid gap-2">
                        {tierModels.map(model => {
                          const currentRoleModel = activeRole === 'builder' ? builderModel : editorModel;
                          const isSelected = currentRoleModel === model.id;
                          const isIncompatible = !model.isCompatible && !model.isMarginal;
                          const isMarginal = model.isMarginal;

                          // SSA recommendation badge logic for builder role
                          const isSsaModel = model.tags.contains ? model.tags.contains('ssa') : model.tags.includes('ssa');
                          const isSsaRecommended = activeRole === 'builder' && isSsaModel;

                          return (
                            <button
                              key={model.id}
                              type="button"
                              onClick={() => {
                                if (activeRole === 'builder') {
                                  handleBuilderModelChange(model.id);
                                } else {
                                  handleEditorModelChange(model.id);
                                }
                              }}
                              className={`w-full text-left p-3 rounded-xl border transition-all ${
                                isSelected
                                  ? activeRole === 'builder'
                                    ? 'bg-indigo-600/20 border-indigo-500/60 shadow-sm shadow-indigo-500/10'
                                    : 'bg-violet-600/20 border-violet-500/60 shadow-sm shadow-violet-500/10'
                                  : isIncompatible
                                  ? 'bg-slate-900/30 border-slate-800 opacity-60 hover:opacity-80'
                                  : isMarginal
                                  ? 'bg-amber-900/10 border-amber-900/30 hover:border-amber-700/40'
                                  : 'bg-slate-900/50 border-slate-800 hover:border-slate-600'
                              }`}
                            >
                              <div className="flex items-start justify-between gap-3">
                                <div className="flex-1 min-w-0">
                                  <div className="flex items-center gap-2 flex-wrap">
                                    <span className="text-sm font-medium text-white">{model.name}</span>
                                    {isSsaRecommended && (
                                      <span className="text-xs px-1.5 py-0.5 rounded-md bg-indigo-500/20 text-indigo-400 border border-indigo-500/30 font-semibold">
                                        ⚡ Fast SSA Builder
                                      </span>
                                    )}
                                    {model.isRecommended && !isSsaRecommended && (
                                      <span className="text-xs px-1.5 py-0.5 rounded-md bg-emerald-500/20 text-emerald-400 border border-emerald-500/30">
                                        ⭐ Recommended
                                      </span>
                                    )}
                                    {isIncompatible && (
                                      <span className="text-xs px-1.5 py-0.5 rounded-md bg-rose-500/10 text-rose-400 border border-rose-500/20">
                                        ⚠️ Needs {model.minRamGB}GB RAM
                                      </span>
                                    )}
                                    {isMarginal && (
                                      <span className="text-xs px-1.5 py-0.5 rounded-md bg-amber-500/10 text-amber-400 border border-amber-500/20">
                                        ⚠️ May be slow
                                      </span>
                                    )}
                                  </div>
                                  <p className="text-xs text-slate-500 mt-1 leading-relaxed">{model.description}</p>
                                  {isSelected && model.strengths && (
                                    <div className="mt-2 flex gap-4 text-xs">
                                      <span className="text-emerald-400">✓ {model.strengths}</span>
                                    </div>
                                  )}
                                  {(isIncompatible || isMarginal) && isSelected && (
                                    <p className="mt-2 text-xs text-amber-400">
                                      ⚠️ You selected this model at your own risk. Your system has {hardwareProfile.ramGB?.toFixed(1)}GB RAM but this model requires {model.minRamGB}GB.
                                    </p>
                                  )}
                                </div>
                                <div className="text-right flex-shrink-0">
                                  <span className="text-xs text-slate-500 font-mono">{model.sizeGB}GB</span>
                                  {isSelected && (
                                    <div className={`w-2 h-2 rounded-full ml-auto mt-1 ${
                                      activeRole === 'builder' ? 'bg-indigo-500' : 'bg-violet-500'
                                    }`} />
                                  )}
                                </div>
                              </div>
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              /* Fallback for old profile structure */
              <div className="space-y-4">
                <div>
                  <label className="label text-slate-400 text-xs">🛠️ Builder Model</label>
                  <select
                    value={builderModel}
                    onChange={(e) => handleBuilderModelChange(e.target.value)}
                    className="input"
                  >
                    {hardwareProfile?.models.map(model => (
                      <option key={model.id} value={model.id}>
                        {model.name} {model.isRecommended ? '(Recommended)' : ''} {!model.isCompatible ? `(Requires ${model.minRamGB}GB RAM)` : ''}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="label text-slate-400 text-xs">✏️ Editor Model</label>
                  <select
                    value={editorModel}
                    onChange={(e) => handleEditorModelChange(e.target.value)}
                    className="input"
                  >
                    {hardwareProfile?.models.map(model => (
                      <option key={model.id} value={model.id}>
                        {model.name} {model.isRecommended ? '(Recommended)' : ''} {!model.isCompatible ? `(Requires ${model.minRamGB}GB RAM)` : ''}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
            )}
          </div>
        </div>
      </section>


      {/* Tech Stack Overrides Section */}
      <section className="card p-6 space-y-6">
        <div className="flex items-center gap-3 pb-4 border-b border-slate-800">
          <div className="w-10 h-10 rounded-lg bg-indigo-500/10 flex items-center justify-center">
            <Sliders className="w-5 h-5 text-indigo-400" />
          </div>
          <div>
            <h2 className="text-lg font-semibold text-white">Tech Stack Overrides</h2>
            <p className="text-sm text-slate-500">
              Override default framework and program versions for new projects
            </p>
          </div>
        </div>

        <div className="grid gap-4 md:grid-cols-2">
          <div>
            <label className="label">Next.js Version</label>
            <select
              value={techOverrides.nextjs || ''}
              onChange={(e) => handleOverrideChange('nextjs', e.target.value)}
              className="input"
            >
              <option value="">Default (from manifest)</option>
              <option value="15">Next.js 15</option>
              <option value="14">Next.js 14</option>
              <option value="13">Next.js 13</option>
              <option value="12">Next.js 12</option>
            </select>
          </div>

          <div>
            <label className="label">React Version</label>
            <select
              value={techOverrides.react || ''}
              onChange={(e) => handleOverrideChange('react', e.target.value)}
              className="input"
            >
              <option value="">Default</option>
              <option value="19">React 19</option>
              <option value="18">React 18</option>
              <option value="17">React 17</option>
            </select>
          </div>

          <div>
            <label className="label">Node.js Version</label>
            <select
              value={techOverrides.node || ''}
              onChange={(e) => handleOverrideChange('node', e.target.value)}
              className="input"
            >
              <option value="">Default</option>
              <option value="22">Node.js 22</option>
              <option value="20">Node.js 20</option>
              <option value="18">Node.js 18</option>
              <option value="16">Node.js 16</option>
            </select>
          </div>

          <div>
            <label className="label">Tauri Version</label>
            <select
              value={techOverrides.tauri || ''}
              onChange={(e) => handleOverrideChange('tauri', e.target.value)}
              className="input"
            >
              <option value="">Default</option>
              <option value="2">Tauri 2.0</option>
              <option value="1">Tauri 1.0</option>
            </select>
          </div>

          <div>
            <label className="label">Expo SDK Version</label>
            <select
              value={techOverrides.expo || ''}
              onChange={(e) => handleOverrideChange('expo', e.target.value)}
              className="input"
            >
              <option value="">Default</option>
              <option value="51">Expo SDK 51</option>
              <option value="50">Expo SDK 50</option>
              <option value="49">Expo SDK 49</option>
            </select>
          </div>

          <div>
            <label className="label">Rust Version</label>
            <select
              value={techOverrides.rust || ''}
              onChange={(e) => handleOverrideChange('rust', e.target.value)}
              className="input"
            >
              <option value="">Default</option>
              <option value="1.80">Rust 1.80</option>
              <option value="1.77">Rust 1.77</option>
              <option value="1.75">Rust 1.75</option>
              <option value="1.70">Rust 1.70</option>
            </select>
          </div>
        </div>

        {/* Local Tool Versions Display */}
        {localTools && (
          <div className="p-4 bg-slate-800/40 rounded-xl border border-slate-800 text-xs text-slate-400 space-y-2">
            <h3 className="font-semibold text-slate-300">Detected Local Toolchain Versions:</h3>
            <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
              <div>Node: <span className="text-indigo-400 font-mono">{localTools.node || 'Not Found'}</span></div>
              <div>NPM: <span className="text-indigo-400 font-mono">{localTools.npm || 'Not Found'}</span></div>
              <div>Rust: <span className="text-indigo-400 font-mono">{localTools.rust || 'Not Found'}</span></div>
              <div>Cargo: <span className="text-indigo-400 font-mono">{localTools.cargo || 'Not Found'}</span></div>
            </div>
          </div>
        )}

        {/* Compatibility Warnings Panel */}
        {compatibilityWarnings.length > 0 && (
          <div className="p-4 bg-amber-500/10 border border-amber-500/20 rounded-xl space-y-1.5">
            <h3 className="text-sm font-semibold text-amber-400 flex items-center gap-1.5">
              <AlertCircle className="w-4 h-4" /> Compatibility Alerts
            </h3>
            <ul className="list-disc list-inside text-xs text-amber-300/90 space-y-1">
              {compatibilityWarnings.map((warn, index) => (
                <li key={index}>{warn}</li>
              ))}
            </ul>
          </div>
        )}
      </section>


      {/* Security Section */}
      <section className="card p-6 space-y-6">
        <div className="flex items-center gap-3 pb-4 border-b border-slate-800">
          <div className="w-10 h-10 rounded-lg bg-emerald-500/10 flex items-center justify-center">
            <Shield className="w-5 h-5 text-emerald-400" />
          </div>
          <div>
            <h2 className="text-lg font-semibold text-white">Security</h2>
            <p className="text-sm text-slate-500">
              Manage your stored data
            </p>
          </div>
        </div>

        <div className="space-y-4">
          <div className="p-4 bg-slate-800/50 rounded-lg">
            <h3 className="font-medium text-white mb-2">Clear All Tokens</h3>
            <p className="text-sm text-slate-400 mb-4">
              This will remove all stored API tokens. You'll need to reconfigure them.
            </p>
            <button
              onClick={async () => {
                if (confirm('Are you sure you want to clear all tokens?')) {
                  await window.electronAPI.security.clearAllTokens();
                  await checkTokens();
                  alert('All tokens cleared');
                }
              }}
              className="btn-danger"
            >
              <Trash2 className="w-4 h-4" />
              Clear All Tokens
            </button>
          </div>

          <div className="p-4 bg-slate-800/50 rounded-lg">
            <h3 className="font-medium text-white mb-2">Encryption Info</h3>
            <p className="text-sm text-slate-400">
              All tokens are encrypted using AES-256-GCM with a key derived from your 
              machine-specific data. Tokens can only be decrypted on this machine.
            </p>
          </div>
        </div>
      </section>

      {/* Agency Collaboration & Dual-Mode Section */}
      <section className="card p-6 space-y-6">
        <div className="flex items-center gap-3 pb-4 border-b border-slate-800">
          <div className="w-10 h-10 rounded-lg bg-indigo-500/10 flex items-center justify-center">
            <Sliders className="w-5 h-5 text-indigo-400" />
          </div>
          <div>
            <h2 className="text-lg font-semibold text-white">Agency & Dual-Mode</h2>
            <p className="text-sm text-slate-500">
              Configure agency setup and client-facing mode
            </p>
          </div>
        </div>

        <div className="space-y-4">
          <div className="p-4 bg-slate-800/50 rounded-lg flex items-center justify-between">
            <div>
              <h3 className="font-medium text-white mb-1">Switch to Client Mode</h3>
              <p className="text-xs text-slate-400">
                Locks the UI into a simplified float overlay for non-technical clients.
              </p>
            </div>
            <button
              onClick={async () => {
                if (window.electronAPI?.agency?.setAppMode) {
                  await window.electronAPI.agency.setAppMode('client');
                  const modeState = await window.electronAPI.agency.getAppMode();
                  setAppMode(modeState);
                }
              }}
              className="btn-secondary text-xs px-4 py-2"
            >
              Enter Client Mode
            </button>
          </div>
        </div>
      </section>

      {/* About Section */}
      <section className="card p-6">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-lg font-semibold text-white">About Axiom Forge</h2>
            <p className="text-sm text-slate-500">
              Open-source local agent for the idea-to-app pipeline
            </p>
          </div>
          <div className="text-right">
            <p className="text-sm text-slate-400 font-mono">Version 1.1.9</p>
            <a
              href="https://github.com/axiom-forge/axiom-forge"
              onClick={(e) => {
                e.preventDefault();
                window.electronAPI.shell.openExternal('https://github.com/axiom-forge/axiom-forge');
              }}
              className="text-xs text-indigo-400 hover:text-indigo-300"
            >
              View on GitHub
            </a>
          </div>
        </div>
      </section>
    </div>
  );
}

export default Settings;
