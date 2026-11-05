import React, { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  CheckCircle2, XCircle, X, GitPullRequest, FileCode, Check,
  Sparkles, Code2, Loader2, ArrowRight
} from 'lucide-react';

export default function ReviewModal({ proposal, onClose, onUpdateStatus, onProposalUpdated }) {
  const navigate = useNavigate();
  const [currentProposal, setCurrentProposal] = useState(proposal);
  const [isProcessing, setIsProcessing] = useState(false);
  const [activeFileIndex, setActiveFileIndex] = useState(0);

  // Developer AI Refinement state
  const [devPrompt, setDevPrompt] = useState('');
  const [isRefining, setIsRefining] = useState(false);
  const [refineFeedback, setRefineFeedback] = useState(null); // { type: 'success' | 'error', message: string }

  if (!currentProposal) return null;

  const handleAction = async (status) => {
    setIsProcessing(true);
    try {
      await onUpdateStatus(currentProposal.projectId, currentProposal.id, status);
      onClose();
    } catch (err) {
      console.error(`Failed to update status to ${status}:`, err);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleRefineWithAi = async (e) => {
    e.preventDefault();
    if (!devPrompt.trim() || isRefining) return;

    setIsRefining(true);
    setRefineFeedback(null);

    try {
      if (window.electronAPI?.agency?.refineProposalWithAi) {
        const updated = await window.electronAPI.agency.refineProposalWithAi(
          currentProposal.projectId,
          currentProposal.id,
          devPrompt.trim()
        );
        setCurrentProposal(updated);
        onProposalUpdated?.(updated);
        setDevPrompt('');
        setRefineFeedback({
          type: 'success',
          message: 'AI je uspešno doradio predlog na klijentskoj grani! Diff je ažuriran ispod.'
        });
        setActiveFileIndex(0);
      } else {
        throw new Error('refineProposalWithAi API nije dostupan');
      }
    } catch (err) {
      console.error('Failed to refine proposal with AI:', err);
      setRefineFeedback({
        type: 'error',
        message: `Greška pri AI doradi: ${err?.message || err}`
      });
    } finally {
      setIsRefining(false);
    }
  };

  const handleOpenInEditor = async () => {
    setIsProcessing(true);
    try {
      if (window.electronAPI?.agency?.checkoutProposalBranch) {
        await window.electronAPI.agency.checkoutProposalBranch(
          currentProposal.projectId,
          currentProposal.branchName
        );
      }
      onClose();
      navigate(
        `/projects/${currentProposal.projectId}/editor?proposalId=${currentProposal.id}&branch=${encodeURIComponent(
          currentProposal.branchName
        )}&prompt=${encodeURIComponent(currentProposal.prompt)}`
      );
    } catch (err) {
      console.error('Failed to checkout and open in editor:', err);
      setIsProcessing(false);
    }
  };

  const activeFile = currentProposal.files?.[activeFileIndex] || null;

  return (
    <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-md flex items-center justify-center p-6 z-50 animate-fade-in">
      <div className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-5xl h-[88vh] flex flex-col shadow-2xl overflow-hidden">
        {/* Modal Header */}
        <div className="p-5 border-b border-slate-800 bg-slate-900/90 flex items-center justify-between shrink-0">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-xl bg-indigo-500/10 border border-indigo-500/20 flex items-center justify-center text-indigo-400">
              <GitPullRequest className="w-5 h-5" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h2 className="font-semibold text-base text-slate-100">
                  Pregled i Dorada Klijentskog Predloga
                </h2>
                <span className="font-mono text-xs px-2 py-0.5 rounded bg-slate-800 text-indigo-400 border border-slate-700">
                  {currentProposal.branchName}
                </span>
              </div>
              <p className="text-xs text-slate-400 mt-0.5">
                Projekat ID: <span className="font-mono text-slate-300">{currentProposal.projectId}</span> • Poslato: {new Date(currentProposal.createdAt).toLocaleString()}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={handleOpenInEditor}
              disabled={isProcessing}
              className="px-3.5 py-1.5 rounded-xl bg-indigo-600/20 hover:bg-indigo-600/30 text-indigo-300 border border-indigo-500/30 text-xs font-semibold transition-all flex items-center gap-2"
              title="Otvori puni Monaco editor i živi preview na ovoj grani"
            >
              <Code2 className="w-4 h-4 text-indigo-400" />
              <span>Otvori u Editoru (Live Edit & Preview)</span>
              <ArrowRight className="w-3.5 h-3.5" />
            </button>

            <button
              onClick={onClose}
              className="p-1.5 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-slate-200 transition-colors"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* Client Prompt & Banner */}
        <div className="px-6 py-3 bg-slate-950 border-b border-slate-800/80 flex items-center justify-between gap-4 shrink-0">
          <div className="flex items-center gap-3 flex-1 min-w-0">
            <span className="text-xs font-semibold uppercase tracking-wider text-indigo-400 shrink-0">
              Prompt Klijenta:
            </span>
            <p className="text-sm font-medium text-slate-200 bg-slate-900/80 px-3 py-1 rounded-lg border border-slate-800 truncate">
              "{currentProposal.prompt}"
            </p>
          </div>
          <div className="text-xs text-slate-400 shrink-0">
            Status: <span className="capitalize font-semibold text-amber-400">{currentProposal.status.replace('_', ' ')}</span>
          </div>
        </div>

        {/* Developer AI Refinement Bar */}
        <div className="px-6 py-2.5 bg-slate-900/60 border-b border-slate-800 shrink-0">
          <form onSubmit={handleRefineWithAi} className="flex items-center gap-2">
            <div className="flex items-center gap-1.5 text-xs text-indigo-400 font-semibold uppercase tracking-wider shrink-0">
              <Sparkles className="w-3.5 h-3.5" />
              <span>AI Dorada:</span>
            </div>
            <input
              type="text"
              value={devPrompt}
              onChange={(e) => setDevPrompt(e.target.value)}
              placeholder="Unesite precizan prompt za dodatnu AI doradu koda (npr. 'Popravi padding na py-3 px-6, uskladi boju u indigo-600 i dodaj tranziciju')..."
              disabled={isRefining}
              className="flex-1 bg-slate-950 border border-slate-800 rounded-lg px-3 py-1.5 text-xs text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            />
            <button
              type="submit"
              disabled={isRefining || !devPrompt.trim()}
              className="px-4 py-1.5 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white font-medium text-xs rounded-lg transition-colors flex items-center gap-1.5 shrink-0 shadow-md shadow-indigo-600/20"
            >
              {isRefining ? (
                <>
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  <span>Dorađujem...</span>
                </>
              ) : (
                <>
                  <Sparkles className="w-3.5 h-3.5" />
                  <span>Doradi uz AI</span>
                </>
              )}
            </button>
          </form>

          {refineFeedback && (
            <div
              className={`mt-2 text-xs px-3 py-1 rounded-md border ${
                refineFeedback.type === 'success'
                  ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
                  : 'bg-rose-500/10 text-rose-400 border-rose-500/20'
              }`}
            >
              {refineFeedback.message}
            </div>
          )}
        </div>

        {/* Diff Content Body */}
        <div className="flex-1 flex overflow-hidden">
          {/* File Selector Sidebar */}
          <div className="w-64 border-r border-slate-800 bg-slate-900/40 p-3 space-y-1 overflow-y-auto shrink-0">
            <p className="text-[10px] uppercase font-bold text-slate-500 tracking-wider mb-2 px-2">
              Izmenjeni Fajlovi ({currentProposal.files?.length || 0})
            </p>
            {currentProposal.files?.map((f, idx) => (
              <button
                key={idx}
                onClick={() => setActiveFileIndex(idx)}
                className={`w-full text-left px-3 py-2 rounded-lg text-xs flex items-center gap-2 transition-all ${
                  activeFileIndex === idx
                    ? 'bg-indigo-600/10 text-indigo-400 border border-indigo-500/20 font-medium'
                    : 'text-slate-400 hover:bg-slate-800/50 hover:text-slate-200'
                }`}
              >
                <FileCode className="w-4 h-4 shrink-0" />
                <span className="truncate">{f.filePath}</span>
              </button>
            ))}
          </div>

          {/* Diff Viewer Area */}
          <div className="flex-1 bg-slate-950 p-4 overflow-auto font-mono text-xs">
            {activeFile ? (
              <div className="bg-slate-900/80 border border-slate-800 rounded-xl p-4 overflow-x-auto whitespace-pre">
                {activeFile.diff ? (
                  activeFile.diff.split('\n').map((line, i) => {
                    const isAdd = line.startsWith('+');
                    const isDel = line.startsWith('-');
                    const isHeader = line.startsWith('@@') || line.startsWith('diff');
                    return (
                      <div
                        key={i}
                        className={`px-2 py-0.5 rounded-sm ${
                          isAdd
                            ? 'bg-emerald-500/10 text-emerald-400'
                            : isDel
                            ? 'bg-rose-500/10 text-rose-400'
                            : isHeader
                            ? 'text-indigo-400 font-bold'
                            : 'text-slate-300'
                        }`}
                      >
                        {line}
                      </div>
                    );
                  })
                ) : (
                  <p className="text-slate-500 italic">Nema zabeleženih linija izmena.</p>
                )}
              </div>
            ) : (
              <div className="h-full flex items-center justify-center text-slate-500 italic">
                Izaberite fajl sa leve strane za pregled izmena.
              </div>
            )}
          </div>
        </div>

        {/* Footer Actions */}
        <div className="p-4 border-t border-slate-800 bg-slate-900/90 flex items-center justify-between px-6 shrink-0">
          <p className="text-xs text-slate-500">
            Odobrenjem predloga, grana <span className="font-mono text-slate-300">{currentProposal.branchName}</span> biće automatski spojena u <span className="font-mono text-slate-300">main</span>.
          </p>

          <div className="flex items-center gap-3">
            <button
              onClick={() => handleAction('rejected')}
              disabled={isProcessing}
              className="px-4 py-2 rounded-xl border border-rose-500/30 text-rose-400 hover:bg-rose-500/10 text-xs font-semibold transition-colors flex items-center gap-1.5"
            >
              <XCircle className="w-4 h-4" />
              Odbij Predlog
            </button>

            <button
              onClick={handleOpenInEditor}
              disabled={isProcessing}
              className="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-indigo-300 border border-indigo-500/30 text-xs font-semibold transition-colors flex items-center gap-1.5"
            >
              <Code2 className="w-4 h-4" />
              Direktan Edit & Preview
            </button>

            <button
              onClick={() => handleAction('approved')}
              disabled={isProcessing}
              className="px-5 py-2 rounded-xl bg-gradient-to-r from-emerald-600 to-teal-500 hover:from-emerald-500 hover:to-teal-400 text-white text-xs font-semibold shadow-lg shadow-emerald-600/20 transition-all flex items-center gap-1.5"
            >
              <CheckCircle2 className="w-4 h-4" />
              Odobri i Merge-uj u Main
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
