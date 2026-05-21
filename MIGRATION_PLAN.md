# Axiom Forge - Migration Plan (Electron to Rust/Tauri)

Ovaj dokument služi kao mapa puta za prelazak sa Node.js backend-a na ultra-brzi Rust backend koristeći Tauri framework.

## 1. Pregled Funkcionalnosti (Audit)

Na osnovu analize `preload.cjs` i `main.js`, ovo su moduli koje moramo da portujemo u Rust:

### A. Core Engine
- **Security API**: Upravljanje tokenima i enkripcija (Korišćenje `tauri-plugin-stronghold` ili `keyring`).
- **Ollama Engine**: Provera zdravlja, pull-ovanje modela i streaming (Korišćenje `reqwest` za HTTP).
- **Hardware API**: Detekcija RAM-a, procesora i preporuka modela (Korišćenje `sysinfo` crate-a).

### B. Project & File Management
- **Project API**: CRUD operacije nad projektima, čitanje/pisanje fajlova (Nativni Rust `std::fs`).
- **Git Integration**: Commit, push, GitHub sync (Korišćenje `git2` ili direktno CLI pozivanje).
- **Editor API**: AI editovanje fajlova i direktno pisanje koda.

### C. Advanced Services
- **Task Orchestrator**: Upravljanje asinhronim pipeline-om generacije aplikacija.
- **Server API**: Upravljanje lokalnim dev serverima (Next.js/Vite) preko child procesa.
- **Deep Linking**: Obrada `axiom://` protokola u Rustu.

---

## 2. Nova Arhitektura Povezivanja (IPC Bridge)

U Electronu smo koristili:
```javascript
window.electronAPI.hardware.getProfile()
```

U Tauriju ćemo koristiti:
```javascript
import { invoke } from "@tauri-apps/api/core";
const profile = await invoke("get_hardware_profile");
```

**Strategija:** Napravićemo "Shim" (adapter) u `src/lib/ipc-bridge.js` koji će glumiti stari `electronAPI` ali slati komande ka Rustu, kako ne bismo morali da menjamo svaku komponentu posebno.

---

## 3. Redosled Implementacije (Roadmap)

### Faza 1: Temelj (Sada)
- [ ] Postavljanje Main prozora i osnovnog stila.
- [ ] Implementacija **Hardware API** (Rust je tu 10x brži).
- [ ] Implementacija **Security API** (Lokalni storage za tokene).

### Faza 2: AI Komunikacija
- [ ] Povezivanje sa lokalnom Ollama-om.
- [ ] Implementacija streaming-a odgovora nazad u UI.

### Faza 3: Filesystem & Projects
- [ ] Portovanje logike za upravljanje folderima i projektima.
- [ ] Implementacija Git komandi.

### Faza 4: Orchestrator
- [ ] Prevođenje `taskOrchestrator.js` logike u Rust asinhronu state mašinu.
- [ ] Upravljanje procesima (Dev server).

---

## 4. Kako testirati?

1. Otvori `axiom-forge-rust` u terminalu.
2. Pokreni `npm install` (ako već nisi).
3. Pokreni `npm run tauri dev`.
