import React, { useState, useEffect, useCallback } from 'react';
import { useAppStore } from '../hooks/useAppStore';
import ReviewModal from '../components/ReviewModal';
import { GitPullRequest, CheckCircle2, Clock, XCircle, RefreshCw, Layers } from 'lucide-react';

export default function AgencyDashboard() {
  const { projects, loadProjects } = useAppStore();
  const [proposals, setProposals] = useState([]);
  const [isLoading, setIsLoading] = useState(true);
  const [selectedProposal, setSelectedProposal] = useState(null);
  const [selectedFilter, setSelectedFilter] = useState('all'); // all | pending_review | approved | rejected

  const fetchProposals = useCallback(async () => {
    setIsLoading(true);
    let allProps = [];
    try {
      if (projects.length === 0) {
        await loadProjects();
      }
      for (const proj of projects) {
        if (window.electronAPI?.agency?.getProposals) {
          const res = await window.electronAPI.agency.getProposals(proj.id);
          if (Array.isArray(res)) {
            allProps.push(...res);
          }
        }
      }
      allProps.sort((a, b) => new Date(b.createdAt) - new Date(a.createdAt));
      setProposals(allProps);
    } catch (e) {
      console.error('Failed to fetch agency proposals:', e);
    } finally {
      setIsLoading(false);
    }
  }, [projects, loadProjects]);

  useEffect(() => {
    fetchProposals();
  }, [fetchProposals]);

  const handleUpdateStatus = async (projectId, proposalId, status) => {
    try {
      if (window.electronAPI?.agency?.updateProposalStatus) {
        await window.electronAPI.agency.updateProposalStatus(projectId, proposalId, status);
        await fetchProposals();
      }
    } catch (e) {
      console.error('Error updating proposal status:', e);
    }
  };

  const filteredProposals = proposals.filter(p => {
    if (selectedFilter === 'all') return true;
    return p.status === selectedFilter;
  });

  const pendingCount = proposals.filter(p => p.status === 'pending_review').length;
  const approvedCount = proposals.filter(p => p.status === 'approved').length;
  const rejectedCount = proposals.filter(p => p.status === 'rejected').length;

  return (
    <div className="space-y-6">
      {/* Header & Stats Banner */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold text-white tracking-tight flex items-center gap-3">
            <GitPullRequest className="w-7 h-7 text-indigo-400" />
            Agency Master Review Dashboard
          </h1>
          <p className="text-sm text-slate-400 mt-1">
            Pregled pristiglih klijentskih predloga, koda i odobravanje izmena u repozitorijum.
          </p>
        </div>

        <button
          onClick={fetchProposals}
          className="btn-secondary text-xs px-4 py-2 flex items-center gap-2 self-start md:self-auto"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${isLoading ? 'animate-spin' : ''}`} />
          Osveži Predloge
        </button>
      </div>

      {/* Metrics Bar */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div className="card p-4 flex items-center justify-between">
          <div>
            <p className="text-xs font-medium text-slate-400">Na Čekanju Pregleda</p>
            <p className="text-2xl font-bold text-amber-400 mt-1">{pendingCount}</p>
          </div>
          <div className="w-10 h-10 rounded-xl bg-amber-500/10 flex items-center justify-center text-amber-400">
            <Clock className="w-5 h-5" />
          </div>
        </div>

        <div className="card p-4 flex items-center justify-between">
          <div>
            <p className="text-xs font-medium text-slate-400">Odobreni Predlozi</p>
            <p className="text-2xl font-bold text-emerald-400 mt-1">{approvedCount}</p>
          </div>
          <div className="w-10 h-10 rounded-xl bg-emerald-500/10 flex items-center justify-center text-emerald-400">
            <CheckCircle2 className="w-5 h-5" />
          </div>
        </div>

        <div className="card p-4 flex items-center justify-between">
          <div>
            <p className="text-xs font-medium text-slate-400">Odbijeni Predlozi</p>
            <p className="text-2xl font-bold text-rose-400 mt-1">{rejectedCount}</p>
          </div>
          <div className="w-10 h-10 rounded-xl bg-rose-500/10 flex items-center justify-center text-rose-400">
            <XCircle className="w-5 h-5" />
          </div>
        </div>

        <div className="card p-4 flex items-center justify-between">
          <div>
            <p className="text-xs font-medium text-slate-400">Ukupno Projekata</p>
            <p className="text-2xl font-bold text-indigo-400 mt-1">{projects.length}</p>
          </div>
          <div className="w-10 h-10 rounded-xl bg-indigo-500/10 flex items-center justify-center text-indigo-400">
            <Layers className="w-5 h-5" />
          </div>
        </div>
      </div>

      {/* Filter Tabs */}
      <div className="flex items-center gap-2 border-b border-slate-800 pb-3">
        {['all', 'pending_review', 'approved', 'rejected'].map(filter => (
          <button
            key={filter}
            onClick={() => setSelectedFilter(filter)}
            className={`px-3 py-1.5 rounded-lg text-xs font-medium capitalize transition-all ${
              selectedFilter === filter
                ? 'bg-indigo-600/10 text-indigo-400 border border-indigo-500/30'
                : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/40'
            }`}
          >
            {filter === 'all' ? 'Svi Predlozi' : filter.replace('_', ' ')}
          </button>
        ))}
      </div>

      {/* Proposal Cards List */}
      {filteredProposals.length > 0 ? (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {filteredProposals.map(proposal => (
            <div
              key={proposal.id}
              className="card p-5 hover:border-slate-700 transition-all flex flex-col justify-between space-y-4"
            >
              <div>
                <div className="flex items-center justify-between mb-2">
                  <span className="font-mono text-xs text-indigo-400 bg-indigo-500/10 px-2 py-0.5 rounded border border-indigo-500/20">
                    {proposal.branchName}
                  </span>
                  <span className={`text-xs font-semibold px-2.5 py-0.5 rounded-full capitalize ${
                    proposal.status === 'pending_review' ? 'bg-amber-500/10 text-amber-400 border border-amber-500/30' :
                    proposal.status === 'approved' ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/30' :
                    'bg-rose-500/10 text-rose-400 border border-rose-500/30'
                  }`}>
                    {proposal.status.replace('_', ' ')}
                  </span>
                </div>

                <h3 className="font-medium text-slate-100 text-sm mb-1">
                  "{proposal.prompt}"
                </h3>
                <p className="text-xs text-slate-400">
                  Projekat: <span className="font-mono text-slate-300">{proposal.projectId}</span>
                </p>
              </div>

              <div className="flex items-center justify-between pt-3 border-t border-slate-800/60">
                <span className="text-[11px] text-slate-500">
                  {new Date(proposal.createdAt).toLocaleString()}
                </span>
                <button
                  onClick={() => setSelectedProposal(proposal)}
                  className="px-3 py-1.5 bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-medium rounded-lg transition-colors shadow-md shadow-indigo-600/20"
                >
                  Pregledaj Izmene
                </button>
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="card p-12 text-center text-slate-500 space-y-2">
          <GitPullRequest className="w-10 h-10 mx-auto text-slate-600" />
          <p className="font-medium text-slate-400">Nema pronađenih klijentskih predloga u ovom filteru.</p>
          <p className="text-xs text-slate-500 max-w-sm mx-auto">
            Kada klijenti u Client Mode-u pošalju izmenu, njihove predložene grane i diff-ovi će se pojaviti ovde za vaš pregled.
          </p>
        </div>
      )}

      {/* Review Modal */}
      {selectedProposal && (
        <ReviewModal
          proposal={selectedProposal}
          onClose={() => setSelectedProposal(null)}
          onUpdateStatus={handleUpdateStatus}
        />
      )}
    </div>
  );
}
