# Axiom Forge Rust — Architecture Map (`v0.1.52+`)

Ovaj dokument predstavlja kanonsku mapu izvršavanja koda u **Axiom Forge** desktop aplikaciji (Tauri v2 + Rust backend + React/Vite frontend), sa ciljem jasnog pregleda svih faza, politika i memorije sistema tokom refaktorisanja (`v0.1.53+`).

---

## 1. End-to-End Tok Generisanja i Učenja

```text
  [UI: ProjectConfig / Dashboard / Editor]
                     │
             (IPC: ipc-bridge.js)
                     ▼
     [1. Manifest & Phase Planner]          ──► orchestrator.rs / sanitizer.rs
     - Sanitacija manifesta & runaway guard
     - GenerationPhase (Config -> DB -> Api -> UI)
                     │
                     ▼
     [2. Prompt & Context Assembly]         ──► orchestrator.rs / packages.rs
     - `get_expert_system_prompt()`
     - Scoped Rulepacks + `{metakognitivan_spisak_pravila}`
     - Axiom Contracts (`axiom-contracts.json`, LBG, Routes)
     - Relevant `.d.ts` & package DB (`packages.json`)
                     │
                     ▼
     [3. Local LLM Inference (Ollama)]      ──► ollama.rs / hardware.rs
     - Dynamic VRAM/context sizing (`num_ctx`, `temperature`)
                     │
                     ▼
     [4. Pre-Save Validator]                ──► orchestrator.rs (`validate_file_with_*`)
     - App Router route & default export guards
     - RootLayout JSX `{children}` & no-table/hook guards
     - Middleware matcher (`:path*`, bez `/*` globa)
                     │
                     ▼
     [5. Deterministic Sanitizer]           ──► sanitizer.rs (`post_process_generated_file`)
     - `'use client'`, UI primitives, hook type exports
     - Prisma relations, enums, FK scalars, datasource
     - NextAuth v4 imports & optional password casts
     - RootLayout `{children}` fallback recovery
                     │
                     ▼
     [6. Contract Harvest & Maps]           ──► orchestrator.rs / healer.rs
     - `axiom-contracts.json`, `axiom-map.json`, `LiveBuilderGuide`
                     │
                     ▼
     [7. Multi-Tier Self-Healing Loop]      ──► self_healing.rs / healer.rs / server.rs
     ├── Tier 0: Hygiene + auto-import + Prisma push + `.d.ts` reconciler
     ├── Tier 1: `tsc --noEmit` po slojevima prioriteta + Rollback + Stagnant-round breaker
     ├── Tier 2: `next build` + ekstrakcija ruta / extensionless & chunk URLs
     └── Runtime: Preview DevLogs & Chunk/Webpack guard -> `project_heal_compile_error`
                     │
                     ▼
     [8. Episodic Memory, Telemetry & SFT]  ──► katastar.rs + `/ai-optimizer`
     - Katastar (`katastar.json`, dvofazna verifikacija patch-a)
     - `.axiom/heal-ledger.json` (DiagnosticDiff po fajlu)
     - Exponential Accuracy Growth Equation & `učestalost_greske`
     - Export `axiom_sft_dataset.jsonl` + Ollama `Modelfile`
```

---

## 2. Mapa Modula (Trenutno Stanje — Pred Refaktor)

### Rust Backend (`src-tauri/src/`)

| Modul | Veličina | Primarna uloga | Napomena za refaktor (`v0.1.53+`) |
| :--- | :---: | :--- | :--- |
| `lib.rs` | ~385 L* | Tauri ulaz, deep link (`axiom://`), agent bridge, IPC handler | *Testovi izmešteni u `src-tauri/src/tests/` (Faza 0)* |
| `orchestrator.rs` | 6.592 L | Generisanje, manifest, rulepacks, validator, ugovori (`contracts`) | Podeliti u `pipeline/*` i `domains/*` |
| `sanitizer.rs` | 7.448 L | Determinističko čišćenje TSX/TS/Prisma/CSS/JSON fajlova | Razdvojiti Prisma, UI/React, Auth i Manifest sanajzere + uvesti `tree-sitter` |
| `self_healing.rs` | 5.022 L | Post-gen loop: Prisma DB, `.d.ts` inspekcija, Tier 1 (`tsc`), Tier 2 (`next build`) | Razdvojiti `verifier.rs` (`Tier 0/1/2`) i `dts_inspector.rs` |
| `healer.rs` | 4.528 L | Single-file preview repair, Prisma heal loop, JSON/targeted edit parser, drop-guard | Izdvojiti `patch_parser.rs` i Prisma healer |
| `packages.rs` | 2.402 L | Katalog verzija/praksi paketa + `npm install` offline/ETARGET recovery | Zadržati kao katalog + npm healer |
| `katastar.rs` | 2.202 L | Katastar grešaka, `heal-ledger`, telemetrija tačnosti i SFT trainer | Dodata atribucija po `ruleId` potpisima preko `policy_registry.rs` |
| `policy_registry.rs` | ~420 L | Kanonski **Policy Registry** (`ruleId`), prompt direktive i mapa 6 pipeline agenata | *Novo u Fazi 2 (`v0.1.54`)* — povezan sa `orchestrator.rs`, `reviewer.rs` i `/ai-optimizer` |
| `reviewer.rs` | ~835 L | Pre-save token-aware AST-balans, Edge middleware matcher & API route audit | *Novo u Fazi 1 (`v0.1.54`)* — proverava `{}`/`()`/`[]` balans i stagnaciju |
| `server.rs` | 1.047 L | Dev server runner, parsanje logova, `map_next_chunk_url_to_source` | Čisto izdvojen runtime modul |
| `editor_ai.rs` | 926 L | Ručni AI asistent za pojedinačne fajlove/projekte | Čisto izdvojen editor modul |
| `project.rs` | 789 L | CRUD projekata, podešavanja, keychain tokeni, import, `.env` detekcija | Stabilno |
| `hardware.rs`, `ollama.rs`, `git_engine.rs`, `mode.rs`, `relay.rs` | ~1.490 L | GPU/VRAM profilisanje, Ollama klijent, Git grane/predlozi, Agency mode | Čisti, dobro izolovani moduli |

### React Frontend (`src/`)

- **Ključne stranice (`src/pages/`):** `Dashboard.jsx`, `ProjectConfig.jsx`, `ProjectDetail.jsx`, `Editor.jsx` (`116 KB` — kandidat za razbijanje u Fazi 4), `AiOptimizer.jsx` (sadrži i klikabilni **Pipeline Orchestrator & Policy Registry Inspector**), `Settings.jsx`, `AgencyDashboard.jsx`, `InstallationWizard.jsx`.
- **Most (`src/lib/ipc-bridge.js`):** Mapira `window.electronAPI.*` pozive na Tauri `invoke(...)` i event listener-e.

---

## 3. Plan Refaktora (`v0.1.53+`)

- **Faza 0 (završena u `v0.1.53`):** `ARCHITECTURE.md` + izdvajanje svih testova iz `lib.rs` u `src-tauri/src/tests/`.
- **Faza 1 (`v0.1.54`):** Uvođenje `reviewer.rs` (semantički/strukturni pregled + anti-stagnacija) i priprema za domensko cepanje u `domains/` / `pipeline/` / `healing/`.
- **Faza 2 (završena u `v0.1.54`):** Jedinstveni **Policy Registry** (`policy_registry.rs`, `ruleId`) — svako pravilo na jednom mestu daje prompt direktive, pre-save validator, deterministički sanajzer, heal direktivu i klikabilan Pipeline Inspector sa Katastar statistikom u `/ai-optimizer`.
- **Faza 3 (`v0.1.55`):** Uvođenje `tree-sitter` + `tree-sitter-typescript` AST engine-a umesto krhkih regex-a za import/export i JSX strukturu.
- **Faza 4:** Razbijanje `Editor.jsx` u manje komponente i proširenje vizuelnog Pipeline/Policy Inspectora u UI.
