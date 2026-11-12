use crate::*;

    #[test]
    fn test_katastar_normalization_and_scrubbing() {
        // 1. Prisma P1012 signature normalizes identifiers and line numbers
        let prisma_err = "Error: Prisma schema validation - (get-dmmf wasm)\nError code: P1012\nerror: Error validating: You defined the enum `Role`. But the current connector does not support enums.\n  -->  prisma\\schema.prisma:10";
        let prisma_sig = crate::katastar::normalize_error_signature("prisma", prisma_err, Some("prisma/schema.prisma"));
        assert_eq!(prisma_sig.category, "prisma");
        assert_eq!(prisma_sig.error_code.as_deref(), Some("P1012"));
        assert!(prisma_sig.error_signature.starts_with("prisma:P1012:"));
        assert!(prisma_sig.normalized_sample.contains("<IDENT>"));

        // 2. npm ETARGET extracts error code + offending package
        let npm_err = "npm error code ETARGET\nnpm error notarget No matching version found for shell-quote@^1.10.0.";
        let npm_sig = crate::katastar::normalize_error_signature("npm", npm_err, Some("package.json"));
        assert_eq!(npm_sig.category, "npm");
        assert_eq!(npm_sig.error_code.as_deref(), Some("ETARGET"));
        assert_eq!(npm_sig.error_signature, "npm:ETARGET:shell-quote");

        // 3. Compile / runtime error normalization
        let ts_local = "src/app/page.tsx(4,20): error TS2307: Cannot find module '@/components/Navbar' or its corresponding type declarations.";
        let ts_sig = crate::katastar::normalize_error_signature("", ts_local, Some("src/app/page.tsx"));
        assert_eq!(ts_sig.error_signature, "compile:TS2307:local-import");

        let client_err = "You're importing a component that needs useState. It only works in a Client Component, mark the file with the 'use client' directive.";
        let client_sig = crate::katastar::normalize_error_signature("", client_err, Some("src/components/Navbar.tsx"));
        assert_eq!(client_sig.error_signature, "compile:NEXT_USE_CLIENT:missing-client-directive");

        let ref_err = "Unhandled Runtime Error\nReferenceError: isMobileMenuOpen is not defined";
        let ref_sig = crate::katastar::normalize_error_signature("", ref_err, Some("src/components/Navbar.tsx"));
        assert_eq!(ref_sig.category, "runtime");
        assert_eq!(ref_sig.error_signature, "runtime:ReferenceError:identifier-not-defined");

        // 4. Sensitive path, ANSI, and credential scrubbing
        let dirty = "\x1b[31mError\x1b[0m in C:\\Users\\Alice\\AppData\\Local\\axiom-forge\\projects\\123e4567-e89b-12d3-a456-426614174000\\src\\app.ts with postgresql://admin:supersecret@db.internal:5432/prod and sk_live_1234567890abcdef Bearer eyJhbGciOiJIUzI1NiJ9.payload";
        let scrubbed = crate::katastar::scrub_sensitive_text(dirty);
        assert!(!scrubbed.contains("Alice"));
        assert!(!scrubbed.contains("supersecret"));
        assert!(!scrubbed.contains("sk_live_"));
        assert!(!scrubbed.contains("eyJhbGciOi"));
        assert!(scrubbed.contains("<USER_DIR>"));
        assert!(scrubbed.contains("postgresql://<REDACTED>"));
        assert!(scrubbed.contains("<SECRET>"));
    }


    #[test]
    fn test_katastar_safe_patch_extraction_and_guards() {
        // Atomic import fix is accepted
        let before_imp = "import { useState } from 'react';\nimport { Home } from 'react-icons';\nexport default function A() { return <div />; }\n";
        let after_imp = "import { useState } from 'react';\nimport { FiHome as Home } from 'react-icons/fi';\nexport default function A() { return <div />; }\n";
        let patch_imp = crate::katastar::extract_safe_atomic_patch("src/components/Header.tsx", before_imp, after_imp);
        assert!(patch_imp.is_some());

        // Atomic next.config fix is accepted
        let before_cfg = "const nextConfig = {\n  webpack: (config) => {\n    config.resolve.fallback = { path: require.resolve('path-browserify') };\n    return config;\n  }\n};\n";
        let after_cfg = "const nextConfig = {\n  webpack: (config) => {\n    config.resolve.fallback = { path: false };\n    return config;\n  }\n};\n";
        assert!(crate::katastar::extract_safe_atomic_patch("next.config.js", before_cfg, after_cfg).is_some());

        // Prisma universal referential fix accepted; provider & model edits rejected
        let before_prisma = "model Item {\n  parent Item? @relation(fields: [p], references: [id], onDelete: SET_NULL)\n}\n";
        let after_prisma = "model Item {\n  parent Item? @relation(fields: [p], references: [id], onDelete: SetNull)\n}\n";
        assert!(crate::katastar::extract_safe_atomic_patch("prisma/schema.prisma", before_prisma, after_prisma).is_some());

        let before_prov = "datasource db {\n  provider = \"sqlite\"\n  url = env(\"DATABASE_URL\")\n}\n";
        let after_prov = "datasource db {\n  provider = \"postgresql\"\n  url = env(\"DATABASE_URL\")\n}\n";
        assert!(crate::katastar::extract_safe_atomic_patch("prisma/schema.prisma", before_prov, after_prov).is_none());

        // JSX component markup edits are NEVER auto-patched globally across projects
        let before_jsx = "export default function Nav() {\n  return <div>{isOpen ? 'Open' : 'Closed'}</div>;\n}\n";
        let after_jsx = "export default function Nav() {\n  return <div><span>Menu</span></div>;\n}\n";
        assert!(crate::katastar::extract_safe_atomic_patch("src/components/Navbar.tsx", before_jsx, after_jsx).is_none());
    }


    #[test]
    fn test_katastar_two_phase_verification_and_disable_lifecycle() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_katastar_lifecycle_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let store_path = temp_dir.join("axiom-error-katastar.json");

        let before_code = "import { Home } from 'react-icons';\nexport default function Hero() { return <main />; }\n";
        let after_code = "import { FiHome as Home } from 'react-icons/fi';\nexport default function Hero() { return <main />; }\n";
        let patch = crate::katastar::extract_safe_atomic_patch("src/components/Hero.tsx", before_code, after_code).unwrap();
        let err_msg = "src/components/Hero.tsx(1,22): error TS2307: Cannot find module 'react-icons' or its corresponding type declarations.";

        // 1. Register as pending -> must NOT surface in verified lookup or blind sanitizer patches yet
        let eid = crate::katastar::register_repair_attempt_at(
            &store_path,
            "compile",
            err_msg,
            "src/components/Hero.tsx",
            "ai",
            "search_replace",
            Some(patch),
            "Pending fix",
            "qwen2.5-coder:7b",
        );
        let pre_lookup = crate::katastar::lookup_katastar_at(&store_path, "compile", err_msg, "src/components/Other.tsx", before_code, &[]);
        assert!(pre_lookup.applicable_patches.is_empty());
        assert!(pre_lookup.hints.is_empty());
        assert!(crate::katastar::get_verified_sanitizer_patches_at(&store_path, "src/components/Other.tsx").is_empty());

        // 2. Finalize passing build test -> promotes to "verified" and becomes usable
        crate::katastar::finalize_katastar_outcome_at(
            &store_path,
            &eid,
            true,
            "ai",
            "next_build",
            None,
            Some("Import react-icons icons from specific subpath like 'react-icons/fi'"),
            "qwen2.5-coder:7b",
        );
        let post_lookup = crate::katastar::lookup_katastar_at(&store_path, "compile", err_msg, "src/components/Other.tsx", before_code, &[]);
        assert_eq!(post_lookup.applicable_patches.len(), 1);
        assert_eq!(post_lookup.applicable_patches[0].entry_id, eid);
        assert_eq!(post_lookup.hints.len(), 1);
        assert_eq!(crate::katastar::get_verified_sanitizer_patches_at(&store_path, "src/components/Other.tsx").len(), 1);

        // 3. Two consecutive failures auto-disable the verified rule
        crate::katastar::finalize_katastar_outcome_at(&store_path, &eid, false, "katastar", "tsc_noemit", None, None, "qwen2.5-coder:7b");
        crate::katastar::finalize_katastar_outcome_at(&store_path, &eid, false, "katastar", "tsc_noemit", None, None, "qwen2.5-coder:7b");
        let disabled_lookup = crate::katastar::lookup_katastar_at(&store_path, "compile", err_msg, "src/components/Other.tsx", before_code, &[]);
        assert!(disabled_lookup.applicable_patches.is_empty());
        assert!(crate::katastar::get_verified_sanitizer_patches_at(&store_path, "src/components/Other.tsx").is_empty());

        let store = crate::katastar::load_katastar_store_at(&store_path);
        assert_eq!(store.entries[0].status, "disabled");
        assert_eq!(store.entries[0].history.len(), 3);

        // 4. Interactive pending repair tracking & reoccurrence detection
        let runtime_err = "ReferenceError: isMobileMenuOpen is not defined";
        let sig_info = crate::katastar::normalize_error_signature("runtime", runtime_err, Some("src/components/Navbar.tsx"));
        let eid2 = crate::katastar::register_repair_attempt_at(
            &store_path,
            &sig_info.category,
            runtime_err,
            "src/components/Navbar.tsx",
            "ai",
            "ai_edit",
            None,
            "Declare useState for mobile menu",
            "qwen2.5-coder:7b",
        );
        crate::katastar::track_pending_project_repair_at(
            &store_path,
            "proj-demo",
            "src/components/Navbar.tsx",
            &sig_info.error_signature,
            &eid2,
            "ai",
        );
        let reoccurred = crate::katastar::resolve_reoccurred_pending_repair_at(
            &store_path,
            "proj-demo",
            "src/components/Navbar.tsx",
            &sig_info.error_signature,
            "qwen2.5-coder:7b",
        );
        assert_eq!(reoccurred, vec![eid2.clone()]);
        let store_after = crate::katastar::load_katastar_store_at(&store_path);
        assert!(store_after.pending_repairs.is_empty());
        let failed_entry = store_after.entries.iter().find(|e| e.id == eid2).unwrap();
        assert_eq!(failed_entry.status, "failed");
        assert_eq!(failed_entry.fail_count, 1);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_katastar_boilerplate_filtering_and_features_mapping_v48() {
        let raw_prisma_err = "Error: Prisma schema validation - (get-dmmf wasm)\nError code: P1012\nerror: Type \"Phantom\" is neither a built-in type, nor refers to another model.\n  -->  prisma\\schema.prisma:12\n   |\n12 |   ref Phantom[]\nValidation Error Count: 1\n[Context: getDmmf]\nPrisma CLI Version : 5.22.0";
        let prisma_sig = crate::katastar::normalize_error_signature("prisma", raw_prisma_err, Some("prisma/schema.prisma"));
        assert_eq!(prisma_sig.error_code.as_deref(), Some("P1012"));
        assert!(!prisma_sig.normalized_sample.contains("get-dmmf"));
        assert!(!prisma_sig.normalized_sample.contains("Validation Error Count"));

        let raw_next_err = "Failed to compile.\nBuild failed because of webpack errors\n> Build error occurred\nImport trace for requested module:\n./src/hooks/useEvents.ts\nModule not found: Can't resolve 'some-missing-pkg'";
        let next_sig = crate::katastar::normalize_error_signature("", raw_next_err, Some("src/hooks/useEvents.ts"));
        assert_eq!(next_sig.error_code.as_deref(), Some("MODULE_NOT_FOUND"));
        assert!(!next_sig.normalized_sample.starts_with("Failed to compile"));

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v48_feat_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join(".axiom")).unwrap();

        let files = vec![
            serde_json::json!({ "path": "src/app/api/events/route.ts" }),
            serde_json::json!({ "path": "src/app/events/page.tsx" }),
            serde_json::json!({ "path": "src/services/sync.ts" }),
            serde_json::json!({ "path": "src/types/record.ts" }),
            serde_json::json!({ "path": "src/config/env.ts" }),
        ];
        crate::editor_ai::generate_axiom_features_json(&temp_dir, "demo", &files);
        let feat_val: Value = serde_json::from_str(&std::fs::read_to_string(temp_dir.join("axiom-features.json")).unwrap()).unwrap();
        let feat_names: Vec<String> = feat_val["features"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["name"].as_str().map(|s| s.to_string()))
            .collect();
        assert!(feat_names.contains(&"API & Backend".to_string()));
        assert!(feat_names.contains(&"App Pages & Routes".to_string()));
        assert!(feat_names.contains(&"Services".to_string()));
        assert!(feat_names.contains(&"TypeScript Types".to_string()));
        assert!(feat_names.contains(&"Configuration".to_string()));
        assert!(!feat_names.contains(&"src".to_string()));

        // Contract prompt prioritizes direct dependencies (rank 0)
        std::fs::write(
            temp_dir.join(".axiom/axiom-contracts.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "1.0",
                "files": [
                    {
                        "path": "src/components/EventPanel.tsx",
                        "role": "ui-component",
                        "dependencies": { "internal": ["src/hooks/useEvents.ts"], "external": [] },
                        "capabilities": { "exports": ["EventPanel"] },
                        "consumedContracts": { "apiCalls": [] },
                        "connectedRoutes": []
                    },
                    {
                        "path": "src/hooks/useUnrelated.ts",
                        "role": "client-hook",
                        "dependencies": { "internal": [], "external": [] },
                        "capabilities": { "exports": ["useUnrelated"] },
                        "consumedContracts": { "apiCalls": [] },
                        "connectedRoutes": []
                    },
                    {
                        "path": "src/hooks/useEvents.ts",
                        "role": "client-hook",
                        "dependencies": { "internal": [], "external": [] },
                        "capabilities": { "exports": ["useEvents"] },
                        "consumedContracts": { "apiCalls": ["/api/events"] },
                        "connectedRoutes": []
                    }
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        let prompt = crate::orchestrator::format_axiom_contracts_prompt(&temp_dir, "src/components/EventPanel.tsx");
        let events_idx = prompt.find("src/hooks/useEvents.ts").unwrap();
        let unrelated_idx = prompt.find("src/hooks/useUnrelated.ts").unwrap();
        assert!(events_idx < unrelated_idx, "Direct internal dependency must sort first:\n{}", prompt);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_heal_ledger_and_chain_of_information_v49() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v49_coi_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let baseline = vec![
            "Line 10, Col 5: [TS2304] Cannot find name 'IconA'.".to_string(),
            "Line 22, Col 9: [TS2339] Property 'badField' does not exist on type 'Row'.".to_string(),
        ];
        // Simulates adding an import shifted line 22 -> line 23 while resolving IconA
        let post = vec![
            "Line 23, Col 9: [TS2339] Property 'badField' does not exist on type 'Row'.".to_string(),
        ];
        let diff = crate::katastar::compute_diagnostic_diff(&baseline, &post);
        assert_eq!(diff.resolved.len(), 1);
        assert!(diff.resolved[0].contains("IconA"));
        assert!(diff.introduced.is_empty(), "Line shift 22->23 must not count as newly introduced error");
        assert!(diff.count_reduced);

        crate::katastar::record_project_heal_round(
            &temp_dir,
            "src/app/board/page.tsx",
            1,
            "ai",
            "partial_progress",
            &diff.resolved,
            &post,
            "Added missing IconA import",
        );

        let prompt = crate::katastar::format_chain_of_information_prompt(
            &temp_dir,
            "src/app/board/page.tsx",
            &[baseline[0].clone(), post[0].clone()],
            2,
        );
        assert!(!prompt.contains("Cannot find name 'IconA'"), "Resolved error must be omitted from active unresolved prompt:\n{}", prompt);
        assert!(prompt.contains("badField"), "Remaining unresolved error must stay in Chain-of-Information:\n{}", prompt);
        assert!(prompt.contains("ALREADY RESOLVED"), "Must instruct LLM not to undo earlier verified fix:\n{}", prompt);
        assert!(prompt.contains("DEEP CAUSAL REFLECTION REQUIRED"), "Round 2+ must require deep causal reflection:\n{}", prompt);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_katastar_classifies_runtime_typeerror_v51() {
        let sig = crate::katastar::normalize_error_signature(
            "",
            "⨯ TypeError: Cannot read properties of null (reading 'map')\n  at BoardPage (./src/app/board/page.tsx:30:29)",
            Some("src/app/board/page.tsx"),
        );
        assert_eq!(sig.category, "runtime");
        assert_eq!(sig.error_code.as_deref(), Some("TypeError"));
        assert!(
            sig.error_signature.starts_with("runtime:TypeError:"),
            "Unexpected runtime TypeError signature: {}",
            sig.error_signature
        );

        // Related dependency extraction automatically pulls local hooks on runtime null/.map crash
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v51_rel_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/app/board")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        let page_src = r#"'use client';
import { useRecords } from '@/hooks/useRecords';
export default function BoardPage() {
  const { items } = useRecords();
  return <ul>{items.map(r => <li key={r.id}>{r.title}</li>)}</ul>;
}
"#;
        let hook_src = "'use client';\nimport { useState } from 'react';\nexport function useRecords() { const [items, setItems] = useState(null); return { items, setItems }; }\n";
        std::fs::write(temp_dir.join("src/app/board/page.tsx"), page_src).unwrap();
        std::fs::write(temp_dir.join("src/hooks/useRecords.ts"), hook_src).unwrap();

        let related = crate::healer::extract_related_project_files_for_error(
            &temp_dir,
            page_src,
            "src/app/board/page.tsx",
            "TypeError: Cannot read properties of null (reading 'map')",
        );
        assert!(
            related.iter().any(|(p, _)| p == "src/hooks/useRecords.ts"),
            "Expected runtime crash to pull imported hook file, got: {:?}",
            related.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&temp_dir);
    }


