/**
 * Axiom Forge - Installation Wizard
 * Step-by-step onboarding for first-time users
 */

import React, { useState } from 'react';
import { 
  Check, 
  ChevronRight, 
  ChevronLeft, 
  Key, 
  Server, 
  Sparkles,
  ExternalLink,
  Eye,
  EyeOff,
  AlertCircle,
  Play,
  Globe
} from 'lucide-react';
import { useAppStore } from '../hooks/useAppStore';

const STEPS = [
  { id: 'welcome', title: 'Welcome', description: 'Get started with Axiom Forge' },
  { id: 'ollama', title: 'AI Brain', description: 'Configure local AI' },
  { id: 'github', title: 'GitHub', description: 'Connect your account' },
  { id: 'hosting', title: 'Hosting Tokens', description: 'Deploy with ease' },
  { id: 'complete', title: 'Complete', description: 'Ready to build' }
];

function StepIndicator({ currentStep, steps }) {
  return (
    <div className="flex items-center justify-center gap-2 mb-8">
      {steps.map((step, index) => {
        const isActive = index === currentStep;
        const isCompleted = index < currentStep;
        
        return (
          <React.Fragment key={step.id}>
            <div className="flex flex-col items-center">
              <div
                className={`w-10 h-10 rounded-full flex items-center justify-center font-semibold transition-all ${
                  isActive
                    ? 'bg-indigo-600 text-white shadow-lg shadow-indigo-500/30'
                    : isCompleted
                    ? 'bg-emerald-600 text-white'
                    : 'bg-slate-800 text-slate-500 border border-slate-700'
                }`}
              >
                {isCompleted ? (
                  <Check className="w-5 h-5" />
                ) : (
                  index + 1
                )}
              </div>
              <span className={`text-xs mt-2 ${isActive ? 'text-indigo-400' : 'text-slate-500'}`}>
                {step.title}
              </span>
            </div>
            {index < steps.length - 1 && (
              <div className={`w-12 h-0.5 ${isCompleted ? 'bg-emerald-600' : 'bg-slate-800'}`} />
            )}
          </React.Fragment>
        );
      })}
    </div>
  );
}

function WelcomeStep({ onNext }) {
  return (
    <div className="text-center space-y-6">
      <div className="w-20 h-20 mx-auto rounded-2xl bg-gradient-to-br from-indigo-500 to-violet-600 flex items-center justify-center shadow-xl shadow-indigo-500/20">
        <Sparkles className="w-10 h-10 text-white" />
      </div>
      
      <div>
        <h2 className="text-3xl font-bold text-white mb-3">
          Welcome to <span className="gradient-text">Axiom Forge</span>
        </h2>
        <p className="text-slate-400 max-w-md mx-auto">
          Your open-source local agent for the idea-to-app pipeline. 
          Generate, configure, and deploy applications using local AI.
        </p>
      </div>

      <div className="grid grid-cols-3 gap-4 max-w-lg mx-auto">
        <div className="p-4 bg-slate-900/50 border border-slate-800 rounded-xl">
          <div className="w-8 h-8 rounded-lg bg-indigo-500/10 flex items-center justify-center mb-2">
            <Server className="w-4 h-4 text-indigo-400" />
          </div>
          <p className="text-sm font-medium text-slate-300">Local AI</p>
          <p className="text-xs text-slate-500">Privacy-first</p>
        </div>
        <div className="p-4 bg-slate-900/50 border border-slate-800 rounded-xl">
          <div className="w-8 h-8 rounded-lg bg-violet-500/10 flex items-center justify-center mb-2">
            <Key className="w-4 h-4 text-violet-400" />
          </div>
          <p className="text-sm font-medium text-slate-300">Secure</p>
          <p className="text-xs text-slate-500">Encrypted tokens</p>
        </div>
        <div className="p-4 bg-slate-900/50 border border-slate-800 rounded-xl">
          <div className="w-8 h-8 rounded-lg bg-fuchsia-500/10 flex items-center justify-center mb-2">
            <Sparkles className="w-4 h-4 text-fuchsia-400" />
          </div>
          <p className="text-sm font-medium text-slate-300">Automated</p>
          <p className="text-xs text-slate-500">One-click deploy</p>
        </div>
      </div>

      <button onClick={onNext} className="btn-primary px-8">
        Get Started
        <ChevronRight className="w-5 h-5" />
      </button>
    </div>
  );
}

