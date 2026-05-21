/**
 * Axiom Forge - Floating Progress Window
 * Always-on-top draggable window for generation/deployment progress
 */

import React, { useState, useEffect } from 'react';
import { 
  X, 
  Pause, 
  Play, 
  RotateCcw,
  CheckCircle2,
  AlertCircle,
  Loader2,
  ExternalLink,
  Minimize2
} from 'lucide-react';

// Progress phase display
const PHASE_DISPLAY = {
  'fetch-manifest': { label: 'Fetching Manifest', icon: Loader2 },
  'generate-files': { label: 'Generating Code', icon: Loader2 },
  'pull-model': { label: 'Pulling Model', icon: Loader2 },
  'save-project': { label: 'Saving Project', icon: Loader2 },
  'push-github': { label: 'Pushing to GitHub', icon: Loader2 },
  'deploy-vercel': { label: 'Deploying to Vercel', icon: Loader2 },
  'verify-deployment': { label: 'Verifying Deployment', icon: Loader2 },
  'complete': { label: 'Complete!', icon: CheckCircle2 },
  'error': { label: 'Error', icon: AlertCircle }
};

function FloatingProgress() {
  const [isMinimized, setIsMinimized] = useState(false);
  const [showLogs, setShowLogs] = useState(false);
  const [progress, setProgress] = useState({
    phase: 'idle',
    message: 'Waiting to start...',
    progress: 0,
    projectId: null,
    taskId: null,
    urls: null,
    history: []
  });

  const [version, setVersion] = useState('v1.1.9');

  // Subscribe to task progress
  useEffect(() => {
    // Get real version
    window.electronAPI.app.getVersion().then(res => {
      if (res.success) setVersion(`v${res.version}`);
    });

    // 1. Parse projectId from URL
    const params = new URLSearchParams(window.location.search);
    const projectId = params.get('projectId');

    // 2. Initial sync - check if there's already an active task
    const syncStatus = async () => {
      if (projectId) {
        try {
          const result = await window.electronAPI.task.getActiveTaskByProject(projectId);
          if (result.success && result.task) {
            setProgress(prev => ({
              ...prev,
              ...result.task,
              phase: result.task.phase || prev.phase,
              message: result.task.message || prev.message || 'Restoring session...',
              history: [...(prev.history || []), { time: Date.now(), msg: 'Synced with background task' }]
            }));
          }
        } catch (error) {
          console.error('[Floating] Sync failed:', error);
        }
      }
    };

    syncStatus();

    // 3. Listen for future updates
    const unsubscribe = window.electronAPI.task.onProgress((data) => {
      setProgress(prev => ({
        ...prev,
        ...data,
        phase: data.phase || prev.phase,
        message: data.message || prev.message,
        history: [...(prev.history || []), { time: Date.now(), msg: data.message }]
      }));
    });

    return () => unsubscribe();
  }, []);

  // Handle window controls
  const handleClose = () => {
    window.electronAPI.window.hideFloating();
  };

  const handleStop = async () => {
    if (progress.taskId) {
      await window.electronAPI.task.stop(progress.taskId);
    }
  };

  const handleRestart = () => {
    // Restart the current task
    if (progress.projectId) {
      window.electronAPI.task.startGeneration(progress.projectId);
    }
  };

  const phaseInfo = PHASE_DISPLAY[progress.phase] || PHASE_DISPLAY['fetch-manifest'];
  const PhaseIcon = phaseInfo.icon;
  const isComplete = progress.phase === 'complete';
  const isError = progress.phase === 'error';
  const isRunning = !isComplete && !isError;

  if (isMinimized) {
    return (
      <div 
        className="window-drag bg-slate-900/95 backdrop-blur-xl border border-slate-700 rounded-xl shadow-2xl overflow-hidden"
        style={{ width: '200px' }}
      >
        <div className="flex items-center justify-between p-3">
          <div className="flex items-center gap-2">
            <PhaseIcon className={`w-4 h-4 ${isRunning ? 'text-indigo-400 animate-spin' : 'text-emerald-400'}`} />
            <span className="text-xs font-medium text-slate-300 truncate">
              {phaseInfo.label}
            </span>
          </div>
          <button
            onClick={() => setIsMinimized(false)}
            className="window-no-drag p-1 rounded hover:bg-slate-800 text-slate-500 hover:text-white"
          >
            <ExternalLink className="w-3 h-3" />
          </button>
        </div>
        
        {/* Mini progress bar */}
        <div className="h-1 bg-slate-800">
          <div 
            className="h-full bg-gradient-to-r from-indigo-500 to-violet-500 transition-all duration-300"
            style={{ width: `${progress.progress}%` }}
          />
        </div>
      </div>
    );
  }

  return (
    <div 
      className="window-drag bg-slate-900/95 backdrop-blur-xl border border-slate-700 rounded-xl shadow-2xl overflow-hidden"
      style={{ width: '400px' }}
    >
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-slate-800">
        <div className="flex items-center gap-3">
          <div className={`w-8 h-8 rounded-lg flex items-center justify-center ${
            isComplete ? 'bg-emerald-500/20' : 
            isError ? 'bg-rose-500/20' : 
            'bg-indigo-500/20'
          }`}>
            <PhaseIcon className={`w-4 h-4 ${
              isComplete ? 'text-emerald-400' : 
              isError ? 'text-rose-400' : 
              'text-indigo-400 animate-spin'
            }`} />
          </div>
          <div>
            <h3 className="font-semibold text-white text-sm">{phaseInfo.label}</h3>
            <div className="flex items-center gap-2">
              <p className="text-xs text-slate-500">Axiom Forge</p>
              <span className="px-1.5 py-0.5 rounded bg-indigo-500/20 text-indigo-400 text-[10px] font-bold border border-indigo-500/30">
                {version}
              </span>
            </div>
          </div>
        </div>
        
        <div className="flex items-center gap-1 window-no-drag">
          <button
            onClick={() => setShowLogs(!showLogs)}
            className={`p-2 rounded-lg transition-colors ${
              showLogs ? 'bg-indigo-600 text-white' : 'text-slate-500 hover:bg-slate-800 hover:text-white'
            }`}
            title="Toggle Diagnostics"
          >
            <AlertCircle className="w-4 h-4" />
          </button>
          <button
            onClick={() => setIsMinimized(true)}
            className="p-2 rounded-lg hover:bg-slate-800 text-slate-500 hover:text-white transition-colors"
            title="Minimize"
          >
            <Minimize2 className="w-4 h-4" />
          </button>
          <button
            onClick={handleClose}
            className="p-2 rounded-lg hover:bg-rose-500/20 text-slate-500 hover:text-rose-400 transition-colors"
            title="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="p-4 space-y-4">
        {/* Progress Message */}
        <p className="text-sm text-slate-300">
          {progress.message}
        </p>

        {/* Progress Bar */}
        <div className="space-y-1">
          <div className="flex items-center justify-between text-xs">
            <span className="text-slate-500">Progress</span>
            <span className="text-slate-400">{Math.round(progress.progress)}%</span>
          </div>
          <div className="h-2 bg-slate-800 rounded-full overflow-hidden">
            <div 
              className={`h-full rounded-full transition-all duration-300 ${
                isComplete ? 'bg-emerald-500' : 
                isError ? 'bg-rose-500' : 
                'bg-gradient-to-r from-indigo-500 to-violet-500'
              }`}
              style={{ width: `${progress.progress}%` }}
            />
          </div>
        </div>

        {/* URLs (if complete) */}
        {isComplete && progress.urls && (
          <div className="space-y-2 pt-2 border-t border-slate-800">
            <p className="text-xs text-slate-500 uppercase tracking-wider">Deployment URLs</p>
            <div className="space-y-1">
              {progress.urls.repo && (
                <a
                  href={progress.urls.repo}
                  onClick={(e) => {
                    e.preventDefault();
                    window.electronAPI.shell.openExternal(progress.urls.repo);
                  }}
                  className="flex items-center gap-2 text-sm text-indigo-400 hover:text-indigo-300"
                >
                  <ExternalLink className="w-3 h-3" />
                  View Repository
                </a>
              )}
              {progress.urls.site && (
                <a
                  href={progress.urls.site}
                  onClick={(e) => {
                    e.preventDefault();
                    window.electronAPI.shell.openExternal(progress.urls.site);
                  }}
                  className="flex items-center gap-2 text-sm text-emerald-400 hover:text-emerald-300"
                >
                  <ExternalLink className="w-3 h-3" />
                  View Live Site
                </a>
              )}
            </div>
          </div>
        )}

        {/* Diagnostics Logs */}
        {showLogs && (
          <div className="space-y-2 pt-2 border-t border-slate-800">
            <p className="text-[10px] text-slate-500 uppercase tracking-wider">Engine Logs</p>
            <div className="max-h-80 overflow-y-auto space-y-1 bg-black/30 rounded p-2 font-mono text-[10px]">
              {progress.history?.length > 0 ? (
                progress.history.slice().reverse().map((log, i) => (
                  <div key={i} className="flex gap-2">
                    <span className="text-slate-600">[{new Date(log.time).toLocaleTimeString([], { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' })}]</span>
                    <span className="text-slate-400">{log.msg}</span>
                  </div>
                ))
              ) : (
                <div className="text-slate-600 italic">No logs available...</div>
              )}
            </div>
          </div>
        )}

        {/* Error Message */}
        {isError && progress.error && (
          <div className="p-3 bg-rose-500/10 border border-rose-500/20 rounded-lg">
            <p className="text-sm text-rose-400">{progress.error}</p>
          </div>
        )}
      </div>

      {/* Footer Actions */}
      <div className="flex items-center justify-between p-4 border-t border-slate-800 window-no-drag">
        {isRunning ? (
          <button
            onClick={handleStop}
            className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-rose-500/10 text-rose-400 hover:bg-rose-500/20 transition-colors text-sm"
          >
            <Pause className="w-4 h-4" />
            Stop
          </button>
        ) : isError ? (
          <button
            onClick={handleRestart}
            className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-indigo-500/10 text-indigo-400 hover:bg-indigo-500/20 transition-colors text-sm"
          >
            <RotateCcw className="w-4 h-4" />
            Retry
          </button>
        ) : (
          <div />
        )}

        {isComplete && (
          <button
            onClick={handleClose}
            className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-emerald-500/10 text-emerald-400 hover:bg-emerald-500/20 transition-colors text-sm"
          >
            <CheckCircle2 className="w-4 h-4" />
            Done
          </button>
        )}
      </div>
    </div>
  );
}

export default FloatingProgress;
