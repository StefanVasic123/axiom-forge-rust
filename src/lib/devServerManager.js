/**
 * Axiom Forge - Dev Server Manager
 * 
 * Manages the local development server (spawn, kill, stream logs)
 * Supports different platforms: Web, Mobile, Desktop.
 */

import { spawn } from 'child_process';
import path from 'path';
import net from 'net';

export class DevServerManager {
  constructor() {
    this.process = null;
    this.projectId = null;
    this.cdpPort = null;
    this.listeners = new Set();
  }

  async getFreePort() {
    return new Promise((resolve, reject) => {
      const server = net.createServer();
      server.listen(0, '127.0.0.1', () => {
        const port = server.address().port;
        server.close(() => resolve(port));
      });
      server.on('error', reject);
    });
  }

  /**
   * Start the dev server for a project
   * @param {string} projectId 
   * @param {string} projectPath 
   * @param {string} platform - 'web', 'mobile', or 'desktop'
   * @param {string} techStack - 'nextjs', 'react', 'php', etc.
   */
  async start(projectId, projectPath, platform = 'web', techStack = 'nextjs') {
    if (this.process) {
      if (this.projectId === projectId) {
        this.emitLog('Server is already running for this project.\n');
        return this.cdpPort;
      }
      this.stop();
    }

    this.projectId = projectId;
    this.cdpPort = null;
    
    // Determine command based on platform and techStack
    let command = 'npm';
    let args = ['run', 'dev'];

    if (platform === 'mobile') {
      command = 'npx';
      args = ['expo', 'start'];
    } else if (platform === 'desktop') {
      command = 'npm';
      this.cdpPort = await this.getFreePort();
      args = ['start', '--', `--remote-debugging-port=${this.cdpPort}`];
    } else if (techStack === 'php') {
      command = 'php';
      args = ['-S', 'localhost:8000'];
    } else if (techStack === 'python' || techStack === 'django') {
      command = 'python';
      args = ['manage.py', 'runserver'];
    }

    this.emitLog(`> Starting ${platform} server: ${command} ${args.join(' ')}\n`);
    this.emitLog(`> Directory: ${projectPath}\n`);

    try {
      this.process = spawn(command, args, {
        cwd: projectPath,
        shell: true,
        env: { ...process.env, FORCE_COLOR: '1' }
      });

      this.process.stdout.on('data', (data) => {
        this.emitLog(data.toString());
      });

      this.process.stderr.on('data', (data) => {
        this.emitLog(data.toString(), 'error');
      });

      this.process.on('close', (code) => {
        this.emitLog(`\n> Server process exited with code ${code}\n`);
        this.process = null;
        this.projectId = null;
        this.emitStatus('stopped');
      });

      this.process.on('error', (err) => {
        this.emitLog(`\n> Failed to start server: ${err.message}\n`, 'error');
        this.process = null;
        this.projectId = null;
        this.emitStatus('stopped');
      });

      this.emitStatus('running');
      return this.cdpPort;

    } catch (err) {
      this.emitLog(`\n> Error spawning server: ${err.message}\n`, 'error');
      return null;
    }
  }

  stop() {
    if (this.process) {
      this.emitLog('\n> Stopping server...\n');
      
      // On Windows, killing the shell wrapper doesn't kill child processes.
      // We use taskkill if on Windows.
      if (process.platform === 'win32') {
        spawn('taskkill', ['/pid', this.process.pid, '/f', '/t']);
      } else {
        this.process.kill('SIGTERM');
      }
      
      this.process = null;
      this.projectId = null;
      this.cdpPort = null;
      this.emitStatus('stopped');
    }
  }

  isRunning(projectId) {
    return this.process !== null && this.projectId === projectId;
  }

  onLog(callback) {
    this.listeners.add(callback);
    return () => this.listeners.delete(callback);
  }

  emitLog(text, type = 'info') {
    for (const callback of this.listeners) {
      callback({ text, type });
    }
  }

  emitStatus(status) {
    for (const callback of this.listeners) {
      callback({ status });
    }
  }
}
