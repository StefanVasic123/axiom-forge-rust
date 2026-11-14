# Axiom Forge Rust — Architecture Map (`v0.1.55+`)

Ovaj dokument predstavlja kanonsku mapu izvršavanja koda u **Axiom Forge** desktop aplikaciji (Tauri v2 + Rust backend + React/Vite frontend), uključujući **Fazu 3 (`tree-sitter` AST Engine)**, **Univerzalni AI Edit/Heal sistem** i **Multi-Platform (Web / Desktop Electron / Mobile React Native)** arhitekturu.

---

## 1. End-to-End Tok Generisanja, AST Rekoncilijacije i Učenja

```text
  [UI: ProjectConfig / Dashboard / Editor]
                     │
             (IPC: ipc-bridge.js)
                     ▼
     [1. Manifest & Phase Planner]          ──► orchestrator.rs / sanitizer.rs
     - Sanitacija manifesta & runaway guard (`detect_infinite_loop` 1..=12 linija)
     - 6-fazni topološki raspored za SVE platforme (`nextjs`, `electron`, `react_native`):
       Config -> DatabaseOrMain -> LogicOrHooks -> ApiOrState -> ShellOrWidgets -> PagesOrUi
                     │
                     ▼
     [2. Prompt & Context Assembly]         ──► orchestrator.rs / packages.rs / policy_registry.rs
     - `get_expert_system_prompt()` + Scoped Rulepacks (`policy_registry.rs`)
     - Platform-aware šabloni (`main.js` / `preload.js` CommonJS za Electron, dual `@/*` alias)
     - Axiom Contracts (`axiom-contracts.json`, AST potpis hook-ova i tipova)
     - Relevant `.d.ts` & package DB (`packages.json` + blokada zastarelih `@types/*` paketa)
                     │
                     ▼
     [3. Local LLM Inference (Ollama)]      ──► ollama.rs / hardware.rs
     - Dynamic VRAM/context sizing (`num_ctx`, `temperature`)
                     │
                     ▼
     [4. Pre-Save Reviewer & AST Validator] ──► reviewer.rs + ast_engine.rs + orchestrator.rs
     - `tree-sitter` + `tree-sitter-typescript` AST syntax check (`syntax:ast-parse-error`)
     - App Router route & default export guards, RootLayout `{children}` & Middleware matcher
                     │
                     ▼
     [5. Deterministic Sanitizer]           ──► sanitizer.rs (`post_process_generated_file`)
     - `'use client'`, UI primitives (`toast.success/.error`), hook type exports (bez `TS2308`)
     - Prisma relacije, enum-i, FK skalari, auto-import `prisma` u `/api/` rutama
     - NextAuth v4 + `axiom-env.d.ts` (`Session.user.id`, `SignInResponse.user`), OpenAI v3->v4
     - `hoist_enums_before_usage` (`TS2450`), `strip_self_conflicting_local_imports` (`TS2440`)
                     │
                     ▼
     [6. AST Contract Harvest & Maps]       ──► ast_engine.rs + orchestrator.rs
     - `extract_exports`, `extract_hook_signatures`, `extract_hook_call_sites`, `extract_type_shapes`
     - `axiom-contracts.json`, `axiom-map.json`, `LiveBuilderGuide`
                     │
                     ▼
     [7. Multi-Tier Self-Healing Loop]      ──► self_healing.rs / ast_engine.rs / healer.rs / server.rs
     ├── Tier 0: Hygiene (skenira i `src/` i root `app/`, `components/`, `hooks/`, `services/`)
     │           + `ast_engine::reconcile_hook_contracts_in_project` (0-arg default `= {} as any` + `error`/`refetch`)
     │           + `ensure_multiplatform_entry_routes_bridged` (eliminiše Electron/Mobile `404` na `/`)
     │           + `sanitize_tailwind_config_in_project` + Prisma push + `.d.ts` reconciler
     ├── Tier 1: `tsc --noEmit` (sa `tsconfig.json` auto-recovery kad `tsc` padne sa 0 dijagnostika)
     ├── Tier 2: `next build` + ekstrakcija ruta / extensionless & chunk URLs
     └── Runtime: Preview DevLogs & Fuzzy Multi-Line Patch Resolver -> `project_heal_compile_error`
                     │
                     ▼
     [8. Episodic Memory, Telemetry & SFT]  ──► katastar.rs + `/ai-optimizer`
     - Katastar (`katastar.json`, dvofazna verifikacija patch-a)
     - `.axiom/heal-ledger.json` (realna `1st-Pass Clean` telemetrija — nikad `100%` kad postoje greške/upozorenja)
     - Export `axiom_sft_dataset.jsonl` + Ollama `Modelfile`
```

---

## 2. Mapa Modula (`v0.1.55`)

### Rust Backend (`src-tauri/src/`)

| Modul | Veličina | Primarna uloga | Status u `v0.1.55` |
| :--- | :---: | :--- | :--- |
| `lib.rs` | ~385 L | Tauri ulaz, deep link (`axiom://`), agent bridge, IPC handler | Testovi u `src-tauri/src/tests/` (Faza 0) |
| `ast_engine.rs` | ~1.220 L | **Faza 3 AST Engine (`tree-sitter` + `tree-sitter-typescript`)**: parsanje TS/TSX, ekstrakcija importa/eksporta, hook potpisa, call-site zahteva, AST rekoncilijacija ugovora i fuzzy multi-line target matcher | **Novo u Fazi 3 (`v0.1.55`)** |
| `policy_registry.rs` | ~420 L | Kanonski **Policy Registry** (`ruleId`), prompt direktive i mapa 6 pipeline agenata | Faza 2 (`v0.1.54`) |
| `reviewer.rs` | ~850 L | Pre-save AST + token-aware balans, Edge middleware matcher & API route audit | Faza 1 + Faza 3 AST integracija |
| `orchestrator.rs` | ~6.780 L | Generisanje, manifest, 6-fazni topološki planer za Web/Desktop/Mobile, AST ugovori (`contracts`) | Proširen za Electron & React Native |
| `sanitizer.rs` | ~9.300 L | Determinističko čišćenje TSX/TS/Prisma/CSS/JSON/Tailwind fajlova i multi-platform route bridging | Proširen za sve 13.11/14.11 obrasce |
| `self_healing.rs` | ~5.130 L | Post-gen loop: Dual-root (`src/` + root) Hygiene, AST hook reconciler, Tier 1 (`tsc`), Tier 2 (`next build`) | Proširen za split-root projekte |
| `healer.rs` | ~4.550 L | Single-file preview repair, Prisma heal loop, JSON/targeted edit parser + AST fuzzy target resolver | Povezan sa `ast_engine.rs` |
| `packages.rs` | ~2.420 L | Katalog verzija/praksi paketa + blokada pokvarenih `@types/*` paketa + `npm install` recovery | Proširena `banned_always_pkgs` lista |
| `katastar.rs` | ~2.220 L | Katastar grešaka, `heal-ledger`, istinita telemetrija tačnosti (`firstPassCleanRatePct`) i SFT trainer | Ispravljena `100% 1st-Pass Clean` distorzija |
| `server.rs` | ~1.055 L | Dev server runner, pre-flight multi-platform route bridge, parsanje logova | Eliminiše `404` pri pokretanju Preview-a |
| `editor_ai.rs` | ~935 L | Ručni AI asistent za pojedinačne fajlove/projekte sa AST/fuzzy target matchingom | Povezan sa `ast_engine.rs` |
| `project.rs`, `hardware.rs`, `ollama.rs`, `git_engine.rs`, `mode.rs`, `relay.rs` | ~2.280 L | CRUD projekata, GPU/VRAM profilisanje, Ollama klijent, Git grane/predlozi, Agency mode | Stabilno |

---

## 3. Plan Razvoja Multi-Platform Generatora (Web / Desktop / Mobile) i Univerzalnog AI Edit/Heal Sistema

1. **Jedinstveni 6-fazni Topološki Redosled (`Config -> DatabaseOrMain -> LogicOrHooks -> ApiOrState -> ShellOrWidgets -> PagesOrUi`):**
   - Sva tri generatora (`nextjs`, `electron`, `react_native`) sada generišu tipove, modele, servise i hook-ove **pre** komponenti i ekrana, dok se AST ugovori (`axiom-contracts.json`) sakupljaju iz svakog generisanog fajla i prosleđuju narednim fazama.
2. **Dual-Root Svest (`src/` + root `app/`, `components/`, `hooks/`, `services/`):**
   - Kod Mobile (Expo Router) i Desktop (Electron) projekata LLM često kombinuje fajlove u root direktorijumima (`app/(tabs)/index.tsx`, `components/`, `hooks/`, `services/`) sa fajlovima u `src/` (`src/app/api/`, `src/renderer/App.tsx`).
   - `tsconfig.json` sada mapira `"@/*": ["./src/*", "./*"]`, a Hygiene, Barrel generator, AST Hook Reconciler, Auto-Importer i Auto-Test Suite skeniraju **oba** stabla.
3. **Automatski Entry-Route Bridge (`ensure_multiplatform_entry_routes_bridged`):**
   - Kada projekat ima `src/app/api/*`, Next.js ignoriše root `app/` i traži `src/app/page.tsx`.
   - Za **Desktop (Electron)** automatski se generiše `src/app/page.tsx` koji montira `@/renderer/App`, čime se trajno eliminiše `404` stranica na inicijalnom učitavanju.
   - Za **Mobile (React Native / Expo)** automatski se premošćuju `app/(tabs)/index.tsx`, `app/index.tsx` i ostali ekrani (`app/dashboard.tsx` -> `src/app/dashboard/page.tsx`) za trenutni prikaz u Live Preview-u.
4. **Univerzalni AI Edit/Heal Sistem (čak i za polovično generisane projekte):**
   - **Sloj 1 — Pre-Flight Strukturna Rekonstrukcija:** `ensure_valid_tsconfig`, `sanitize_tailwind_config_in_project`, `ensure_directory_barrel_files`, `ensure_ui_primitives_reconciled`, `ensure_multiplatform_entry_routes_bridged` i `sanitize_truncated_import_repetition_loop` oporavljaju čak i prekinute ili polovično generisane projekte.
   - **Sloj 2 — AST Cross-File Contract Reconciler (`ast_engine.rs`):** `reconcile_hook_contracts_in_project` pomoću `tree-sitter` AST-a usklađuje potpise svih custom hook-ova i njihovih pozivalaca u projektu bez potrebe za LLM pozivom.
   - **Sloj 3 — AST & Line-Normalized Fuzzy Patch Resolver (`find_best_fuzzy_target_match`):** Kada lokalni 7B model u `healer.rs` ili `editor_ai.rs` pošalje `target` blok koji se razlikuje u razmacima, tačka-zarezima ili navodnicima, AST/fuzzy resolver pronalazi tačan raspon u izvornom fajlu umesto odbijanja patch-a posle 3 pokušaja.

---

## 4. Status Faza Refaktora (`v0.1.53` – `v0.1.56+`)

- **Faza 0 (`v0.1.53` — Završeno):** `ARCHITECTURE.md` + izdvajanje svih testova iz `lib.rs` u `src-tauri/src/tests/`.
- **Faza 1 (`v0.1.54` — Završeno):** Uvođenje `reviewer.rs` (pre-save strukturni pregled + anti-stagnacija).
- **Faza 2 (`v0.1.54` — Završeno):** Jedinstveni **Policy Registry** (`policy_registry.rs`, `ruleId`) + Pipeline Inspector u `/ai-optimizer`.
- **Faza 3 (`v0.1.55` — Završeno):** Uvođenje `tree-sitter` + `tree-sitter-typescript` AST engine-a (`ast_engine.rs`) za parsanje TS/TSX, ekstrakciju ugovora, cross-file AST rekoncilijaciju hook-ova i fuzzy target matching u `healer.rs` / `editor_ai.rs`.
- **Faza 4 (Pripremljeno za `v0.1.56`):**
  1. Razbijanje monolitnog `src/pages/Editor.jsx` (`116 KB`) u modularne sub-komponente (`EditorPreviewPane.jsx`, `EditorCodePane.jsx`, `EditorAiAssistantPane.jsx`, `EditorDevLogsDrawer.jsx`, `EditorEnvBanner.jsx`).
  2. Fizičko razdvajanje `sanitizer.rs` i `self_healing.rs` u podmodule (`domains/prisma_sanitizer.rs`, `domains/ui_reconciler.rs`, `healing/tier_runner.rs`) oslanjajući se na `ast_engine.rs`.
