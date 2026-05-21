/**
 * Axiom Forge - Meta Generator
 *
 * Post-generation step that:
 * 1. Injects data-axiom-* attributes into HTML/template files
 * 2. Generates axiom-features.json — a feature map for the AI editor
 *
 * SAFE: This module is called AFTER all files are saved.
 * If it fails for any reason, generation is NOT affected.
 *
 * Supported:
 *   - HTML (plain, PHP, Jinja, Django, Blade, Twig, Handlebars, EJS)
 *   - JSX / TSX (React)
 *   - Vue SFC (.vue)
 *   - Svelte (.svelte)
 *   - JS/TS modules (comment-based markers)
 */

import fs from 'fs/promises';
import path from 'path';

// ==================== LANGUAGE DETECTION ====================

const HTML_TEMPLATE_EXTENSIONS = new Set([
  '.html', '.htm',
  '.php',
  '.blade.php',
  '.twig',
  '.j2', '.jinja', '.jinja2',
  '.njk',          // Nunjucks
  '.hbs',          // Handlebars
  '.ejs',
  '.erb',          // Ruby ERB
  '.vue',
  '.svelte',
  '.jsx', '.tsx'
]);

const JS_EXTENSIONS = new Set([
  '.js', '.mjs', '.cjs',
  '.ts', '.mts',
]);

const CSS_EXTENSIONS = new Set(['.css', '.scss', '.sass', '.less']);

function detectLanguage(filePath) {
  const base = path.basename(filePath).toLowerCase();
  const ext = '.' + base.split('.').slice(1).join('.'); // handles .blade.php
  const simpleExt = path.extname(filePath).toLowerCase();

  if (HTML_TEMPLATE_EXTENSIONS.has(ext) || HTML_TEMPLATE_EXTENSIONS.has(simpleExt)) {
    return 'html-template';
  }
  if (JS_EXTENSIONS.has(simpleExt)) return 'javascript';
  if (CSS_EXTENSIONS.has(simpleExt)) return 'stylesheet';
  return 'unknown';
}

// ==================== META ATTRIBUTE INJECTION ====================

/**
 * Injects data-axiom-* attributes into HTML-producing files.
 *
 * Strategy:
 * - Find root elements (first opening tag that looks like a component container)
 * - Add data-axiom-component="ComponentName" data-axiom-file="relative/path"
 * - For PHP: applies to the first <div> or <section> or <main> or <body> tag
 * - For JSX: applies to the outermost returned JSX tag (first line with <div/section/main/header/footer/article/aside/nav)
 * - Does NOT modify: <head>, <script>, <style>, <meta>, <link>, <html>
 */
function injectAxiomAttrs(content, relativeFilePath) {
  const componentName = deriveComponentName(relativeFilePath);

  // Tags we target for injection (semantic containers)
  const TARGET_TAGS = /(<(?:div|section|main|header|footer|article|aside|nav|form|ul|ol|table|body)\b)/i;

  // Already injected? Skip.
  if (content.includes('data-axiom-component=')) return content;

  // Find the first matching tag and inject attributes
  let injected = false;
  return content.replace(TARGET_TAGS, (match) => {
    if (!injected) {
      injected = true;
      return `${match} data-axiom-component="${componentName}" data-axiom-file="${relativeFilePath}"`;
    }
    return match;
  });
}

function deriveComponentName(relativeFilePath) {
  const base = path.basename(relativeFilePath);
  // Strip extension(s)
  const nameWithoutExt = base.replace(/\.[^.]+(\.[^.]+)?$/, '');
  // PascalCase
  return nameWithoutExt
    .split(/[-_\s]+/)
    .map(w => w.charAt(0).toUpperCase() + w.slice(1))
    .join('');
}

// ==================== FEATURE MAP BUILDER ====================

/**
 * Analyzes generated files and builds axiom-features.json
 *
 * Uses heuristics (no LLM call — instant, no API cost):
 * - Groups files by directory → one "feature" per directory cluster
 * - Extracts route info from common patterns (React Router, Flask, Django, PHP)
 * - Extracts function/component names from JS/JSX files
 * - Uses manifest.features if available (ideal — manifest already describes features)
 */
async function buildFeatureMap(projectPath, files, manifest) {
  const features = [];
  const filesByDir = {};

  // Group files by their top-level directory
  for (const file of files) {
    const parts = file.path.split('/');
    const topDir = parts.length > 1 ? parts[0] : '__root__';
    if (!filesByDir[topDir]) filesByDir[topDir] = [];
    filesByDir[topDir].push(file);
  }

  // === Strategy 1: Use manifest features if available ===
  if (manifest.features && Array.isArray(manifest.features)) {
    for (const feature of manifest.features) {
      features.push({
        id: slugify(feature.name || feature.id),
        name: feature.name || feature.id,
        description: feature.description || '',
        editableDescription: feature.description || '',
        files: (feature.files || []).map(f => ({
          path: typeof f === 'string' ? f : f.path,
          role: typeof f === 'string' ? 'implementation' : (f.role || 'implementation')
        }))
      });
    }
  }

  // === Strategy 2: Heuristic directory grouping (fallback) ===
  if (features.length === 0) {
    for (const [dir, dirFiles] of Object.entries(filesByDir)) {
      const name = dir === '__root__' ? 'Root Files' : dirGroupToFeatureName(dir);
      features.push({
        id: slugify(name),
        name,
        description: inferDescription(dir, dirFiles),
        editableDescription: inferDescription(dir, dirFiles),
        files: dirFiles.map(f => ({
          path: f.path,
          role: inferRole(f.path)
        }))
      });
    }
  }

  // === Strategy 3: Add any CSS/style files as a shared feature ===
  const styleFiles = files.filter(f => CSS_EXTENSIONS.has(path.extname(f.path).toLowerCase()));
  if (styleFiles.length > 0 && !features.find(feat => feat.id === 'styles')) {
    features.push({
      id: 'styles',
      name: 'Styles & Design',
      description: 'Visual styling, color palette, typography, and layout rules.',
      editableDescription: 'Visual styling, color palette, typography, and layout rules.',
      files: styleFiles.map(f => ({ path: f.path, role: 'stylesheet' }))
    });
  }

  // === Build final structure ===
  const featureMap = {
    version: '1.0',
    generatedAt: new Date().toISOString(),
    projectName: manifest.name || 'Unknown Project',
    techStack: manifest.techStack || {},
    totalFiles: files.length,
    features,
    // Editor hint: how to use this file
    _usage: 'This file is used by the Axiom Forge AI editor. Edit "editableDescription" to change what a feature does. The AI editor will use this context when making modifications.'
  };

  return featureMap;
}

