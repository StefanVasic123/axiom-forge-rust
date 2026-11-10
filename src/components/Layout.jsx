/**
 * Axiom Forge - Layout Component
 * Main application layout with sidebar and header
 */

import React from 'react';
import { Outlet, Link, useLocation } from 'react-router-dom';
import { 
  LayoutDashboard, 
  Settings, 
  Plus, 
  Github, 
  Sparkles,
  ChevronRight,
  Brain,
  GitPullRequest,
  Loader2,
  CheckCircle2,
  AlertCircle,
  Pause,
  Play,
  Terminal,
  FileCode,
  ChevronDown,
  ChevronUp,
  X,
  WifiOff
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useAppStore } from '../hooks/useAppStore';
import { useI18n, LanguageSelector } from '../i18n/I18nContext';

function Sidebar() {
  const location = useLocation();
  const { tokensConfigured } = useAppStore();
  const { t } = useI18n();
  const [version, setVersion] = React.useState('...');

  React.useEffect(() => {
    window.electronAPI.app.getVersion()
      .then(res => {
        if (res.success) setVersion(res.version);
      })
      .catch(() => setVersion('1.1.9'));
  }, []);

  const navItems = [
    { path: '/', icon: LayoutDashboard, label: t('nav.dashboard') },
    { path: '/agency-dashboard', icon: GitPullRequest, label: t('nav.agencyReview') },
    { path: '/ai-optimizer', icon: Brain, label: t('nav.aiOptimizer') },
    { path: '/settings', icon: Settings, label: t('nav.settings') }
  ];

  const isActive = (path) => location.pathname === path;

  return (
    <aside className="w-64 bg-slate-900/50 border-r border-slate-800 flex flex-col">
      {/* Logo */}
      <div className="p-6 border-b border-slate-800">
        <Link to="/" className="flex items-center gap-3 group">
          <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-indigo-500 to-violet-600 flex items-center justify-center shadow-lg shadow-indigo-500/20 group-hover:shadow-indigo-500/40 transition-shadow">
            <Sparkles className="w-5 h-5 text-white" />
          </div>
          <div>
            <h1 className="font-bold text-lg text-white">Axiom Forge</h1>
            <p className="text-xs text-slate-500">{t('nav.appPipeline')}</p>
          </div>
        </Link>
      </div>

      {/* Navigation */}
      <nav className="flex-1 p-4 space-y-1">
        {navItems.map((item) => (
          <Link
            key={item.path}
            to={item.path}
            className={`flex items-center gap-3 px-4 py-3 rounded-lg transition-all ${
              isActive(item.path)
                ? 'bg-indigo-600/10 text-indigo-400 border border-indigo-500/20'
                : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50'
            }`}
          >
            <item.icon className="w-5 h-5" />
            <span className="font-medium">{item.label}</span>
            {isActive(item.path) && (
              <ChevronRight className="w-4 h-4 ml-auto" />
            )}
          </Link>
        ))}

        {/* New Project Button */}
        <button
          onClick={() => window.electronAPI.shell.openExternal('https://axiomforge.io/templates')}
          className="w-full flex items-center gap-3 px-4 py-3 mt-4 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white transition-all shadow-lg shadow-indigo-500/20"
        >
          <Plus className="w-5 h-5" />
          <span className="font-medium">{t('nav.newProject')}</span>
        </button>
      </nav>

      {/* Token Status */}
      <div className="p-4 border-t border-slate-800">
        <p className="text-xs text-slate-500 mb-3 uppercase tracking-wider">{t('nav.services')}</p>
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-sm">
            <div className={`w-2 h-2 rounded-full ${tokensConfigured.github ? 'bg-emerald-500' : 'bg-slate-600'}`} />
            <span className={tokensConfigured.github ? 'text-slate-300' : 'text-slate-500'}>
              GitHub
            </span>
          </div>
          <div className="flex items-center gap-2 text-sm">
            <div className={`w-2 h-2 rounded-full ${tokensConfigured.vercel ? 'bg-emerald-500' : 'bg-slate-600'}`} />
            <span className={tokensConfigured.vercel ? 'text-slate-300' : 'text-slate-500'}>
              Vercel
            </span>
          </div>
        </div>
      </div>

      {/* Footer */}
      <div className="p-4 border-t border-slate-800 flex flex-col gap-3">
        <a
          href="https://github.com/axiom-forge/axiom-forge"
          target="_blank"
          rel="noopener noreferrer"
          onClick={(e) => {
            e.preventDefault();
            window.electronAPI.shell.openExternal('https://github.com/axiom-forge/axiom-forge');
          }}
          className="flex items-center gap-2 text-sm text-slate-500 hover:text-slate-300 transition-colors"
        >
          <Github className="w-4 h-4" />
          <span>{t('nav.openSource')}</span>
        </a>
        <div className="text-[10px] text-slate-600 font-bold uppercase tracking-[0.2em] pl-1">
          Axiom Forge v{version}
        </div>
      </div>
    </aside>
  );
}

