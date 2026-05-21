/**
 * Axiom Forge - Dashboard
 * Main dashboard with project registry
 */

import React, { useEffect, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { 
  Plus, 
  Search, 
  Filter, 
  MoreVertical, 
  ExternalLink, 
  Github, 
  Globe,
  Clock,
  AlertCircle,
  CheckCircle2,
  Loader2,
  Trash2,
  Play,
  Settings
} from 'lucide-react';
import { useAppStore } from '../hooks/useAppStore';

// Status badge component
function StatusBadge({ status }) {
  const configs = {
    'generating': { icon: Loader2, className: 'badge-amber', text: 'Generating', animate: true },
    'awaiting-keys': { icon: AlertCircle, className: 'badge-amber', text: 'Awaiting Keys' },
    'ready': { icon: CheckCircle2, className: 'badge-indigo', text: 'Ready' },
    'deploying': { icon: Loader2, className: 'badge-amber', text: 'Deploying', animate: true },
    'live': { icon: Globe, className: 'badge-emerald', text: 'Live' },
    'error': { icon: AlertCircle, className: 'badge-rose', text: 'Error' },
    'idle': { icon: Clock, className: 'badge-slate', text: 'Idle' }
  };

  const config = configs[status] || configs.idle;
  const Icon = config.icon;

  return (
    <span className={config.className}>
      <Icon className={`w-3 h-3 ${config.animate ? 'animate-spin' : ''}`} />
      {config.text}
    </span>
  );
}

// Project card component
function ProjectCard({ project, onDelete }) {
  const [showMenu, setShowMenu] = useState(false);
  const navigate = useNavigate();

  const formatDate = (dateString) => {
    const date = new Date(dateString);
    return date.toLocaleDateString('en-US', { 
      month: 'short', 
      day: 'numeric',
      year: 'numeric'
    });
  };

  return (
    <div 
      onClick={(e) => {
        if (!e.target.closest('button') && !e.target.closest('a')) {
          navigate(`/projects/${project.id}`);
        }
      }}
      className="card-hover p-5 group relative cursor-pointer"
    >
      {/* Header */}
      <div className="flex items-start justify-between mb-4">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-indigo-500/20 to-violet-500/20 flex items-center justify-center border border-indigo-500/20">
            <span className="text-lg font-bold text-indigo-400">
              {project.name.charAt(0).toUpperCase()}
            </span>
          </div>
          <div>
            <h3 className="font-semibold text-white group-hover:text-indigo-400 transition-colors">
              {project.name}
            </h3>
            <p className="text-xs text-slate-500">
              Created {formatDate(project.createdAt)}
            </p>
          </div>
        </div>
        
        {/* Actions Menu */}
        <div className="relative">
          <button
            onClick={() => setShowMenu(!showMenu)}
            className="p-2 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-white transition-colors"
          >
            <MoreVertical className="w-4 h-4" />
          </button>
          
          {showMenu && (
            <>
              <div 
                className="fixed inset-0 z-10" 
                onClick={() => setShowMenu(false)}
              />
              <div className="absolute right-0 top-full mt-1 w-48 bg-slate-900 border border-slate-800 rounded-lg shadow-xl z-20 py-1">
                <Link
                  to={`/projects/${project.id}`}
                  className="flex items-center gap-2 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800 hover:text-white"
                >
                  <ExternalLink className="w-4 h-4" />
                  View Details
                </Link>
                {project.status === 'awaiting-keys' && (
                  <Link
                    to={`/projects/${project.id}/config`}
                    className="flex items-center gap-2 px-4 py-2 text-sm text-amber-400 hover:bg-slate-800"
                  >
                    <Settings className="w-4 h-4" />
                    Configure
                  </Link>
                )}
                {project.status === 'ready' && (
                  <button
                    onClick={() => {
                      // Trigger deployment
                      window.electronAPI.task.startDeployment(project.id);
                      setShowMenu(false);
                    }}
                    className="w-full flex items-center gap-2 px-4 py-2 text-sm text-emerald-400 hover:bg-slate-800"
                  >
                    <Play className="w-4 h-4" />
                    Deploy
                  </button>
                )}
                <hr className="my-1 border-slate-800" />
                <button
                  onClick={() => {
                    onDelete(project.id);
                    setShowMenu(false);
                  }}
                  className="w-full flex items-center gap-2 px-4 py-2 text-sm text-rose-400 hover:bg-slate-800"
                >
                  <Trash2 className="w-4 h-4" />
                  Delete
                </button>
              </div>
            </>
          )}
        </div>
      </div>

      {/* Description */}
      <p className="text-sm text-slate-400 mb-4 line-clamp-2">
        {project.description || 'No description provided'}
      </p>

      {/* Footer */}
      <div className="flex items-center justify-between">
        <StatusBadge status={project.status} />
        
        <div className="flex items-center gap-2">
          {project.metadata?.githubUrl && (
            <a
              href={project.metadata.githubUrl}
              onClick={(e) => {
                e.preventDefault();
                window.electronAPI.shell.openExternal(project.metadata.githubUrl);
              }}
              className="p-1.5 rounded-lg hover:bg-slate-800 text-slate-500 hover:text-white transition-colors"
              title="View on GitHub"
            >
              <Github className="w-4 h-4" />
            </a>
          )}
          {project.metadata?.deployUrl && (
            <a
              href={project.metadata.deployUrl}
              onClick={(e) => {
                e.preventDefault();
                window.electronAPI.shell.openExternal(project.metadata.deployUrl);
              }}
              className="p-1.5 rounded-lg hover:bg-slate-800 text-slate-500 hover:text-white transition-colors"
              title="View Live Site"
            >
              <Globe className="w-4 h-4" />
            </a>
          )}
        </div>
      </div>

      {/* Tech Stack Tags */}
      {project.metadata?.techStack && (
        <div className="flex flex-wrap gap-1.5 mt-4 pt-4 border-t border-slate-800">
          {Object.entries(project.metadata.techStack).slice(0, 3).map(([key, value]) => (
            <span 
              key={key}
              className="px-2 py-0.5 bg-slate-800 text-slate-400 text-xs rounded"
            >
              {value}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

// Empty state component
function EmptyState() {
  return (
    <div className="text-center py-16">
      <div className="w-20 h-20 mx-auto rounded-2xl bg-slate-900 border border-slate-800 flex items-center justify-center mb-6">
        <Plus className="w-10 h-10 text-slate-600" />
      </div>
      <h3 className="text-xl font-semibold text-white mb-2">
        No projects yet
      </h3>
      <p className="text-slate-400 max-w-md mx-auto mb-6">
        Start by browsing templates on Axiom Forge and click "Build with Axiom Forge" 
        to create your first project.
      </p>
      <button
        onClick={() => window.electronAPI.shell.openExternal('https://axiomforge.io/templates')}
        className="btn-primary"
      >
        Browse Templates
        <ExternalLink className="w-4 h-4" />
      </button>
    </div>
  );
}

// Main Dashboard Component
function Dashboard() {
  const { projects, loadProjects, removeProject, isLoading } = useAppStore();
  const [searchQuery, setSearchQuery] = useState('');
  const [filterStatus, setFilterStatus] = useState('all');
  const [showFilterMenu, setShowFilterMenu] = useState(false);

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  // Filter and search projects
  const filteredProjects = projects.filter(project => {
    const matchesSearch = project.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
                         project.description?.toLowerCase().includes(searchQuery.toLowerCase());
    const matchesFilter = filterStatus === 'all' || project.status === filterStatus;
    return matchesSearch && matchesFilter;
  });

  const handleDelete = async (projectId) => {
    if (confirm('Are you sure you want to delete this project?')) {
      await removeProject(projectId);
    }
  };

  const statusFilters = [
    { value: 'all', label: 'All Projects' },
    { value: 'generating', label: 'Generating' },
    { value: 'awaiting-keys', label: 'Awaiting Keys' },
    { value: 'ready', label: 'Ready' },
    { value: 'deploying', label: 'Deploying' },
    { value: 'live', label: 'Live' },
    { value: 'error', label: 'Error' }
  ];

  return (
    <div className="space-y-6">
      {/* Header Actions */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold text-white">Projects</h1>
          <p className="text-slate-400">
            {projects.length} project{projects.length !== 1 ? 's' : ''} total
          </p>
        </div>
        
        <button
          onClick={() => window.electronAPI.shell.openExternal('https://axiomforge.io/templates')}
          className="btn-primary"
        >
          <Plus className="w-5 h-5" />
          New Project
        </button>
      </div>

      {/* Search and Filter */}
      <div className="flex flex-col sm:flex-row gap-3">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-slate-500" />
          <input
            type="text"
            placeholder="Search projects..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="input pl-10"
          />
        </div>
        
        <div className="relative">
          <button
            onClick={() => setShowFilterMenu(!showFilterMenu)}
            className="btn-secondary"
          >
            <Filter className="w-4 h-4" />
            {filterStatus === 'all' ? 'All Status' : statusFilters.find(f => f.value === filterStatus)?.label}
          </button>
          
          {showFilterMenu && (
            <>
              <div 
                className="fixed inset-0 z-10" 
                onClick={() => setShowFilterMenu(false)}
              />
              <div className="absolute right-0 top-full mt-1 w-48 bg-slate-900 border border-slate-800 rounded-lg shadow-xl z-20 py-1">
                {statusFilters.map((filter) => (
                  <button
                    key={filter.value}
                    onClick={() => {
                      setFilterStatus(filter.value);
                      setShowFilterMenu(false);
                    }}
                    className={`w-full text-left px-4 py-2 text-sm hover:bg-slate-800 ${
                      filterStatus === filter.value ? 'text-indigo-400' : 'text-slate-300'
                    }`}
                  >
                    {filter.label}
                  </button>
                ))}
              </div>
            </>
          )}
        </div>
      </div>

      {/* Projects Grid */}
      {isLoading ? (
        <div className="flex items-center justify-center py-16">
          <Loader2 className="w-8 h-8 text-indigo-500 animate-spin" />
        </div>
      ) : filteredProjects.length > 0 ? (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredProjects.map((project) => (
            <ProjectCard 
              key={project.id} 
              project={project} 
              onDelete={handleDelete}
            />
          ))}
        </div>
      ) : projects.length === 0 ? (
        <EmptyState />
      ) : (
        <div className="text-center py-16">
          <p className="text-slate-500">No projects match your search</p>
        </div>
      )}
    </div>
  );
}

export default Dashboard;