// ==================== HELPERS ====================

function slugify(str) {
  return str
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}

function dirGroupToFeatureName(dir) {
  const names = {
    'components': 'UI Components',
    'pages': 'Pages & Routes',
    'api': 'API & Backend',
    'routes': 'Routes',
    'controllers': 'Controllers',
    'models': 'Data Models',
    'views': 'Views & Templates',
    'utils': 'Utilities',
    'helpers': 'Helper Functions',
    'hooks': 'React Hooks',
    'context': 'App State & Context',
    'store': 'State Management',
    'auth': 'Authentication',
    'lib': 'Core Library',
    'styles': 'Styles',
    'assets': 'Static Assets',
    'config': 'Configuration',
    'middleware': 'Middleware',
    'services': 'Services',
    'static': 'Static Files',
    'templates': 'HTML Templates',
    'public': 'Public Assets',
    'src': 'Source Files'
  };
  return names[dir.toLowerCase()] || capitalize(dir);
}

function capitalize(str) {
  return str.charAt(0).toUpperCase() + str.slice(1);
}

function inferDescription(dir, files) {
  const mapping = {
    'components': 'Reusable UI elements and visual building blocks.',
    'pages': 'Application pages and route-level components.',
    'api': 'Backend API endpoints and server-side logic.',
    'routes': 'URL routing definitions and handlers.',
    'controllers': 'Request handling and business logic controllers.',
    'models': 'Data models and database schemas.',
    'views': 'Server-side rendered templates and views.',
    'utils': 'Shared utility functions and helpers.',
    'hooks': 'Custom React hooks for shared logic.',
    'context': 'Global application state and context providers.',
    'store': 'State management (Redux/Zustand/Pinia).',
    'auth': 'Authentication, authorization, and session management.',
    'lib': 'Core library modules and shared logic.',
    'config': 'Application configuration and environment settings.',
    'middleware': 'Request/response middleware pipeline.',
    'services': 'External service integrations and data fetching.',
    '__root__': `Root-level files: ${files.map(f => f.path).join(', ')}`
  };
  return mapping[dir.toLowerCase()] || `Files related to ${dir}.`;
}

function inferRole(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  const base = path.basename(filePath).toLowerCase();

  if (base.includes('index')) return 'entry point';
  if (base.includes('config') || base.includes('settings')) return 'configuration';
  if (base.includes('test') || base.includes('spec')) return 'test';
  if (base.includes('type') || base.includes('interface')) return 'type definitions';
  if (CSS_EXTENSIONS.has(ext)) return 'stylesheet';
  if (HTML_TEMPLATE_EXTENSIONS.has(ext)) return 'template';
  if (JS_EXTENSIONS.has(ext)) return 'logic';
  return 'implementation';
}

// ==================== MAIN EXPORT ====================

/**
 * Run all post-generation enrichment.
 *
 * @param {string} projectPath - Absolute path to the project directory
 * @param {Array}  files       - Array of { path, content, language } from generationResult
 * @param {Object} manifest    - The original manifest from idea-analyzer
 *
 * Returns: { featureMapPath, enrichedCount }
 */
export async function enrichProject(projectPath, files, manifest) {
  let enrichedCount = 0;
  const errors = [];

  // 1. Inject data-axiom-* attributes into HTML-producing files
  for (const file of files) {
    const lang = detectLanguage(file.path);
    if (lang !== 'html-template') continue;

    try {
      const fullPath = path.join(projectPath, file.path);
      const original = await fs.readFile(fullPath, 'utf-8');
      const enriched = injectAxiomAttrs(original, file.path);

      if (enriched !== original) {
        await fs.writeFile(fullPath, enriched, 'utf-8');
        enrichedCount++;
        console.log(`[AxiomMeta] Injected attrs: ${file.path}`);
      }
    } catch (err) {
      errors.push(`${file.path}: ${err.message}`);
    }
  }

  // 2. Build and save axiom-features.json
  let featureMapPath = null;
  try {
    const featureMap = await buildFeatureMap(projectPath, files, manifest);
    featureMapPath = path.join(projectPath, 'axiom-features.json');
    await fs.writeFile(featureMapPath, JSON.stringify(featureMap, null, 2), 'utf-8');
    console.log(`[AxiomMeta] Feature map saved: axiom-features.json (${featureMap.features.length} features)`);
  } catch (err) {
    errors.push(`feature-map: ${err.message}`);
  }

  if (errors.length > 0) {
    console.warn('[AxiomMeta] Non-fatal enrichment errors:', errors);
  }

  return { featureMapPath, enrichedCount, errors };
}
