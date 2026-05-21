import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

let activeProjectId = null;

function parseAxiomUrl(urlStr) {
  if (!urlStr.startsWith('axiom://')) return null;
  const rest = urlStr.substring('axiom://'.length);
  const parts = rest.split('?');
  const action = parts[0].replace(/\/$/, '');
  const query = parts[1] || '';
  
  const params = {};
  query.split('&').forEach(pair => {
    const [k, v] = pair.split('=');
    if (k && v) {
      params[decodeURIComponent(k)] = decodeURIComponent(v);
    }
  });
  
  if (action === 'build' || action === 'generate') {
    const manifestId = params.id || '';
    const projectId = params.projectId || manifestId;
    const token = params.token || '';
    const host = params.host || '';
    
    if (manifestId) {
      return {
        manifestId,
        projectId,
        token,
        host
      };
    }
  } else if (action === 'config') {
    const projectId = params.projectId || '';
    if (projectId) {
      return { projectId };
    }
  } else if (action === 'deploy') {
    const projectId = params.projectId || '';
    if (projectId) {
      return { projectId };
    }
  }
  return null;
}

/**
 * Axiom Forge - IPC Bridge
 * Translates old Electron IPC calls (window.electronAPI) to new Tauri commands.
 * This allows us to keep the React components mostly unchanged during migration.
 */
