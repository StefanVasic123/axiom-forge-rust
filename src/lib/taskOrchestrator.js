/**
 * Axiom Forge - Task Orchestrator
 * 
 * Manages the state machine flow:
 * 1. GENERATION: Fetch manifest -> File-by-file codegen via Ollama
 * 2. CONFIGURATION: Collect ENV variables
 * 3. DEPLOYMENT: Push to GitHub -> Trigger Vercel Deploy
 */

import { EventEmitter } from 'events';
import { OllamaClient } from './ollamaClient.js';
import { DeploymentService } from './deploymentService.js';
import { enrichProject } from './axiomMetaGenerator.js';
import path from 'path';
import fs from 'fs/promises';
import os from 'os';
import { spawn, exec } from 'child_process';
import { promisify } from 'util';
import { APP_CONFIG } from './config.js';

const execAsync = promisify(exec);

// ==================== STATE DEFINITIONS ====================
export const TaskState = {
  IDLE: 'idle',
  GENERATING: 'generating',
  GENERATED: 'generated',
  AWAITING_KEYS: 'awaiting-keys',
  CONFIGURED: 'configured',
  DEPLOYING: 'deploying',
  DEPLOYED: 'deployed',
  LIVE: 'live',
  ERROR: 'error',
  STOPPED: 'stopped'
};

export const TaskPhase = {
  FETCH_MANIFEST: 'fetch-manifest',
  GENERATE_FILES: 'generate-files',
  SAVE_PROJECT: 'save-project',
  AWAIT_CONFIGURATION: 'await-configuration',
  PUSH_GITHUB: 'push-github',
  DEPLOY_VERCEL: 'deploy-vercel',
  VERIFY_DEPLOYMENT: 'verify-deployment'
};

// ==================== TASK ORCHESTRATOR CLASS ====================
export class TaskOrchestrator extends EventEmitter {
  constructor(securityManager) {
    super();
    this.securityManager = securityManager;
    this.ollama = new OllamaClient();
    this.deployment = new DeploymentService(securityManager);
    this.activeTasks = new Map();
    this.projectStoragePath = path.join(os.homedir(), '.axiom-forge', 'projects');
    
    // Ensure storage directory exists
    this._ensureStorage();
  }

  // ==================== PRIVATE METHODS ====================

  async _ensureStorage() {
    try {
      await fs.mkdir(this.projectStoragePath, { recursive: true });
    } catch (error) {
      console.error('[Orchestrator] Failed to create storage:', error);
    }
  }

  _createTaskId() {
    return `task-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }

  _createProjectId() {
    return `proj-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }

  _updateTask(taskId, updates) {
    const task = this.activeTasks.get(taskId);
    if (task) {
      Object.assign(task, updates);
      this.emit('task:update', { taskId, task });
    }
  }

  async _fetchManifest(manifestId, token) {
    const apiUrl = APP_CONFIG.IDEA_ANALYZER_URL;
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 15000); // 15 second timeout