function OllamaStep({ onNext, onBack }) {
  const [status, setStatus] = useState('scanning'); // scanning, checking, engine_missing, model_missing, ready, error
  const [isInstalled, setIsInstalled] = useState(false);
  const [message, setMessage] = useState('Scanning hardware...');
  
  const [hardwareProfile, setHardwareProfile] = useState(null);
  const [selectedModel, setSelectedModel] = useState('');
  const [ollamaModels, setOllamaModels] = useState([]);

  const [pullProgress, setPullProgress] = useState(0);
  const [pullStatus, setPullStatus] = useState('');
  const [isPulling, setIsPulling] = useState(false);

  // 1. Initial Hardware Scan
  const scanHardware = async () => {
    setStatus('scanning');
    setMessage('Scanning hardware capabilities...');
    try {
      const profile = await window.electronAPI.hardware.getProfile();
      setHardwareProfile(profile);
      setSelectedModel(profile.recommendedModelId);
      
      // Save default selected model immediately so context is aware
      await window.electronAPI.hardware.setSelectedModel(profile.recommendedModelId);
      
      await checkOllama(profile.recommendedModelId);
    } catch (error) {
      console.error('[Hardware] Scan failed:', error);
      setStatus('error');
      setMessage('Failed to scan hardware');
    }
  };

  // 2. Check Ollama & Model presence
  const checkOllama = async (modelToCheck) => {
    setStatus('checking');
    setMessage('Checking AI Engine status...');
    
    try {
      const installedStatus = await window.electronAPI.ollama.checkInstalled();
      setIsInstalled(installedStatus.installed);

      const health = await window.electronAPI.ollama.checkHealth();
      
      if (!health.available) {
        setStatus('engine_missing');
        setMessage(installedStatus.installed ? 'Ollama Engine is installed but stopped' : 'Local AI Engine not found');
        return;
      }

      setOllamaModels(health.models || []);

      const hasRequiredModel = (health.models || []).some(m => m === modelToCheck || m.startsWith(modelToCheck));
      
      if (!hasRequiredModel) {
        setStatus('model_missing');
        setMessage('Selected AI Model not found locally');
        return;
      }

      setStatus('ready');
      setMessage('AI Engine is ready to forge!');
    } catch (error) {
      console.error('[Ollama] Check failed:', error);
      setStatus('error');
      setMessage('Failed to communicate with AI Engine');
    }
  };

  // Change model selection
  const handleModelSelect = async (modelId) => {
    setSelectedModel(modelId);
    await window.electronAPI.hardware.setSelectedModel(modelId);
    await checkOllama(modelId);
  };

  const handleStartServer = async () => {
    setStatus('checking');
    setMessage('Waking up AI engine...');
    try {
      const result = await window.electronAPI.ollama.startServer();
      if (result.success) {
        await checkOllama(selectedModel);
      } else {
        setStatus('error');
        setMessage(result.error || 'Failed to start AI Engine');
      }
    } catch (error) {
      setStatus('error');
      setMessage('Could not launch AI Engine process');
    }
  };

  const handlePullModel = async () => {
    setIsPulling(true);
    setPullProgress(0);
    setPullStatus('Starting download...');

    try {
      const result = await window.electronAPI.ollama.pullModel(selectedModel);
      if (result.success) {
        setStatus('ready');
        setMessage('Model downloaded successfully!');
      } else {
        setStatus('error');
        setMessage(result.error || 'Failed to download model');
      }
    } catch (error) {
      setStatus('error');
      setMessage('Download interrupted');
    } finally {
      setIsPulling(false);
    }
  };

  React.useEffect(() => {
    scanHardware();

    const unsubscribe = window.electronAPI.ollama.onPullProgress((data) => {
      if (data.percent) setPullProgress(data.percent);
      if (data.status) setPullStatus(data.status);
    });
    return () => unsubscribe();
  }, []);

  const selectedModelObj = hardwareProfile?.models.find(m => m.id === selectedModel);

  return (
    <div className="space-y-6">
      <div className="text-center">
        <div className="w-16 h-16 mx-auto rounded-xl bg-gradient-to-br from-orange-500 to-red-600 flex items-center justify-center mb-4 shadow-lg shadow-orange-500/20">
          <Server className="w-8 h-8 text-white" />
        </div>
        <h2 className="text-2xl font-bold text-white mb-2">Local AI Setup</h2>
        <p className="text-slate-400">
          Axiom Forge runs AI completely privately on your hardware
        </p>
      </div>

      <div className="card p-6 max-w-md mx-auto space-y-6">
        
        {/* Hardware Status Header */}
        <div className="flex items-center gap-4 pb-4 border-b border-slate-800/50">
          <div className={`w-3 h-3 rounded-full ${
            status === 'scanning' || status === 'checking' || isPulling ? 'bg-amber-500 animate-pulse' :
            status === 'ready' ? 'bg-emerald-500' :
            'bg-rose-500'
          }`} />
          <div className="flex-1">
            <span className="text-sm font-medium text-slate-300 block">{message}</span>
            {hardwareProfile && (
              <span className="text-xs text-slate-500">
                Detected: {hardwareProfile.ramGB.toFixed(1)} GB RAM • {hardwareProfile.cpus} Logical Cores
              </span>
            )}
          </div>
        </div>

        {/* Model Selection */}
        {hardwareProfile && (
          <div className="space-y-3">
            <label className="text-xs font-semibold text-slate-400 uppercase tracking-wider">
              Select AI Brain
            </label>
            <div className="space-y-2 max-h-[220px] overflow-y-auto pr-1">
              {hardwareProfile.models.map(model => (
                <button
                  key={model.id}
                  onClick={() => handleModelSelect(model.id)}
                  disabled={!model.isCompatible || isPulling}
                  className={`w-full text-left p-3 rounded-xl border transition-all ${
                    selectedModel === model.id
                      ? 'border-indigo-500 bg-indigo-500/10'
                      : !model.isCompatible 
                        ? 'border-slate-800/30 bg-slate-900/30 opacity-50 cursor-not-allowed'
                        : 'border-slate-800 hover:border-slate-700 bg-slate-900/50 hover:bg-slate-800/50'
                  }`}
                >
                  <div className="flex justify-between items-center mb-1">
                    <span className={`text-sm font-semibold ${selectedModel === model.id ? 'text-indigo-400' : 'text-slate-300'}`}>
                      {model.name}
                    </span>
                    {model.isRecommended && (
                      <span className="text-[10px] bg-emerald-500/20 text-emerald-400 px-2 py-0.5 rounded-full font-medium">
                        Recommended
                      </span>
                    )}
                    {!model.isCompatible && (
                      <span className="text-[10px] text-rose-400">
                        Requires {model.minRamGB}GB RAM
                      </span>
                    )}
                  </div>
                  <div className="text-[11px] text-slate-500">{model.description}</div>
                  <div className="text-[10px] text-slate-600 mt-1">Download size: ~{model.sizeGB} GB</div>
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Engine Missing */}
        {status === 'engine_missing' && (
          <div className="p-4 bg-rose-500/10 border border-rose-500/20 rounded-xl space-y-4">
            <div className="flex items-start gap-3">
              <AlertCircle className="w-5 h-5 text-rose-400 flex-shrink-0 mt-0.5" />
              <div>
                <p className="text-sm text-rose-400 font-medium mb-1">
                  {isInstalled ? 'Ollama is stopped' : 'Ollama Engine is required'}
                </p>
                <p className="text-xs text-rose-300/70 mb-3">
                  {isInstalled 
                    ? 'The Ollama software is installed, but the background service is not running. Please start it.' 
                    : 'To protect your privacy and run AI locally, we use the open-source Ollama engine. Please install it to continue.'}
                </p>
                {!isInstalled && (
                  <button 
                    onClick={() => window.electronAPI.shell.openExternal('https://ollama.com/download')}
                    className="flex items-center justify-center w-full gap-2 py-2 px-4 bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 text-xs font-semibold rounded-lg transition-colors"
                  >
                    Download Ollama (.exe)
                    <ExternalLink className="w-3 h-3" />
                  </button>
                )}
              </div>
            </div>
            
            {isInstalled ? (
              <button onClick={handleStartServer} className="btn-primary w-full h-10">
                <Play className="w-4 h-4 mr-2" /> Start Ollama Engine
              </button>
            ) : (
              <button onClick={() => checkOllama(selectedModel)} className="btn-secondary w-full h-10">
                I have installed it, check again
              </button>
            )}
          </div>
        )}

        {/* Model Missing */}
        {status === 'model_missing' && !isPulling && (
          <div className="p-4 bg-indigo-500/10 border border-indigo-500/20 rounded-xl">
            <div className="flex flex-col gap-3">
              <div className="flex items-start gap-3">
                <Sparkles className="w-5 h-5 text-indigo-400 flex-shrink-0 mt-0.5" />
                <div>
                  <p className="text-sm text-indigo-400 font-medium mb-1">Download Required</p>
                  <p className="text-xs text-indigo-300/70">
                    We need to download the <strong>{selectedModelObj?.name}</strong> AI model (~{selectedModelObj?.sizeGB} GB) before you can start forging code.
                  </p>
                </div>
              </div>
              <button onClick={handlePullModel} className="btn-primary w-full h-10">
                Download {selectedModelObj?.name}
                <ChevronRight className="w-4 h-4 ml-1" />
              </button>
            </div>
          </div>
        )}

        {/* Pulling Progress */}
        {isPulling && (
          <div className="p-4 bg-slate-900 border border-slate-800 rounded-xl space-y-4">
            <div className="flex items-center justify-between text-xs mb-1">
              <span className="text-slate-400 line-clamp-1 mr-4">{pullStatus}</span>
              <span className="text-indigo-400 font-bold whitespace-nowrap">{pullProgress}%</span>
            </div>
            <div className="w-full h-2.5 bg-slate-950 rounded-full overflow-hidden border border-slate-800">
              <div 
                className="h-full bg-indigo-500 transition-all duration-300 ease-out"
                style={{ width: `${pullProgress}%` }}
              />
            </div>
            <p className="text-[10px] text-slate-500 text-center italic">
              Please keep Axiom Forge open. Depending on your internet speed, this may take a few minutes.
            </p>
          </div>
        )}

        {/* Ready */}
        {status === 'ready' && !isPulling && (
          <div className="p-4 bg-emerald-500/10 border border-emerald-500/20 rounded-xl">
            <div className="flex items-center gap-3">
              <div className="w-8 h-8 rounded-full bg-emerald-500/20 flex items-center justify-center">
                <Check className="w-4 h-4 text-emerald-400" />
              </div>
              <div>
                <p className="text-sm text-emerald-400 font-semibold">Engine Configured</p>
                <p className="text-xs text-emerald-500/70">Using {selectedModelObj?.name}</p>
              </div>
            </div>
          </div>
        )}

        {status === 'error' && (
           <button onClick={() => scanHardware()} className="btn-secondary w-full">
             Scan Again
           </button>
        )}
      </div>

      <div className="flex justify-between max-w-md mx-auto">
        <button onClick={onBack} className="btn-ghost" disabled={isPulling}>
          <ChevronLeft className="w-5 h-5" />
          Back
        </button>
        <button 
          onClick={onNext} 
          className="btn-primary"
          disabled={status !== 'ready' || isPulling}
        >
          Continue
          <ChevronRight className="w-5 h-5" />
        </button>
      </div>
    </div>
  );
}

function GitHubStep({ onNext, onBack }) {
  const [token, setToken] = useState('');
  const [showToken, setShowToken] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState(null);
  const { saveToken } = useAppStore();

  const handleSave = async () => {
    if (!token.trim()) {
      setError('Please enter a GitHub token');
      return;
    }

    setIsSaving(true);
    setError(null);

    try {
      await saveToken('github-token', token);
      onNext();
    } catch (err) {
      setError(err.message);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="space-y-6">
      <div className="text-center">
        <div className="w-16 h-16 mx-auto rounded-xl bg-gradient-to-br from-slate-700 to-slate-800 flex items-center justify-center mb-4 border border-slate-600">
          <svg className="w-8 h-8 text-white" fill="currentColor" viewBox="0 0 24 24">
            <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
          </svg>
        </div>
        <h2 className="text-2xl font-bold text-white mb-2">Connect GitHub</h2>
        <p className="text-slate-400">
          Store your code and enable automatic deployments
        </p>
      </div>

      <div className="card p-6 max-w-md mx-auto space-y-4">
        <div>
          <label className="label flex items-center gap-2">
            <Key className="w-4 h-4" />
            Personal Access Token
          </label>
          <div className="relative">
            <input
              type={showToken ? 'text' : 'password'}
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="ghp_xxxxxxxxxxxxxxxxxxxx"
              className="input pr-10"
            />
            <button
              onClick={() => setShowToken(!showToken)}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300"
            >
              {showToken ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
            </button>
          </div>
          <p className="text-xs text-slate-500 mt-1.5">
            Your token is encrypted and stored locally
          </p>
        </div>

        {error && (
          <div className="p-3 bg-rose-500/10 border border-rose-500/20 rounded-lg">
            <p className="text-sm text-rose-400">{error}</p>
          </div>
        )}

        <a
          href="https://github.com/settings/tokens/new"
          onClick={(e) => {
            e.preventDefault();
            window.electronAPI.shell.openExternal('https://github.com/settings/tokens/new?scopes=repo,workflow&description=Axiom%20Forge');
          }}
          className="flex items-center gap-2 text-sm text-indigo-400 hover:text-indigo-300"
        >
          <ExternalLink className="w-4 h-4" />
          Create a GitHub token
        </a>

        <button
          onClick={handleSave}
          disabled={isSaving}
          className="btn-primary w-full"
        >
          {isSaving ? 'Saving...' : 'Save & Continue'}
          <ChevronRight className="w-5 h-5" />
        </button>
      </div>

      <div className="flex justify-start max-w-md mx-auto">
        <button onClick={onBack} className="btn-ghost">
          <ChevronLeft className="w-5 h-5" />
          Back
        </button>
      </div>
    </div>
  );
}

function HostingStep({ onNext, onBack }) {
  const [vercelToken, setVercelToken] = useState('');
  const [hostingerToken, setHostingerToken] = useState('');
  const [showVercel, setShowVercel] = useState(false);
  const [showHostinger, setShowHostinger] = useState(false);
  
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState(null);
  const { saveToken } = useAppStore();

  const handleSave = async () => {
    // We allow passing without tokens, or we can enforce at least one.
    // For now, let's just save whatever they entered.
    setIsSaving(true);
    setError(null);

    try {
      if (vercelToken.trim()) {
        await saveToken('vercel-token', vercelToken.trim());
      }
      if (hostingerToken.trim()) {
        await saveToken('hostinger-token', hostingerToken.trim());
      }
      // If neither is provided but they click save, we can still proceed (or warn them)
      onNext();
    } catch (err) {
      setError(err.message);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="space-y-6">
      <div className="text-center">
        <div className="w-16 h-16 mx-auto rounded-xl bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center mb-4 border border-slate-800">
          <Globe className="w-8 h-8 text-white" />
        </div>
        <h2 className="text-2xl font-bold text-white mb-2">Hosting Tokens</h2>
        <p className="text-slate-400">
          Connect your favorite web hosting providers
        </p>
      </div>

      <div className="card p-6 max-w-md mx-auto space-y-6">
        
        {/* VERCEL */}
        <div className="p-4 bg-slate-900 border border-slate-800 rounded-xl space-y-4">
          <div className="flex justify-between items-center">
             <div className="flex items-center gap-2">
               <svg className="w-5 h-5 text-white" viewBox="0 0 76 65" fill="none">
                 <path d="M37.5274 0L75.0548 65H0L37.5274 0Z" fill="currentColor"/>
               </svg>
               <span className="font-semibold text-white">Vercel</span>
             </div>
             <a
              href="https://vercel.com/account/tokens"
              onClick={(e) => {
                e.preventDefault();
                window.electronAPI.shell.openExternal('https://vercel.com/account/tokens');
              }}
              className="text-xs text-indigo-400 hover:text-indigo-300 flex items-center gap-1"
            >
              Get Token <ExternalLink className="w-3 h-3" />
            </a>
          </div>
          
          <div className="relative">
            <input
              type={showVercel ? 'text' : 'password'}
              value={vercelToken}
              onChange={(e) => setVercelToken(e.target.value)}
              placeholder="vercel_token_xxxxxxxx"
              className="input pr-10 text-sm"
            />
            <button
              onClick={() => setShowVercel(!showVercel)}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300"
            >
              {showVercel ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
            </button>
          </div>
        </div>

        {/* HOSTINGER */}
        <div className="p-4 bg-slate-900 border border-slate-800 rounded-xl space-y-4 opacity-70 hover:opacity-100 transition-opacity">
          <div className="flex justify-between items-center">
             <div className="flex items-center gap-2">
               <div className="w-5 h-5 bg-purple-600 rounded flex items-center justify-center text-white font-bold text-xs">H</div>
               <span className="font-semibold text-white">Hostinger (SSH/FTP)</span>
             </div>
             <span className="text-[10px] bg-slate-800 text-slate-400 px-2 py-0.5 rounded-full">Coming Soon</span>
          </div>
          
          <div className="relative">
            <input
              type={showHostinger ? 'text' : 'password'}
              value={hostingerToken}
              onChange={(e) => setHostingerToken(e.target.value)}
              placeholder="hostinger_token_xxxxxxxx"
              className="input pr-10 text-sm"
              disabled
            />
            <button
              onClick={() => setShowHostinger(!showHostinger)}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300"
              disabled
            >
              {showHostinger ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
            </button>
          </div>
        </div>

        {error && (
          <div className="p-3 bg-rose-500/10 border border-rose-500/20 rounded-lg">
            <p className="text-sm text-rose-400">{error}</p>
          </div>
        )}

        <button
          onClick={handleSave}
          disabled={isSaving}
          className="btn-primary w-full h-10"
        >
          {isSaving ? 'Saving...' : 'Save & Continue'}
          <ChevronRight className="w-4 h-4 ml-1" />
        </button>
      </div>

      <div className="flex justify-start max-w-md mx-auto">
        <button onClick={onBack} className="btn-ghost">
          <ChevronLeft className="w-5 h-5" />
          Back
        </button>
      </div>
    </div>
  );
}

function CompleteStep() {
  const { setFirstRun } = useAppStore();

  const handleFinish = () => {
    setFirstRun(false);
  };

  return (
    <div className="text-center space-y-6">
      <div className="w-20 h-20 mx-auto rounded-full bg-emerald-500/20 flex items-center justify-center border-2 border-emerald-500">
        <Check className="w-10 h-10 text-emerald-500" />
      </div>
      
      <div>
        <h2 className="text-3xl font-bold text-white mb-3">
          You're All Set!
        </h2>
        <p className="text-slate-400 max-w-md mx-auto">
          Axiom Forge is ready to build. Start by creating your first project 
          or explore the dashboard.
        </p>
      </div>

      <div className="card p-6 max-w-md mx-auto">
        <h3 className="font-semibold text-white mb-4">What's Next?</h3>
        <ul className="space-y-3 text-left">
          <li className="flex items-start gap-3">
            <div className="w-6 h-6 rounded-full bg-indigo-500/10 flex items-center justify-center flex-shrink-0 mt-0.5">
              <span className="text-xs text-indigo-400 font-medium">1</span>
            </div>
            <span className="text-slate-300 text-sm">Browse templates on Axiom Forge</span>
          </li>
          <li className="flex items-start gap-3">
            <div className="w-6 h-6 rounded-full bg-indigo-500/10 flex items-center justify-center flex-shrink-0 mt-0.5">
              <span className="text-xs text-indigo-400 font-medium">2</span>
            </div>
            <span className="text-slate-300 text-sm">Click "Build with Axiom Forge" to start</span>
          </li>
          <li className="flex items-start gap-3">
            <div className="w-6 h-6 rounded-full bg-indigo-500/10 flex items-center justify-center flex-shrink-0 mt-0.5">
              <span className="text-xs text-indigo-400 font-medium">3</span>
            </div>
            <span className="text-slate-300 text-sm">Watch your app come to life!</span>
          </li>
        </ul>
      </div>

      <button onClick={handleFinish} className="btn-primary px-8">
        Go to Dashboard
        <ChevronRight className="w-5 h-5" />
      </button>
    </div>
  );
}

function InstallationWizard() {
  const [currentStep, setCurrentStep] = useState(0);

  const handleNext = () => {
    if (currentStep < STEPS.length - 1) {
      setCurrentStep(currentStep + 1);
    }
  };

  const handleBack = () => {
    if (currentStep > 0) {
      setCurrentStep(currentStep - 1);
    }
  };

  const renderStep = () => {
    switch (currentStep) {
      case 0:
        return <WelcomeStep onNext={handleNext} />;
      case 1:
        return <OllamaStep onNext={handleNext} onBack={handleBack} />;
      case 2:
        return <GitHubStep onNext={handleNext} onBack={handleBack} />;
      case 3:
        return <HostingStep onNext={handleNext} onBack={handleBack} />;
      case 4:
        return <CompleteStep />;
      default:
        return <WelcomeStep onNext={handleNext} />;
    }
  };

  return (
    <div className="min-h-screen bg-slate-950 flex items-center justify-center p-6">
      <div className="w-full max-w-2xl">
        <StepIndicator currentStep={currentStep} steps={STEPS} />
        
        <div className="card p-8">
          {renderStep()}
        </div>

        <p className="text-center text-slate-500 text-sm mt-6">
          Your data is stored locally and encrypted.{' '}
          <button 
            onClick={() => window.electronAPI.shell.openExternal('https://axiomforge.io/privacy')}
            className="text-indigo-400 hover:text-indigo-300"
          >
            Learn more
          </button>
        </p>
      </div>
    </div>
  );
}

export default InstallationWizard;
