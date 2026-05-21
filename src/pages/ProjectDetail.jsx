/**
 * Axiom Forge - Project Detail
 * Detailed view of a project with file explorer and actions
 */

import React, { useEffect, useState } from 'react';
import { useParams, useNavigate, Link } from 'react-router-dom';
import { 
  ArrowLeft, 
  Folder, 
  FileCode, 
  Settings, 
  Play, 
  Github, 
  Globe,
  Trash2,
  Loader2,
  CheckCircle2,
  AlertCircle,
  Clock,
  Code2
} from 'lucide-react';
import { useAppStore } from '../hooks/useAppStore';

// File tree component
function FileTree({ files, onSelect }) {
  const [expanded, setExpanded] = useState(new Set());

  const toggleFolder = (path) => {
    const newExpanded = new Set(expanded);
    if (newExpanded.has(path)) {
      newExpanded.delete(path);
    } else {
      newExpanded.add(path);
    }
    setExpanded(newExpanded);
  };

  // Group files by directory
  const fileTree = files.reduce((acc, file) => {
    const parts = file.path.split('/');
    let current = acc;
    
    parts.forEach((part, index) => {
      if (index === parts.length - 1) {
        current.files = current.files || [];
        current.files.push(file);
      } else {
        current.folders = current.folders || {};
        current.folders[part] = current.folders[part] || {};
        current = current.folders[part];
      }
    });
    
    return acc;
  }, {});

  const renderNode = (node, path = '', depth = 0) => {
    const folders = node.folders ? Object.entries(node.folders) : [];
    const files = node.files || [];

    return (
      <div key={path} style={{ marginLeft: depth * 12 }}>
        {folders.map(([name, subNode]) => {
          const fullPath = path ? `${path}/${name}` : name;
          const isExpanded = expanded.has(fullPath);

          return (
            <div key={fullPath}>
              <button
                onClick={() => toggleFolder(fullPath)}
                className="flex items-center gap-2 w-full py-1 px-2 rounded hover:bg-slate-800 text-left"
              >
                <Folder className="w-4 h-4 text-indigo-400" />
                <span className="text-sm text-slate-300">{name}</span>
              </button>
              {isExpanded && renderNode(subNode, fullPath, depth + 1)}
            </div>
          );
        })}
        
        {files.map((file) => (
          <button
            key={file.path}
            onClick={() => onSelect(file)}
            className="flex items-center gap-2 w-full py-1 px-2 rounded hover:bg-slate-800 text-left"
          >
            <FileCode className="w-4 h-4 text-slate-500" />
            <span className="text-sm text-slate-400">{file.path.split('/').pop()}</span>
          </button>
        ))}
      </div>
    );
  };

  return (
    <div className="space-y-1">
      {renderNode(fileTree)}
    </div>
  );
}

// Status badge
function StatusBadge({ status }) {
  const configs = {
    'generating': { icon: Loader2, className: 'text-amber-400', text: 'Generating', animate: true },
    'awaiting-keys': { icon: AlertCircle, className: 'text-amber-400', text: 'Awaiting Keys' },
    'ready': { icon: CheckCircle2, className: 'text-indigo-400', text: 'Ready' },
    'deploying': { icon: Loader2, className: 'text-amber-400', text: 'Deploying', animate: true },
    'live': { icon: Globe, className: 'text-emerald-400', text: 'Live' },
    'error': { icon: AlertCircle, className: 'text-rose-400', text: 'Error' }
  };

  const config = configs[status] || configs['ready'];
  const Icon = config.icon;

  return (
    <span className={`flex items-center gap-2 ${config.className}`}>
      <Icon className={`w-4 h-4 ${config.animate ? 'animate-spin' : ''}`} />
      <span className="font-medium">{config.text}</span>
    </span>
  );
}

