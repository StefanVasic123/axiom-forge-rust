/**
 * Axiom Forge - Main App Component
 * 
 * Handles routing and global state management
 */

import React, { useEffect, useState } from 'react';
import { Routes, Route, Navigate } from 'react-router-dom';
import { AppProvider } from './contexts/AppContext';
import Layout from './components/Layout';
import InstallationWizard from './pages/InstallationWizard';
import Dashboard from './pages/Dashboard';
import ProjectDetail from './pages/ProjectDetail';
import Settings from './pages/Settings';
import ProjectConfig from './pages/ProjectConfig';
import EditorPage from './pages/Editor';
import AiOptimizer from './pages/AiOptimizer';
import AgencyDashboard from './pages/AgencyDashboard';
import ClientLayout from './components/ClientLayout';
import UpdatePromptModal from './components/UpdatePromptModal';
import { useAppStore } from './hooks/useAppStore';

function AppContent() {
  const [isLoading, setIsLoading] = useState(true);
  const [updatePrompt, setUpdatePrompt] = useState(null);
  const [pendingDeepLinkBuild, setPendingDeepLinkBuild] = useState(null);
  const { isFirstRun, checkFirstRun, startGeneration, appMode, setAppMode } = useAppStore();

  useEffect(() => {
    const init = async () => {
      await checkFirstRun();
      if (window.electronAPI?.agency?.getAppMode) {
        try {
          const modeState = await window.electronAPI.agency.getAppMode();
          setAppMode(modeState);
        } catch (e) {
          console.warn('Failed to load app mode:', e);
        }
      }
      setIsLoading(false);
    };
    init();
  }, []);

  // Non-blocking startup update check only after first-run wizard is complete
  useEffect(() => {
    if (isLoading || isFirstRun) return;
    if (window.electronAPI?.app?.checkForUpdate) {
      window.electronAPI.app
        .checkForUpdate()
        .then((info) => {
          if (info?.status === 'update_recommended' || info?.status === 'update_required') {
            setUpdatePrompt((prev) => prev || info);
          }
        })
        .catch(() => {});
    }
  }, [isLoading, isFirstRun]);

  // Subscribe to deep link events GLOBALLY (exactly once)
  useEffect(() => {
    if (isLoading) return;

    const unsubscribeBuild = window.electronAPI.deepLink.onBuild(async (data) => {
      console.log('Deep link build received globally:', data);
      if (data.manifestId) {
        try {
          // Verify desktop version before kicking off code generation
          if (window.electronAPI?.app?.checkForUpdate) {
            const versionCheck = await window.electronAPI.app.checkForUpdate(data.host || null);
            if (versionCheck?.status === 'update_required') {
              console.warn('Generation blocked: desktop app is 10+ versions behind:', versionCheck);
              setPendingDeepLinkBuild(null);
              setUpdatePrompt(versionCheck);
              return;
            }
            if (versionCheck?.status === 'update_recommended') {
              console.log('Soft update recommendation shown before generation:', versionCheck);
              setPendingDeepLinkBuild(data);
              setUpdatePrompt(versionCheck);
              return;
            }
          }

          // Up to date (or offline): trigger generation immediately
          await startGeneration(data.manifestId, data.projectId || data.manifestId, data.token, data.host);
        } catch (error) {
          console.error('Failed to auto-start generation globally:', error);
          const msg = String(error?.message || error || '');
          if (msg.includes('DESKTOP_UPDATE_REQUIRED')) {
            const verRes = await window.electronAPI?.app?.getVersion?.().catch(() => null);
            setPendingDeepLinkBuild(null);
            setUpdatePrompt({
              checked: true,
              status: 'update_required',
              installedVersion: verRes?.version || 'unknown',
              latestVersion: 'latest',
              versionsBehind: 10,
              mandatoryThreshold: 10,
              downloadUrl: 'https://github.com/StefanVasic123/axiom-forge-rust/releases/latest',
            });
          }
        }
      }
    });

    const unsubscribeConfig = window.electronAPI.deepLink.onConfig((data) => {
      console.log('Deep link config received globally:', data);
    });

    const unsubscribeDeploy = window.electronAPI.deepLink.onDeploy((data) => {
      console.log('Deep link deploy received globally:', data);
    });

    return () => {
      unsubscribeBuild();
      unsubscribeConfig();
      unsubscribeDeploy();
    };
  }, [isLoading, startGeneration]);

  const handleContinueBuildAnyway = async () => {
    const queued = pendingDeepLinkBuild;
    setUpdatePrompt(null);
    setPendingDeepLinkBuild(null);
    if (queued?.manifestId) {
      try {
        await startGeneration(queued.manifestId, queued.projectId || queued.manifestId, queued.token, queued.host);
      } catch (error) {
        console.error('Failed to start generation after soft update warning:', error);
      }
    }
  };

  const handleCloseUpdateModal = () => {
    setUpdatePrompt(null);
    setPendingDeepLinkBuild(null);
  };

  if (isLoading) {
    return (
      <div className="min-h-screen bg-slate-950 flex items-center justify-center">
        <div className="flex flex-col items-center gap-4">
          <div className="w-12 h-12 border-4 border-indigo-500 border-t-transparent rounded-full animate-spin" />
          <p className="text-slate-400">Loading Axiom Forge...</p>
        </div>
      </div>
    );
  }

  if (appMode?.mode === 'client') {
    return <ClientLayout />;
  }

  return (
    <>
      <Routes>
        {isFirstRun ? (
          <Route path="*" element={<InstallationWizard />} />
        ) : (
          <>
            {/* Full-screen routes (no Layout) */}
            <Route path="/projects/:projectId/editor" element={<EditorPage />} />

            {/* Standard routes (with Layout) */}
            <Route element={<Layout />}>
              <Route path="/" element={<Dashboard />} />
              <Route path="/agency-dashboard" element={<AgencyDashboard />} />
              <Route path="/projects/:projectId" element={<ProjectDetail />} />
              <Route path="/projects/:projectId/config" element={<ProjectConfig />} />
              <Route path="/settings" element={<Settings />} />
              <Route path="/ai-optimizer" element={<AiOptimizer />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Route>
          </>
        )}
      </Routes>

      <UpdatePromptModal
        updateInfo={updatePrompt}
        hasPendingBuild={Boolean(pendingDeepLinkBuild)}
        onContinueAnyway={handleContinueBuildAnyway}
        onClose={handleCloseUpdateModal}
      />
    </>
  );
}

function App() {
  return (
    <AppProvider>
      <AppContent />
    </AppProvider>
  );
}

export default App;
