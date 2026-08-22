import React, { useState, useEffect } from 'react';
import { useAppStore } from '../hooks/useAppStore';

export default function ClientLayout() {
  const { appMode, setAppMode, projects, loadProjects } = useAppStore();
  const [selectedProjectId, setSelectedProjectId] = useState('');
  const [promptText, setPromptText] = useState('');
  const [isGenerating, setIsGenerating] = useState(false);
  const [proposalStatus, setProposalStatus] = useState('idle'); // idle | modified | submitted
  const [setupKeyInput, setSetupKeyInput] = useState('');
  const [showConfigModal, setShowConfigModal] = useState(false);
  const [statusMessage, setStatusMessage] = useState('');

  const agencyConfig = appMode?.agencyConfig || {
    agencyName: 'Axiom Partner Agency',
    brandColor: '#4f46e5',
    logoUrl: null
  };

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  useEffect(() => {
    if (projects.length > 0 && !selectedProjectId) {
      setSelectedProjectId(projects[0].id);
    }
  }, [projects, selectedProjectId]);

  const activeProject = projects.find(p => p.id === selectedProjectId);

  const [lastPrompt, setLastPrompt] = useState('');

  const handlePromptSubmit = async (e) => {
    e.preventDefault();
    if (!promptText.trim() || !selectedProjectId || isGenerating) return;

    const currentPrompt = promptText;
    setIsGenerating(true);
    setStatusMessage('AI primenjuje vašu izmenu na lokalni preview...');

    try {
      if (window.electronAPI?.editor?.aiProjectEdit) {
        await window.electronAPI.editor.aiProjectEdit(currentPrompt, selectedProjectId);
      }
      setLastPrompt(currentPrompt);
      setProposalStatus('modified');
      setStatusMessage('Izmena je primenjena! Proverite preview i kliknite "Pošalji developeru".');
      setPromptText('');
    } catch (err) {
      console.error('Error applying client edit:', err);
      setStatusMessage('Greška pri izmeni. Molimo pokušajte ponovo.');
    } finally {
      setIsGenerating(false);
    }
  };

  const handleSubmitProposal = async () => {
    if (proposalStatus !== 'modified' || !selectedProjectId) return;
    setStatusMessage('Kreiranje klijentske grane i slanje predloga developeru...');

    try {
      let createdProp = null;
      if (window.electronAPI?.agency?.createProposal) {
        createdProp = await window.electronAPI.agency.createProposal(selectedProjectId, lastPrompt || 'Klijentska izmena');
      }

      if (agencyConfig?.relayUrl && window.electronAPI?.agency?.submitToRelay && createdProp) {
        try {
          await window.electronAPI.agency.submitToRelay(agencyConfig.relayUrl, {
            setupKey: agencyConfig.setupKey || 'DEFAULT_KEY',
            projectId: selectedProjectId,
            prompt: lastPrompt || 'Klijentska izmena',
            branchName: createdProp.branchName || '',
            files: createdProp.files || []
          });
        } catch (relayErr) {
          console.warn('Relay notification skipped:', relayErr);
        }
      }

      setProposalStatus('submitted');
      setStatusMessage('✅ Vaša izmena je uspešno poslata developeru u novu klijentsku granu!');
    } catch (err) {
      console.error('Error submitting proposal:', err);
      setStatusMessage('Greška pri slanju predloga developeru.');
    }
  };

  const handleSwitchToDeveloper = async () => {
    try {
      await window.electronAPI.agency.setAppMode('developer');
      const updatedMode = await window.electronAPI.agency.getAppMode();
      setAppMode(updatedMode);
    } catch (e) {
      console.error('Failed to switch to developer mode:', e);
    }
  };

  const handleSaveAgencyKey = async () => {
    if (!setupKeyInput.trim()) return;
    try {
      const newConfig = {
        agencyName: agencyConfig.agencyName || 'Partner Agency',
        setupKey: setupKeyInput.trim(),
        brandColor: '#6366f1',
        relayUrl: 'https://relay.axiom-forge.com'
      };
      await window.electronAPI.agency.saveAgencyConfig(newConfig);
      const updatedMode = await window.electronAPI.agency.getAppMode();
      setAppMode(updatedMode);
      setShowConfigModal(false);
      setStatusMessage('Agencijski ključ uspešno sačuvan!');
    } catch (e) {
      console.error('Failed to save config:', e);
    }
  };

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100 flex flex-col font-sans select-none">
      {/* Header Bar */}
      <header className="h-16 border-b border-slate-800 bg-slate-900/80 backdrop-blur px-6 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-xl bg-gradient-to-tr from-indigo-600 to-violet-500 flex items-center justify-center font-bold text-white shadow-lg shadow-indigo-500/20">
            {agencyConfig.agencyName.charAt(0)}
          </div>
          <div>
            <h1 className="font-semibold text-sm text-slate-100 leading-tight">
              {agencyConfig.agencyName}
            </h1>
            <p className="text-xs text-indigo-400 font-medium">Klijentski AI Self-Service Portal</p>
          </div>
        </div>

        <div className="flex items-center gap-3">
          {/* Project Selector */}
          {projects.length > 1 && (
            <select
              value={selectedProjectId}
              onChange={(e) => setSelectedProjectId(e.target.value)}
              className="bg-slate-800 border border-slate-700 text-xs text-slate-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-2 focus:ring-indigo-500"
            >
              {projects.map(p => (
                <option key={p.id} value={p.id}>{p.name || p.id}</option>
              ))}
            </select>
          )}

          {/* Config / Mode Switcher */}
          <button
            onClick={() => setShowConfigModal(true)}
            className="text-xs px-3 py-1.5 rounded-lg border border-slate-700 hover:bg-slate-800 text-slate-400 hover:text-slate-200 transition-colors"
          >
            Podešavanje Ključa
          </button>
          
          <button
            onClick={handleSwitchToDeveloper}
            className="text-xs px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-indigo-300 font-medium transition-colors border border-indigo-500/30"
          >
            Pređi u Dev Režim
          </button>
        </div>
      </header>

      {/* Main Canvas Viewport */}
      <main className="flex-1 relative flex flex-col items-center justify-center p-6 bg-slate-950 overflow-hidden">
        {activeProject ? (
          <div className="w-full h-full max-w-6xl bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl flex flex-col overflow-hidden">
            {/* Window Top Bar */}
            <div className="h-10 border-b border-slate-800 bg-slate-900/90 px-4 flex items-center justify-between">
              <div className="flex items-center gap-2">
                <div className="w-3 h-3 rounded-full bg-rose-500/80" />
                <div className="w-3 h-3 rounded-full bg-amber-500/80" />
                <div className="w-3 h-3 rounded-full bg-emerald-500/80" />
                <span className="ml-2 text-xs text-slate-400 font-mono">
                  {activeProject.name || activeProject.id} (Lokalni Uživo Pregled)
                </span>
              </div>
              {proposalStatus === 'modified' && (
                <span className="text-xs font-semibold px-2.5 py-0.5 rounded-full bg-amber-500/10 text-amber-400 border border-amber-500/30 animate-pulse">
                  Nove izmene spremne za slanje
                </span>
              )}
            </div>

            {/* Live Preview Area */}
            <div className="flex-1 bg-slate-950 flex flex-col items-center justify-center p-8 text-center relative">
              <div className="w-16 h-16 rounded-2xl bg-indigo-500/10 border border-indigo-500/20 flex items-center justify-center mb-4 text-indigo-400">
                <svg className="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                </svg>
              </div>
              <h3 className="text-lg font-medium text-slate-200 mb-1">
                Lokalni Pregled Sajta / Aplikacije
              </h3>
              <p className="text-sm text-slate-400 max-w-md mb-6">
                Ukucajte ispod šta želite da promenite na sajtu. AI će odmah napraviti izmenu uživo, a vi potom možete poslati predlog developeru na odobrenje.
              </p>

              {statusMessage && (
                <div className="px-4 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-indigo-300 font-medium">
                  {statusMessage}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="text-center">
            <p className="text-slate-400 text-sm">Nema aktivnih klijentskih projekata.</p>
          </div>
        )}
      </main>

      {/* Bottom Floating Control Bar */}
      <footer className="border-t border-slate-800 bg-slate-900/90 backdrop-blur p-4 px-6">
        <div className="max-w-4xl mx-auto flex items-center gap-3">
          <form onSubmit={handlePromptSubmit} className="flex-1 flex items-center gap-2 bg-slate-950 border border-slate-800 focus-within:border-indigo-500 rounded-xl px-4 py-2 transition-colors">
            <input
              type="text"
              value={promptText}
              onChange={(e) => setPromptText(e.target.value)}
              placeholder="Opišite vašu željenu izmenu (npr. 'Promeni boju dugmeta u plavu i dodaj telefon u footer')..."
              className="w-full bg-transparent text-sm text-slate-100 placeholder-slate-500 focus:outline-none"
              disabled={isGenerating}
            />
            <button
              type="submit"
              disabled={isGenerating || !promptText.trim()}
              className="px-4 py-1.5 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white font-medium text-xs rounded-lg transition-colors flex items-center gap-1.5 shadow-md shadow-indigo-600/20"
            >
              {isGenerating ? (
                <>
                  <div className="w-3 h-3 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                  Primenjujem...
                </>
              ) : (
                'Izmeni'
              )}
            </button>
          </form>

          {/* Submit to Agency Button */}
          <button
            onClick={handleSubmitProposal}
            disabled={proposalStatus !== 'modified'}
            className="px-5 py-2 bg-gradient-to-r from-emerald-600 to-teal-500 hover:from-emerald-500 hover:to-teal-400 disabled:opacity-40 disabled:cursor-not-allowed text-white font-semibold text-xs rounded-xl shadow-lg shadow-emerald-600/20 transition-all flex items-center gap-2 whitespace-nowrap"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
            </svg>
            Pošalji Developeru na Pregled
          </button>
        </div>
      </footer>

      {/* Config Modal */}
      {showConfigModal && (
        <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4 z-50">
          <div className="bg-slate-900 border border-slate-800 rounded-2xl p-6 w-full max-w-md shadow-2xl">
            <h2 className="text-lg font-semibold text-slate-100 mb-1">Podešavanje Agencijskog Ključa</h2>
            <p className="text-xs text-slate-400 mb-4">
              Unesite Setup Key koji vam je poslala agencija (npr. MOJ-RESTORAN-2026).
            </p>

            <input
              type="text"
              value={setupKeyInput}
              onChange={(e) => setSetupKeyInput(e.target.value)}
              placeholder="Unesite vaš Agencijski Setup Key..."
              className="w-full bg-slate-950 border border-slate-800 rounded-xl px-4 py-2.5 text-sm text-slate-100 focus:outline-none focus:ring-2 focus:ring-indigo-500 mb-4 font-mono"
            />

            <div className="flex justify-end gap-2">
              <button
                onClick={() => setShowConfigModal(false)}
                className="px-4 py-2 text-xs text-slate-400 hover:text-slate-200 transition-colors"
              >
                Otkaži
              </button>
              <button
                onClick={handleSaveAgencyKey}
                className="px-4 py-2 text-xs font-semibold bg-indigo-600 hover:bg-indigo-500 text-white rounded-xl shadow-lg shadow-indigo-600/20 transition-colors"
              >
                Sačuvaj Ključ
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
