/**
 * Axiom Forge - AI Editor
 * Full-screen Monaco Editor + AI Chat Panel
 * Route: /projects/:projectId/editor
 */

import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useParams, useNavigate } from 'react-router-dom';

import Editor, { DiffEditor, loader } from '@monaco-editor/react';

// Configure Monaco to load locally from public/vs folder to prevent any CDN network errors or offline crashes
loader.config({
  paths: {
    vs: '/vs'
  }
});
// ────────────────────────────────────────────────────────────────────────

import {
  ArrowLeft, Save, GitCommit, ChevronRight, ChevronDown,
  File, Folder, FolderOpen, Send, Loader2, Check, X,
  RotateCcw, Sparkles, Code2, AlertCircle, Eye, Monitor,
  FunctionSquare, Search, Info, Terminal, Settings,
  MousePointer, Crosshair
} from 'lucide-react';

// ==================== HELPERS ====================

function getLanguage(filePath) {
  const ext = filePath.split('.').pop()?.toLowerCase();
  const map = {
    js: 'javascript', jsx: 'javascript', mjs: 'javascript',
    ts: 'typescript', tsx: 'typescript',
    html: 'html', htm: 'html',
    css: 'css', scss: 'scss', sass: 'sass', less: 'less',
    json: 'json', jsonc: 'json',
    md: 'markdown', mdx: 'markdown',
    php: 'php',
    py: 'python',
    rb: 'ruby',
    rs: 'rust',
    go: 'go',
    cpp: 'cpp', cc: 'cpp', cxx: 'cpp',
    c: 'c', h: 'c',
    java: 'java',
    sh: 'shell', bash: 'shell',
    yaml: 'yaml', yml: 'yaml',
    toml: 'ini',
    xml: 'xml',
    svg: 'xml',
    sql: 'sql',
  };
  return map[ext] || 'plaintext';
}

function getFileIcon(filePath, isDir) {
  if (isDir) return null;
  const ext = filePath.split('.').pop()?.toLowerCase();
  const colors = {
    js: '#f7df1e', jsx: '#61dafb', ts: '#3178c6', tsx: '#61dafb',
    css: '#264de4', scss: '#cc6699', html: '#e34c26',
    json: '#cbcb41', md: '#519aba',
    py: '#3572A5', php: '#8892be', rs: '#dea584',
    go: '#00add8', java: '#b07219',
  };
  return colors[ext] || '#a0aec0';
}

// ==================== FILE TREE COMPONENT ====================

function FileTreeNode({ node, depth = 0, onSelect, selectedPath }) {
  const [open, setOpen] = useState(depth < 2);
  const isDir = node.type === 'directory';
  const isSelected = selectedPath === node.path;
  const color = getFileIcon(node.name, isDir);

  return (
    <div>
      <div
        className={`flex items-center gap-1 px-2 py-[3px] cursor-pointer rounded text-sm transition-colors group
          ${isSelected ? 'bg-indigo-600/30 text-white' : 'text-slate-400 hover:bg-slate-700/50 hover:text-slate-200'}`}
        style={{ paddingLeft: `${8 + depth * 14}px` }}
        onClick={() => isDir ? setOpen(o => !o) : onSelect(node)}
      >
        {isDir ? (
          <>
            {open
              ? <ChevronDown className="w-3 h-3 shrink-0 text-slate-500" />
              : <ChevronRight className="w-3 h-3 shrink-0 text-slate-500" />}
            {open
              ? <FolderOpen className="w-3.5 h-3.5 shrink-0 text-yellow-400/80" />
              : <Folder className="w-3.5 h-3.5 shrink-0 text-yellow-400/60" />}
          </>
        ) : (
          <>
            <span className="w-3 h-3 shrink-0" />
            <File className="w-3.5 h-3.5 shrink-0" style={{ color: color || '#a0aec0' }} />
          </>
        )}
        <span className="truncate ml-0.5">{node.name}</span>
      </div>
      {isDir && open && node.children?.map(child => (
        <FileTreeNode
          key={child.path}
          node={child}
          depth={depth + 1}
          onSelect={onSelect}
          selectedPath={selectedPath}
        />
      ))}
    </div>
  );
}