    try {
      console.log(`[Orchestrator] Fetching from: ${apiUrl}/api/axiom/manifest/${manifestId}`);
      const response = await fetch(`${apiUrl}/api/axiom/manifest/${manifestId}`, {
        headers: {
          'Authorization': `Bearer ${token}`
        },
        signal: controller.signal
      });
      
      if (response.ok) {
        const jsonResponse = await response.json();
        if (jsonResponse.success) {
          return jsonResponse.data;
        }
      }
      const errTxt = await response.text();
      throw new Error(`Server response: ${response.status} - ${errTxt}`);
    } catch (error) {
      const isTimeout = error.name === 'AbortError';
      const msg = isTimeout ? 'Cloud connection timed out (15s)' : error.message;
      
      console.error('[Orchestrator] Manifest fetch failed:', msg);
      throw new OrchestratorError(
        'MANIFEST_FETCH_ERROR',
        `Could not fetch manifest: ${msg}`
      );
    } finally {
      clearTimeout(timeoutId);
    }
  }

  async _saveProjectFiles(projectId, files) {
    const projectPath = path.join(this.projectStoragePath, projectId);
    await fs.mkdir(projectPath, { recursive: true });

    for (const file of files) {
      const filePath = path.join(projectPath, file.path);
      const dir = path.dirname(filePath);
      
      await fs.mkdir(dir, { recursive: true });
      await fs.writeFile(filePath, file.content, 'utf-8');
    }

    return projectPath;
  }

  /**
   * Initialize a local git repo and make the baseline "Generated" commit.
   * Does NOT push to GitHub — that happens on Deploy or "Commit to GitHub".
   * Safe to call multiple times (idempotent — skips if .git already exists).
   */
  async _initLocalGit(projectPath) {
    const run = (cmd) => execAsync(cmd, { cwd: projectPath }).catch(e => {
      console.warn(`[Git] Non-fatal: ${cmd} → ${e.message}`);
    });

    // Skip if already initialized
    try {
      await fs.access(path.join(projectPath, '.git'));
      console.log('[Git] Repo already initialized, skipping init.');
      return;
    } catch { /* not initialized yet — proceed */ }

    await run('git init');
    await run('git checkout -b main').catch(() => run('git branch -M main'));
    await run('git config user.email "axiom-forge@localhost"');
    await run('git config user.name "Axiom Forge"');
    await run('git add .');
    await run('git commit -m "🚀 Generated by Axiom Forge"');

    console.log('[Git] Baseline commit created locally.');
  }

  async _ensureOllama(onProgress) {
    const health = await this.ollama.healthCheck();
    if (health.available) return true;

    console.log('[Orchestrator] Ollama not running. Attempting auto-start...');
    onProgress({ 
      phase: 'generate-files', 
      message: 'Ollama engine is sleeping. Waking it up...',
      progress: 10
    });
    
    try {
      const ollamaProcess = spawn('ollama', ['serve'], {
        detached: true,
        stdio: 'ignore'
      });
      ollamaProcess.unref();
      
    // Wait for availability (max 10 seconds)
    for (let i = 0; i < 10; i++) {
      await new Promise(r => setTimeout(r, 1000));
      const check = await this.ollama.healthCheck();
      if (check.available) {
        console.log('[Orchestrator] Ollama is reachable at 127.0.0.1.');
        return true;
      }
    }
    } catch (e) {
      console.error('[Orchestrator] Failed to auto-start Ollama:', e);
    }
    
    return false;
  }

  // ==================== PUBLIC API ====================

  /**
   * Push project to GitHub only (no Vercel).
   * Used by the "Commit to GitHub" button — does not trigger a redeploy.
   */
  async pushToGitHub(projectId, onProgress = () => {}) {
    const projectPath = path.join(this.projectStoragePath, projectId);
    return this.deployment.pushToGitHub(projectId, projectPath, onProgress);
  }

  /**
   * Start the code generation phase
   */
  async startGeneration(manifestId, existingProjectId = null, token, onProgress = () => {}) {
    const taskId = this._createTaskId();
    const projectId = existingProjectId || this._createProjectId();
    
    const task = {
      id: taskId,
      projectId,
      manifestId,
      state: TaskState.GENERATING,
      phase: TaskPhase.FETCH_MANIFEST,
      progress: 0,
      startTime: Date.now(),
      logs: [],
       result: null,
      error: null
    };

    this.activeTasks.set(taskId, task);
    this.emit('task:started', { taskId, type: 'generation' });

    console.log(`[Orchestrator] Starting generation task: ${taskId} for project: ${projectId}`);

    // Send immediate 1% progress to avoid "idle" 0% wait
    onProgress({ 
      phase: 'fetch-manifest', 
      message: 'Initializing generation pipeline...', 
      progress: 1 
    });

    try {
      // Phase 1: Fetch Manifest
      console.log(`[Orchestrator] Phase 1: Fetching manifest for ${manifestId}...`);
      
      const manifest = await this._fetchManifest(manifestId, token);
      
      // DEBUG: Log activity
      try {
        const fs = await import('fs');
        fs.appendFileSync('C:\\Users\\StefanV\\Desktop\\Projects\\axiom-forge\\axiom-live-debug.log', `[${new Date().toISOString()}] Manifest fetched. Expecting clear text...\n`);
        fs.writeFileSync('C:\\Users\\StefanV\\Desktop\\Projects\\axiom-forge\\axiom-live-manifest.json', JSON.stringify(manifest, null, 2));
      } catch (e) {}

      this._updateTask(taskId, { phase: TaskPhase.FETCH_MANIFEST, progress: 10 });
      onProgress({ phase: 'fetch-manifest', message: 'Manifest secured and ready.', progress: 10 });
      
      process.stdout.write(`\n[DIAGNOSTIC] Manifest Audit: ${manifest?.name} | Files: ${manifest?.files?.length || 0} | Tech: ${JSON.stringify(manifest?.techStack?.framework)}\n`);
      task.logs.push({ time: Date.now(), message: `Fetched manifest: ${manifest.metadata?.name || manifestId}` });

      // Phase 2: Generate Files
      console.log(`[Orchestrator] Phase 2: Checking local AI engine...`);
      this._updateTask(taskId, { phase: TaskPhase.GENERATE_FILES, progress: 10 });
      
      // Auto-start or verify Ollama
      const isOllamaReady = await this._ensureOllama(onProgress);
      
      if (!isOllamaReady) {
        throw new OrchestratorError('OLLAMA_UNAVAILABLE', 'Ollama engine is not running and could not be started automatically.');
      }

      onProgress({ 
        phase: 'generate-files', 
        message: 'Local AI engine ready.', 
        progress: 15 
      });

      // Verify model - provide feedback before this potentially slow call
      onProgress({ 
        phase: 'generate-files', 
        message: `Checking if AI model (${this.ollama.model}) is loaded...`, 
        progress: 17 
      });
      
      const health = await this.ollama.healthCheck();
      
      if (!health.hasModel) {
        console.log(`[Orchestrator] Model missing. Starting pull...`);
        onProgress({ 
          phase: 'pull-model', 
          message: `Model ${this.ollama.model} not found. Attempting download...`,
          progress: 18
        });
        
        await this.ollama.pullModel(this.ollama.model, (pullProgress) => {
          const msg = pullProgress.percent 
            ? `Downloading Model: ${pullProgress.percent}% - ${pullProgress.status}`
            : `Model Status: ${pullProgress.status}`;
          
          // Allocate 5% of total progress bar to the pull phase
          const pullContribution = (pullProgress.percent || 0) * 0.05;
          const currentProgress = 18 + pullContribution;

          this._updateTask(taskId, { progress: currentProgress });
          onProgress({ 
            phase: 'pull-model', 
            message: msg,
            progress: currentProgress
          });
        });
      }

      onProgress({ 
        phase: 'generate-files', 
        message: 'AI Model ready. Preparing instructions...', 
        progress: 23 
      });

      // Generate files
      console.log(`[Orchestrator] Starting file generation...`);
      // DEBUG: Log start of generation
      try {
        const fs = await import('fs');
        fs.appendFileSync('C:\\Users\\StefanV\\Desktop\\Projects\\axiom-forge\\axiom-live-debug.log', `[${new Date().toISOString()}] TRIGGERING OLLAMA GENERATION: ${manifest.name} | Files: ${manifest.files?.length}\n`);
      } catch (e) {}

      const generationResult = await this.ollama.generateProject(manifest, (progress) => {
        // Safe progress calculation to avoid NaN
        const subProgress = typeof progress.progress === 'number' ? progress.progress : 0;
        const currentProgress = 25 + (subProgress * 0.7); 
        
        this._updateTask(taskId, { progress: currentProgress });
        
        onProgress({ 
          ...progress, 
          progress: currentProgress,
          message: progress.message || (progress.phase === 'generating' 
            ? `Generating: ${progress.file}` 
            : `Project Phase: ${progress.phase}`)
        });
      });

      task.logs.push({ 
        time: Date.now(), 
        message: `Generated ${generationResult.stats.successful}/${generationResult.stats.total} files` 
      });

      if (generationResult.stats.failed > 0) {
        task.logs.push({ 
          time: Date.now(), 
          message: `Warning: ${generationResult.stats.failed} files failed to generate`,
          level: 'warn'
        });
      }

      // Phase 3: Save Project
      this._updateTask(taskId, { phase: TaskPhase.SAVE_PROJECT, progress: 80 });
      onProgress({ phase: 'save-project', message: 'Saving project files...', progress: 80 });

      const projectPath = await this._saveProjectFiles(projectId, generationResult.files);

      // ── POST-GENERATION ENRICHMENT (non-fatal) ──────────────────────────
      // Injects data-axiom-* attributes and generates axiom-features.json.
      // Runs in its own try/catch: if it fails, generation still succeeds.
      try {
        onProgress({ phase: 'save-project', message: 'Building feature map...', progress: 87 });
        const enrichResult = await enrichProject(projectPath, generationResult.files, manifest);
        console.log(`[Orchestrator] Enrichment: ${enrichResult.enrichedCount} files tagged, feature map saved.`);
      } catch (enrichErr) {
        console.warn('[Orchestrator] Enrichment failed (non-fatal):', enrichErr.message);
      }

      // ── LOCAL GIT BASELINE (non-fatal) ──────────────────────────────────
      // Creates local git repo + initial commit so the editor has a diff baseline.
      // Does NOT push to GitHub. Push happens on "Commit to GitHub" or "Deploy".
      try {
        onProgress({ phase: 'save-project', message: 'Creating git baseline...', progress: 93 });
        await this._initLocalGit(projectPath);
      } catch (gitErr) {
        console.warn('[Orchestrator] Git init failed (non-fatal):', gitErr.message);
      }
      // ─────────────────────────────────────────────────────────────────────
      
      // Create project metadata
      const project = {
        id: projectId,
        name: manifest.name,
        description: manifest.description,
        manifestId,
        path: projectPath,
        files: generationResult.files.map(f => ({ path: f.path, language: f.language })),
        status: TaskState.GENERATED,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        metadata: {
          platform: manifest.platform || 'web',
          deployment: manifest.deployment || { strategy: 'vercel' },
          techStack: manifest.techStack,
          generationStats: generationResult.stats,
          envTemplate: manifest.envVars || []
        }
      };

      task.logs.push({ time: Date.now(), message: `Project saved to: ${projectPath}` });

      // Update task
      this._updateTask(taskId, { 
        state: TaskState.GENERATED,
        phase: TaskPhase.AWAIT_CONFIGURATION,
        progress: 100,
        result: { project, generationResult }
      });

      const finalProject = {
        id: projectId,
        name: manifest.name || 'Unnamed Project',
        description: manifest.description || '',
        status: TaskState.READY,
        metadata: {
          ...manifest.meta,
          filesGenerated: manifest.files?.length || 0,
          techStack: manifest.techStack
        },
        updatedAt: new Date().toISOString()
      };

      this.emit('task:completed', { taskId, result: finalProject });
      onProgress({ 
        phase: 'complete', 
        message: 'Generation complete! Project is ready.', 
        project: finalProject, 
        progress: 100 
      });

      return { taskId, project: finalProject, generationResult };

    } catch (error) {
      this._updateTask(taskId, { 
        state: TaskState.ERROR,
        error: error.message,
        progress: 0
      });
      
      task.logs.push({ time: Date.now(), message: `Error: ${error.message}`, level: 'error' });
      
      this.emit('task:error', { taskId, error });
      throw error;
    }
  }

  /**
   * Configure project with environment variables
   */
  async configureProject(projectId, envVars) {
    const taskId = this._createTaskId();
    
    const task = {
      id: taskId,
      projectId,
      state: TaskState.CONFIGURED,
      phase: TaskPhase.AWAIT_CONFIGURATION,
      startTime: Date.now(),
      logs: []
    };

    this.activeTasks.set(taskId, task);

    try {
      // Store each env var securely
      const stored = [];
      for (const [key, value] of Object.entries(envVars)) {
        if (value && value.trim()) {
          await this.securityManager.storeToken(`env-${projectId}-${key}`, value);
          stored.push(key);
        }
      }

      // Create .env file in project directory
      const projectPath = path.join(this.projectStoragePath, projectId);
      const envContent = Object.entries(envVars)
        .map(([key, value]) => `${key}=${value}`)
        .join('\n');
      
      await fs.writeFile(path.join(projectPath, '.env.local'), envContent, 'utf-8');

      task.logs.push({ time: Date.now(), message: `Configured ${stored.length} environment variables` });

      this.emit('task:configured', { taskId, projectId, stored });

      return { taskId, stored, count: stored.length };

    } catch (error) {
      task.state = TaskState.ERROR;
      task.error = error.message;
      task.logs.push({ time: Date.now(), message: `Error: ${error.message}`, level: 'error' });
      
      this.emit('task:error', { taskId, error });
      throw error;
    }
  }

  /**
   * Start deployment to GitHub + Hosting Provider (Vercel)
   */
  async startDeployment(projectId, projectMetadata = {}, onProgress = () => {}) {
    const taskId = this._createTaskId();
    
    const task = {
      id: taskId,
      projectId,
      state: TaskState.DEPLOYING,
      phase: TaskPhase.PUSH_GITHUB,
      progress: 0,
      startTime: Date.now(),
      logs: [],
      result: null,
      error: null
    };

    this.activeTasks.set(taskId, task);
    this.emit('task:started', { taskId, type: 'deployment' });

    try {
      // Get project info
      const projectPath = path.join(this.projectStoragePath, projectId);
      
      // Phase 1: Push to GitHub
      this._updateTask(taskId, { phase: TaskPhase.PUSH_GITHUB, progress: 10 });
      onProgress({ phase: 'push-github', message: 'Pushing to GitHub...' });

      const githubResult = await this.deployment.pushToGitHub(projectId, projectPath, (progress) => {
        this._updateTask(taskId, { progress: 10 + (progress * 0.4) });
        onProgress({ ...progress, progress: 10 + (progress * 0.4) });
      });

      task.logs.push({ time: Date.now(), message: `Pushed to GitHub: ${githubResult.repoUrl}` });

      // Phase 2: Deploy to Provider (ONLY FOR WEB PROJECTS)
      const platform = projectMetadata?.techStack?.platform || 'web';
      let deployResult = null;
      let verified = false;

      if (platform === 'web') {
        const provider = projectMetadata?.deployment?.provider || 'vercel';
        this._updateTask(taskId, { phase: `deploy-${provider}`, progress: 50 });
        onProgress({ phase: 'deploying', message: `Deploying to ${provider}...` });

        deployResult = await this.deployment.deploy(
          provider,
          projectId, 
          githubResult,
          (progress) => {
            this._updateTask(taskId, { progress: 50 + (progress * 0.4) });
            onProgress({ ...progress, progress: 50 + (progress * 0.4) });
          }
        );

        task.logs.push({ time: Date.now(), message: `Deployed to ${provider}: ${deployResult.url}` });

        // Phase 3: Verify Deployment
        this._updateTask(taskId, { phase: TaskPhase.VERIFY_DEPLOYMENT, progress: 90 });
        onProgress({ phase: 'verify-deployment', message: 'Verifying deployment...' });

        verified = await this.deployment.verifyDeployment(deployResult.url);

        if (verified) {
          task.logs.push({ time: Date.now(), message: 'Deployment verified successfully' });
        } else {
          task.logs.push({ time: Date.now(), message: 'Deployment verification pending', level: 'warn' });
        }
      } else {
        task.logs.push({ time: Date.now(), message: `Deployment to web hosting skipped (Platform: ${platform})` });
        onProgress({ phase: 'complete', message: `Web hosting skipped for ${platform} apps.` });
      }

      // Update task
      this._updateTask(taskId, { 
        state: TaskState.LIVE,
        phase: 'complete',
        progress: 100,
        result: {
          github: githubResult,
          deployment: deployResult,
          verified
        }
      });

      this.emit('task:deployed', { 
        taskId, 
        projectId,
        urls: {
          repo: githubResult.repoUrl,
          site: deployResult.url
        }
      });

      onProgress({ 
        phase: 'complete', 
        message: 'Deployment complete!',
        urls: {
          repo: githubResult.repoUrl,
          site: deployResult.url
        }
      });

      return {
        taskId,
        github: githubResult,
        deployment: deployResult,
        verified
      };

    } catch (error) {
      this._updateTask(taskId, { 
        state: TaskState.ERROR,
        error: error.message,
        progress: 0
      });
      
      task.logs.push({ time: Date.now(), message: `Error: ${error.message}`, level: 'error' });
      
      this.emit('task:error', { taskId, error });
      throw error;
    }
  }

  /**
   * Stop an active task
   */
  async stopTask(taskId) {
    const task = this.activeTasks.get(taskId);
    
    if (!task) {
      throw new OrchestratorError('TASK_NOT_FOUND', `Task not found: ${taskId}`);
    }

    if (task.state === TaskState.GENERATING) {
      // Stop Ollama generation
      // Note: Ollama doesn't have a direct stop API, but we can mark task as stopped
    }

    task.state = TaskState.STOPPED;
    task.stoppedAt = Date.now();
    
    this.emit('task:stopped', { taskId });
    
    return { success: true };
  }

  /**
   * Get task status
   */
  async getTaskStatus(taskId) {
    const task = this.activeTasks.get(taskId);
    
    if (!task) {
      throw new OrchestratorError('TASK_NOT_FOUND', `Task not found: ${taskId}`);
    }

    return {
      id: task.id,
      state: task.state,
      phase: task.phase,
      progress: task.progress,
      startTime: task.startTime,
      logs: task.logs,
      error: task.error
    };
  }

  /**
   * Get active task by project ID
   */
  getActiveTaskByProject(projectId) {
    return Array.from(this.activeTasks.values()).find(task => 
      task.projectId === projectId && 
      ![TaskState.LIVE, TaskState.ERROR, TaskState.STOPPED].includes(task.state)
    );
  }

  /**
   * Get all active tasks
   */
  getActiveTasks() {
    return Array.from(this.activeTasks.values()).map(task => ({
      id: task.id,
      projectId: task.projectId,
      state: task.state,
      phase: task.phase,
      progress: task.progress
    }));
  }

  /**
   * Clean up completed tasks
   */
  cleanupTasks(maxAge = 24 * 60 * 60 * 1000) { // 24 hours
    const now = Date.now();
    
    for (const [taskId, task] of this.activeTasks) {
      const isComplete = [
        TaskState.LIVE,
        TaskState.ERROR,
        TaskState.STOPPED
      ].includes(task.state);
      
      const isOld = (now - task.startTime) > maxAge;
      
      if (isComplete && isOld) {
        this.activeTasks.delete(taskId);
      }
    }
  }
}

// ==================== CUSTOM ERROR CLASS ====================
export class OrchestratorError extends Error {
  constructor(code, message) {
    super(message);
    this.name = 'OrchestratorError';
    this.code = code;
  }
}

export default TaskOrchestrator;
