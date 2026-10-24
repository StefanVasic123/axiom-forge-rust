/**
 * Axiom Forge - Main Entry Point
 */

import React from 'react';
import ReactDOM from 'react-dom/client';
import { HashRouter, Routes, Route } from 'react-router-dom';
import App from './App';
import FloatingProgress from './components/FloatingProgress';
import './styles/index.css';
import './lib/ipc-bridge';

// ================= GLOBAL ERROR CATCHER =================
const displayError = (msg, err) => {
  let existing = document.getElementById('axiom-crash-overlay');
  if (existing) existing.remove();
  const div = document.createElement('div');
  div.id = 'axiom-crash-overlay';
  div.style.position = 'fixed';
  div.style.top = '0';
  div.style.left = '0';
  div.style.width = '100vw';
  div.style.height = '100vh';
  div.style.backgroundColor = 'rgba(220, 38, 38, 0.95)';
  div.style.color = 'white';
  div.style.zIndex = '999999';
  div.style.padding = '2rem';
  div.style.fontFamily = 'monospace';
  div.style.overflow = 'auto';
  div.innerHTML = `
    <div style="display: flex; justify-content: space-between; align-items: center; border-bottom: 1px solid rgba(255,255,255,0.3); padding-bottom: 1rem; margin-bottom: 1.5rem;">
      <h2 style="margin: 0; font-size: 1.25rem;">CRASH LOG</h2>
      <button id="close-crash-log" style="background: white; color: #dc2626; border: none; padding: 0.4rem 1rem; border-radius: 4px; font-weight: bold; cursor: pointer;">Zatvori i nastavi rad</button>
    </div>
    <pre style="white-space: pre-wrap; font-size: 0.85rem; line-height: 1.5;">${msg}\n\n${err}</pre>
  `;
  document.body.appendChild(div);
  document.getElementById('close-crash-log')?.addEventListener('click', () => div.remove());
};

const isBenignError = (errStr) => {
  if (!errStr) return false;
  return errStr.includes('InstantiationService has been disposed')
    || errStr.includes('ResizeObserver loop')
    || errStr.includes('Canceled')
    || errStr.includes('model was disposed')
    || errStr.includes('disposed');
};

window.addEventListener('error', (e) => {
  const errMsg = e.error?.stack || e.error?.message || e.message || '';
  if (isBenignError(errMsg)) {
    e.preventDefault();
    return;
  }
  displayError('Uncaught Error', errMsg);
});

window.addEventListener('unhandledrejection', (e) => {
  const reasonMsg = e.reason?.stack || e.reason?.message || String(e.reason || '');
  if (isBenignError(reasonMsg)) {
    e.preventDefault();
    return;
  }
  displayError('Unhandled Promise Rejection', reasonMsg);
});
// ========================================================

// Determine which component to render based on route
const isFloatingWindow = window.location.hash.includes('/floating') || 
                         window.location.hash.includes('floating') ||
                         window.location.pathname.includes('/floating');

ReactDOM.createRoot(document.getElementById('root')).render(
  <React.StrictMode>
    {isFloatingWindow ? (
      <FloatingProgress />
    ) : (
      <HashRouter>
        <App />
      </HashRouter>
    )}
  </React.StrictMode>
);