// Build tree from flat file list
function buildTree(files) {
  const root = { name: 'root', type: 'directory', children: [], path: '' };
  for (const f of files) {
    const parts = f.path.split('/');
    let node = root;
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const isLast = i === parts.length - 1;
      if (isLast) {
        node.children.push({ name: part, path: f.path, type: 'file', size: f.size });
      } else {
        let dir = node.children.find(c => c.name === part && c.type === 'directory');
        if (!dir) {
          dir = { name: part, path: parts.slice(0, i + 1).join('/'), type: 'directory', children: [] };
          node.children.push(dir);
        }
        node = dir;
      }
    }
  }
  // Sort: dirs first, then files, both alphabetically
  const sort = (n) => {
    if (n.children) {
      n.children.sort((a, b) => {
        if (a.type !== b.type) return a.type === 'directory' ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
      n.children.forEach(sort);
    }
  };
  sort(root);
  return root.children;
}

// ==================== AI PANEL COMPONENT ====================

function AIPanel({
  activeFile,
  activeContent,
  projectId,
  onApply,
  pendingContent,
  setPendingContent,
  visualContext,
  onClearVisualContext,
  
  // Project-level props
  aiScope,
  setAiScope,
  pendingProjectChanges,
  setPendingProjectChanges,
  aiProgress,
  setAiProgress,
  onApplyProjectFile,
  onDiscardProjectFile,
  onApplyAllProjectChanges,
  onDiscardAllProjectChanges,
  onReviewProjectFile
}) {
  const [messages, setMessages] = useState([
    { role: 'assistant', content: 'Hello! I\'m connected to your local Ollama model. Tell me what you\'d like to change.' }
  ]);
  const [input, setInput] = useState('');
  const [isThinking, setIsThinking] = useState(false);
  const messagesEndRef = useRef(null);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  useEffect(() => {
    // Slušamo direktivu
    console.log("[AIPanel] Registering onAiDirective listener");
    const unsubDirective = window.electronAPI.editor.onAiDirective(({ directive, optimizedPrompt, scope }) => {
      console.log("[AIPanel] Received directive:", directive, "Scope:", scope);
      setMessages(m => [...m, { 
        role: 'system', 
        content: `**IK Firewall Audit Završen**\n\n**Identifikovan Opseg (Scope):**\n\`${scope || 'ceo fajl'}\`\n\n**Optimizovan Prompt:**\n${optimizedPrompt}\n\n**Agent Directive:**\n${directive}` 
      }]);
    });

    // Slušamo strim
    console.log("[AIPanel] Registering onAiChunk listener");
    const unsubChunk = window.electronAPI.editor.onAiChunk(({ token }) => {
      setMessages(m => {
        const lastMsg = m[m.length - 1];
        if (lastMsg && lastMsg.role === 'assistant-stream') {
          return [...m.slice(0, -1), { ...lastMsg, content: lastMsg.content + token }];
        } else {
          return [...m, { role: 'assistant-stream', content: token }];
        }
      });
    });

    return () => {
      if (unsubDirective) unsubDirective();
      if (unsubChunk) unsubChunk();
    };
  }, []);

  const handleSend = async () => {
    if (!input.trim() || isThinking) return;

    const userMsg = input.trim();
    setInput('');
    setMessages(m => [...m, { role: 'user', content: userMsg }]);
    setIsThinking(true);

    if (aiScope === 'file') {
      if (!activeFile) {
        setMessages(m => [...m, { role: 'assistant', content: '⚠️ Please open a file first before asking me to make changes.' }]);
        setIsThinking(false);
        return;
      }
      setPendingContent(null);

      try {
        const result = await window.electronAPI.editor.aiEdit(
          activeFile.path,
          activeContent,
          userMsg,
          projectId,
          visualContext
        );

        if (result.success) {
          setPendingContent(result.newContent);
          setMessages(m => [...m, {
            role: 'assistant',
            content: `✅ I've prepared the changes for **${activeFile.name || activeFile.path}**. Review below and click **Apply** to save.`,
            hasDiff: true
          }]);
        } else {
          setMessages(m => [...m, { role: 'assistant', content: `❌ Error: ${result.error}` }]);
        }
      } catch (err) {
        setMessages(m => [...m, { role: 'assistant', content: `❌ Connection error: ${err.message}` }]);
      } finally {
        setIsThinking(false);
      }
    } else {
      // Entire Project Scope
      setPendingProjectChanges(null);
      setAiProgress({ status: 'Starting project analysis...', percent: 0 });

      try {
        const result = await window.electronAPI.editor.aiProjectEdit(
          userMsg,
          projectId
        );

        if (result.success && result.modifiedFiles) {
          setPendingProjectChanges(result.modifiedFiles);
          setMessages(m => [...m, {
            role: 'assistant',
            content: `✅ Generated changes for **${Object.keys(result.modifiedFiles).length} files**. Click 'Review' (👁️) next to each file below to inspect, edit, or apply changes.`,
          }]);
        } else {
          setMessages(m => [...m, { role: 'assistant', content: `❌ Error: ${result.error}` }]);
        }
      } catch (err) {
        setMessages(m => [...m, { role: 'assistant', content: `❌ Connection error: ${err.message}` }]);
      } finally {
        setIsThinking(false);
        setAiProgress(null);
      }
    }
  };

  const handleApply = async () => {
    if (!pendingContent) return;
    await onApply(pendingContent, `AI: ${messages.filter(m => m.role === 'user').at(-1)?.content?.slice(0, 60)}`);
    setPendingContent(null);
    setMessages(m => [...m, { role: 'assistant', content: '✅ Changes applied and saved to disk.' }]);
  };

  const handleDiscard = () => {
    setPendingContent(null);
    setMessages(m => [...m, { role: 'assistant', content: 'Changes discarded.' }]);
  };

  return (
    <div className="flex flex-col h-full bg-slate-900 border-l border-slate-700/50">
      {/* Header */}
      <div className="flex items-center gap-2 px-4 py-3 border-b border-slate-700/50 shrink-0">
        <Sparkles className="w-4 h-4 text-indigo-400" />
        <span className="text-sm font-semibold text-white">AI Assistant</span>
        <span className="text-xs text-slate-500 ml-auto">ollama</span>
      </div>

      {/* Scope Toggles */}
      <div className="flex border-b border-slate-700/50 bg-slate-900/60 shrink-0">
        <button
          onClick={() => setAiScope('file')}
          className={`flex-1 py-2 text-xs font-semibold text-center transition-colors
            ${aiScope === 'file' ? 'text-indigo-400 border-b-2 border-indigo-500 bg-slate-900' : 'text-slate-400 hover:text-slate-200'}`}
        >
          Active File
        </button>
        <button
          onClick={() => setAiScope('project')}
          className={`flex-1 py-2 text-xs font-semibold text-center transition-colors
            ${aiScope === 'project' ? 'text-indigo-400 border-b-2 border-indigo-500 bg-slate-900' : 'text-slate-400 hover:text-slate-200'}`}
        >
          Entire Project
        </button>
      </div>

      {/* Active file context indicator (only for File scope) */}
      {aiScope === 'file' && activeFile && (
        <div className="flex items-center gap-2 px-3 py-1.5 bg-slate-800/50 border-b border-slate-700/30 shrink-0">
          <Code2 className="w-3 h-3 text-indigo-400" />
          <span className="text-xs text-slate-400 truncate">{activeFile.path}</span>
        </div>
      )}

      {/* Real-time Project Edit progress bar */}
      {aiScope === 'project' && aiProgress && (
        <div className="px-3 py-2 bg-indigo-950/40 border-b border-indigo-500/20 text-xs text-indigo-300 shrink-0">
          <div className="flex justify-between mb-1">
            <span className="truncate font-medium">{aiProgress.status}</span>
            <span>{Math.round(aiProgress.percent)}%</span>
          </div>
          <div className="w-full bg-slate-800 rounded-full h-1 overflow-hidden">
            <div className="bg-indigo-500 h-full transition-all duration-300" style={{ width: `${aiProgress.percent}%` }} />
          </div>
        </div>
      )}

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-3 py-3 space-y-3 custom-scrollbar">
        {messages.map((msg, i) => (
          <div key={i} className={`flex ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}>
            <div className={`max-w-[90%] rounded-lg px-3 py-2 text-sm leading-relaxed whitespace-pre-wrap
              ${msg.role === 'user'
                ? 'bg-indigo-600 text-white rounded-br-sm'
                : msg.role === 'system'
                ? 'bg-slate-800/80 border border-indigo-500/30 text-indigo-200 text-xs rounded-bl-sm font-mono'
                : 'bg-slate-800 text-slate-300 rounded-bl-sm border border-slate-700/50'}`}>
              {msg.content}
            </div>
          </div>
        ))}

        {isThinking && !aiProgress && (
          <div className="flex justify-start">
            <div className="bg-slate-800 border border-slate-700/50 rounded-lg rounded-bl-sm px-3 py-2">
              <div className="flex gap-1 items-center">
                <div className="w-1.5 h-1.5 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                <div className="w-1.5 h-1.5 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                <div className="w-1.5 h-1.5 bg-indigo-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
              </div>
            </div>
          </div>
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* Project scope pending changes list */}
      {aiScope === 'project' && pendingProjectChanges && Object.keys(pendingProjectChanges).length > 0 && (
        <div className="px-3 py-2.5 border-t border-slate-700/50 bg-slate-850/80 shrink-0">
          <p className="text-[10px] font-bold text-slate-400 mb-2 uppercase tracking-wider">Pending Changes ({Object.keys(pendingProjectChanges).length})</p>
          <div className="space-y-1.5 max-h-32 overflow-y-auto custom-scrollbar mb-2.5">
            {Object.keys(pendingProjectChanges).map(filePath => (
              <div key={filePath} className="flex items-center justify-between bg-slate-900/60 px-2 py-1.5 rounded border border-slate-700/30 gap-2">
                <span className="text-xs text-slate-300 truncate flex-1 font-mono">{filePath}</span>
                <div className="flex gap-1 shrink-0">
                  <button
                    onClick={() => onReviewProjectFile(filePath)}
                    title="Review Diff"
                    className="p-1 text-slate-400 hover:text-indigo-400 hover:bg-slate-800 rounded transition-colors"
                  >
                    <Eye className="w-3.5 h-3.5" />
                  </button>
                  <button
                    onClick={() => onApplyProjectFile(filePath, pendingProjectChanges[filePath])}
                    title="Apply file changes"
                    className="p-1 text-emerald-400 hover:text-emerald-300 hover:bg-emerald-950/30 rounded transition-colors"
                  >
                    <Check className="w-3.5 h-3.5" />
                  </button>
                  <button
                    onClick={() => onDiscardProjectFile(filePath)}
                    title="Discard file changes"
                    className="p-1 text-red-400 hover:text-red-300 hover:bg-red-950/30 rounded transition-colors"
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            ))}
          </div>
          <div className="flex gap-2">
            <button
              onClick={() => onApplyAllProjectChanges(pendingProjectChanges)}
              className="flex-1 flex items-center justify-center gap-1.5 py-1.5 bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold rounded-lg transition-colors shadow-md"
            >
              <Sparkles className="w-3.5 h-3.5" /> Apply All
            </button>
            <button
              onClick={onDiscardAllProjectChanges}
              className="flex-1 flex items-center justify-center gap-1.5 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-semibold rounded-lg transition-colors border border-slate-700"
            >
              <X className="w-3.5 h-3.5" /> Discard All
            </button>
          </div>
        </div>
      )}

      {/* File scope pending diff action buttons */}
      {aiScope === 'file' && pendingContent && (
        <div className="px-3 py-2 border-t border-slate-700/50 bg-slate-800/50 shrink-0">
          <p className="text-xs text-slate-400 mb-2">Ready to apply changes:</p>
          <div className="flex gap-2">
            <button
              onClick={handleApply}
              className="flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-medium rounded-lg transition-colors"
            >
              <Check className="w-3.5 h-3.5" /> Apply
            </button>
            <button
              onClick={handleDiscard}
              className="flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 bg-slate-700 hover:bg-slate-600 text-slate-300 text-xs font-medium rounded-lg transition-colors"
            >
              <X className="w-3.5 h-3.5" /> Discard
            </button>
          </div>
        </div>
      )}

      {/* Input */}
      <div className="px-3 py-3 border-t border-slate-700/50 shrink-0">
        {visualContext && (
          <div className="flex items-center justify-between bg-indigo-950/70 border border-indigo-500/40 rounded-lg px-2.5 py-1.5 mb-2 text-xs text-indigo-300">
            <div className="flex items-center gap-1.5 min-w-0">
              <span className="font-mono bg-indigo-600/40 text-indigo-200 px-1.5 py-0.5 rounded text-[11px] font-semibold shrink-0">
                &lt;{visualContext.tagName || 'element'}&gt;
              </span>
              <span className="truncate text-slate-300 text-[11px]">
                {visualContext.text ? `"${visualContext.text.substring(0, 32)}${visualContext.text.length > 32 ? '...' : ''}"` : (visualContext.component || visualContext.file || 'Selected')}
              </span>
            </div>
            {onClearVisualContext && (
              <button
                onClick={onClearVisualContext}
                className="text-slate-400 hover:text-white ml-2 text-xs font-bold shrink-0"
                title="Ukloni vizuelni element"
              >
                ✕
              </button>
            )}
          </div>
        )}
        <div className="flex gap-2">
          <textarea
            id="ai-prompt-input"
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={e => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                handleSend();
              }
            }}
            placeholder={aiScope === 'file' ? (activeFile ? `Ask AI to modify ${activeFile.name || 'this file'}...` : 'Open a file first...') : 'Explain changes to make across the project...'}
            rows={3}
            disabled={isThinking}
            className="flex-1 bg-slate-800 border border-slate-600 rounded-lg px-3 py-2 text-sm text-slate-200 placeholder-slate-500 resize-none focus:outline-none focus:border-indigo-500 disabled:opacity-50"
          />
          <button
            onClick={handleSend}
            disabled={isThinking || !input.trim()}
            className="self-end p-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-40 text-white rounded-lg transition-colors"
          >
            {isThinking ? <Loader2 className="w-4 h-4 animate-spin" /> : <Send className="w-4 h-4" />}
          </button>
        </div>
        <p className="text-xs text-slate-600 mt-1">Enter to send · Shift+Enter for new line</p>
      </div>
    </div>
  );
}

// ==================== MAIN EDITOR PAGE ====================

export default function EditorPage() {
  const { projectId } = useParams();
  const navigate = useNavigate();

  const [project, setProject] = useState(null);
  const [files, setFiles] = useState([]);
  const [treeNodes, setTreeNodes] = useState([]);
  const [projectDependencies, setProjectDependencies] = useState({});
  const [integrationsExpanded, setIntegrationsExpanded] = useState(false);
  const [isPrismaGenerating, setIsPrismaGenerating] = useState(false);
  const [prismaResult, setPrismaResult] = useState(null);
  const [openTabs, setOpenTabs] = useState([]);     // [{path, name, content, isDirty}]
  const [activeTabPath, setActiveTabPath] = useState(null);
  const [isSaving, setIsSaving] = useState(false);
  const [isCommitting, setIsCommitting] = useState(false);
  const [pendingContent, setPendingContent] = useState(null); // Lifted state for DiffEditor
  const [isThinking, setIsThinking] = useState(false);
  const [showPreview, setShowPreview] = useState(false);
  const [symbolsExpanded, setSymbolsExpanded] = useState(false);
  const [functions, setFunctions] = useState([]);
  const [previewUrl, setPreviewUrl] = useState('http://localhost:5173');
  // Dev Server State
  const [serverRunning, setServerRunning] = useState(false);
  const [serverLogs, setServerLogs] = useState([]);
  const [cdpPort, setCdpPort] = useState(null);
  const [cdpConnected, setCdpConnected] = useState(false);
  const [showTerminal, setShowTerminal] = useState(false);
  const [compileError, setCompileError] = useState(null);
  const [healingInProgress, setHealingInProgress] = useState(false);
  const [healStatus, setHealStatus] = useState('');
  const [missingPackage, setMissingPackage] = useState(null);
  const [installingPackage, setInstallingPackage] = useState(false);
  const [installStatus, setInstallStatus] = useState('');
  
  // Status & Inspector States
  const [statusMsg, setStatusMsg] = useState('');
  const [selectedElement, setSelectedElement] = useState(null);
  const [isInspectMode, setIsInspectMode] = useState(false);
  const [isEditingTitle, setIsEditingTitle] = useState(false);
  const [titleInput, setTitleInput] = useState('');

  const handleTitleSubmit = async () => {
    if (titleInput.trim() && project) {
      const updated = { ...project, name: titleInput.trim() };
      await window.electronAPI.project.save(updated);
      setProject(updated);
    }
    setIsEditingTitle(false);
  };

  const syncInspectMode = useCallback((enabled) => {
    if (webviewRef.current && webviewRef.current.contentWindow) {
      try {
        webviewRef.current.contentWindow.postMessage({
          channel: 'axiom-toggle-inspect',
          enabled: enabled
        }, '*');
      } catch (e) {
        console.warn('Failed to postMessage to iframe:', e);
      }
    }
  }, []);

  useEffect(() => {
    syncInspectMode(isInspectMode);
  }, [isInspectMode, syncInspectMode]);

  // Project-level AI states
  const [aiScope, setAiScope] = useState('file'); // 'file' or 'project'
  const [pendingProjectChanges, setPendingProjectChanges] = useState(null); // { [filePath]: newContent }
  const [aiProgress, setAiProgress] = useState(null); // { status, percent }

  // Refs
  const editorRef = useRef(null);
  const webviewRef = useRef(null);
  const terminalEndRef = useRef(null);

  // Derived State
  const activeTab = openTabs.find(t => t.path === activeTabPath) || null;
  const dirtyCount = openTabs.filter(t => t.isDirty).length;

  // Open a file into tabs
  const openFile = useCallback(async (node) => {
    // If already open, just switch
    if (openTabs.find(t => t.path === node.path)) {
      setActiveTabPath(node.path);
      return;
    }
    const res = await window.electronAPI.project.readFile(projectId, node.path);
    const content = res.success ? res.content : `// Error loading file: ${res.error}`;
    const tab = { path: node.path, name: node.name, content, isDirty: false };
    setOpenTabs(tabs => [...tabs, tab]);
    setActiveTabPath(node.path);
  }, [openTabs, projectId]);

  // Auto-scroll terminal
  useEffect(() => {
    if (showTerminal) {
      terminalEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [serverLogs, showTerminal]);

  // Auto-hide preview mode when pendingContent is active to show the DiffEditor
  useEffect(() => {
    if (pendingContent) {
      setShowPreview(false);
    }
  }, [pendingContent]);

  // Server IPC listeners
  useEffect(() => {
    setMissingPackage(null);
    setInstallingPackage(false);
    setInstallStatus('');

    if (!window.electronAPI.server) return;

    // Check initial status
    window.electronAPI.server.getStatus(projectId).then(res => {
      setServerRunning(res.isRunning);
    });

    const unsubStatus = window.electronAPI.server.onStatus(({ status }) => {
      setServerRunning(status === 'running');
    });

    const unsubLog = window.electronAPI.server.onLog(({ text, type }) => {
      setServerLogs(logs => {
        const newLogs = [...logs, { text, type, id: Date.now() + Math.random() }];
        return newLogs.slice(-200); // Keep last 200 logs
      });

      // Auto-detect preview URL from server output (e.g. Next.js or Vite logs)
      if (text) {
        const match = text.match(/https?:\/\/(localhost|127\.0\.0\.1|0\.0\.0\.0|192\.\d+\.\d+\.\d+):(\d+)/i);
        if (match) {
          let detectedUrl = match[0];
          // Map 0.0.0.0 to localhost so browser frame can load it correctly
          if (detectedUrl.includes('0.0.0.0')) {
            detectedUrl = detectedUrl.replace('0.0.0.0', 'localhost');
          }
          console.log('[Editor] Auto-detected local server URL from logs:', detectedUrl);
          setPreviewUrl(detectedUrl);
        }
      }
    });

    const unsubCompileError = window.electronAPI.server.onCompileError((errorData) => {
      console.log('[Editor] Received compilation error:', errorData);
      setCompileError(errorData);
      setShowTerminal(true);
    });

    const unsubHealStatus = window.electronAPI.server.onHealStatus((statusData) => {
      console.log('[Editor] Healing progress:', statusData);
      setHealStatus(statusData.status);
    });

    const unsubMissingPackage = window.electronAPI.server.onMissingPackage
      ? window.electronAPI.server.onMissingPackage((packageData) => {
          console.log('[Editor] Received missing package event:', packageData);
          if (packageData.projectId === projectId) {
            setMissingPackage(packageData.packageName);
            setShowTerminal(true);
          }
        })
      : null;

    return () => {
      unsubStatus();
      unsubLog();
      unsubCompileError();
      unsubHealStatus();
      if (unsubMissingPackage) unsubMissingPackage();
    };
  }, [projectId]);

  // AI Progress IPC listener
  useEffect(() => {
    if (!window.electronAPI.editor || !window.electronAPI.editor.onProjectEditProgress) return;
    
    const unsubProgress = window.electronAPI.editor.onProjectEditProgress((progress) => {
      setAiProgress(progress);
    });
    
    return () => {
      if (unsubProgress) unsubProgress();
    };
  }, []);

  const toggleServer = async () => {
    if (serverRunning) {
      await window.electronAPI.server.stop(projectId);
      setCdpPort(null);
      setCdpConnected(false);
    } else {
      setServerLogs([{ text: '> Starting server...', type: 'info', id: Date.now() }]);
      setShowTerminal(true);
      const res = await window.electronAPI.server.start({ 
        projectId, 
        platform: project?.manifest?.platform || 'web',
        techStack: project?.manifest?.techStack?.framework || 'nextjs'
      });
      if (res.success && res.cdpPort) {
        setCdpPort(res.cdpPort);
      }
    }
  };

  const handleHealError = async () => {
    if (!compileError) return;
    setHealingInProgress(true);
    setHealStatus('Započinjem popravku...');
    try {
      const res = await window.electronAPI.server.healCompileError(
        projectId,
        compileError.filePath,
        compileError.errorMessage
      );
      if (res) {
        setCompileError(null);
      }
    } catch (err) {
      console.error("Auto-healing error:", err);
      setHealStatus('Greška pri popravci: ' + err.toString());
    } finally {
      setHealingInProgress(false);
    }
  };

  const handleInstallPackage = async () => {
    if (!missingPackage) return;
    if (!window.electronAPI.server.installPackage) {
      console.error("installPackage API is not defined.");
      setInstallStatus('Greška: API nije učitan. Osvežite stranu (Ctrl+R/F5).');
      return;
    }
    setInstallingPackage(true);
    setInstallStatus('Instaliram paket...');
    try {
      const success = await window.electronAPI.server.installPackage(
        projectId,
        missingPackage
      );
      if (success) {
        setInstallStatus('Paket uspešno instaliran! Ponovo pokrećem server...');
        setMissingPackage(null);
        
        if (serverRunning) {
          await window.electronAPI.server.stop(projectId);
          await new Promise(resolve => setTimeout(resolve, 1000));
          setServerLogs([{ text: '> Starting server...', type: 'info', id: Date.now() }]);
          const res = await window.electronAPI.server.start({ 
            projectId, 
            platform: project?.manifest?.platform || 'web',
            techStack: project?.manifest?.techStack?.framework || 'nextjs'
          });
          if (res.success && res.cdpPort) {
            setCdpPort(res.cdpPort);
          }
        }
      } else {
        setInstallStatus('Greška pri instalaciji: npm install nije uspeo.');
      }
    } catch (err) {
      console.error("Greška pri instalaciji paketa:", err);
      setInstallStatus('Greška pri instalaciji: ' + err.toString());
    } finally {
      setInstallingPackage(false);
    }
  };

  // CDP WebSocket Connection (For Desktop WYSIWYG)
  useEffect(() => {
    if (!cdpPort) return;
    
    let ws = null;
    let pollInterval = null;
    let attempts = 0;
    
    const connectToCDP = async () => {
       const res = await window.electronAPI.server.getCdpWsUrl(cdpPort);
       if (res.success && res.url) {
         clearInterval(pollInterval);
         console.log('[CDP] Connecting to:', res.url);
         
         ws = new WebSocket(res.url);
         ws.onopen = () => {
           console.log('[CDP] Connected');
           setCdpConnected(true);
           
           // Enable Runtime events to catch console.log
           ws.send(JSON.stringify({ id: 1, method: 'Runtime.enable' }));
           
           // Inject our Inspector script into the target window
           const inspectorScript = `
             if (!window.__axiom_inspect_injected) {
               window.__axiom_inspect_injected = true;
               
               const HIGHLIGHT_STYLE = \`
                 .axiom-inspect-highlight {
                   outline: 2px solid #6366f1 !important;
                   outline-offset: -2px !important;
                   cursor: crosshair !important;
                   background-color: rgba(99, 102, 241, 0.1) !important;
                   transition: all 0.1s ease;
                 }
               \`;
               
               const styleEl = document.createElement('style');
               styleEl.textContent = HIGHLIGHT_STYLE;
               document.head.appendChild(styleEl);

               let highlightedElement = null;

               function findAxiomTarget(el) {
                 while (el && el !== document.body) {
                   if (el.dataset && (el.dataset.axiomFile || el.dataset.axiomComponent)) {
                     return el;
                   }
                   el = el.parentElement;
                 }
                 return null;
               }

               document.addEventListener('mouseover', (e) => {
                 const target = findAxiomTarget(e.target);
                 if (highlightedElement && highlightedElement !== target) {
                   highlightedElement.classList.remove('axiom-inspect-highlight');
                 }
                 if (target) {
                   target.classList.add('axiom-inspect-highlight');
                   highlightedElement = target;
                 }
               }, true);

               document.addEventListener('mouseout', (e) => {
                 if (highlightedElement) {
                   highlightedElement.classList.remove('axiom-inspect-highlight');
                   highlightedElement = null;
                 }
               }, true);

               document.addEventListener('click', (e) => {
                 const target = findAxiomTarget(e.target);
                 if (target) {
                   e.preventDefault();
                   e.stopPropagation();
                   const payload = {
                     file: target.dataset.axiomFile || null,
                     component: target.dataset.axiomComponent || null,
                     tagName: target.tagName.toLowerCase(),
                     text: target.innerText?.substring(0, 50) || ''
                   };
                   console.log('AXIOM_INSPECT::' + JSON.stringify(payload));
                 }
               }, true);
               
               console.log("Axiom Inspector injected via CDP.");
             }
           `;
           
           ws.send(JSON.stringify({
             id: 2,
             method: 'Runtime.evaluate',
             params: { expression: inspectorScript }
           }));
         };
         
         ws.onmessage = (msg) => {
           const data = JSON.parse(msg.data);
           if (data.method === 'Runtime.consoleAPICalled') {
              const args = data.params.args;
              if (args && args.length > 0 && args[0].value && typeof args[0].value === 'string' && args[0].value.startsWith('AXIOM_INSPECT::')) {
                 const jsonStr = args[0].value.substring(15);
                 try {
                   const payload = JSON.parse(jsonStr);
                   console.log('[Editor] CDP Inspect click received:', payload);
                   setSelectedElement(payload);
                   
                   if (payload.file) {
                     const fileNode = { path: payload.file, name: payload.file.split('/').pop() };
                     openFile(fileNode);
                     
                     setTimeout(() => {
                       const promptInput = document.getElementById('ai-prompt-input');
                       if (promptInput) promptInput.focus();
                     }, 100);
                   }
                 } catch (e) {
                   console.warn("Failed to parse AXIOM_INSPECT payload:", e);
                 }
              }
           }
         };
         
         ws.onclose = () => setCdpConnected(false);
       } else {
         attempts++;
         if (attempts > 15) { // 15 seconds timeout
           clearInterval(pollInterval);
           console.warn("[CDP] Failed to connect after 15 attempts.");
         }
       }
    };
    
    pollInterval = setInterval(connectToCDP, 1000);
    connectToCDP(); // Try immediately
    
    return () => {
      clearInterval(pollInterval);
      if (ws) ws.close();
    };
  }, [cdpPort, openFile]);

  // Parse functions from content
  useEffect(() => {
    if (!activeTab?.content) {
      setFunctions([]);
      return;
    }
    const content = activeTab.content;
    const found = [];
    const functionRegex = /(?:function\s+([a-zA-Z0-9_]+)|(?:const|let|var)\s+([a-zA-Z0-9_]+)\s*=\s*(?:async\s*)?\([^)]*\)\s*=>)/g;
    let match;
    while ((match = functionRegex.exec(content)) !== null) {
      const name = match[1] || match[2];
      if (name) {
        const line = content.substring(0, match.index).split('\n').length;
        found.push({ name, line });
      }
    }
    setFunctions(found);
  }, [activeTab?.content]);

  const jumpToLine = (line) => {
    if (editorRef.current) {
      editorRef.current.revealLineInCenter(line);
      editorRef.current.setPosition({ lineNumber: line, column: 1 });
      editorRef.current.focus();
    }
  };



  // Setup Webview/Iframe message listener for visual inspector clicks
  useEffect(() => {
    const handleWindowMessage = (event) => {
      const data = event.data;
      if (data && data.channel === 'axiom-inspect-click') {
        const payload = data.args ? data.args[0] : data.payload;
        console.log('[Editor] Inspect click received:', payload);
        setSelectedElement(payload);
        
        // Open the file in editor if it's not open
        if (payload.file) {
          const fileNode = { path: payload.file, name: payload.file.split('/').pop() };
          openFile(fileNode);
        } else if (payload.path && files.length > 0) {
          // Fallback: If no explicit file attribute exists, resolve by current URL path
          const cleanRoute = payload.path.replace(/^\/(en|sr|es|pt|fr|de|it)/, '').replace(/\/$/, '') || '';
          const candidateSuffixes = [
            cleanRoute ? `${cleanRoute}/page.tsx` : 'page.tsx',
            cleanRoute ? `${cleanRoute}/page.jsx` : 'page.jsx',
            cleanRoute ? `${cleanRoute}.tsx` : 'index.tsx',
            cleanRoute ? `${cleanRoute}.jsx` : 'index.jsx',
          ];
          const matchedFile = files.find(f => {
            const norm = f.path.replace(/\\/g, '/');
            return candidateSuffixes.some(s => norm.endsWith(s));
          });
          if (matchedFile) {
            openFile(matchedFile);
          }
        }

        // Focus prompt input and set visual placeholder
        setTimeout(() => {
          const promptInput = document.getElementById('ai-prompt-input');
          if (promptInput) {
            promptInput.focus();
            if (payload.text) {
              promptInput.placeholder = `Opiši izmenu za <${payload.tagName || 'element'}> "${payload.text.substring(0, 25)}..."`;
            }
          }
        }, 100);
      }
    };

    window.addEventListener('message', handleWindowMessage);
    return () => {
      window.removeEventListener('message', handleWindowMessage);
    };
  }, [openFile, files]);

  console.log("[EditorPage] Render. pendingContent exists?", !!pendingContent);
  if (pendingContent) {
    console.log("[EditorPage] pendingContent length:", pendingContent.length);
  }



  // Load project + files
  useEffect(() => {
    const load = async () => {
      const res = await window.electronAPI.project.getAll();
      const proj = res.projects?.find(p => p.id === projectId);
      setProject(proj || null);

      const fileRes = await window.electronAPI.project.getFiles(projectId);
      if (fileRes.success) {
        setFiles(fileRes.files);
        setTreeNodes(buildTree(fileRes.files));
        
        // Try to read package.json to detect integrations
        try {
          const pkgRes = await window.electronAPI.project.readFile(projectId, 'package.json');
          if (pkgRes.success && pkgRes.content) {
            const pkg = JSON.parse(pkgRes.content);
            const deps = { ...(pkg.dependencies || {}), ...(pkg.devDependencies || {}) };
            setProjectDependencies(deps);
          } else {
            setProjectDependencies({});
          }
        } catch (e) {
          console.error("Failed to read package.json", e);
          setProjectDependencies({});
        }
      }
    };
    load();
  }, [projectId]);



  // Close a tab
  const closeTab = (tabPath, e) => {
    e?.stopPropagation();
    const tab = openTabs.find(t => t.path === tabPath);
    if (tab?.isDirty && !confirm('Unsaved changes. Close anyway?')) return;
    const remaining = openTabs.filter(t => t.path !== tabPath);
    setOpenTabs(remaining);
    if (activeTabPath === tabPath) {
      setActiveTabPath(remaining.at(-1)?.path || null);
      setPendingContent(null);
    }
  };

  // Monaco editor change handler
  const handleEditorChange = (value) => {
    setOpenTabs(tabs => tabs.map(t =>
      t.path === activeTabPath ? { ...t, content: value, isDirty: true } : t
    ));
  };

  // Save active file (Ctrl+S)
  const saveActiveFile = useCallback(async () => {
    if (!activeTab) return;
    setIsSaving(true);
    const result = await window.electronAPI.editor.writeFile(projectId, activeTab.path, activeTab.content);
    if (result.success) {
      setOpenTabs(tabs => tabs.map(t => t.path === activeTabPath ? { ...t, isDirty: false } : t));
      setStatusMsg(`Saved: ${activeTab.name}`);
      setTimeout(() => setStatusMsg(''), 2000);
      
      // If package.json was saved, reload dependencies
      if (activeTab.path === 'package.json') {
        try {
          const pkg = JSON.parse(activeTab.content);
          const deps = { ...(pkg.dependencies || {}), ...(pkg.devDependencies || {}) };
          setProjectDependencies(deps);
        } catch (e) {
          console.error("Failed to parse package.json dependencies on save", e);
        }
      }
    } else {
      setStatusMsg(`Error saving: ${result.error}`);
    }
    setIsSaving(false);
  }, [activeTab, activeTabPath, projectId]);

  // Keyboard shortcut Ctrl+S
  useEffect(() => {
    const handler = (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        saveActiveFile();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [saveActiveFile]);

  // Apply AI changes to active tab
  const handleAIApply = useCallback(async (newContent, commitMsg) => {
    if (!activeTab) return;

    // 1. Update the React state with the new content (saved on disk, so isDirty is false)
    setOpenTabs(tabs => tabs.map(t =>
      t.path === activeTabPath ? { ...t, content: newContent, isDirty: false } : t
    ));

    // 2. Unmount the DiffEditor immediately
    setPendingContent(null);

    // 3. Write to disk & Git commit
    await window.electronAPI.editor.writeFile(projectId, activeTab.path, newContent);
    await window.electronAPI.editor.gitCommit(projectId, commitMsg || `AI: edited ${activeTab.path}`);
    setStatusMsg(`AI changes applied & committed`);
    setTimeout(() => setStatusMsg(''), 3000);
  }, [activeTab, activeTabPath, projectId, setOpenTabs]);

  // Apply a single file's changes in project-level AI view
  const handleApplyProjectFile = useCallback(async (filePath, newContent) => {
    // 1. Update tab state if open
    setOpenTabs(tabs => tabs.map(t =>
      t.path === filePath ? { ...t, content: newContent, isDirty: false } : t
    ));

    // 2. Write to disk & Git commit
    await window.electronAPI.editor.writeFile(projectId, filePath, newContent);
    await window.electronAPI.editor.gitCommit(projectId, `AI: applied changes to ${filePath}`);

    // 3. Remove file from pendingProjectChanges
    setPendingProjectChanges(prev => {
      if (!prev) return null;
      const copy = { ...prev };
      delete copy[filePath];
      if (Object.keys(copy).length === 0) {
        return null;
      }
      return copy;
    });

    // 4. If current active tab is the applied file, exit DiffEditor
    if (activeTabPath === filePath) {
      setPendingContent(null);
    }

    setStatusMsg(`Applied changes to ${filePath}`);
    setTimeout(() => setStatusMsg(''), 3000);
  }, [activeTabPath, projectId, setOpenTabs]);

  // Discard a single file's changes in project-level AI view
  const handleDiscardProjectFile = useCallback((filePath) => {
    // 1. Remove file from pendingProjectChanges
    setPendingProjectChanges(prev => {
      if (!prev) return null;
      const copy = { ...prev };
      delete copy[filePath];
      if (Object.keys(copy).length === 0) {
        return null;
      }
      return copy;
    });

    // 2. If current active tab is the discarded file, exit DiffEditor
    if (activeTabPath === filePath) {
      setPendingContent(null);
    }

    setStatusMsg(`Discarded changes to ${filePath}`);
    setTimeout(() => setStatusMsg(''), 3000);
  }, [activeTabPath]);

  // Apply all changes in project-level AI view
  const handleApplyAllProjectChanges = useCallback(async (changes) => {
    if (!changes) return;

    // 1. Update open tabs
    setOpenTabs(tabs => tabs.map(t => {
      if (changes[t.path] !== undefined) {
        return { ...t, content: changes[t.path], isDirty: false };
      }
      return t;
    }));

    // 2. Write all changes to disk
    for (const [filePath, content] of Object.entries(changes)) {
      await window.electronAPI.editor.writeFile(projectId, filePath, content);
    }

    // 3. Perform a single bulk git commit
    const fileList = Object.keys(changes).join(', ');
    await window.electronAPI.editor.gitCommit(projectId, `AI: applied changes to ${fileList}`);

    // 4. Clear pending states
    setPendingProjectChanges(null);
    setPendingContent(null);

    setStatusMsg(`Successfully applied changes to all files`);
    setTimeout(() => setStatusMsg(''), 3000);
  }, [projectId, setOpenTabs]);

  // Discard all changes in project-level AI view
  const handleDiscardAllProjectChanges = useCallback(() => {
    setPendingProjectChanges(null);
    setPendingContent(null);
    setStatusMsg(`Discarded all changes`);
    setTimeout(() => setStatusMsg(''), 3000);
  }, []);

  // Review a specific file from the project changes list
  const handleReviewProjectFile = useCallback(async (filePath) => {
    // 1. Open file to active tab
    const node = { path: filePath, name: filePath.split('/').pop() };
    await openFile(node);
    
    // 2. Load the pending content for this file to show in DiffEditor
    if (pendingProjectChanges && pendingProjectChanges[filePath] !== undefined) {
      setPendingContent(pendingProjectChanges[filePath]);
    }
  }, [openFile, pendingProjectChanges]);

  // Manual git commit + push
  const handleCommit = async () => {
    // Save all dirty tabs first
    for (const tab of openTabs.filter(t => t.isDirty)) {
      await window.electronAPI.editor.writeFile(projectId, tab.path, tab.content);
    }
    setOpenTabs(tabs => tabs.map(t => ({ ...t, isDirty: false })));

    setIsCommitting(true);
    const msg = `Axiom Editor — ${new Date().toLocaleString()}`;
    await window.electronAPI.editor.gitCommit(projectId, msg);
    const pushResult = await window.electronAPI.project.pushToGitHub(projectId);
    setIsCommitting(false);
    setStatusMsg(pushResult.success ? '✅ Committed & pushed to GitHub' : `Push failed: ${pushResult.error}`);
    setTimeout(() => setStatusMsg(''), 4000);
  };



  return (
    <div className="flex flex-col h-screen bg-slate-950 text-slate-200 overflow-hidden">
      {/* ── TOP BAR ── */}
      <div className="flex items-center gap-3 px-4 py-2 bg-slate-900 border-b border-slate-700/50 shrink-0">
        <button
          onClick={() => navigate(`/projects/${projectId}`)}
          className="flex items-center gap-1.5 text-slate-400 hover:text-white transition-colors text-sm"
        >
          <ArrowLeft className="w-4 h-4" />
          Back
        </button>

        <div className="w-px h-4 bg-slate-700" />

        <div className="flex items-center gap-2">
          <Code2 className="w-4 h-4 text-indigo-400 shrink-0" />
          {isEditingTitle ? (
            <input
              type="text"
              value={titleInput}
              onChange={(e) => setTitleInput(e.target.value)}
              onBlur={handleTitleSubmit}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleTitleSubmit();
                if (e.key === 'Escape') setIsEditingTitle(false);
              }}
              autoFocus
              className="text-sm font-semibold text-white bg-slate-800 border border-indigo-500 rounded px-2 py-0.5 outline-none shadow-inner"
            />
          ) : (
            <span 
              onClick={() => {
                setTitleInput(project?.name || '');
                setIsEditingTitle(true);
              }}
              className="text-sm font-semibold text-white hover:text-indigo-300 cursor-pointer flex items-center gap-1.5 group select-none transition-colors"
              title="Klikni da preimenuješ projekat"
            >
              {project?.name || projectId}
              <span className="text-[11px] text-slate-500 opacity-0 group-hover:opacity-100 transition-opacity">✏️</span>
            </span>
          )}
          {dirtyCount > 0 && (
            <span className="text-xs bg-amber-500/20 text-amber-400 border border-amber-500/30 px-1.5 py-0.5 rounded">
              {dirtyCount} unsaved
            </span>
          )}
        </div>

        <div className="mx-6 h-8 w-px bg-slate-700/50" />

        {/* View Toggles */}
        <div className="flex items-center gap-1 bg-slate-800/50 p-1 rounded-lg border border-slate-700/50">
          <button 
            onClick={() => setShowPreview(false)}
            className={`px-3 py-1.5 rounded-md text-[10px] uppercase font-bold flex items-center gap-2 transition-all ${!showPreview ? 'bg-indigo-600 text-white shadow-lg shadow-indigo-500/20' : 'text-slate-500 hover:text-slate-300'}`}
          >
            <Code2 className="w-3.5 h-3.5" /> Code
          </button>
          <button 
            onClick={() => setShowPreview(true)}
            className={`px-3 py-1.5 rounded-md text-[10px] uppercase font-bold flex items-center gap-2 transition-all ${showPreview ? 'bg-indigo-600 text-white shadow-lg shadow-indigo-500/20' : 'text-slate-500 hover:text-slate-300'}`}
          >
            <Monitor className="w-3.5 h-3.5" /> Preview
          </button>
        </div>

        <div className="mx-2 h-6 w-px bg-slate-700/50" />

        <button
          onClick={toggleServer}
          className={`flex items-center gap-1.5 px-3 py-1.5 text-xs font-bold rounded-lg transition-colors border ${
            serverRunning 
              ? 'bg-red-500/10 text-red-400 border-red-500/20 hover:bg-red-500/20' 
              : 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20 hover:bg-emerald-500/20'
          }`}
        >
          {serverRunning ? (
            <><div className="w-2 h-2 rounded-sm bg-red-400" /> Stop Server</>
          ) : (
            <><div className="w-0 h-0 border-t-[5px] border-t-transparent border-l-[8px] border-l-emerald-400 border-b-[5px] border-b-transparent ml-0.5" /> Start Server</>
          )}
        </button>
        
        <button
          onClick={() => setShowTerminal(t => !t)}
          className={`px-2 py-1.5 rounded-lg text-slate-400 hover:text-white transition-colors ${showTerminal ? 'bg-slate-700/50 text-white' : ''}`}
          title="Toggle Terminal"
        >
          <Terminal className="w-4 h-4" />
        </button>

        <div className="ml-auto flex items-center gap-2">
          {statusMsg && (
            <span className="text-xs text-slate-400 italic">{statusMsg}</span>
          )}

          <button
            onClick={saveActiveFile}
            disabled={isSaving || !activeTab?.isDirty}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-slate-700 hover:bg-slate-600 disabled:opacity-40 text-slate-200 rounded-lg transition-colors"
          >
            {isSaving ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Save className="w-3.5 h-3.5" />}
            Save
          </button>

          <button
            onClick={handleCommit}
            disabled={isCommitting}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-indigo-700 hover:bg-indigo-600 disabled:opacity-40 text-white rounded-lg transition-colors"
          >
            {isCommitting ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <GitCommit className="w-3.5 h-3.5" />}
            Commit & Push
          </button>
        </div>
      </div>

      {/* ── MAIN AREA ── */}
      <div className="flex flex-1 overflow-hidden">

        {/* SIDEBAR (Combined Explorer + Symbols Outline) */}
        <div className="w-56 shrink-0 bg-slate-900 border-r border-slate-700/50 flex flex-col overflow-hidden h-full">
          {/* FILE TREE */}
          <div className="flex-1 flex flex-col overflow-hidden">
            <div className="px-3 py-2 border-b border-slate-700/30 flex items-center gap-2 select-none shrink-0">
              <Search className="w-3.5 h-3.5 text-slate-500" />
              <span className="text-xs font-semibold text-slate-500 uppercase tracking-wider">Explorer</span>
            </div>
            <div className="flex-1 overflow-y-auto py-1 custom-scrollbar">
              {treeNodes.length === 0 ? (
                <div className="px-3 py-4 text-xs text-slate-600 text-center">No files found</div>
              ) : treeNodes.map(node => (
                <FileTreeNode
                  key={node.path || node.name}
                  node={node}
                  depth={0}
                  onSelect={openFile}
                  selectedPath={activeTabPath}
                />
              ))}
            </div>
          </div>

          {/* Symbols Panel */}
          <div className="flex-initial border-t border-slate-700/50 flex flex-col overflow-hidden max-h-[40%] bg-slate-900/50">
            <div
              onClick={() => setSymbolsExpanded(e => !e)}
              className="px-3 py-2 border-b border-slate-700/30 flex items-center gap-2 cursor-pointer select-none hover:bg-slate-850 transition-colors shrink-0"
            >
              <FunctionSquare className="w-3.5 h-3.5 text-slate-500" />
              <span className="text-xs font-semibold text-slate-500 uppercase tracking-wider flex-1">Symbols ({functions.length})</span>
              {symbolsExpanded ? <ChevronDown className="w-3 h-3 text-slate-500" /> : <ChevronRight className="w-3 h-3 text-slate-500" />}
            </div>
            {symbolsExpanded && (
              <div className="flex-1 overflow-y-auto p-2 space-y-0.5 custom-scrollbar bg-slate-950/25">
                {functions.length > 0 ? functions.map((fn, idx) => (
                  <button
                    key={idx}
                    onClick={() => jumpToLine(fn.line)}
                    className="w-full text-left px-2 py-1.5 rounded text-xs text-slate-400 hover:bg-indigo-600/20 hover:text-indigo-300 transition-colors flex items-center gap-2 group"
                  >
                    <div className="w-1 h-1 rounded-full bg-slate-600 group-hover:bg-indigo-500" />
                    <span className="truncate">{fn.name}</span>
                    <span className="ml-auto text-[10px] opacity-30">L{fn.line}</span>
                  </button>
                )) : (
                  <div className="p-4 text-center text-[10px] text-slate-600 italic">No functions detected</div>
                )}
              </div>
            )}
          </div>
        </div>

        {/* CENTER: TABS + MONACO */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Tabs */}
          <div className="flex items-end bg-slate-900 border-b border-slate-700/50 overflow-x-auto shrink-0">
            {openTabs.length === 0 && (
              <div className="px-4 py-2 text-xs text-slate-600 italic">Open a file from the explorer</div>
            )}
            {openTabs.map(tab => (
              <div
                key={tab.path}
                onClick={() => {
                  setActiveTabPath(tab.path);
                  setPendingContent(null); // Clear pending content when switching tabs
                }}
                className={`flex items-center gap-2 px-3 py-2 text-xs border-r border-slate-700/30 cursor-pointer shrink-0 transition-colors
                  ${activeTabPath === tab.path
                    ? 'bg-slate-950 text-white border-t-2 border-t-indigo-500'
                    : 'bg-slate-800 text-slate-400 hover:bg-slate-850 hover:text-slate-300'}`}
              >
                <span className={tab.isDirty ? 'text-amber-400' : ''}>
                  {tab.isDirty ? '●' : ''} {tab.name}
                </span>
                <button
                  onClick={(e) => closeTab(tab.path, e)}
                  className="w-3.5 h-3.5 rounded hover:bg-slate-600 flex items-center justify-center opacity-60 hover:opacity-100"
                >
                  <X className="w-2.5 h-2.5" />
                </button>
              </div>
            ))}
          </div>

          {/* Monaco Editor Wrapper */}
          <div className="flex-1 overflow-hidden relative flex flex-col">
            {showPreview ? (() => {
              const integrationsList = [];
              const usesResend = (projectDependencies['resend'] || projectDependencies['@resend/node'] || 
                                  files.some(f => f.path.toLowerCase().includes('resend')));
              if (usesResend) {
                integrationsList.push({
                  id: 'resend',
                  name: 'Resend (Email)',
                  desc: 'Detektovan je servis za slanje e-pošte. Potrebno je da kreirate nalog i preuzmete API ključ.',
                  signupUrl: 'https://resend.com/signup',
                  envVar: 'RESEND_API_KEY'
                });
              }

              const usesStripe = (projectDependencies['stripe'] || projectDependencies['@stripe/stripe-js'] || 
                                  files.some(f => f.path.toLowerCase().includes('stripe')));
              if (usesStripe) {
                integrationsList.push({
                  id: 'stripe',
                  name: 'Stripe (Naplata)',
                  desc: 'Detektovana je Stripe integracija za plaćanja. Potrebno je da postavite API ključ i webhook tajnu.',
                  signupUrl: 'https://dashboard.stripe.com/register',
                  envVar: 'STRIPE_API_KEY, STRIPE_WEBHOOK_SECRET'
                });
              }

              const usesSupabase = (projectDependencies['@supabase/supabase-js'] || 
                                    files.some(f => f.path.toLowerCase().includes('supabase')));
              if (usesSupabase) {
                integrationsList.push({
                  id: 'supabase',
                  name: 'Supabase (Baza & Auth)',
                  desc: 'Detektovana je Supabase integracija. Potrebno je da unesete URL projekta i anonimni ključ.',
                  signupUrl: 'https://supabase.com/dashboard/sign-in',
                  envVar: 'NEXT_PUBLIC_SUPABASE_URL, NEXT_PUBLIC_SUPABASE_ANON_KEY'
                });
              }

              const usesPrisma = (projectDependencies['@prisma/client'] || projectDependencies['prisma'] || 
                                  files.some(f => f.path.toLowerCase().includes('prisma/schema.prisma') || f.path.toLowerCase().includes('schema.prisma')));
              if (usesPrisma) {
                integrationsList.push({
                  id: 'prisma',
                  name: 'Prisma (Baza podataka)',
                  desc: 'Detektovano je upravljanje bazom podataka preko Prisma. Osigurajte da je baza sinhronizovana.',
                  signupUrl: null,
                  envVar: 'DATABASE_URL'
                });
              }

              const handlePrismaGenerate = async () => {
                setIsPrismaGenerating(true);
                setPrismaResult({ type: 'info', message: 'Generisanje Prisma klijenta je u toku...' });
                const res = await window.electronAPI.project.runPrismaGenerate(projectId);
                setIsPrismaGenerating(false);
                if (res.success) {
                  setPrismaResult({ type: 'success', message: 'Prisma klijent je uspešno generisan!' });
                } else {
                  setPrismaResult({ type: 'error', message: `Greška pri generisanju: ${res.error}` });
                }
              };

              return (
                <div className="flex-1 flex flex-col bg-white">
                  <div className="h-8 bg-slate-100 border-b border-slate-200 flex items-center px-4 gap-4 shrink-0">
                    <div className="flex gap-1.5">
                      <div className="w-2.5 h-2.5 rounded-full bg-red-400" />
                      <div className="w-2.5 h-2.5 rounded-full bg-yellow-400" />
                      <div className="w-2.5 h-2.5 rounded-full bg-green-400" />
                    </div>
                    <div className="flex-1 bg-white border border-slate-300 rounded px-2 py-0.5 text-[10px] text-slate-500 flex items-center gap-2">
                      <Info className="w-3 h-3" /> {previewUrl}
                    </div>
                    <div className="flex items-center bg-slate-200 p-0.5 rounded gap-1 text-[10px]">
                      <button
                        onClick={() => setIsInspectMode(false)}
                        className={`flex items-center gap-1 px-2 py-0.5 rounded transition-all ${
                          !isInspectMode
                            ? 'bg-white text-indigo-600 font-bold shadow-sm'
                            : 'text-slate-600 hover:text-slate-900'
                        }`}
                        title="Interakcija: Kliktanje na dugmad, menije, forme i linkove radi normalno u pregledaču"
                      >
                        <MousePointer className="w-3 h-3" /> Interakcija
                      </button>
                      <button
                        onClick={() => setIsInspectMode(true)}
                        className={`flex items-center gap-1 px-2 py-0.5 rounded transition-all ${
                          isInspectMode
                            ? 'bg-indigo-600 text-white font-bold shadow-sm'
                            : 'text-slate-600 hover:text-slate-900'
                        }`}
                        title="Live Edit mod: Klikni na bilo koji element u aplikaciji da ga izmeniš pomoću AI asistenta"
                      >
                        <Crosshair className="w-3 h-3" /> Live Edit
                      </button>
                    </div>
                  </div>
                  
                  {/* Premium Warning Box */}
                  {integrationsList.length > 0 && (
                    <div className="bg-amber-50 border-b border-amber-200 text-amber-950 px-4 py-2 shrink-0">
                      <div className="flex items-center justify-between text-xs font-semibold">
                        <div className="flex items-center gap-2">
                          <AlertCircle className="w-4 h-4 text-amber-600 shrink-0" />
                          <span>Detektovani servisi ({integrationsList.length}): {integrationsList.map(i => i.name).join(', ')}</span>
                        </div>
                        <div className="flex items-center gap-3">
                          <button 
                            onClick={() => setIntegrationsExpanded(!integrationsExpanded)}
                            className="text-[10px] text-amber-700 hover:text-amber-900 underline font-bold uppercase"
                          >
                            {integrationsExpanded ? 'Sakrij detalje' : 'Prikaži detalje'}
                          </button>
                          <button 
                            onClick={() => navigate(`/projects/${projectId}/config`)}
                            className="flex items-center gap-1 bg-amber-600 hover:bg-amber-700 text-white px-2 py-0.5 rounded text-[10px] font-bold shadow-sm transition-colors"
                          >
                            <Settings className="w-3 h-3" /> Podesi ENV
                          </button>
                        </div>
                      </div>
                      
                      {integrationsExpanded && (
                        <div className="mt-2.5 pt-2.5 border-t border-amber-200/60 flex flex-col gap-2.5 max-h-48 overflow-y-auto pr-1">
                          {integrationsList.map(item => (
                            <div key={item.id} className="bg-white/70 rounded p-2 text-slate-800 flex flex-col md:flex-row md:items-center justify-between gap-3 border border-amber-200/40">
                              <div className="flex-1">
                                <h4 className="font-bold text-xs text-slate-900 flex items-center gap-1.5">
                                  <span className="w-1.5 h-1.5 rounded-full bg-amber-500" />
                                  {item.name}
                                  {item.envVar && <code className="text-[10px] bg-slate-100 px-1 py-0.5 rounded text-indigo-700 font-mono ml-2">{item.envVar}</code>}
                                </h4>
                                <p className="text-[11px] text-slate-600 mt-0.5">{item.desc}</p>
                              </div>
                              
                              <div className="flex items-center gap-2 shrink-0 self-end md:self-center">
                                {item.signupUrl && (
                                  <button
                                    onClick={() => window.electronAPI.shell.openExternal(item.signupUrl)}
                                    className="text-[10px] font-bold text-indigo-600 hover:text-indigo-850 bg-indigo-50 border border-indigo-200 hover:bg-indigo-100 px-2 py-1 rounded transition-colors"
                                  >
                                    Registracija
                                  </button>
                                )}
                                
                                {item.id === 'prisma' && (
                                  <button
                                    onClick={handlePrismaGenerate}
                                    disabled={isPrismaGenerating}
                                    className="flex items-center gap-1.5 text-[10px] font-bold text-white bg-slate-850 hover:bg-slate-950 disabled:bg-slate-400 px-2.5 py-1.5 rounded shadow-sm transition-colors"
                                  >
                                    {isPrismaGenerating ? (
                                      <>
                                        <Loader2 className="w-3.5 h-3.5 animate-spin" />
                                        Generisanje...
                                      </>
                                    ) : (
                                      'Generiši Prisma Client'
                                    )}
                                  </button>
                                )}
                              </div>
                            </div>
                          ))}
                          
                          {prismaResult && (
                            <div className={`text-[10px] font-semibold px-2.5 py-1.5 rounded flex items-center justify-between gap-2 border ${
                              prismaResult.type === 'success' ? 'bg-emerald-50 text-emerald-800 border-emerald-200' :
                              prismaResult.type === 'error' ? 'bg-rose-50 text-rose-800 border-rose-200' :
                              'bg-blue-50 text-blue-855 border-blue-200'
                            }`}>
                              <span className="truncate">{prismaResult.message}</span>
                              <button onClick={() => setPrismaResult(null)} className="hover:text-slate-950 font-bold">Zatvori</button>
                            </div>
                          )}
                        </div>
                      )}
                    </div>
                  )}
                  
                  {project?.manifest?.platform === 'desktop' ? (
                    <div className="flex-1 flex flex-col items-center justify-center bg-slate-900 text-center px-8 border border-slate-800 m-4 rounded-xl shadow-inner">
                      <Monitor className="w-16 h-16 text-indigo-500/50 mb-6" />
                      <h3 className="text-xl font-bold text-white mb-2">Desktop WYSIWYG Active</h3>
                      <p className="text-slate-400 max-w-sm mb-6">
                        The desktop app is running in a separate native window. Axiom Forge is attached as a debugger.
                      </p>
                      <div className="flex items-center gap-3 bg-slate-800 px-4 py-2 rounded-lg border border-slate-700">
                        <div className={`w-3 h-3 rounded-full ${cdpConnected ? 'bg-green-500 shadow-[0_0_10px_rgba(34,197,94,0.5)]' : 'bg-amber-500 animate-pulse'}`} />
                        <span className="text-sm font-medium text-slate-300">
                          {cdpConnected ? 'CDP Debugger Connected' : 'Waiting for app to start...'}
                        </span>
                      </div>
                      <p className="text-xs text-slate-500 mt-4">
                        Hover over elements in the app window to inspect. Click to open the file and trigger the AI prompt here.
                      </p>
                    </div>
                  ) : (
                    <iframe
                      ref={webviewRef}
                      src={previewUrl}
                      className="flex-1 w-full border-none bg-white"
                      title="Project Preview"
                      sandbox="allow-same-origin allow-scripts allow-popups allow-forms"
                      onLoad={() => syncInspectMode(isInspectMode)}
                    />
                  )}
                </div>
              );
            })()
            : activeTab ? (
              <>
                {/* Main Editor */}
                <div className={`flex-1 ${pendingContent ? 'hidden' : 'block'}`}>
                  <Editor
                    key={activeTabPath}
                    height="100%"
                    language={getLanguage(activeTab.path)}
                    value={activeTab.content}
                    theme="vs-dark"
                    onChange={handleEditorChange}
                    onMount={(editor) => { editorRef.current = editor; }}
                    options={{
                      fontSize: 13,
                      fontFamily: '"Fira Code", "Cascadia Code", Consolas, monospace',
                      fontLigatures: true,
                      minimap: { enabled: true, scale: 1 },
                      lineNumbers: 'on',
                      wordWrap: 'on',
                      scrollBeyondLastLine: false,
                      smoothScrolling: true,
                      cursorBlinking: 'smooth',
                      cursorSmoothCaretAnimation: 'on',
                      renderLineHighlight: 'all',
                      bracketPairColorization: { enabled: true },
                      automaticLayout: true,
                      padding: { top: 12 },
                    }}
                  />
                </div>

                {/* Diff Review View */}
                {pendingContent && (
                  <div className="h-full flex flex-col absolute inset-0 z-10 bg-slate-900">
                    <div className="bg-indigo-900/40 border-b border-indigo-500/30 px-4 py-2 flex justify-between items-center shrink-0">
                      <span className="text-sm text-indigo-300 font-semibold flex items-center gap-2">
                        <Sparkles className="w-4 h-4" /> Reviewing AI Changes
                      </span>
                      <span className="text-xs text-slate-400">
                        Original (Left) ➔ Modified (Right)
                      </span>
                    </div>
                    <div className="flex-1 overflow-hidden">
                      <SafeDiffEditor
                        key={`diff-view-${activeTabPath}-${pendingContent ? 'active' : 'none'}`}
                        height="100%"
                        language={getLanguage(activeTab.path)}
                        original={activeTab.content}
                        modified={pendingContent}
                        theme="vs-dark"
                        options={{
                          fontSize: 13,
                          fontFamily: '"Fira Code", "Cascadia Code", Consolas, monospace',
                          minimap: { enabled: false },
                          renderSideBySide: true,
                          readOnly: false,
                          automaticLayout: true,
                          scrollBeyondLastLine: false,
                          originalEditable: false,
                          diffCodeLens: false
                        }}
                        onChange={(newValue) => {
                          if (pendingProjectChanges) {
                            setPendingProjectChanges(prev => ({
                              ...prev,
                              [activeTabPath]: newValue
                            }));
                          }
                          setPendingContent(newValue);
                        }}
                      />
                    </div>
                  </div>
                )}
              </>
            ) : (
              <div className="flex flex-col items-center justify-center h-full text-slate-600 gap-3">
                <Code2 className="w-12 h-12 opacity-20" />
                <p className="text-sm">Select a file from the explorer to edit</p>
                <p className="text-xs opacity-60">Ctrl+S to save · AI panel on the right to modify with prompts</p>
              </div>
            )}
          </div>

          {/* Terminal Console */}
          {showTerminal && (
            <div className="h-48 shrink-0 bg-[#1e1e1e] border-t border-slate-700/50 flex flex-col z-20">
              <div className="px-3 py-1.5 bg-slate-800/80 border-b border-slate-700/50 flex justify-between items-center shrink-0">
                <span className="text-[10px] uppercase font-bold text-slate-400 flex items-center gap-2">
                  <Terminal className="w-3.5 h-3.5" /> Dev Server Logs
                </span>
                <button onClick={() => setShowTerminal(false)} className="text-slate-400 hover:text-white">
                  <X className="w-3.5 h-3.5" />
                </button>
              </div>
              
              {compileError && (
                <div className="bg-red-950/90 border-b border-red-500/50 p-2.5 text-white flex flex-col gap-1.5 shrink-0 shadow-md">
                  <div className="flex justify-between items-center">
                    <span className="text-xs font-bold text-red-300 flex items-center gap-2">
                      ⚠️ Greška pri kompajliranju u: <code className="bg-red-900/60 px-1.5 py-0.5 rounded text-yellow-300 font-mono text-[10px]">{compileError.filePath}</code>
                    </span>
                    <div className="flex gap-2">
                      <button 
                        onClick={handleHealError} 
                        disabled={healingInProgress}
                        className="bg-emerald-600 hover:bg-emerald-500 text-white font-semibold px-3 py-1 rounded text-[10px] flex items-center gap-1.5 disabled:opacity-50 transition-colors max-w-xs truncate"
                      >
                        {healingInProgress ? (
                          <>⏳ {healStatus || 'Popravljam...'}</>
                        ) : (
                          <>✨ Popravi sa AI</>
                        )}
                      </button>
                      <button 
                        onClick={() => setCompileError(null)} 
                        className="text-slate-400 hover:text-white text-[10px] px-1"
                      >
                        Ignoriši
                      </button>
                    </div>
                  </div>
                  <pre className="text-[10px] bg-black/40 p-2 rounded font-mono overflow-auto max-h-24 whitespace-pre-wrap text-red-200 border border-red-900/30 leading-normal">
                    {compileError.errorMessage}
                  </pre>
                </div>
              )}

              {missingPackage && (
                <div className="bg-amber-950/95 border-b border-amber-500/50 p-2.5 text-white flex flex-col gap-1.5 shrink-0 shadow-md transition-all duration-300">
                  <div className="flex justify-between items-center">
                    <span className="text-xs font-bold text-amber-300 flex items-center gap-2">
                      <AlertCircle className="w-4 h-4 text-amber-400 shrink-0" /> Nedostaje paket: <code className="bg-amber-900/60 px-1.5 py-0.5 rounded text-yellow-300 font-mono text-[10px]">{missingPackage}</code>
                    </span>
                    <div className="flex gap-2">
                      <button 
                        onClick={handleInstallPackage} 
                        disabled={installingPackage}
                        className="bg-amber-600 hover:bg-amber-500 text-white font-semibold px-3 py-1 rounded text-[10px] flex items-center gap-1.5 disabled:opacity-50 transition-colors shadow-sm cursor-pointer"
                      >
                        {installingPackage ? (
                          <>⏳ {installStatus || 'Instaliram...'}</>
                        ) : (
                          <>⚡ Instaliraj paket</>
                        )}
                      </button>
                      <button 
                        onClick={() => {
                          setMissingPackage(null);
                          setInstallStatus('');
                        }} 
                        className="text-slate-400 hover:text-white text-[10px] px-1 cursor-pointer"
                      >
                        Ignoriši
                      </button>
                    </div>
                  </div>
                  {installStatus && (
                    <div className="text-[10px] text-amber-200/90 bg-amber-900/30 px-2 py-1 rounded border border-amber-800/30 font-mono">
                      {installStatus}
                    </div>
                  )}
                </div>
              )}

              <div className="flex-1 overflow-y-auto p-2 font-mono text-[11px] text-slate-300 custom-scrollbar leading-relaxed">
                {serverLogs.length === 0 ? (
                  <div className="text-slate-600 italic">No logs yet...</div>
                ) : (
                  serverLogs.map((log) => (
                    <div key={log.id} className={`${log.type === 'error' ? 'text-red-400' : 'text-slate-300'} whitespace-pre-wrap font-mono`}>
                      {log.text}
                    </div>
                  ))
                )}
                <div ref={terminalEndRef} />
              </div>
            </div>
          )}

          {/* Status Bar */}
          <div className="flex items-center gap-4 px-4 py-1 bg-indigo-700/20 border-t border-slate-700/30 text-xs text-slate-500 shrink-0">
            {activeTab && (
              <>
                <span>{getLanguage(activeTab.path)}</span>
                <span>·</span>
                <span>{activeTab.path}</span>
                {activeTab.isDirty && <span className="text-amber-400 ml-auto">● Unsaved changes</span>}
              </>
            )}
          </div>
        </div>

        {/* AI PANEL */}
        <div className="w-80 shrink-0">
          <AIPanel
            activeFile={activeTab}
            activeContent={activeTab?.content}
            projectId={projectId}
            pendingContent={pendingContent}
            setPendingContent={setPendingContent}
            visualContext={selectedElement}
            onClearVisualContext={() => setSelectedElement(null)}
            onApply={handleAIApply}
            
            // Project props
            aiScope={aiScope}
            setAiScope={setAiScope}
            pendingProjectChanges={pendingProjectChanges}
            setPendingProjectChanges={setPendingProjectChanges}
            aiProgress={aiProgress}
            setAiProgress={setAiProgress}
            onApplyProjectFile={handleApplyProjectFile}
            onDiscardProjectFile={handleDiscardProjectFile}
            onApplyAllProjectChanges={handleApplyAllProjectChanges}
            onDiscardAllProjectChanges={handleDiscardAllProjectChanges}
            onReviewProjectFile={handleReviewProjectFile}
          />
        </div>
      </div>
    </div>
  );
}

// A wrapper around DiffEditor to cleanly handle unmounting and prevent "TextModel got disposed before DiffEditorWidget model got reset" crash
const SafeDiffEditor = ({ original, modified, language, theme, options, onChange }) => {
  const diffEditorRef = useRef(null);

  useEffect(() => {
    return () => {
      if (diffEditorRef.current) {
        try {
          diffEditorRef.current.setModel(null);
        } catch (e) {
          console.warn("[SafeDiffEditor] Error setting model to null on unmount:", e);
        }
      }
    };
  }, []);

  return (
    <DiffEditor
      height="100%"
      language={language}
      original={original}
      modified={modified}
      theme={theme}
      options={options}
      onMount={(editor) => {
        diffEditorRef.current = editor;
        const modifiedEditor = editor.getModifiedEditor();
        modifiedEditor.onDidChangeModelContent(() => {
          if (onChange) {
            onChange(modifiedEditor.getValue());
          }
        });
      }}
    />
  );
};
