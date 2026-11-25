/**
 * Axiom Forge - Isolated Floating AI Copilot Widget
 *
 * Appears ONLY when a large Editor model (>= 22B / power+) with native Ollama
 * tool-calling support is active. Completely isolated from the small-model
 * AIPanel (editor_ai_edit / editor_ai_project_edit).
 *
 * All Agent edits live in an in-memory virtual overlay until the user reviews
 * the Diff and explicitly clicks Apply.
 */

import React, { useState, useEffect, useRef, useCallback } from 'react';
import {
  Bot, X, Send, Square, RotateCcw, Eye, Check, Trash2,
  ChevronDown, ChevronRight, Wrench, Brain, ShieldCheck,
  Maximize2, Minimize2, Sparkles, FileCode2, AlertCircle
} from 'lucide-react';

function ToolCallBadge({ tool, onOpenFile }) {
  const [expanded, setExpanded] = useState(false);
  const filePath = tool.arguments?.path || tool.arguments?.dir || null;

  return (
    <div className="rounded-md border border-slate-700/60 bg-slate-900/80 text-[11px] overflow-hidden">
      <div className="flex items-center gap-1.5 px-2 py-1">
        <button
          type="button"
          onClick={() => setExpanded(v => !v)}
          className="text-slate-400 hover:text-slate-200 flex items-center gap-1 flex-1 min-w-0 text-left"
        >
          {expanded ? <ChevronDown className="w-3 h-3 shrink-0" /> : <ChevronRight className="w-3 h-3 shrink-0" />}
          <Wrench className={`w-3 h-3 shrink-0 ${tool.status === 'running' ? 'text-amber-400 animate-pulse' : tool.isError ? 'text-rose-400' : 'text-emerald-400'}`} />
          <span className="font-mono text-indigo-300 shrink-0">{tool.name}</span>
          {filePath && (
            <span className="font-mono text-slate-400 truncate">{filePath}</span>
          )}
        </button>
        {filePath && tool.name !== 'list_files' && onOpenFile && (
          <button
            type="button"
            onClick={() => onOpenFile(filePath)}
            title="Open file in editor"
            className="text-slate-500 hover:text-indigo-400 shrink-0 px-1"
          >
            <FileCode2 className="w-3 h-3" />
          </button>
        )}
        <span className={`text-[10px] px-1.5 py-0.5 rounded shrink-0 ${
          tool.status === 'running'
            ? 'bg-amber-500/15 text-amber-300'
            : tool.isError
            ? 'bg-rose-500/15 text-rose-300'
            : 'bg-emerald-500/15 text-emerald-300'
        }`}>
          {tool.status === 'running' ? 'running' : tool.isError ? 'error' : 'ok'}
        </span>
      </div>
      {expanded && (
        <div className="px-2.5 py-1.5 border-t border-slate-800 space-y-1 bg-slate-950/70">
          {tool.arguments && Object.keys(tool.arguments).length > 0 && (
            <pre className="text-[10px] text-slate-400 font-mono whitespace-pre-wrap break-all">
              {JSON.stringify(tool.arguments, null, 2)}
            </pre>
          )}
          {tool.preview && (
            <pre className="text-[10px] text-slate-300 font-mono whitespace-pre-wrap break-words border-t border-slate-800/80 pt-1">
              {tool.preview}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

export default function CopilotWidget({
  projectId,
  activeFile,
  onReviewStagedFile,
  onApplyStagedFile,
  onApplyAllStagedFiles,
  onOpenFile,
}) {
  const [capability, setCapability] = useState(null);
  const [isOpen, setIsOpen] = useState(false);
  const [isExpanded, setIsExpanded] = useState(false);
  const [showHoverCard, setShowHoverCard] = useState(false);

  const [mode, setMode] = useState('ask'); // 'ask' | 'agent'
  const [numCtxOverride, setNumCtxOverride] = useState(null); // null = Auto
  const [input, setInput] = useState('');
  const [isRunning, setIsRunning] = useState(false);
  const [messages, setMessages] = useState([]);
  const [stagedChanges, setStagedChanges] = useState([]);
  const [applyingPath, setApplyingPath] = useState(null);

  const messagesEndRef = useRef(null);
  const isRunningRef = useRef(false);

  const refreshCapability = useCallback(async () => {
    if (!window.electronAPI?.copilot?.getCapability) return;
    try {
      const cap = await window.electronAPI.copilot.getCapability();
      setCapability(cap);
      if (!cap?.eligible) {
        setIsOpen(false);
      }
    } catch {
      setCapability(null);
    }
  }, []);

  const refreshStaged = useCallback(async () => {
    if (!projectId || !window.electronAPI?.copilot?.getStagedChanges) return;
    try {
      const list = await window.electronAPI.copilot.getStagedChanges(projectId);
      setStagedChanges(Array.isArray(list) ? list : []);
    } catch {}
  }, [projectId]);

  useEffect(() => {
    refreshCapability();
    const onModelChanged = () => refreshCapability();
    window.addEventListener('axiom:editor-model-changed', onModelChanged);
    return () => window.removeEventListener('axiom:editor-model-changed', onModelChanged);
  }, [refreshCapability]);

  useEffect(() => {
    if (capability?.eligible && projectId) {
      refreshStaged();
    }
  }, [capability?.eligible, projectId, refreshStaged]);

  // Subscribe to streaming Copilot events
  useEffect(() => {
    if (!capability?.eligible || !window.electronAPI?.copilot?.onEvent) return;
    const unsub = window.electronAPI.copilot.onEvent((evt) => {
      if (!evt || evt.projectId !== projectId) return;

      if (evt.type === 'staged_updated' && Array.isArray(evt.stagedChanges)) {
        setStagedChanges(evt.stagedChanges);
        return;
      }

      if (evt.type === 'token') {
        setMessages((prev) => {
          if (prev.length === 0) return prev;
          const copy = [...prev];
          const last = { ...copy[copy.length - 1] };
          if (last.role === 'assistant') {
            last.content = (last.content || '') + (evt.content || '');
            copy[copy.length - 1] = last;
          }
          return copy;
        });
        return;
      }

      if (evt.type === 'thinking') {
        setMessages((prev) => {
          if (prev.length === 0) return prev;
          const copy = [...prev];
          const last = { ...copy[copy.length - 1] };
          if (last.role === 'assistant') {
            last.thinking = (last.thinking || '') + (evt.content || '');
            copy[copy.length - 1] = last;
          }
          return copy;
        });
        return;
      }

      if (evt.type === 'tool_start') {
        setMessages((prev) => {
          if (prev.length === 0) return prev;
          const copy = [...prev];
          const last = { ...copy[copy.length - 1] };
          if (last.role === 'assistant') {
            const tools = [...(last.tools || [])];
            tools.push({
              id: `${evt.step}-${evt.name}-${tools.length}`,
              step: evt.step,
              name: evt.name,
              arguments: evt.arguments,
              status: 'running',
              preview: '',
              isError: false,
            });
            last.tools = tools;
            copy[copy.length - 1] = last;
          }
          return copy;
        });
        return;
      }

      if (evt.type === 'tool_result') {
        setMessages((prev) => {
          if (prev.length === 0) return prev;
          const copy = [...prev];
          const last = { ...copy[copy.length - 1] };
          if (last.role === 'assistant' && Array.isArray(last.tools)) {
            const tools = [...last.tools];
            for (let i = tools.length - 1; i >= 0; i--) {
              if (tools[i].name === evt.name && tools[i].status === 'running') {
                tools[i] = {
                  ...tools[i],
                  status: 'done',
                  preview: evt.preview || '',
                  isError: !!evt.isError,
                };
                break;
              }
            }
            last.tools = tools;
            copy[copy.length - 1] = last;
          }
          return copy;
        });
        return;
      }

      if (evt.type === 'done' && Array.isArray(evt.stagedChanges)) {
        setStagedChanges(evt.stagedChanges);
      }
    });
    return () => {
      if (typeof unsub === 'function') unsub();
    };
  }, [capability?.eligible, projectId]);

  useEffect(() => {
    if (isOpen) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, stagedChanges, isOpen]);

  const handleSend = async () => {
    const trimmed = input.trim();
    if (!trimmed || isRunningRef.current || !capability?.eligible) return;

    setInput('');
    setIsRunning(true);
    isRunningRef.current = true;

    setMessages((prev) => [
      ...prev,
      { role: 'user', content: trimmed, mode },
      { role: 'assistant', content: '', thinking: '', tools: [], streaming: true, mode },
    ]);

    try {
      const res = await window.electronAPI.copilot.sendMessage({
        projectId,
        modelId: capability.modelId,
        mode,
        prompt: trimmed,
        activeFile: activeFile?.path || null,
        numCtxOverride,
      });

      if (Array.isArray(res?.stagedChanges)) {
        setStagedChanges(res.stagedChanges);
      }

      setMessages((prev) => {
        if (prev.length === 0) return prev;
        const copy = [...prev];
        const last = { ...copy[copy.length - 1], streaming: false };
        if (res?.cancelled) {
          last.content = (last.content || '') + '\n\n_(Stopped by user)_';
        } else if (!res?.success && res?.error) {
          last.error = res.error;
        } else if (!last.content && res?.finalMessage) {
          last.content = res.finalMessage;
        }
        copy[copy.length - 1] = last;
        return copy;
      });
    } catch (err) {
      setMessages((prev) => {
        if (prev.length === 0) return prev;
        const copy = [...prev];
        copy[copy.length - 1] = {
          ...copy[copy.length - 1],
          streaming: false,
          error: err?.message || String(err),
        };
        return copy;
      });
    } finally {
      setIsRunning(false);
      isRunningRef.current = false;
    }
  };

  const handleCancel = async () => {
    await window.electronAPI.copilot.cancel(projectId);
  };

  const handleReset = async () => {
    if (isRunning) {
      await window.electronAPI.copilot.cancel(projectId);
    }
    await window.electronAPI.copilot.resetSession(projectId, false);
    setMessages([]);
    setStagedChanges([]);
  };

  const handleDiscardSingle = async (filePath) => {
    const updated = await window.electronAPI.copilot.discardStagedFile(projectId, filePath);
    setStagedChanges(Array.isArray(updated) ? updated : []);
  };

  const handleDiscardAll = async () => {
    await window.electronAPI.copilot.clearStagedChanges(projectId);
    setStagedChanges([]);
  };

  const handleApplySingle = async (stagedChange) => {
    if (!onApplyStagedFile) return;
    setApplyingPath(stagedChange.path);
    try {
      const ok = await onApplyStagedFile(stagedChange);
      if (ok !== false) {
        const updated = await window.electronAPI.copilot.discardStagedFile(projectId, stagedChange.path);
        setStagedChanges(Array.isArray(updated) ? updated : []);
      }
    } finally {
      setApplyingPath(null);
    }
  };

  const handleApplyAll = async () => {
    if (!onApplyAllStagedFiles || stagedChanges.length === 0) return;
    setApplyingPath('__all__');
    try {
      const appliedPaths = await onApplyAllStagedFiles(stagedChanges);
      if (Array.isArray(appliedPaths)) {
        for (const p of appliedPaths) {
          await window.electronAPI.copilot.discardStagedFile(projectId, p);
        }
        await refreshStaged();
      }
    } finally {
      setApplyingPath(null);
    }
  };

  // Strict gate: render NOTHING when model is not eligible (zero footprint for small models)
  if (!capability?.eligible) {
    return null;
  }

  const ctxLabel = `${Math.round((numCtxOverride || capability.recommendedNumCtx || 16384) / 1024)}K`;

  return (
    <div className="fixed bottom-9 right-[336px] z-40 flex flex-col items-end pointer-events-none">
      {/* Floating Chat Window */}
      {isOpen && (
        <div
          className={`pointer-events-auto mb-3 flex flex-col rounded-xl border border-indigo-500/40 bg-slate-950/95 backdrop-blur-md shadow-2xl shadow-black/60 overflow-hidden transition-all ${
            isExpanded
              ? 'w-[620px] max-w-[calc(100vw-360px)] h-[72vh]'
              : 'w-[430px] max-w-[calc(100vw-360px)] h-[560px] max-h-[calc(100vh-120px)]'
          }`}
        >
          {/* Header */}
          <div className="flex items-center gap-2 px-3 py-2.5 bg-slate-900 border-b border-slate-800 shrink-0">
            <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-indigo-500 to-violet-600 flex items-center justify-center shrink-0">
              <Bot className="w-4 h-4 text-white" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-1.5">
                <span className="text-xs font-semibold text-white">AI Copilot</span>
                <span className="text-[10px] px-1.5 py-0.5 rounded bg-emerald-500/15 text-emerald-300 border border-emerald-500/30 font-mono truncate max-w-[155px]">
                  {capability.modelId}
                </span>
              </div>
              <p className="text-[10px] text-slate-400">
                Native tools · {ctxLabel} ctx · Virtual overlay
              </p>
            </div>
            <select
              value={numCtxOverride || ''}
              onChange={(e) => setNumCtxOverride(e.target.value ? Number(e.target.value) : null)}
              title="Context window override"
              className="bg-slate-800 text-[10px] text-slate-300 border border-slate-700 rounded px-1.5 py-1 outline-none"
            >
              <option value="">Auto ({Math.round((capability.recommendedNumCtx || 16384) / 1024)}K)</option>
              <option value="16384">16K</option>
              <option value="32768">32K</option>
              <option value="65536">64K</option>
            </select>
            <button
              type="button"
              onClick={handleReset}
              title="New chat (clears history and overlay)"
              className="p-1.5 text-slate-400 hover:text-white hover:bg-slate-800 rounded transition-colors"
            >
              <RotateCcw className="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              onClick={() => setIsExpanded(v => !v)}
              title={isExpanded ? 'Compact size' : 'Expand size'}
              className="p-1.5 text-slate-400 hover:text-white hover:bg-slate-800 rounded transition-colors"
            >
              {isExpanded ? <Minimize2 className="w-3.5 h-3.5" /> : <Maximize2 className="w-3.5 h-3.5" />}
            </button>
            <button
              type="button"
              onClick={() => setIsOpen(false)}
              title="Minimize Copilot"
              className="p-1.5 text-slate-400 hover:text-white hover:bg-slate-800 rounded transition-colors"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>

          {/* Mode Toggle: Ask vs Agent */}
          <div className="flex items-center gap-1.5 px-3 py-2 bg-slate-900/70 border-b border-slate-800/80 shrink-0">
            <button
              type="button"
              onClick={() => setMode('ask')}
              className={`flex-1 py-1.5 px-2 rounded-lg text-xs font-medium transition-colors flex items-center justify-center gap-1.5 ${
                mode === 'ask'
                  ? 'bg-indigo-600/25 text-indigo-300 border border-indigo-500/40'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/60 border border-transparent'
              }`}
            >
              <Eye className="w-3.5 h-3.5" /> Ask (Read-Only)
            </button>
            <button
              type="button"
              onClick={() => setMode('agent')}
              className={`flex-1 py-1.5 px-2 rounded-lg text-xs font-medium transition-colors flex items-center justify-center gap-1.5 ${
                mode === 'agent'
                  ? 'bg-violet-600/25 text-violet-300 border border-violet-500/40'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/60 border border-transparent'
              }`}
            >
              <Sparkles className="w-3.5 h-3.5" /> Agent (Overlay)
            </button>
          </div>

          {/* Messages */}
          <div className="flex-1 overflow-y-auto px-3 py-3 space-y-3 custom-scrollbar">
            {messages.length === 0 && (
              <div className="rounded-lg border border-slate-800 bg-slate-900/60 p-3 text-xs text-slate-300 space-y-2">
                <p className="font-semibold text-white flex items-center gap-1.5">
                  <ShieldCheck className="w-4 h-4 text-emerald-400" />
                  Independent Large-Model Copilot
                </p>
                <p className="text-slate-400 leading-relaxed">
                  Directly inspects your project using native tool-calling (<span className="font-mono text-slate-300">read_file</span>, <span className="font-mono text-slate-300">search_code</span>, <span className="font-mono text-slate-300">run_check</span>) without small-model prompt rails or sanitizers.
                </p>
                <p className="text-slate-400 leading-relaxed">
                  In <strong className="text-violet-300">Agent</strong> mode, all edits are staged in a virtual overlay first — nothing writes to disk until you review the Diff and click <strong>Apply</strong>.
                </p>
              </div>
            )}

            {messages.map((msg, idx) => (
              <div key={idx} className={`flex flex-col ${msg.role === 'user' ? 'items-end' : 'items-start'}`}>
                {msg.role === 'user' ? (
                  <div className="max-w-[88%] rounded-xl rounded-br-sm bg-indigo-600 text-white px-3 py-2 text-xs leading-relaxed whitespace-pre-wrap">
                    {msg.content}
                  </div>
                ) : (
                  <div className="w-full space-y-1.5">
                    {msg.thinking && (
                      <details className="rounded-lg border border-slate-800 bg-slate-900/60 text-[11px]">
                        <summary className="cursor-pointer px-2.5 py-1.5 text-slate-400 hover:text-slate-200 flex items-center gap-1.5 select-none">
                          <Brain className="w-3.5 h-3.5 text-violet-400" />
                          <span>Reasoning ({msg.thinking.length} chars)</span>
                        </summary>
                        <pre className="px-2.5 pb-2 pt-1 text-[11px] text-slate-400 font-mono whitespace-pre-wrap break-words border-t border-slate-800/70 max-h-40 overflow-y-auto">
                          {msg.thinking}
                        </pre>
                      </details>
                    )}

                    {Array.isArray(msg.tools) && msg.tools.length > 0 && (
                      <div className="space-y-1">
                        {msg.tools.map((tool) => (
                          <ToolCallBadge key={tool.id} tool={tool} onOpenFile={onOpenFile} />
                        ))}
                      </div>
                    )}

                    {(msg.content || msg.streaming) && (
                      <div className="rounded-xl rounded-bl-sm bg-slate-900 border border-slate-800 px-3 py-2 text-xs text-slate-200 leading-relaxed whitespace-pre-wrap break-words">
                        {msg.content || (
                          <span className="text-slate-400 italic">Thinking &amp; inspecting codebase...</span>
                        )}
                      </div>
                    )}

                    {msg.error && (
                      <div className="rounded-lg bg-rose-950/50 border border-rose-500/30 px-2.5 py-2 text-xs text-rose-300 flex items-start gap-1.5">
                        <AlertCircle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
                        <span>{msg.error}</span>
                      </div>
                    )}
                  </div>
                )}
              </div>
            ))}
            <div ref={messagesEndRef} />
          </div>

          {/* Virtual Overlay Staged Changes */}
          {stagedChanges.length > 0 && (
            <div className="px-3 py-2 border-t border-violet-500/30 bg-slate-900/95 shrink-0 space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-[10px] font-bold uppercase tracking-wider text-violet-300 flex items-center gap-1">
                  <ShieldCheck className="w-3.5 h-3.5" />
                  Virtual Overlay ({stagedChanges.length} pending — not on disk)
                </span>
              </div>
              <div className="space-y-1 max-h-28 overflow-y-auto custom-scrollbar">
                {stagedChanges.map((sc) => (
                  <div
                    key={sc.path}
                    className="flex items-center gap-1.5 bg-slate-950/80 border border-slate-800 rounded px-2 py-1"
                  >
                    <span className={`text-[9px] font-bold uppercase px-1 py-0.5 rounded shrink-0 ${
                      sc.action === 'delete'
                        ? 'bg-rose-500/20 text-rose-300'
                        : sc.action === 'create'
                        ? 'bg-emerald-500/20 text-emerald-300'
                        : 'bg-amber-500/20 text-amber-300'
                    }`}>
                      {sc.action === 'delete' ? 'DEL' : sc.action === 'create' ? 'NEW' : 'MOD'}
                    </span>
                    <span className="text-[11px] font-mono text-slate-200 truncate flex-1" title={sc.summary || sc.path}>
                      {sc.path}
                    </span>
                    <button
                      type="button"
                      onClick={() => onReviewStagedFile && onReviewStagedFile(sc)}
                      title="Review Diff in Editor"
                      className="p-1 text-slate-400 hover:text-indigo-400 hover:bg-slate-800 rounded"
                    >
                      <Eye className="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      disabled={!!applyingPath}
                      onClick={() => handleApplySingle(sc)}
                      title="Apply change to disk"
                      className="p-1 text-emerald-400 hover:text-emerald-300 hover:bg-emerald-950/40 rounded disabled:opacity-40"
                    >
                      <Check className="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      disabled={!!applyingPath}
                      onClick={() => handleDiscardSingle(sc.path)}
                      title="Discard staged change"
                      className="p-1 text-rose-400 hover:text-rose-300 hover:bg-rose-950/40 rounded disabled:opacity-40"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                ))}
              </div>
              <div className="flex gap-2">
                <button
                  type="button"
                  disabled={!!applyingPath}
                  onClick={handleApplyAll}
                  className="flex-1 py-1.5 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold flex items-center justify-center gap-1 disabled:opacity-50"
                >
                  <Check className="w-3.5 h-3.5" /> Apply All to Disk
                </button>
                <button
                  type="button"
                  disabled={!!applyingPath}
                  onClick={handleDiscardAll}
                  className="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-medium border border-slate-700"
                >
                  Discard All
                </button>
              </div>
            </div>
          )}

          {/* Input Composer */}
          <div className="p-2.5 border-t border-slate-800 bg-slate-900/90 shrink-0">
            <div className="flex items-end gap-2">
              <textarea
                rows={2}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    handleSend();
                  }
                }}
                placeholder={
                  mode === 'ask'
                    ? 'Ask Copilot to analyze code, trace bugs, or explain architecture...'
                    : 'Tell Copilot Agent what to refactor or implement (staged in overlay)...'
                }
                className="flex-1 resize-none rounded-lg bg-slate-950 border border-slate-700/80 focus:border-indigo-500 px-2.5 py-2 text-xs text-slate-100 placeholder-slate-500 outline-none"
              />
              {isRunning ? (
                <button
                  type="button"
                  onClick={handleCancel}
                  title="Stop generation"
                  className="p-2.5 rounded-lg bg-rose-600 hover:bg-rose-500 text-white shrink-0"
                >
                  <Square className="w-4 h-4" />
                </button>
              ) : (
                <button
                  type="button"
                  disabled={!input.trim()}
                  onClick={handleSend}
                  title="Send message"
                  className="p-2.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 disabled:opacity-40 text-white shrink-0"
                >
                  <Send className="w-4 h-4" />
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Floating Launcher Button + Rich Hover Explanation */}
      <div
        className="relative pointer-events-auto"
        onMouseEnter={() => setShowHoverCard(true)}
        onMouseLeave={() => setShowHoverCard(false)}
      >
        {showHoverCard && !isOpen && (
          <div className="absolute bottom-full right-0 mb-2 w-72 rounded-xl border border-indigo-500/40 bg-slate-950/95 p-3 shadow-2xl shadow-black/70 text-xs pointer-events-none">
            <div className="flex items-center justify-between gap-2 mb-1">
              <span className="font-semibold text-white flex items-center gap-1.5">
                <Bot className="w-3.5 h-3.5 text-indigo-400" />
                AI Copilot
              </span>
              <span className="text-[10px] font-mono text-emerald-300 bg-emerald-500/15 px-1.5 py-0.5 rounded">
                {capability.modelId}
              </span>
            </div>
            <p className="text-slate-300 leading-relaxed">
              Independent large-model codebase assistant with native tool-calling ({ctxLabel} context).
            </p>
            <p className="text-slate-400 mt-1.5 leading-relaxed">
              Analyze code in <strong>Ask</strong> mode or stage multi-file edits in a safe virtual overlay in <strong>Agent</strong> mode — nothing writes to disk until you review and click Apply.
            </p>
          </div>
        )}

        <button
          type="button"
          onClick={() => setIsOpen(v => !v)}
          className="flex items-center gap-2 px-3.5 py-2.5 rounded-full bg-gradient-to-r from-indigo-600 to-violet-600 hover:from-indigo-500 hover:to-violet-500 text-white text-xs font-semibold shadow-lg shadow-indigo-950/70 border border-indigo-400/40 transition-all"
        >
          <Bot className="w-4 h-4" />
          <span>AI Copilot</span>
          {stagedChanges.length > 0 && (
            <span className="px-1.5 py-0.5 text-[10px] font-bold rounded-full bg-amber-400 text-slate-950">
              {stagedChanges.length}
            </span>
          )}
        </button>
      </div>
    </div>
  );
}
