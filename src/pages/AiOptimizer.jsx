import React, { useState, useEffect } from 'react';

export default function AiOptimizer() {
  const [status, setStatus] = useState("Sistem spreman za rad.");
  const [config, setConfig] = useState({ aggressiveness: 0.8, balance: 0.5, safeMode: true });
  const [customDirective, setCustomDirective] = useState("");
  const [usageStats, setUsageStats] = useState({ totalTokens: 0, model: "llama3.2:1b" });

  useEffect(() => {
    // 1. Učitaj trenutnu konfiguraciju iz main procesa
    window.electronAPI?.ipcRenderer?.invoke('ik:get-config').then(res => {
      if (res) setConfig(res);
    });

    // Učitavamo sačuvane direktive iz lokalne memorije/baze
    window.electronAPI?.ipcRenderer?.invoke('ik:get-custom-directive').then(res => {
      if (res) setCustomDirective(res);
    });

    // 2. Slušaj Live Status poruke iz Firewall-a
    const unsubs = [];
    if (window.electronAPI?.ipcRenderer) {
      const unsubStatus = window.electronAPI.ipcRenderer.on('ik:status', (msg) => {
        setStatus(msg);
      });
      unsubs.push(unsubStatus);

      // 3. Slušaj Audit rezultate da prikupimo potrošnju tokena
      const unsubAudit = window.electronAPI.ipcRenderer.on('ik:audit', (metrics) => {
        if (metrics?.semanticInsights?.usageMetadata) {
          setUsageStats(prev => ({
            ...prev,
            totalTokens: prev.totalTokens + metrics.semanticInsights.usageMetadata.total_tokens
          }));
        }
      });
      unsubs.push(unsubAudit);
    }

    return () => {
      unsubs.forEach(unsub => {
        if (typeof unsub === 'function') unsub();
      });
    };
  }, []);

  const handleConfigChange = (key, value) => {
    const newConfig = { ...config, [key]: value };
    setConfig(newConfig);
    // Šaljemo novu konfiguraciju nazad u Firewall
    window.electronAPI?.ipcRenderer?.invoke('ik:set-config', newConfig);
  };

  const handleDirectiveChange = (e) => {
    const val = e.target.value;
    setCustomDirective(val);
    window.electronAPI?.ipcRenderer?.invoke('ik:set-custom-directive', val);
  };

  return (
    <div className="max-w-4xl mx-auto pb-12">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-white">AI Optimizer Dashboard</h1>
        <p className="text-slate-400">Podešavanje i nadzor lokalnog AI sistema (IK Firewall)</p>
      </div>

      {/* Live Status Bar */}
      <div className="bg-slate-900/50 border border-indigo-500/30 rounded-xl p-4 mb-8 shadow-inner shadow-indigo-500/10">
        <p className="text-indigo-300 font-mono flex items-center gap-3">
          <span className="w-2.5 h-2.5 rounded-full bg-indigo-500 animate-pulse shadow-[0_0_8px_rgba(99,102,241,0.8)]"></span>
          {status}
        </p>
      </div>

      {/* Kontrole */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6 mb-6">
        <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-6">
          <h2 className="text-lg font-semibold text-white mb-6 flex items-center gap-2">
            ⚙️ Firewall Tuning
          </h2>
          
          <div className="mb-6">
            <div className="flex justify-between items-center mb-2">
              <label className="text-slate-400 text-sm font-medium">Aggressiveness</label>
              <span className="text-indigo-400 text-sm font-mono">{config.aggressiveness}</span>
            </div>
            <input 
              type="range" min="0" max="1" step="0.1" 
              value={config.aggressiveness}
              onChange={(e) => handleConfigChange('aggressiveness', parseFloat(e.target.value))}
              className="w-full accent-indigo-500 bg-slate-800 rounded-lg appearance-none h-2"
            />
            <p className="text-xs text-slate-500 mt-2">Povećaj za kraće prompte i agresivniju kompresiju.</p>
          </div>

          <div className="mb-6">
            <div className="flex justify-between items-center mb-2">
              <label className="text-slate-400 text-sm font-medium">Balance (Quality vs Speed)</label>
              <span className="text-indigo-400 text-sm font-mono">{config.balance}</span>
            </div>
            <input 
              type="range" min="0" max="1" step="0.1" 
              value={config.balance}
              onChange={(e) => handleConfigChange('balance', parseFloat(e.target.value))}
              className="w-full accent-indigo-500 bg-slate-800 rounded-lg appearance-none h-2"
            />
            <p className="text-xs text-slate-500 mt-2">Manja vrednost ubrzava AI model, veća zahteva veću preciznost.</p>
          </div>
        </div>

        {/* Analitika za Lokalne modele */}
        <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-6 flex flex-col justify-center">
           <h2 className="text-lg font-semibold text-white mb-6">📊 Lokalna Potrošnja (Sesija)</h2>
           <div className="flex gap-8 items-center bg-slate-950/50 p-6 rounded-lg border border-slate-800/50">
              <div>
                <p className="text-slate-500 text-xs uppercase tracking-wider mb-1">Model</p>
                <p className="text-xl text-white font-mono">{usageStats.model}</p>
              </div>
              <div className="h-10 w-px bg-slate-800"></div>
              <div>
                <p className="text-slate-500 text-xs uppercase tracking-wider mb-1">Tokeni (Obrada)</p>
                <p className="text-2xl text-emerald-400 font-mono drop-shadow-[0_0_8px_rgba(52,211,153,0.3)]">{usageStats.totalTokens}</p>
              </div>
           </div>
        </div>
      </div>

      {/* Custom Directives */}
      <div className="bg-slate-900/40 border border-slate-800 rounded-xl p-6">
        <h2 className="text-lg font-semibold text-white mb-2 flex items-center gap-2">
          🎯 Custom Directives
        </h2>
        <p className="text-slate-500 text-sm mb-4">
          Ovaj tekst se čuva lokalno i šalje se kao <span className="font-mono text-indigo-400 text-xs bg-indigo-500/10 px-1 py-0.5 rounded">customContext</span> Firewall-u pri svakom zahtevu iz editora.
          Model će prilagoditi `agentDirective` ovim pravilima.
        </p>
        <textarea
          className="w-full h-32 bg-slate-950 text-slate-300 p-4 rounded-lg border border-slate-700 focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 focus:outline-none transition-all placeholder:text-slate-700 font-mono text-sm resize-y"
          placeholder="Npr. 'Ovo je custom PHP projekat, ne koristi Laravel funkcije, isključivo Vanilla PHP PDO za bazu.'"
          value={customDirective}
          onChange={handleDirectiveChange}
        />
      </div>

    </div>
  );
}