function ProjectDetail() {
  const { projectId } = useParams();
  const navigate = useNavigate();
  const { 
    projects, 
    loadProjects, 
    removeProject, 
    updateProjectStatus, 
    startDeployment,
    taskProgress 
  } = useAppStore();
  
  const [project, setProject] = useState(null);
  const [selectedFile, setSelectedFile] = useState(null);
  const [isDeploying, setIsDeploying] = useState(false);
  const [isCommitting, setIsCommitting] = useState(false);
  const [deployMessage, setDeployMessage] = useState(null);
  const [fileContent, setFileContent] = useState('');
  const [diskFiles, setDiskFiles] = useState([]);
  const [isLoadingFiles, setIsLoadingFiles] = useState(false);

  // Find active task for this project
  const activeTask = Object.values(taskProgress).find(t => t.projectId === projectId);

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  useEffect(() => {
    const found = projects.find(p => p.id === projectId);
    if (found) {
      setProject(found);
      // Load real files from disk
      setIsLoadingFiles(true);
      window.electronAPI.project.getFiles(projectId)
        .then(result => {
          if (result.success) setDiskFiles(result.files);
        })
        .catch(console.error)
        .finally(() => setIsLoadingFiles(false));
    }
  }, [projectId, projects]);

  const handleDeploy = async () => {
    setIsDeploying(true);
    setDeployMessage(null);
    try {
      const result = await startDeployment(projectId);
      if (result?.success) {
        setDeployMessage({ type: 'success', text: 'Deployment started! Check GitHub and Vercel for progress.' });
        loadProjects();
      }
    } catch (error) {
      console.error('Deployment failed:', error);
      setDeployMessage({ type: 'error', text: `Deploy failed: ${error.message}` });
    } finally {
      setIsDeploying(false);
    }
  };

  const handleCommitToGitHub = async () => {
    setDeployMessage(null);
    setIsCommitting(true);
    try {
      // 1. Local commit
      const commitMsg = `Axiom Forge — ${new Date().toLocaleString()}`;
      await window.electronAPI.project.commit(projectId, commitMsg);
      // 2. Push to GitHub (no Vercel)
      const result = await window.electronAPI.project.pushToGitHub(projectId);
      if (result.success) {
        setDeployMessage({ type: 'success', text: `✅ Committed & pushed to GitHub${result.repoUrl ? ` → ${result.repoUrl}` : ''}` });
        loadProjects();
      } else {
        setDeployMessage({ type: 'error', text: `Push failed: ${result.error}` });
      }
    } catch (error) {
      setDeployMessage({ type: 'error', text: `Commit failed: ${error.message}` });
    } finally {
      setIsCommitting(false);
    }
  };

  const handleDelete = async () => {
    if (confirm('Are you sure you want to delete this project?')) {
      await removeProject(projectId);
      navigate('/');
    }
  };

  const handleFileSelect = async (file) => {
    setSelectedFile(file);
    setFileContent('Loading...');
    try {
      const result = await window.electronAPI.project.readFile(projectId, file.path);
      if (result.success) {
        setFileContent(result.content);
      } else {
        setFileContent(`// Could not read file: ${result.error}`);
      }
    } catch (e) {
      setFileContent(`// ${file.path}\n// (File preview not available)`);
    }
  };

  if (!project) {
    return (
      <div className="flex items-center justify-center py-16">
        <Loader2 className="w-8 h-8 text-indigo-500 animate-spin" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          <button
            onClick={() => navigate(-1)}
            className="p-2 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-white transition-colors"
          >
            <ArrowLeft className="w-5 h-5" />
          </button>
          <div>
            <h1 className="text-2xl font-bold text-white">{project.name}</h1>
            <div className="flex items-center gap-3 mt-1">
              <StatusBadge status={project.status} />
              <span className="text-slate-500">•</span>
              <span className="text-sm text-slate-500">
                Created {new Date(project.createdAt).toLocaleDateString()}
              </span>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {/* Open Editor — always available */}
          <button
            onClick={() => navigate(`/projects/${projectId}/editor`)}
            className="btn-secondary"
            title="Open AI-powered code editor"
          >
            <Code2 className="w-4 h-4" />
            Open Editor
          </button>

          {project.status === 'awaiting-keys' && (
            <Link
              to={`/projects/${projectId}/config`}
              className="btn-secondary"
            >
              <Settings className="w-4 h-4" />
              Configure
            </Link>
          )}
          
          {(project.status === 'ready' || project.status === 'error') && (
            <button
              onClick={handleDeploy}
              disabled={isDeploying}
              className={project.status === 'error' ? 'btn-danger' : 'btn-primary'}
            >
              {isDeploying ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  Deploying...
                </>
              ) : (
                <>
                  <Play className="w-4 h-4" />
                  {project.status === 'error' ? 'Retry Deploy' : 'Deploy'}
                </>
              )}
            </button>
          )}

          {(project.status === 'ready' || project.status === 'error' || project.status === 'live') && (
            <button
              onClick={handleCommitToGitHub}
              disabled={isCommitting || isDeploying}
              className="btn-secondary"
              title="Commit & push to GitHub without redeploying to Vercel"
            >
              {isCommitting ? (
                <><Loader2 className="w-4 h-4 animate-spin" />Committing...</>
              ) : (
                <><Github className="w-4 h-4" />Commit to GitHub</>
              )}
            </button>
          )}

          {project.metadata?.githubUrl && (
            <a
              href={project.metadata.githubUrl}
              onClick={(e) => {
                e.preventDefault();
                window.electronAPI.shell.openExternal(project.metadata.githubUrl);
              }}
              className="btn-secondary"
            >
              <Github className="w-4 h-4" />
              GitHub
            </a>
          )}

          {project.metadata?.deployUrl && (
            <a
              href={project.metadata.deployUrl}
              onClick={(e) => {
                e.preventDefault();
                window.electronAPI.shell.openExternal(project.metadata.deployUrl);
              }}
              className="btn-success"
            >
              <Globe className="w-4 h-4" />
              Live Site
            </a>
          )}

          <button
            onClick={handleDelete}
            className="btn-danger"
          >
            <Trash2 className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Description */}
      {project.description && (
        <p className="text-slate-400">{project.description}</p>
      )}

      {/* Deployment Progress */}
      {isDeploying && (
        <div className="card p-6 bg-slate-800/50 border-indigo-500/30 animate-pulse-subtle">
          <div className="flex items-center justify-between mb-4">
            <h3 className="font-semibold text-white flex items-center gap-2">
              <Loader2 className="w-4 h-4 animate-spin text-indigo-400" />
              Deployment in Progress
            </h3>
            <span className="text-sm font-medium text-indigo-400">
              {Math.round(activeTask?.progress || 0)}%
            </span>
          </div>
          
          <div className="w-full bg-slate-900 rounded-full h-2 mb-4 overflow-hidden">
            <div 
              className="bg-indigo-500 h-full transition-all duration-500 ease-out"
              style={{ width: `${activeTask?.progress || 0}%` }}
            />
          </div>
          
          <p className="text-sm text-slate-400 italic">
            {activeTask?.message || 'Initializing deployment sequence...'}
          </p>
        </div>
      )}

      {/* Main Content */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {deployMessage && (
            <div className={`mt-3 p-3 rounded-lg text-sm ${
              deployMessage.type === 'success' 
                ? 'bg-emerald-500/10 border border-emerald-500/20 text-emerald-400'
                : 'bg-rose-500/10 border border-rose-500/20 text-rose-400'
            }`}>
              {deployMessage.text}
            </div>
          )}

          {/* File Explorer */}
          <div className="lg:col-span-1">
            <div className="card p-4">
              <h3 className="font-semibold text-white mb-4 flex items-center gap-2">
                <Folder className="w-4 h-4" />
                Files
                {isLoadingFiles && <Loader2 className="w-3 h-3 animate-spin text-slate-500 ml-1" />}
              </h3>
              {isLoadingFiles ? (
                <p className="text-sm text-slate-500">Scanning project files...</p>
              ) : diskFiles.length > 0 ? (
                <FileTree 
                  files={diskFiles} 
                  onSelect={handleFileSelect}
                />
              ) : (
                <p className="text-sm text-slate-500">No files found on disk</p>
              )}
            </div>
          </div>

        {/* File Preview */}
        <div className="lg:col-span-2">
          <div className="card p-4 h-full">
            <h3 className="font-semibold text-white mb-4 flex items-center gap-2">
              <Code2 className="w-4 h-4" />
              {selectedFile ? selectedFile.path : 'Select a file to preview'}
            </h3>
            
            {selectedFile ? (
              <pre className="code-block text-xs overflow-auto max-h-96">
                {fileContent}
              </pre>
            ) : (
              <div className="flex items-center justify-center h-64 text-slate-500">
                <p>Click on a file to view its contents</p>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Stats */}
      {project.metadata?.generationStats && (
        <div className="card p-6">
          <h3 className="font-semibold text-white mb-4">Generation Stats</h3>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <div className="p-4 bg-slate-800/50 rounded-lg">
              <p className="text-2xl font-bold text-indigo-400">
                {project.metadata.generationStats.total}
              </p>
              <p className="text-sm text-slate-500">Total Files</p>
            </div>
            <div className="p-4 bg-slate-800/50 rounded-lg">
              <p className="text-2xl font-bold text-emerald-400">
                {project.metadata.generationStats.successful}
              </p>
              <p className="text-sm text-slate-500">Successful</p>
            </div>
            <div className="p-4 bg-slate-800/50 rounded-lg">
              <p className="text-2xl font-bold text-amber-400">
                {project.metadata.generationStats.failed}
              </p>
              <p className="text-sm text-slate-500">Failed</p>
            </div>
            <div className="p-4 bg-slate-800/50 rounded-lg">
              <p className="text-2xl font-bold text-violet-400">
                {project.metadata.generationStats.totalTokens?.toLocaleString() || 'N/A'}
              </p>
              <p className="text-sm text-slate-500">Tokens Used</p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default ProjectDetail;
