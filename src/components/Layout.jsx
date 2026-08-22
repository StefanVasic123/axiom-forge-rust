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
  X
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useAppStore } from '../hooks/useAppStore';

function Sidebar() {
  const location = useLocation();
  const { tokensConfigured } = useAppStore();
  const [version, setVersion] = React.useState('...');

  React.useEffect(() => {
    window.electronAPI.app.getVersion()
      .then(res => {
        if (res.success) setVersion(res.version);
      })
      .catch(() => setVersion('1.1.9'));
  }, []);

  const navItems = [
    { path: '/', icon: LayoutDashboard, label: 'Dashboard' },
    { path: '/agency-dashboard', icon: GitPullRequest, label: 'Agency Review' },
    { path: '/ai-optimizer', icon: Brain, label: 'AI Optimizer' },
    { path: '/settings', icon: Settings, label: 'Settings' }
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
            <p className="text-xs text-slate-500">Idea to App Pipeline</p>
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
          <span className="font-medium">New Project</span>
        </button>
      </nav>

      {/* Token Status */}
      <div className="p-4 border-t border-slate-800">
        <p className="text-xs text-slate-500 mb-3 uppercase tracking-wider">Services</p>
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
          <span>Open Source</span>
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
  
  const getPageTitle = () => {
    const path = location.pathname;
    if (path === '/') return 'Dashboard';
    if (path === '/agency-dashboard') return 'Agency Review Dashboard';
    if (path === '/settings') return 'Settings';
    if (path.startsWith('/projects/')) return 'Project Details';
    return 'Axiom Forge';
  };

  return (
    <header className="h-16 border-b border-slate-800 bg-slate-950/50 backdrop-blur-sm flex items-center justify-between px-6">
      <h2 className="text-xl font-semibold text-white">{getPageTitle()}</h2>
      
      <div className="flex items-center gap-4">
        {/* Status Indicator */}
        <div className="flex items-center gap-2 px-3 py-1.5 bg-emerald-500/10 border border-emerald-500/20 rounded-full">
          <div className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
          <span className="text-xs font-medium text-emerald-400">System Ready</span>
        </div>
      </div>
    </header>
  );
}

function Layout() {
  const { taskProgress } = useAppStore();
  const [dismissedTaskId, setDismissedTaskId] = React.useState(null);
  const navigate = useNavigate();

  // Find the first task that is active or recently completed and not dismissed
  const activeTask = Object.values(taskProgress)
    .find(t => t.taskId !== dismissedTaskId);

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

  const isComplete = activeTask && (activeTask.phase === 'complete' || activeTask.phase === 'success');
  const isError = activeTask && activeTask.phase === 'error';
  const isRunning = activeTask && !isComplete && !isError;

  return (
    <div className="min-h-screen bg-slate-950 flex relative">
      <Sidebar />
      <div className="flex-1 flex flex-col min-w-0">
        <Header />
        <main className="flex-1 overflow-auto p-6">
          <Outlet />
        </main>
      </div>

      {/* Background Task Progress Overlay */}
      {activeTask && (
        <div className="fixed bottom-6 right-6 z-50 w-96 bg-slate-900/90 backdrop-blur-xl border border-indigo-500/20 rounded-xl shadow-2xl overflow-hidden animate-fade-in transition-all duration-300">
          <div className={`h-1 bg-gradient-to-r ${
            isComplete ? 'from-emerald-500 to-teal-500' :
            isError ? 'from-rose-500 to-red-500' :
            'from-indigo-500 to-violet-500'
          }`} style={{ width: `${activeTask.progress || 0}%` }} />
          
          <div className="p-4">
            <div className="flex items-start justify-between gap-3">
              <div className="flex items-center gap-3">
                <div className={`w-8 h-8 rounded-lg flex items-center justify-center ${
                  isComplete ? 'bg-emerald-500/10 text-emerald-400' :
                  isError ? 'bg-rose-500/10 text-rose-400' :
                  'bg-indigo-500/10 text-indigo-400'
                }`}>
                  {isComplete ? (
                    <CheckCircle2 className="w-4 h-4" />
                  ) : isError ? (
                    <AlertCircle className="w-4 h-4" />
                  ) : (
                    <Loader2 className="w-4 h-4 animate-spin" />
                  )}
                </div>
                <div>
                  <h3 className="font-semibold text-sm text-white">
                    {isComplete ? 'Generation Complete!' :
                     isError ? 'Generation Failed' :
                     'Forging Codebase...'}
                  </h3>
                  <p className="text-[10px] text-slate-500">
                    Task ID: {activeTask.taskId.substring(0, 13)}
                  </p>
                </div>
              </div>
              
              <button 
                onClick={handleDismiss}
                className="p-1 rounded-lg hover:bg-slate-800 text-slate-500 hover:text-slate-300 transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
            
            <div className="mt-3">
              <p className="text-xs text-slate-300 line-clamp-2 italic">
                {activeTask.message || 'Initializing generation sequence...'}
              </p>
            </div>
            
            {isRunning && (
              <div className="mt-3 flex items-center justify-between text-[10px] text-slate-500">
                <span>Progress</span>
                <span className="font-medium text-slate-300">{Math.round(activeTask.progress || 0)}%</span>
              </div>
            )}
            
            {isComplete && activeTask.projectId && (
              <div className="mt-4 flex gap-2">
                <button
                  onClick={handleViewProject}
                  className="w-full py-2 px-3 bg-emerald-600 hover:bg-emerald-500 text-white rounded-lg text-xs font-semibold shadow-lg shadow-emerald-600/20 transition-all text-center"
                >
                  View Generated Project
                </button>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export default Layout;
