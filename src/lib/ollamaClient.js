/**
 * Axiom Forge - Ollama Client
 * 
 * Handles all communication with local Ollama instance for code generation.
 * Supports sequential file generation with context injection.
 */

import fetch from 'node-fetch';
import EventEmitter from 'events';

// ==================== CONSTANTS ====================
const DEFAULT_HOST = 'http://127.0.0.1:11434';
const DEFAULT_MODEL = 'llama3.2:1b';
const DEFAULT_TIMEOUT = 120000; // 2 minutes
const MAX_RETRIES = 3;
const RETRY_DELAY = 1000;

// ==================== OLLAMA CLIENT CLASS ====================
export class OllamaClient extends EventEmitter {
  constructor(options = {}) {
    super();
    this.host = options.host || DEFAULT_HOST;
    this.model = options.model || DEFAULT_MODEL;
    this.timeout = options.timeout || DEFAULT_TIMEOUT;
    this.contextWindow = []; // Conversation context
    this.maxContextLength = options.maxContextLength || 10;
  }

  // ==================== PRIVATE METHODS ====================

  /**
   * Make API request to Ollama
   */
  async _request(endpoint, body, options = {}) {
    const url = `${this.host}/api/${endpoint}`;
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await fetch(url, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(body),
        signal: controller.signal,
        ...options
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        const errorText = await response.text();
        throw new OllamaError(
          `HTTP ${response.status}`,
          `Ollama API error: ${errorText}`
        );
      }

      return await response.json();
    } catch (error) {
      clearTimeout(timeoutId);
      
      if (error.name === 'AbortError') {
        throw new OllamaError('TIMEOUT', 'Request timed out');
      }
      
      throw error;
    }
  }

  /**
   * Retry wrapper for requests
   */
  async _retryRequest(endpoint, body, retries = MAX_RETRIES) {
    let lastError;
    
    for (let i = 0; i < retries; i++) {
      try {
        return await this._request(endpoint, body);
      } catch (error) {
        lastError = error;
        
        if (i < retries - 1) {
          this.emit('retry', { attempt: i + 1, error: error.message });
          await this._sleep(RETRY_DELAY * (i + 1));
        }
      }
    }
    
    throw lastError;
  }

  /**
   * Sleep utility
   */
  _sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Build context from previous messages
   */
  _buildContext() {
    if (this.contextWindow.length === 0) {
      return '';
    }

    return this.contextWindow
      .map(msg => `${msg.role === 'user' ? 'User' : 'Assistant'}: ${msg.content}`)
      .join('\n\n');
  }

  /**
   * Add message to context window
   */
  _addToContext(role, content) {
    this.contextWindow.push({ role, content, timestamp: Date.now() });
    
    // Trim context if too long
    if (this.contextWindow.length > this.maxContextLength) {
      this.contextWindow = this.contextWindow.slice(-this.maxContextLength);
    }
  }

  /**
   * Clean generated code (remove markdown fences, etc.)
   */
  _cleanCode(code, language = '') {
    let cleaned = code.trim();
    
    // Remove markdown code fences
    const fenceRegex = new RegExp(`^\`\`\`${language}\\s*\\n?|\\n?\`\`\`$`, 'gi');
    cleaned = cleaned.replace(fenceRegex, '');
    
    // Remove common prefixes
    cleaned = cleaned.replace(/^(Here is|Here's|This is)\s+(the\s+)?(code|implementation):?\s*/i, '');
    
    return cleaned.trim();
  }

  // ==================== PUBLIC API ====================

  /**
   * Check if Ollama is available
   */
  async healthCheck() {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 3000); // 3 second timeout for health check

    try {
      console.log(`[Ollama] Checking health at ${this.host}/api/tags...`);
      const response = await fetch(`${this.host}/api/tags`, {
        method: 'GET',
        signal: controller.signal
      });
      
      if (!response.ok) {
        return { available: false, error: 'Ollama not responding correctly' };
      }
      
      const data = await response.json();
      const models = data.models || [];
      const hasModel = models.some(m => m.name === this.model || m.name.startsWith(this.model));
      
      console.log(`[Ollama] Health check result:`, { available: true, hasModel, modelCount: models.length });

      return {
        available: true,
        hasModel,
        models: models.map(m => m.name),
        message: hasModel ? 'Ready' : `Model '${this.model}' not found.`
      };
    } catch (error) {
      const isTimeout = error.name === 'AbortError';
      const errorMsg = isTimeout ? 'Ollama connection timed out (3s)' : error.message;
      console.error(`[Ollama] Health check failed: ${errorMsg}`);
      
      return {
        available: false,
        error: errorMsg,
        message: isTimeout ? 'Ollama is busy or not responding' : 'Ollama not running. Please start Ollama.'
      };
    } finally {
      clearTimeout(timeoutId);
    }
  }

  /**
   * Pull a model with streaming progress
   */
  async pullModel(modelName = this.model, onProgress = () => {}) {
    this.emit('pull:start', { model: modelName });
    onProgress({ status: 'Connecting to Ollama to request model pull...' });
    
    try {
      const response = await fetch(`${this.host}/api/pull`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: modelName, stream: true })
      });

      if (!response.ok) {
        throw new OllamaError('PULL_FAILED', `Failed to pull model: ${modelName}. Status: ${response.status}`);
      }

      onProgress({ status: 'Download stream established. Receiving data...' });

      // Read the NDJSON stream
      const reader = response.body;
      let lastPercent = -1;

      for await (const chunk of reader) {
        const lines = chunk.toString().split('\n').filter(l => l.trim());
        
        for (const line of lines) {
          try {
            const data = JSON.parse(line);
            
            if (data.status === 'success') {
              this.emit('pull:complete', { model: modelName });
              return { success: true };
            }

            if (data.total && data.completed) {
              const percent = Math.round((data.completed / data.total) * 100);
              if (percent !== lastPercent) {
                lastPercent = percent;
                this.emit('pull:progress', { 
                  model: modelName, 
                  percent, 
                  status: data.status,
                  completed: data.completed,
                  total: data.total
                });
                onProgress({ percent, status: data.status });
              }
            } else {
              this.emit('pull:status', { status: data.status });
              onProgress({ status: data.status });
            }
          } catch (e) {
            // Skip partial/malformed JSON chunks
          }
        }
      }

      return { success: true };
    } catch (error) {
      this.emit('pull:error', { model: modelName, error: error.message });
      throw error;
    }
  }

  /**
   * Generate a single response via Chat API (Streaming)
   */
  async chat(prompt, options = {}, onProgress = () => {}) {
    const systemPrompt = options.system || '';
    
    this.emit('generate:start', { prompt: prompt.slice(0, 100) + '...' });
    onProgress({ status: 'AI is thinking and preparing code...' });

    try {
      const response = await fetch(`${this.host}/api/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: options.model || this.model,
          messages: [
            { role: 'system', content: systemPrompt },
            { role: 'user', content: prompt }
          ],
          stream: true,
          options: {
            temperature: options.temperature ?? 0.3,
            num_predict: options.maxTokens ?? 4096,
            top_p: options.topP ?? 0.9,
            ...options.params
          }
        })
      });

      if (!response.ok) {
        throw new OllamaError('GENERATE_FAILED', `Ollama generation failed: ${response.status}`);
      }

      const reader = response.body;
      let fullContent = '';
      let tokenCount = 0;

      for await (const chunk of reader) {
        const lines = chunk.toString().split('\n').filter(l => l.trim());
        for (const line of lines) {
          try {
            const data = JSON.parse(line);
            
            if (data.message && data.message.content) {
              const content = data.message.content;
              fullContent += content;
              tokenCount++;
              
              // Periodic logging
              if (tokenCount % 20 === 0) {
                onProgress({ status: `AI is typing... (${tokenCount} tokens generated)` });
              }
            }

            if (data.done) {
              // [Zero-Memory Optimization] We no longer save history to context window.
              // this._addToContext('user', prompt);
              // this._addToContext('assistant', fullContent);
              
              this.emit('generate:complete', { 
                content: fullContent.slice(0, 100) + '...',
                tokens: data.eval_count 
              });

              return {
                content: fullContent,
                tokens: data.eval_count,
                totalDuration: data.total_duration
              };
            }
          } catch (e) {
            // Skip partial chunks
          }
        }
      }

      return { content: fullContent, tokens: tokenCount };
    } catch (error) {
      this.emit('generate:error', { error: error.message });
      throw error;
    }
  }

  /**
   * Generate code for a specific file
   */
  async generateFile(fileSpec, projectContext = {}, options = {}, onProgress = () => {}) {
    const {
      path: filePath,
      description,
      language,
      dependencies = []
    } = fileSpec;

    const platform = projectContext.platform || 'web';
    
    let platformInstructions = '';
    if (platform === 'mobile') {
      platformInstructions = '- You are building a Mobile app (React Native / Expo).\n- Use React Native core components (View, Text, StyleSheet) instead of HTML tags.\n- Avoid web-only APIs (window, document).';
    } else if (platform === 'desktop') {
      platformInstructions = '- You are building a Desktop app (Electron).\n- Use Node.js integration cautiously.\n- Ensure IPC boundaries are respected if requested.';
    } else {
      platformInstructions = '- You are building a Web application.\n- Ensure responsive design and accessible HTML.';
    }

    const systemPrompt = `You are an expert software engineer. Generate clean, production-ready code.
Follow these guidelines:
- Write only the code, no explanations
- Use modern best practices
- Include necessary imports
- Add JSDoc comments for functions
- Handle errors appropriately
${platformInstructions}
- Follow the existing code style if context is provided`;

    // [Dependency-Aware Architecture] Build context ONLY from internal dependencies
    let contextStr = '';
    if (fileSpec.internalDependencies && fileSpec.internalDependencies.length > 0) {
      const depsCode = fileSpec.internalDependencies
        .map(depPath => {
           const found = projectContext.generatedFiles?.find(f => f.path === depPath);
           return found ? `// --- FILE: ${found.path} ---\n${found.content}` : null;
        })
        .filter(Boolean)
        .join('\n\n');
        
      if (depsCode) {
         contextStr = `CODE OF INTERNAL DEPENDENCIES YOU MUST USE/IMPORT:\n${depsCode}\n\n`;
      }
    }

    const depsStr = dependencies.length > 0 
      ? `\nDependencies: ${dependencies.join(', ')}` 
      : '';
    const projectStr = Object.keys(projectContext).length > 0
      ? `\nProject Context:\n${JSON.stringify({
          projectName: projectContext.projectName,
          platform: projectContext.platform,
          techStack: projectContext.techStack
        }, null, 2)}`
      : '';

    const prompt = `Generate ${language} code for file: ${filePath}

Description: ${description}${depsStr}${projectStr}

${contextStr ? `Previous context:\n${contextStr}\n\n` : ''}
Provide only the code, wrapped in \`\`\`${language} blocks:`;

    const result = await this.chat(prompt, {
      system: systemPrompt,
      temperature: 0.3, // Lower temperature for code
      maxTokens: 4096,
      ...options
    }, onProgress);

    return {
      path: filePath,
      content: this._cleanCode(result.content, language),
      language,
      tokens: result.tokens
    };
  }

  /**
   * Generate multiple files sequentially with context
   */
  async generateProject(manifest, onProgress = () => {}) {
    const { name, description, files, techStack = {} } = manifest;
    
    if (!files || !Array.isArray(files)) {
     throw new Error('CORRUPT_MANIFEST: No files found in manifest. It might still be encrypted or the cloud server is outdated.');
    }

    const generatedFiles = [];
    const errors = [];

    this.emit('project:start', { name, fileCount: files.length });
    onProgress({ phase: 'start', total: files.length, current: 0, message: `Starting generation: ${name} (${files.length} files)` });

    // Reset context for new project
    onProgress({ phase: 'init', message: 'Resetting AI conversation context...' });
    this.contextWindow = [];

    // Add project context
    onProgress({ phase: 'context', message: 'Feeding project specifications to the AI engine...' });
    this._addToContext('user', `Project: ${name}\nDescription: ${description}\nTech Stack: ${JSON.stringify(techStack)}`);

    onProgress({ phase: 'ready', message: 'AI Engine context initialized. Starting file sequence...' });

    for (let i = 0; i < files.length; i++) {
      const fileSpec = files[i];
      
      this.emit('file:start', { 
        path: fileSpec.path, 
        index: i + 1, 
        total: files.length 
      });
      
      onProgress({
        phase: 'generating',
        file: fileSpec.path,
        current: i + 1,
        total: files.length,
        progress: ((i + 1) / files.length) * 100
      });

      try {
        const generated = await this.generateFile(fileSpec, {
          projectName: name,
          platform: manifest.platform || 'web',
          description,
          techStack,
          generatedFiles: generatedFiles.map(f => ({ path: f.path, description: f.description, content: f.content }))
        }, {}, (generationProgress) => {
          onProgress({
            phase: 'generating',
            file: fileSpec.path,
            current: i + 1,
            total: files.length,
            progress: ((i + (generationProgress.percent ? generationProgress.percent / 100 : 0)) / files.length) * 100,
            message: generationProgress.status
          });
        });

        generatedFiles.push(generated);
        
        this.emit('file:complete', { 
          path: fileSpec.path, 
          tokens: generated.tokens 
        });
      } catch (error) {
        errors.push({ file: fileSpec.path, error: error.message });
        
        this.emit('file:error', { 
          path: fileSpec.path, 
          error: error.message 
        });
        
        // Continue with other files
      }
    }

    const result = {
      name,
      description,
      files: generatedFiles,
      errors,
      stats: {
        total: files.length,
        successful: generatedFiles.length,
        failed: errors.length,
        totalTokens: generatedFiles.reduce((sum, f) => sum + (f.tokens || 0), 0)
      }
    };

    this.emit('project:complete', result);
    onProgress({ phase: 'complete', ...result });

    return result;
  }

  /**
   * Generate environment variable template
   */
  async generateEnvTemplate(projectDescription, techStack = {}) {
    const prompt = `Based on this project description, generate a list of required environment variables:

Project: ${projectDescription}
Tech Stack: ${JSON.stringify(techStack)}

List each variable with:
- Name
- Description  
- Whether it's required
- Example value
- Help URL for obtaining the key (if applicable)

Format as JSON array:`;

    const result = await this.generate(prompt, {
      temperature: 0.3,
      maxTokens: 2048
    });

    try {
      // Try to parse JSON from response
      const jsonMatch = result.content.match(/\[[\s\S]*\]/);
      if (jsonMatch) {
        return JSON.parse(jsonMatch[0]);
      }
      return [];
    } catch (error) {
      return [];
    }
  }

  /**
   * Clear conversation context
   */
  clearContext() {
    this.contextWindow = [];
    this.emit('context:cleared');
  }

  /**
   * Get current context
   */
  getContext() {
    return [...this.contextWindow];
  }

  /**
   * Update configuration
   */
  configure(options) {
    if (options.host) this.host = options.host;
    if (options.model) this.model = options.model;
    if (options.timeout) this.timeout = options.timeout;
    if (options.maxContextLength) this.maxContextLength = options.maxContextLength;
    
    this.emit('configured', { host: this.host, model: this.model });
  }
}

// ==================== CUSTOM ERROR CLASS ====================
export class OllamaError extends Error {
  constructor(code, message) {
    super(message);
    this.name = 'OllamaError';
    this.code = code;
  }
}

// ==================== FACTORY FUNCTION ====================
export function createOllamaClient(options = {}) {
  return new OllamaClient(options);
}

export default OllamaClient;
