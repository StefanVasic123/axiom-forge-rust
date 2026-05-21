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
  const div = document.createElement('div');
  div.style.position = 'fixed';
  div.style.top = '0';
  div.style.left = '0';
  div.style.width = '100vw';
  div.style.height = '100vh';
  div.style.backgroundColor = 'rgba(255,0,0,0.9)';
  div.style.color = 'white';
  div.style.zIndex = '999999';
  div.style.padding = '2rem';
  div.style.fontFamily = 'monospace';
  div.style.overflow = 'auto';
  div.innerHTML = `<h2>CRASH LOG</h2><pre style="white-space: pre-wrap;">${msg}\n\n${err}</pre>`;
  document.body.appendChild(div);
};

window.addEventListener('error', (e) => {
  displayError('Uncaught Error', e.error?.stack || e.message);
});
window.addEventListener('unhandledrejection', (e) => {
  displayError('Unhandled Promise Rejection', e.reason?.stack || e.reason);
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
