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
import { useAppStore } from './hooks/useAppStore';

function AppContent() {
  const [isLoading, setIsLoading] = useState(true);
  const { isFirstRun, checkFirstRun } = useAppStore();

  useEffect(() => {
    const init = async () => {
      await checkFirstRun();
      setIsLoading(false);
    };
    init();
  }, []);

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