window.electronAPI = {
  // ==================== SECURITY LAYER ====================
  security: {
    storeToken: async (key, value) => {
      try {
        await invoke('store_token', { key, value });
        return { success: true };
      } catch (error) {
        throw new Error(error);
      }
    },
    getToken: async (key) => {
      const res = await invoke('get_token', { key });
      return res.value;
    },
    deleteToken: async (key) => {
      await invoke('delete_token', { key });
      return { success: true };
    },
    hasToken: async (key) => {
      const res = await invoke('has_token', { key });
      return { success: true, exists: res.exists };
    },
    clearAllTokens: async () => {
      await invoke('clear_all_tokens');
      return { success: true };
    }
  },

  // ==================== HARDWARE API ====================
  hardware: {
    getProfile: async () => {
      return await invoke('get_hardware_profile');
    },
    getSelectedModel: async () => {
      return localStorage.getItem('axiom-forge-model');
    },
    setSelectedModel: async (modelId) => {
      localStorage.setItem('axiom-forge-model', modelId);
      console.log("Model selected:", modelId);
    },
    getBuilderModel: async () => {
      return localStorage.getItem('axiom-forge-builder-model');
    },
    setBuilderModel: async (modelId) => {
      localStorage.setItem('axiom-forge-builder-model', modelId);
      console.log("Builder model selected:", modelId);
    },
    getEditorModel: async () => {
      return localStorage.getItem('axiom-forge-editor-model');
    },
    setEditorModel: async (modelId) => {
      localStorage.setItem('axiom-forge-editor-model', modelId);
      console.log("Editor model selected:", modelId);
    }
  },

  // ==================== OLLAMA API ====================
  ollama: {
    checkHealth: async (host, targetModel) => {
      return await invoke('ollama_check_health', { host, targetModel });
    },
    checkInstalled: async () => {
      return await invoke('ollama_check_installed');
    },
    startServer: async () => {
      return await invoke('ollama_start_server');
    },
    pullModel: async (model) => {
      return await invoke('ollama_pull_model', { model });
    },
    unloadModel: async (model) => {
      return await invoke('ollama_unload_model', { model });
    },
    onPullProgress: (callback) => {
      let unlisten;
      listen('ollama:pull-progress', (event) => {
        callback(event.payload);
      }).then(fn => {
        unlisten = fn;
      });
      return () => {
        if (unlisten) unlisten();
      };
    }
  },

  // ==================== PROJECT API ====================
  project: {
    getAll: async () => {
      try {
        const projects = await invoke('project_get_all');
        return { success: true, projects };
      } catch (e) {
        return { success: false, error: e.toString() };
      }
    },
    get: async ({ id }) => {
      try {
        const all = await invoke('project_get_all');
        const project = all.find(p => p.id === id);
        return { success: !!project, project };
      } catch (e) {
        return { success: false, error: e.toString() };
      }
    },
    save: async (projectData) => {
      return await invoke('project_save', { projectData });
    },
    delete: async ({ id }) => {
      try {
        const res = await invoke('project_delete', { id });
        return { success: res };
      } catch (e) {
        return { success: false, error: e.toString() };
      }
    },
    updateStatus: async (id, status, metadata = {}) => {
      try {
        const res = await invoke('project_update_status', { id, status, metadata });
        return { success: res };
      } catch (e) {
        return { success: false, error: e.toString() };
      }
    },
    getFiles: async (projectId) => {
      return await invoke('project_get_files', { projectId });
    },
    readFile: async (projectId, filePath) => {
      return await invoke('project_read_file', { projectId, filePath });
    },
    commit: async (projectId, message) => {
      return await invoke('project_commit', { projectId, message });
    },
    pushToGitHub: async (projectId) => {
      return await invoke('project_push_to_github', { projectId });
    }
  },

  // ==================== EDITOR API ====================
  editor: {
    writeFile: async (projectId, filePath, content) => {
      return await invoke('editor_write_file', { projectId, filePath, content });
    },
    aiEdit: async (filePath, content, prompt, projectId, visualContext) => {
      const modelId = localStorage.getItem('axiom-forge-editor-model') || localStorage.getItem('axiom-forge-model') || 'qwen2.5-coder:7b';
      try {
        return await invoke('editor_ai_edit', {
          filePath,
          content,
          prompt,
          projectId,
          modelId,
          visualContext
        });
      } catch (err) {
        return { success: false, error: err.toString() };
      }
    },
    aiProjectEdit: async (prompt, projectId) => {
      const modelId = localStorage.getItem('axiom-forge-editor-model') || localStorage.getItem('axiom-forge-model') || 'qwen2.5-coder:7b';
      try {
        return await invoke('editor_ai_project_edit', {
          projectId,
          prompt,
          modelId
        });
      } catch (err) {
        return { success: false, error: err.toString() };
      }
    },
    gitCommit: async (projectId, message) => {
      return await invoke('project_commit', { projectId, message });
    },
    onAiDirective: (callback) => {
      return () => {};
    },
    onAiChunk: (callback) => {
      return () => {};
    },
    onProjectEditProgress: (callback) => {
      let unlisten;
      listen('editor:project-edit-progress', (event) => {
        callback(event.payload);
      }).then(fn => {
        unlisten = fn;
      });
      return () => {
        if (unlisten) unlisten();
      };
    }
  },

  // ==================== TASK ORCHESTRATOR API ====================
  task: {
    getStatus: async () => null,
    startGeneration: async (manifestId, projectId, token, host = null) => {
      const builderModelId = localStorage.getItem('axiom-forge-builder-model') || localStorage.getItem('axiom-forge-model') || 'qwen2.5-coder:7b';
      const editorModelId = localStorage.getItem('axiom-forge-editor-model') || localStorage.getItem('axiom-forge-model') || 'qwen2.5-coder:7b';
      
      invoke('debug_log_to_file', { message: `[IPC Bridge] startGeneration called. manifestId: ${manifestId}, projectId: ${projectId}, token length: ${token ? token.length : 0}, model: ${builderModelId}, host: ${host}` });

      // JIT Swapping: Unload the Editor model first to prevent OOM
      if (editorModelId !== builderModelId) {
        try {
          invoke('debug_log_to_file', { message: `[IPC Bridge] JIT Memory Swapping: Unloading Editor model '${editorModelId}' before building...` });
          await invoke('ollama_unload_model', { model: editorModelId });
        } catch (e) {
          invoke('debug_log_to_file', { message: `[IPC Bridge] Failed to unload editor model: ${e.toString()}` });
        }
      }
      
      try {
        const decodedHost = host ? decodeURIComponent(host) : null;
        const res = await invoke('task_start_generation', { 
          manifestId: manifestId, 
          projectId: projectId, 
          token: token, 
          modelId: builderModelId,
          host: decodedHost
        });
        invoke('debug_log_to_file', { message: `[IPC Bridge] task_start_generation invoke returned success: ${res}` });
        return res;
      } catch (e) {
        invoke('debug_log_to_file', { message: `[IPC Bridge] task_start_generation invoke error: ${e.toString()}` });
        throw e;
      }
    },
    onProgress: (callback) => {
      let unlisten;
      listen('task:progress', (event) => {
        const payload = event.payload;
        invoke('debug_log_to_file', { message: `[IPC Bridge] task:progress event received in JS bridge: ${JSON.stringify(payload)}` });
        callback(payload);
      }).then(fn => {
        unlisten = fn;
      });
      
      return () => {
        if (unlisten) unlisten();
      };
    },
    stop: async (taskId) => {
      return { success: true };
    },
    getActiveTaskByProject: async (projectId) => {
      return { success: false };
    }
  },
  server: {
    start: async (options) => {
      if (options && options.projectId) {
        activeProjectId = options.projectId;
      }
      try {
        const success = await invoke('server_start', { options });
        return { success, cdpPort: null };
      } catch (error) {
        return { success: false, error: error.toString() };
      }
    },
    stop: async (projectId) => {
      const pid = projectId || activeProjectId;
      if (!pid) return { success: false };
      try {
        const success = await invoke('server_stop', { project_id: pid });
        return { success };
      } catch (error) {
        return { success: false, error: error.toString() };
      }
    },
    getStatus: async (projectId) => {
      try {
        const isRunning = await invoke('server_status', { project_id: projectId });
        return { isRunning };
      } catch (error) {
        return { isRunning: false };
      }
    },
    getCdpWsUrl: async () => null,
    onLog: (callback) => {
      let unlisten;
      listen('server:log', (event) => {
        callback(event.payload);
      }).then(fn => {
        unlisten = fn;
      });
      return () => {
        if (unlisten) unlisten();
      };
    },
    onStatus: (callback) => {
      let unlisten;
      listen('server:status', (event) => {
        callback(event.payload);
      }).then(fn => {
        unlisten = fn;
      });
      return () => {
        if (unlisten) unlisten();
      };
    },
  },
  shell: {
    openExternal: (url) => {
      console.log("Should open external:", url);
    }
  },
  
  // ==================== DEEP LINK & WINDOW STUBS ====================
  deepLink: {
    onBuild: (callback) => {
      let unlisten;
      listen('deep-link:build', (event) => {
        const payload = event.payload;
        invoke('debug_log_to_file', { message: `[IPC Bridge] Live deep-link:build event received in JS listener: ${JSON.stringify(payload)}` });
        callback(payload);
      }).then(fn => unlisten = fn);

      // JIT Pending Deep Link Pull for Cold Starts
      invoke('get_pending_deep_link')
        .then((pendingUrl) => {
          if (pendingUrl) {
            invoke('debug_log_to_file', { message: `[IPC Bridge] onBuild pulled pending startup URL: ${pendingUrl}` });
            if (window.processedDeepLinks && window.processedDeepLinks.has(pendingUrl)) {
              invoke('debug_log_to_file', { message: `[IPC Bridge] onBuild ignored already-processed startup URL.` });
              return;
            }
            const parsed = parseAxiomUrl(pendingUrl);
            if (parsed && parsed.manifestId && (pendingUrl.includes('/build') || pendingUrl.includes('/generate') || pendingUrl.includes('?id='))) {
              if (!window.processedDeepLinks) window.processedDeepLinks = new Set();
              window.processedDeepLinks.add(pendingUrl);
              
              invoke('debug_log_to_file', { message: `[IPC Bridge] onBuild JIT routing successful. Triggering callback: ${JSON.stringify(parsed)}` });
              callback(parsed);
              
              // Clear it in Rust so it's not pulled again
              invoke('clear_pending_deep_link').catch(console.error);
            } else {
              invoke('debug_log_to_file', { message: `[IPC Bridge] onBuild JIT routing mismatch (not a build action). Leaving pending URL for other channels.` });
            }
          }
        })
        .catch((err) => {
          invoke('debug_log_to_file', { message: `[IPC Bridge] Error onBuild get_pending_deep_link: ${err.toString()}` });
        });

      return () => { if (unlisten) unlisten(); };
    },
    onConfig: (callback) => {
      let unlisten;
      listen('deep-link:config', (event) => {
        const payload = event.payload;
        invoke('debug_log_to_file', { message: `[IPC Bridge] Live deep-link:config event received: ${JSON.stringify(payload)}` });
        callback(payload);
      }).then(fn => unlisten = fn);

      invoke('get_pending_deep_link')
        .then((pendingUrl) => {
          if (pendingUrl) {
            invoke('debug_log_to_file', { message: `[IPC Bridge] onConfig pulled pending startup URL: ${pendingUrl}` });
            if (window.processedDeepLinks && window.processedDeepLinks.has(pendingUrl)) {
              return;
            }
            const parsed = parseAxiomUrl(pendingUrl);
            if (parsed && parsed.projectId && pendingUrl.includes('/config')) {
              if (!window.processedDeepLinks) window.processedDeepLinks = new Set();
              window.processedDeepLinks.add(pendingUrl);
              
              invoke('debug_log_to_file', { message: `[IPC Bridge] onConfig JIT routing successful. Triggering callback: ${JSON.stringify(parsed)}` });
              callback(parsed);
              invoke('clear_pending_deep_link').catch(console.error);
            }
          }
        });

      return () => { if (unlisten) unlisten(); };
    },
    onDeploy: (callback) => {
      let unlisten;
      listen('deep-link:deploy', (event) => {
        const payload = event.payload;
        invoke('debug_log_to_file', { message: `[IPC Bridge] Live deep-link:deploy event received: ${JSON.stringify(payload)}` });
        callback(payload);
      }).then(fn => unlisten = fn);

      invoke('get_pending_deep_link')
        .then((pendingUrl) => {
          if (pendingUrl) {
            invoke('debug_log_to_file', { message: `[IPC Bridge] onDeploy pulled pending startup URL: ${pendingUrl}` });
            if (window.processedDeepLinks && window.processedDeepLinks.has(pendingUrl)) {
              return;
            }
            const parsed = parseAxiomUrl(pendingUrl);
            if (parsed && parsed.projectId && pendingUrl.includes('/deploy')) {
              if (!window.processedDeepLinks) window.processedDeepLinks = new Set();
              window.processedDeepLinks.add(pendingUrl);
              
              invoke('debug_log_to_file', { message: `[IPC Bridge] onDeploy JIT routing successful. Triggering callback: ${JSON.stringify(parsed)}` });
              callback(parsed);
              invoke('clear_pending_deep_link').catch(console.error);
            }
          }
        });

      return () => { if (unlisten) unlisten(); };
    }
  },
  window: {
    showFloating: async () => {},
    closeFloating: async () => {},
    hideFloating: async () => {}
  },
  app: {
    getVersion: async () => {
      // In Tauri we can just invoke the built-in app version or return hardcoded
      return { success: true, version: '0.1.0 (Rust)' };
    }
  }
};

console.log("[IPC Bridge] Tauri translation layer initialized.");
