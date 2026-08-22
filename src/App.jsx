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
import { useAppStore } from './hooks/useAppStore';

function AppContent() {
  const [isLoading, setIsLoading] = useState(true);
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

  // Subscribe to deep link events GLOBALLY (exactly once)
  useEffect(() => {
    if (isLoading) return;

    const unsubscribeBuild = window.electronAPI.deepLink.onBuild(async (data) => {
      console.log('Deep link build received globally:', data);
      if (data.manifestId) {
        try {
          // Trigger the generation process with the security token and originating host
          await startGeneration(data.manifestId, data.projectId || data.manifestId, data.token, data.host);
        } catch (error) {
          console.error('Failed to auto-start generation globally:', error);
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

  if (isLoading) {
    return (
      <div className="min-h-screen bg-slate-950 flex items-center justify-center">
        <div className="flex flex-col items-center gap-4">
          <div className="w-12 h-12 border-4 border-indigo-500 border-t-transparent rounded-full animate-spin" />
          <p className="text-slate-400">Loading Axiom Forge...</p>
        </div>
      </div>
    );
  if (appMode?.mode === 'client') {
    return <ClientLayout />;
  }

  return (
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