function Header() {
  const location = useLocation();
  const { t } = useI18n();
  
  const getPageTitle = () => {
    const path = location.pathname;
    if (path === '/') return t('nav.dashboard');
    if (path === '/agency-dashboard') return t('nav.agencyReview');
    if (path === '/settings') return t('nav.settings');
    if (path.startsWith('/projects/')) return t('dashboard.viewDetails');
    return 'Axiom Forge';
  };

  return (
    <header className="h-16 border-b border-slate-800 bg-slate-950/50 backdrop-blur-sm flex items-center justify-between px-6">
      <h2 className="text-xl font-semibold text-white">{getPageTitle()}</h2>
      
      <div className="flex items-center gap-3">
        <LanguageSelector />
        {/* Status Indicator */}
        <div className="flex items-center gap-2 px-3 py-1.5 bg-emerald-500/10 border border-emerald-500/20 rounded-full">
          <div className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
          <span className="text-xs font-medium text-emerald-400">{t('nav.systemReady')}</span>
        </div>
      </div>
    </header>
  );
}

function Layout() {
  const { projects, taskProgress, setTaskProgress, loadProjects } = useAppStore();
  const { t } = useI18n();
  const [dismissedTaskId, setDismissedTaskId] = React.useState(null);
  const [focusedProjectId, setFocusedProjectId] = React.useState(null);
  const [isTaskActionLoading, setIsTaskActionLoading] = React.useState(false);
  const [isDropdownOpen, setIsDropdownOpen] = React.useState(true);
  const [showLogs, setShowLogs] = React.useState(false);
  const logsEndRef = React.useRef(null);
  const liveCodeContainerRef = React.useRef(null);
  const hydratedRef = React.useRef(false);
  const navigate = useNavigate();

  // Auto-hydrate interrupted/paused/generating task into bottom-right widget on startup
  React.useEffect(() => {
    if (hydratedRef.current || !Array.isArray(projects) || projects.length === 0) return;
    if (Object.keys(taskProgress).length > 0) {
      hydratedRef.current = true;
      return;
    }
    const candidate = projects.find(
      p => p.status === 'generating' || p.status === 'paused' || p.status === 'stopped'
    );
    if (candidate) {
      hydratedRef.current = true;
      window.electronAPI.task
        .getActiveTaskByProject(candidate.id)
        .then(res => {
          if (res?.success && res.task) {
            setTaskProgress(res.task.taskId, res.task);
          }
        })
        .catch(() => {});
    }
  }, [projects, taskProgress, setTaskProgress]);

  React.useEffect(() => {
    const handleShowProgress = async (e) => {
      const projectId = e?.detail?.projectId;
      setDismissedTaskId(null);
      setIsDropdownOpen(true);
      if (projectId) {
        setFocusedProjectId(projectId);
        try {
          const res = await window.electronAPI.task.getActiveTaskByProject(projectId);
          if (res?.success && res.task) {
            setTaskProgress(res.task.taskId, res.task);
          }
        } catch (err) {
          console.warn('Failed to sync task progress on show:', err);
        }
      }
    };
    const handleHideProgress = () => {
      const tasks = Object.values(taskProgress);
      const current = tasks.find(tk => tk.projectId === focusedProjectId) || tasks[0];
      if (current) {
        setDismissedTaskId(current.taskId);
      }
    };
    window.addEventListener('axiom:show-progress', handleShowProgress);
    window.addEventListener('axiom:hide-progress', handleHideProgress);
    return () => {
      window.removeEventListener('axiom:show-progress', handleShowProgress);
      window.removeEventListener('axiom:hide-progress', handleHideProgress);
    };
  }, [setTaskProgress, taskProgress, focusedProjectId]);

  // Find the task matching focusedProjectId first, or the first non-dismissed task
  const activeTask = React.useMemo(() => {
    const tasks = Object.values(taskProgress);
    if (focusedProjectId) {
      const focused = tasks.find(tk => tk.projectId === focusedProjectId && tk.taskId !== dismissedTaskId);
      if (focused) return focused;
    }
    return tasks.find(tk => tk.taskId !== dismissedTaskId);
  }, [taskProgress, focusedProjectId, dismissedTaskId]);

  React.useEffect(() => {
    if (showLogs && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [activeTask?.logs?.length, showLogs]);

  React.useEffect(() => {
    if (isDropdownOpen && liveCodeContainerRef.current) {
      liveCodeContainerRef.current.scrollTop = liveCodeContainerRef.current.scrollHeight;
    }
  }, [activeTask?.liveCode, isDropdownOpen]);

  const handleDismiss = () => {
    if (activeTask) {
      setDismissedTaskId(activeTask.taskId);
    }
  };

  const handleViewProject = () => {
    if (activeTask && activeTask.projectId) {
      setDismissedTaskId(activeTask.taskId);
      navigate(`/projects/${activeTask.projectId}`);
    }
  };

  // Global Agent Bridge listener when user is outside Editor.jsx (e.g. on Dashboard or ProjectDetail)
  React.useEffect(() => {
    if (window.self !== window.top) return;
    if (location.pathname.includes('/editor')) return;
    if (!window.electronAPI?.agentBridge?.pollCommand) return;

    const interval = setInterval(async () => {
      const cmd = await window.electronAPI.agentBridge.pollCommand();
      if (!cmd || !cmd.action) return;
      const cmdId = cmd.id || `cmd-${Date.now()}`;
      if (cmd.action === 'open_project_editor' && cmd.projectId) {
        navigate(`/projects/${cmd.projectId}/editor`);
        await window.electronAPI.agentBridge.writeCommandResult({
          id: cmdId,
          action: cmd.action,
          success: true,
          details: `Opened editor for project ${cmd.projectId}`,
          timestamp: new Date().toISOString()
        });
      } else {
        await window.electronAPI.agentBridge.writeCommandResult({
          id: cmdId,
          action: cmd.action,
          success: false,
          details: `Currently on ${location.pathname} (not in Editor). Use open_project_editor first.`,
          timestamp: new Date().toISOString()
        });
      }
    }, 1000);
    return () => clearInterval(interval);
  }, [location.pathname, navigate]);

  const isComplete = activeTask && (activeTask.phase === 'complete' || activeTask.phase === 'success');
  const isError = activeTask && activeTask.phase === 'error';
  const isPaused = activeTask && (activeTask.phase === 'paused' || activeTask.status === 'paused');
  const isOfflineWaiting = Boolean(isPaused && activeTask?.offlineWaiting);
  const isStopped = activeTask && (activeTask.phase === 'stopped' || activeTask.status === 'stopped');
  const isRunning = activeTask && !isComplete && !isError && !isStopped;
  const activeWarnings = Array.isArray(activeTask?.warnings) ? activeTask.warnings : [];
  const hasCompletionWarnings = Boolean(isComplete && (activeTask?.hasWarnings || activeWarnings.length > 0));

  const handlePauseTask = async () => {
    if (!activeTask) return;
    setIsTaskActionLoading(true);
    try {
      const id = activeTask.projectId || activeTask.taskId;
      await window.electronAPI.task.pause(id);
      setTaskProgress(activeTask.taskId, {
        ...activeTask,
        phase: 'paused',
        status: 'paused',
        message: 'Generisanje je pauzirano.'
      });
      await loadProjects();
    } catch (e) {
      console.error('Pause failed:', e);
    } finally {
      setIsTaskActionLoading(false);
    }
  };

  const handleResumeTask = async () => {
    if (!activeTask) return;
    setIsTaskActionLoading(true);
    const prevTaskState = { ...activeTask };
    try {
      setTaskProgress(activeTask.taskId, {
        ...activeTask,
        phase: activeTask.offlineWaiting ? 'install-deps' : 'generate-files',
        status: 'generating',
        offlineWaiting: false,
        pausedReason: null,
        message: activeTask.offlineWaiting
          ? 'Nastavljam instalaciju zavisnosti sa sačuvanog checkpoint-a...'
          : 'Nastavak generisanja sa sačuvanog checkpoint-a...'
      });
      if (activeTask.projectId) {
        await window.electronAPI.task.resumeGeneration(activeTask.projectId);
      } else if (activeTask.taskId) {
        await window.electronAPI.task.resume(activeTask.taskId);
      }
      await loadProjects();
    } catch (e) {
      console.error('Resume failed:', e);
      setTaskProgress(prevTaskState.taskId, {
        ...prevTaskState,
        phase: 'stopped',
        status: 'stopped',
        message: `Greška pri nastavku: ${e?.message || e}`
      });
    } finally {
      setIsTaskActionLoading(false);
    }
  };

  const handleStopTask = async () => {
    if (!activeTask) return;
    setIsTaskActionLoading(true);
    try {
      const id = activeTask.projectId || activeTask.taskId;
      await window.electronAPI.task.stop(id);
      setTaskProgress(activeTask.taskId, {
        ...activeTask,
        phase: 'stopped',
        status: 'stopped',
        message: 'Generisanje je zaustavljeno. Svi fajlovi su sačuvani.'
      });
      await loadProjects();
    } catch (e) {
      console.error('Stop failed:', e);
    } finally {
      setIsTaskActionLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-slate-950 flex relative">
      <Sidebar />
      <div className="flex-1 flex flex-col min-w-0">
        <Header />
        <main className="flex-1 overflow-auto p-6">
          <Outlet />
        </main>
      </div>

      {/* Background Task Progress Dropdown & Real-Time Live Code Widget */}
      {activeTask && (
        <div
          className={`fixed bottom-5 right-5 z-50 ${
            isDropdownOpen ? 'w-[440px]' : 'w-96'
          } bg-slate-900/95 backdrop-blur-xl border border-indigo-500/25 rounded-2xl shadow-2xl overflow-hidden animate-fade-in transition-all duration-200`}
        >
          {/* Top Progress Bar */}
          <div className="w-full bg-slate-950 h-1.5 overflow-hidden">
            <div
              className={`h-full transition-all duration-300 bg-gradient-to-r ${
                hasCompletionWarnings ? 'from-amber-500 to-orange-500' :
                isComplete ? 'from-emerald-500 to-teal-500' :
                isError || isStopped ? 'from-rose-500 to-red-500' :
                isPaused ? 'from-amber-500 to-orange-500' :
                'from-indigo-500 via-violet-500 to-blue-500'
              }`}
              style={{ width: `${Math.min(Math.max(activeTask.progress || 0, 2), 100)}%` }}
            />
          </div>

          {/* Clickable Pill / Dropdown Header */}
          <div
            onClick={() => setIsDropdownOpen(!isDropdownOpen)}
            className="px-4 py-3 flex items-center justify-between gap-3 cursor-pointer hover:bg-slate-800/40 transition-colors select-none"
          >
            <div className="flex items-center gap-3 min-w-0">
              <div
                className={`w-8 h-8 rounded-lg flex items-center justify-center shrink-0 ${
                  hasCompletionWarnings ? 'bg-amber-500/15 text-amber-400' :
                  isComplete ? 'bg-emerald-500/10 text-emerald-400' :
                  isError || isStopped ? 'bg-rose-500/10 text-rose-400' :
                  isPaused ? 'bg-amber-500/10 text-amber-400' :
                  'bg-indigo-500/15 text-indigo-400'
                }`}
              >
                {hasCompletionWarnings ? (
                  <AlertCircle className="w-4 h-4" />
                ) : isComplete ? (
                  <CheckCircle2 className="w-4 h-4" />
                ) : isError || isStopped ? (
                  <AlertCircle className="w-4 h-4" />
                ) : isOfflineWaiting ? (
                  <WifiOff className="w-4 h-4" />
                ) : isPaused ? (
                  <Pause className="w-4 h-4" />
                ) : (
                  <Loader2 className="w-4 h-4 animate-spin" />
                )}
              </div>
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <h3 className="font-semibold text-sm text-white truncate">
                    {hasCompletionWarnings ? 'Završeno sa upozorenjima' :
                     isComplete ? t('taskOverlay.complete') :
                     isError ? t('taskOverlay.failed') :
                     isOfflineWaiting ? 'Čekam internet konekciju' :
                     isPaused ? 'Generisanje pauzirano' :
                     isStopped ? 'Generisanje prekinuto' :
                     t('taskOverlay.forging')}
                  </h3>
                  {activeTask.totalFiles > 0 && (
                    <span className="px-1.5 py-0.5 rounded bg-slate-800 text-[10px] font-mono text-slate-300 shrink-0">
                      {activeTask.fileIndex || 0}/{activeTask.totalFiles}
                    </span>
                  )}
                </div>
                <p className="text-[11px] text-slate-400 truncate font-mono">
                  {activeTask.currentFile
                    ? activeTask.currentFile
                    : activeTask.message || `Task ID: ${String(activeTask.taskId || '').substring(0, 13)}`}
                </p>
              </div>
            </div>

            <div className="flex items-center gap-1.5 shrink-0">
              <span
                className={`px-2 py-0.5 rounded-md text-xs font-bold border ${
                  hasCompletionWarnings
                    ? 'bg-amber-500/15 border-amber-500/30 text-amber-300'
                    : isComplete
                    ? 'bg-emerald-500/15 border-emerald-500/30 text-emerald-300'
                    : isError || isStopped
                    ? 'bg-rose-500/15 border-rose-500/30 text-rose-300'
                    : isPaused
                    ? 'bg-amber-500/15 border-amber-500/30 text-amber-300'
                    : 'bg-indigo-500/15 border-indigo-500/30 text-indigo-300'
                }`}
              >
                {Math.round(activeTask.progress || 0)}%
              </span>

              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  setIsDropdownOpen(!isDropdownOpen);
                }}
                className="p-1 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-white transition-colors"
                title={isDropdownOpen ? 'Skupi prikaz' : 'Prikaži detalje i kod uživo'}
              >
                {isDropdownOpen ? <ChevronDown className="w-4 h-4" /> : <ChevronUp className="w-4 h-4" />}
              </button>

              <div className="relative group/dismiss" onClick={(e) => e.stopPropagation()}>
                <button
                  type="button"
                  onClick={handleDismiss}
                  className="p-1 rounded-lg hover:bg-slate-800 text-slate-500 hover:text-slate-300 transition-colors"
                >
                  <X className="w-4 h-4" />
                </button>
                <div className="pointer-events-none absolute right-full mr-2 top-1/2 -translate-y-1/2 z-50 w-52 opacity-0 scale-95 group-hover/dismiss:opacity-100 group-hover/dismiss:scale-100 transition-all duration-75 bg-slate-950/95 border border-slate-700 text-slate-200 text-[10px] leading-snug px-2.5 py-1.5 rounded-lg shadow-xl">
                  {t('taskOverlay.hideTooltip')}
                </div>
              </div>
            </div>
          </div>

          {/* Expanded Dropdown Body */}
          {isDropdownOpen && (
            <div className="px-4 pb-4 pt-2 border-t border-slate-800/80 space-y-3">
              {/* Current AI Activity Card */}
              <div className="rounded-xl bg-slate-950/70 border border-slate-800/80 p-2.5 space-y-1.5">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-1.5 min-w-0">
                    <FileCode className="w-3.5 h-3.5 text-indigo-400 shrink-0" />
                    <span className="text-xs font-mono font-semibold text-indigo-200 truncate">
                      {activeTask.currentFile || 'Priprema pipeline-a...'}
                    </span>
                  </div>
                  {activeTask.tokenCount > 0 && (
                    <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-indigo-500/10 text-indigo-300 border border-indigo-500/20 shrink-0">
                      {activeTask.tokenCount} tok
                    </span>
                  )}
                </div>
                {activeTask.fileDescription && (
                  <p className="text-[11px] text-slate-400 line-clamp-2 leading-relaxed">
                    {activeTask.fileDescription}
                  </p>
                )}
                <p className="text-[11px] text-slate-300 italic truncate">
                  {activeTask.message || 'Initializing generation sequence...'}
                </p>
              </div>

              {/* Real-Time Code Streaming Preview */}
              {activeTask.liveCode && (
                <div className="rounded-xl bg-slate-950 border border-slate-800/90 overflow-hidden shadow-inner">
                  <div className="px-3 py-1.5 bg-slate-900/90 border-b border-slate-800/80 flex items-center justify-between">
                    <div className="flex items-center gap-2 min-w-0">
                      <span
                        className={`w-2 h-2 rounded-full ${
                          isRunning && !isPaused
                            ? 'bg-emerald-400 animate-pulse'
                            : isPaused
                            ? 'bg-amber-400'
                            : 'bg-slate-500'
                        }`}
                      />
                      <span className="text-[10px] font-mono uppercase tracking-wider font-semibold text-slate-300 truncate">
                        {isRunning && !isPaused ? 'Uživo pisanje koda' : 'Poslednji generisani kod'}
                      </span>
                    </div>
                    {activeTask.currentFile && (
                      <span className="text-[10px] font-mono text-slate-500 truncate max-w-[180px]">
                        {activeTask.currentFile.split('/').pop()}
                      </span>
                    )}
                  </div>
                  <div
                    ref={liveCodeContainerRef}
                    className="p-2.5 max-h-40 overflow-y-auto overflow-x-auto font-mono text-[10.5px] leading-relaxed text-emerald-300/90 select-text"
                  >
                    <pre className="whitespace-pre-wrap break-words m-0">
                      {activeTask.liveCode}
                      {isRunning && !isPaused && (
                        <span className="inline-block w-1.5 h-3.5 ml-0.5 align-middle bg-emerald-400 animate-pulse" />
                      )}
                    </pre>
                  </div>
                </div>
              )}

              {/* Expandable Engine Logs */}
              {Array.isArray(activeTask.logs) && activeTask.logs.length > 0 && (
                <div className="border-t border-slate-800/70 pt-2">
                  <button
                    type="button"
                    onClick={() => setShowLogs(!showLogs)}
                    className="w-full flex items-center justify-between text-[10px] uppercase tracking-wider font-semibold text-slate-500 hover:text-slate-300 transition-colors"
                  >
                    <span className="flex items-center gap-1.5">
                      <Terminal className="w-3 h-3 text-indigo-400" />
                      Istorija koraka ({activeTask.logs.length})
                    </span>
                    {showLogs ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronUp className="w-3.5 h-3.5" />}
                  </button>
                  {showLogs && (
                    <div className="mt-2 max-h-28 overflow-y-auto rounded-lg bg-slate-950/90 border border-slate-800/80 p-2 font-mono text-[10px] space-y-1">
                      {activeTask.logs.map((log, idx) => {
                        const isLast = idx === activeTask.logs.length - 1;
                        return (
                          <div
                            key={log.id || idx}
                            className={`flex items-start gap-1.5 leading-relaxed ${
                              isLast ? 'text-indigo-300 font-medium' : 'text-slate-500'
                            }`}
                          >
                            <span className="text-slate-600 shrink-0">&gt;</span>
                            <span className="break-all">{log.message}</span>
                          </div>
                        );
                      })}
                      <div ref={logsEndRef} />
                    </div>
                  )}
                </div>
              )}

              {/* Interactive Controls */}
              {(isRunning || isPaused || isStopped) && (
                <div className="pt-2 border-t border-slate-800/80 flex items-center gap-2">
                  {isRunning && !isPaused && (
                    <>
                      <button
                        onClick={handlePauseTask}
                        disabled={isTaskActionLoading}
                        className="flex-1 flex items-center justify-center gap-1.5 py-1.5 px-2.5 rounded-lg bg-amber-500/10 text-amber-400 hover:bg-amber-500/20 text-xs font-medium transition-colors"
                      >
                        <Pause className="w-3.5 h-3.5" />
                        {t('common.pause') || 'Pause'}
                      </button>
                      <button
                        onClick={handleStopTask}
                        disabled={isTaskActionLoading}
                        className="flex-1 flex items-center justify-center gap-1.5 py-1.5 px-2.5 rounded-lg bg-rose-500/10 text-rose-400 hover:bg-rose-500/20 text-xs font-medium transition-colors"
                      >
                        <X className="w-3.5 h-3.5" />
                        {t('common.stop') || 'Stop'}
                      </button>
                    </>
                  )}
                  {(isPaused || isStopped) && (
                    <button
                      onClick={handleResumeTask}
                      disabled={isTaskActionLoading}
                      className="w-full flex items-center justify-center gap-1.5 py-2 px-3 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold shadow-lg shadow-indigo-600/20 transition-all"
                    >
                      <Play className="w-3.5 h-3.5" />
                      {isStopped ? 'Nastavi tamo gde je stao' : (t('common.resume') || 'Resume')}
                    </button>
                  )}
                </div>
              )}

              {hasCompletionWarnings && activeWarnings.length > 0 && (
                <div className="rounded-xl bg-amber-500/10 border border-amber-500/25 p-2.5 space-y-1 max-h-28 overflow-y-auto">
                  <p className="text-[11px] font-semibold text-amber-300 flex items-center gap-1.5">
                    <AlertCircle className="w-3.5 h-3.5 shrink-0" />
                    Preostala upozorenja ({activeWarnings.length})
                  </p>
                  <ul className="space-y-0.5 text-[10.5px] text-amber-200/90 list-disc pl-4">
                    {activeWarnings.slice(0, 4).map((w, idx) => (
                      <li key={idx} className="break-words">{w}</li>
                    ))}
                  </ul>
                </div>
              )}

              {isComplete && activeTask.projectId && (
                <div className="pt-2 flex gap-2">
                  <button
                    onClick={handleViewProject}
                    className={`w-full py-2 px-3 text-white rounded-lg text-xs font-semibold shadow-lg transition-all text-center ${
                      hasCompletionWarnings
                        ? 'bg-amber-600 hover:bg-amber-500 shadow-amber-600/20'
                        : 'bg-emerald-600 hover:bg-emerald-500 shadow-emerald-600/20'
                    }`}
                  >
                    {t('taskOverlay.viewProject')}
                  </button>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default Layout;
