use crate::*;


    #[test]
    fn test_resolve_and_rewrite_import() {
        let db = vec![
            PackageMeta {
                name: "resend".to_string(),
                version: "^3.2.0".to_string(),
                aliases: vec!["@resend/client".to_string(), "@resend/base".to_string(), "resend-node".to_string(), "resend/client".to_string()],
                import_examples: vec!["import { Resend } from 'resend';".to_string()],
                compatibility: serde_json::Map::new(),
                best_practices: "".to_string(),
            },
            PackageMeta {
                name: "next-auth".to_string(),
                version: "^4.24.7".to_string(),
                aliases: vec!["@next-auth/client".to_string(), "next-auth/client".to_string()],
                import_examples: vec![
                    "import { getServerSession } from 'next-auth';".to_string(),
                    "import NextAuth from 'next-auth';".to_string(),
                    "import { useSession } from 'next-auth/react';".to_string(),
                ],
                compatibility: serde_json::Map::new(),
                best_practices: "".to_string(),
            },
            PackageMeta {
                name: "@hookform/resolvers".to_string(),
                version: "^3.4.2".to_string(),
                aliases: vec![],
                import_examples: vec!["import { zodResolver } from '@hookform/resolvers/zod';".to_string()],
                compatibility: serde_json::Map::new(),
                best_practices: "".to_string(),
            },
        ];

        // 1. Should not rewrite relative imports
        assert_eq!(resolve_and_rewrite_import("./components/Header", &db), None);
        assert_eq!(resolve_and_rewrite_import("../hooks/useAuth", &db), None);
        assert_eq!(resolve_and_rewrite_import("@/lib/prisma", &db), None);

        // 2. Should rewrite exact aliases
        assert_eq!(resolve_and_rewrite_import("@resend/client", &db), Some("resend".to_string()));
        assert_eq!(resolve_and_rewrite_import("resend-node", &db), Some("resend".to_string()));

        // 3. Should rewrite next-auth client aliases specifically to next-auth/react
        assert_eq!(resolve_and_rewrite_import("@next-auth/client", &db), Some("next-auth/react".to_string()));
        assert_eq!(resolve_and_rewrite_import("next-auth/client", &db), Some("next-auth/react".to_string()));

        // 4. Should rewrite invalid subpaths of package name
        assert_eq!(resolve_and_rewrite_import("resend/client", &db), Some("resend".to_string()));
        assert_eq!(resolve_and_rewrite_import("resend/something/else", &db), Some("resend".to_string()));

        // 5. Should NOT rewrite valid subpaths of package name
        assert_eq!(resolve_and_rewrite_import("@hookform/resolvers/zod", &db), None);
        assert_eq!(resolve_and_rewrite_import("next-auth/react", &db), None);
    }


    #[test]
    fn test_analyze_npm_install_failure() {
        // Test ETARGET (version mismatch)
        let etarget_log = "npm error code ETARGET\nnpm error notarget No matching version found for tw-elements-react@^4.0.0.\nnpm error notarget In most cases you or one of your dependencies...";
        let res1 = analyze_npm_install_failure(etarget_log);
        assert!(matches!(res1, Some(NpmFailureAction::SetWildcard(ref p)) if p == "tw-elements-react"));

        // Test E404 (not in registry)
        let e404_log = "npm error code E404\nnpm error 404 'some-bad-package' is not in the npm registry.";
        let res2 = analyze_npm_install_failure(e404_log);
        assert!(matches!(res2, Some(NpmFailureAction::Remove(ref p)) if p == "some-bad-package"));

        // Test E404 (legacy/requested resource)
        let legacy_e404_log = "npm ERR! 404 Not Found\nnpm ERR! 404  requested resource 'another-bad-package@^1.0.0'";
        let res3 = analyze_npm_install_failure(legacy_e404_log);
        assert!(matches!(res3, Some(NpmFailureAction::Remove(ref p)) if p == "another-bad-package"));

        // Test E404 (GET request style)
        let get_e404_log = "npm ERR! 404 Not Found - GET https://registry.npmjs.org/third-bad-package - Not found";
        let res4 = analyze_npm_install_failure(get_e404_log);
        assert!(matches!(res4, Some(NpmFailureAction::Remove(ref p)) if p == "third-bad-package"));
    }


    #[test]
    fn test_extract_package_name_from_error() {
        let input1 = "Error: Cannot find module 'decimal.js'";
        assert_eq!(extract_package_name_from_error(input1), Some("decimal.js".to_string()));

        let input2 = "Module not found: Can't resolve '@emotion/react' in 'C:\\projects\\...'";
        assert_eq!(extract_package_name_from_error(input2), Some("@emotion/react".to_string()));

        let input3 = "Error: Cannot find module './extensions'";
        assert_eq!(extract_package_name_from_error(input3), None);

        let input4 = "Error: Cannot find module '../utils/helpers'";
        assert_eq!(extract_package_name_from_error(input4), None);

        let input5 = "Error: Cannot find module 'C:\\projects\\app\\extensions'";
        assert_eq!(extract_package_name_from_error(input5), None);

        let input6 = "Error: Cannot find module 'C:/projects/app/extensions'";
        assert_eq!(extract_package_name_from_error(input6), None);
    }


    #[test]
    fn test_npm_self_healing_shell_quote_and_webpack_dev_server() {
        // 1. ETARGET parser extracts package name cleanly for transitive dependency `shell-quote@^1.10.0.`
        let npm_log = "npm error code ETARGET\nnpm error notarget No matching version found for shell-quote@^1.10.0.\nnpm error notarget In most cases you or one of your dependencies...";
        let parsed = analyze_npm_install_failure(npm_log);
        assert!(matches!(parsed, Some(NpmFailureAction::SetWildcard(ref p)) if p == "shell-quote"));

        // 2. Offline sanitizer strips hallucinated `webpack-dev-server` etc.
        let mut pkg_json = serde_json::json!({
            "name": "carbonchain-navigator",
            "dependencies": {
                "@auth/prisma-adapter": "^1.0.12",
                "@prisma/client": "5.22.0",
                "next": "14.2.35",
                "webpack-dev-server": "^4.7.3",
                "webpack-hot-middleware": "^2.25.4"
            },
            "devDependencies": {
                "html-webpack-plugin": "^5.6.0",
                "prisma": "5.22.0"
            }
        });
        assert!(sanitize_package_json_value_for_offline(&mut pkg_json));
        let deps = pkg_json["dependencies"].as_object().unwrap();
        let dev_deps = pkg_json["devDependencies"].as_object().unwrap();
        assert!(!deps.contains_key("webpack-dev-server"));
        assert!(!deps.contains_key("webpack-hot-middleware"));
        assert!(!dev_deps.contains_key("html-webpack-plugin"));
        assert!(deps.contains_key("@auth/prisma-adapter"));
        assert!(deps.contains_key("next"));

        // 3. No global wildcard override without parent; parent-scoped override only when parent is known
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_pkg_heal_test_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let pkg_path = temp_dir.join("package.json");
        std::fs::write(
            &pkg_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "name": "test-app",
                "dependencies": {
                    "next": "14.2.35",
                    "some-direct-pkg": "^1.2.3"
                },
                "devDependencies": {
                    "webpack-dev-server": "^4.15.2"
                }
            })).unwrap(),
        ).unwrap();

        // Unknown parent -> refuses to pollute package.json with global overrides
        assert!(!apply_npm_etarget_heal(&pkg_path, "shell-quote", Some(npm_log)));
        let after_unknown: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert!(after_unknown.get("overrides").is_none());

        // Known parent in log -> writes parent-scoped override once, second call is a no-op
        let parented_npm_log = "npm error code ETARGET\nnpm error While resolving: test-app@0.1.0\nnpm error notarget No matching version found for shell-quote@^1.10.0.\nnpm error requested by webpack-dev-server@4.15.2";
        assert!(apply_npm_etarget_heal(&pkg_path, "shell-quote", Some(parented_npm_log)));
        assert!(!apply_npm_etarget_heal(&pkg_path, "shell-quote", Some(parented_npm_log)));
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert!(saved["overrides"].get("shell-quote").is_none(), "Must NEVER emit global overrides['shell-quote']!");
        assert_eq!(saved["overrides"]["webpack-dev-server"]["shell-quote"], serde_json::json!("*"));

        // Direct non-core dependency: first call mutates "^1.2.3" -> "*" and returns true; second call returns false (no-op)
        assert!(set_package_version_to_wildcard(&pkg_path, "some-direct-pkg"));
        assert!(!set_package_version_to_wildcard(&pkg_path, "some-direct-pkg"));
        let saved2: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert_eq!(saved2["dependencies"]["some-direct-pkg"], serde_json::json!("*"));

        // Core framework lock: `next` can NEVER be downgraded to "*" or removed
        assert!(!set_package_version_to_wildcard(&pkg_path, "next"));
        assert!(!remove_package_from_json(&pkg_path, "next"));
        let saved3: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert_eq!(saved3["dependencies"]["next"], serde_json::json!("14.2.35"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_npm_offline_network_detection_and_scoped_override_safety() {
        assert!(is_npm_network_error("npm error code ENOTFOUND\nnpm error syscall getaddrinfo"));
        assert!(is_npm_network_error("npm error errno EAI_NODATA"));
        assert!(is_npm_network_error("npm error network request failed: fetch failed"));
        assert!(is_npm_network_error("npm error code ENOTCACHED\nnpm error request to https://registry.npmjs.org/next failed, reason: cache mode is 'only-if-cached'"));
        assert!(!is_npm_network_error("npm warn deprecated offline-plugin@5.0.0"));
        assert!(!is_npm_network_error("npm error code ETARGET\nnpm error notarget No matching version found for shell-quote@^1.10.0."));
        assert!(!is_npm_network_error("npm error code E404\nnpm error 404 Not Found - GET https://registry.npmjs.org/nonexistent"));

        // Non-Next.js Webpack/Express project keeps webpack-dev-server
        let mut standalone_webpack = serde_json::json!({
            "name": "legacy-spa",
            "scripts": { "dev": "webpack serve --mode development" },
            "dependencies": { "express": "^4.18.0" },
            "devDependencies": { "webpack": "^5.0.0", "webpack-dev-server": "^4.7.3" }
        });
        assert!(!sanitize_package_json_value_for_offline(&mut standalone_webpack));
        assert!(standalone_webpack["devDependencies"].get("webpack-dev-server").is_some());

        // Scoped transitive dependency override must NEVER delete @auth/* core/direct deps or write global overrides
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_scoped_override_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let pkg_path = temp_dir.join("package.json");
        std::fs::write(
            &pkg_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "name": "auth-app",
                "dependencies": {
                    "next": "14.2.35",
                    "next-auth": "^4.24.7",
                    "@auth/prisma-adapter": "^1.0.12"
                }
            })).unwrap(),
        ).unwrap();

        // Without parent evidence, returns false and adds no global override
        assert!(!set_package_version_to_wildcard(&pkg_path, "@auth/core"));
        // With parent evidence in npm ERESOLVE/stderr log, writes strictly parent-scoped override
        let auth_log = "npm error Could not resolve dependency:\nnpm error peer @auth/core@^0.18.0 from @auth/prisma-adapter@1.0.12";
        assert!(apply_npm_etarget_heal(&pkg_path, "@auth/core", Some(auth_log)));
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert!(saved["dependencies"].get("@auth/prisma-adapter").is_some(), "@auth/prisma-adapter must NOT be deleted!");
        assert!(saved["overrides"].get("@auth/core").is_none(), "Global @auth/core override must NEVER be written!");
        assert_eq!(saved["overrides"]["@auth/prisma-adapter"]["@auth/core"], serde_json::json!("*"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_extract_offending_parent_from_debug_log_and_core_lock() {
        // Core framework package lock verification
        for core_pkg in &[
            "next",
            "react",
            "react-dom",
            "prisma",
            "@prisma/client",
            "next-auth",
            "@auth/prisma-adapter",
            "typescript",
            "tailwindcss",
        ] {
            assert!(is_core_framework_package(core_pkg), "{} must be core-locked", core_pkg);
        }
        assert!(!is_core_framework_package("lucide-react"));
        assert!(!is_core_framework_package("shell-quote"));
        assert!(!is_core_framework_package("recharts"));

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_parent_log_test_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let pkg_path = temp_dir.join("package.json");
        std::fs::write(
            &pkg_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "name": "decentral-grid",
                "dependencies": {
                    "next": "14.2.35",
                    "react": "^18.2.0",
                    "@prisma/client": "5.22.0",
                    "next-auth": "^4.24.7"
                },
                "devDependencies": {
                    "prisma": "5.22.0",
                    "some-legacy-tool": "^2.1.0"
                }
            })).unwrap(),
        ).unwrap();

        // Core framework packages cannot be modified or removed via self-healing
        for protected in &["next", "react", "@prisma/client", "prisma", "next-auth"] {
            assert!(!apply_npm_etarget_heal(&pkg_path, protected, None));
            assert!(!remove_package_from_json(&pkg_path, protected));
        }

        // Mock npm _logs/debug-0.log referenced by stderr
        let debug_log_path = temp_dir.join("2026-11-10T07_12_26_520Z-debug-0.log");
        std::fs::write(
            &debug_log_path,
            "0 verbose cli node npm\n12 silly placeDep ROOT shell-quote@^1.10.0 KEEP for: some-legacy-tool@2.1.0 want: ^1.10.0\n13 error notarget No matching version found for shell-quote@^1.10.0.\n",
        ).unwrap();

        let stderr_only = format!(
            "npm error code ETARGET\nnpm error While resolving: decentral-grid@0.1.0\nnpm error notarget No matching version found for shell-quote@^1.10.0.\nnpm error A complete log of this run can be found in: {}",
            debug_log_path.display()
        );
        let direct = vec!["next".to_string(), "some-legacy-tool".to_string()];
        let extracted = extract_offending_parent_from_npm_log(
            &stderr_only,
            "shell-quote",
            &direct,
            Some("decentral-grid"),
        );
        assert_eq!(extracted.as_deref(), Some("some-legacy-tool"));

        assert!(apply_npm_etarget_heal(&pkg_path, "shell-quote", Some(&stderr_only)));
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert_eq!(saved["overrides"]["some-legacy-tool"]["shell-quote"], serde_json::json!("*"));
        assert!(saved["overrides"].get("decentral-grid").is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_local_import_guard_and_package_repair_v47() {
        // 1. Bare local specifiers must never be mistaken for npm packages
        assert!(crate::packages::is_local_import_specifier("src/models/MaintenanceAlert"));
        assert!(crate::packages::is_local_import_specifier("app/dashboard/page"));
        assert!(crate::packages::is_local_import_specifier("components/Navbar"));
        assert!(crate::packages::is_local_import_specifier("@/hooks/useMaintenanceAlerts"));
        assert!(crate::packages::is_local_import_specifier("~/lib/prisma"));
        assert!(!crate::packages::is_local_import_specifier("lucide-react"));
        assert!(!crate::packages::is_local_import_specifier("next-auth/providers/credentials"));

        // 2. Official next-auth v4 subpaths must not be mangled even against incomplete cached DBs
        let mock_db = vec![PackageMeta {
            name: "next-auth".to_string(),
            version: "^4.24.7".to_string(),
            aliases: vec![],
            import_examples: vec!["import NextAuth from 'next-auth';".to_string()],
            compatibility: serde_json::Map::new(),
            best_practices: String::new(),
        }];
        assert_eq!(resolve_and_rewrite_import("next-auth/providers/credentials", &mock_db), None);
        assert_eq!(resolve_and_rewrite_import("next-auth/react", &mock_db), None);
        assert_eq!(resolve_and_rewrite_import("next-auth/next", &mock_db), None);
        assert_eq!(resolve_and_rewrite_import("next-auth/middleware", &mock_db), None);
        assert_eq!(resolve_and_rewrite_import("src/models/MaintenanceAlert", &mock_db), None);

        // 3. Server log package extractor must ignore local paths & builtins, and normalize scoped/subpath packages
        assert_eq!(
            crate::server::extract_package_name_from_error("Module not found: Can't resolve 'src/models/MaintenanceAlert'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Cannot find module '@/hooks/useMaintenanceAlerts'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Can't resolve '@components/Button'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Cannot find module 'node:fs'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Cannot find module 'fs'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Can't resolve 'next/navigation'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Can't resolve 'react'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Can't resolve 'next-auth/providers/credentials'"),
            Some("next-auth".to_string())
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Can't resolve '@mui/material/Button'"),
            Some("@mui/material".to_string())
        );

        // 4. repair_package_json_dependencies rewrites src/... and ~/... imports to @/... and scrubs bogus local deps
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_pkg_repair_v47_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/app")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::write(temp_dir.join("src/app/page.tsx"), "export default function Home() { return null; }").unwrap();
        std::fs::write(
            temp_dir.join("src/hooks/useMaintenanceAlerts.ts"),
            "import { MaintenanceAlert } from 'src/models/MaintenanceAlert';\nimport { cn } from '~/lib/utils';\nimport { z } from 'zod';\nimport { Wrench } from 'lucide-react';\nexport { MaintenanceAlert, cn, z, Wrench };\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("package.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "name": "shoecraft-test",
                "dependencies": {
                    "next": "14.2.35",
                    "react": "^18.2.0",
                    "src": "*",
                    "models": "*"
                },
                "devDependencies": {}
            }))
            .unwrap(),
        )
        .unwrap();

        crate::packages::repair_package_json_dependencies(&temp_dir);

        let updated_hook = std::fs::read_to_string(temp_dir.join("src/hooks/useMaintenanceAlerts.ts")).unwrap();
        assert!(updated_hook.contains("from '@/models/MaintenanceAlert'"));
        assert!(updated_hook.contains("from '@/lib/utils'"));
        assert!(!updated_hook.contains("'src/models/MaintenanceAlert'"));

        let updated_pkg: Value = serde_json::from_str(&std::fs::read_to_string(temp_dir.join("package.json")).unwrap()).unwrap();
        let deps = updated_pkg["dependencies"].as_object().unwrap();
        assert!(!deps.contains_key("src"));
        assert!(!deps.contains_key("models"));
        assert!(!deps.contains_key("app"));
        assert!(deps.contains_key("zod"));
        assert!(deps.contains_key("lucide-react"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_relevant_packages_db_summary_scoping() {
        let make_pkg = |name: &str, ex: &str| crate::packages::PackageMeta {
            name: name.to_string(),
            version: "^1.0.0".to_string(),
            aliases: vec![],
            import_examples: vec![ex.to_string()],
            compatibility: serde_json::Map::new(),
            best_practices: format!("Use {} safely", name),
        };
        let db = vec![
            make_pkg("next-auth", "import { getServerSession } from 'next-auth';"),
            make_pkg("@prisma/client", "import { PrismaClient } from '@prisma/client';"),
            make_pkg("zod", "import { z } from 'zod';"),
            make_pkg("lucide-react", "import { Home } from 'lucide-react';"),
            make_pkg("react-icons", "import { HiHome } from 'react-icons/hi';"),
            make_pkg("stripe", "import Stripe from 'stripe';"),
            make_pkg("ioredis", "import Redis from 'ioredis';"),
            make_pkg("bullmq", "import { Queue } from 'bullmq';"),
            make_pkg("phaser", "import Phaser from 'phaser';"),
        ];
        let pkg_deps = vec!["next".to_string(), "react".to_string(), "stripe".to_string()];
        let ctx = "Error in src/app/api/auth/route.ts: Cannot find module 'zod' when using getServerSession";
        let summary = crate::packages::format_relevant_packages_db_summary(&db, &pkg_deps, ctx);

        assert!(summary.contains("zod"));
        assert!(summary.contains("next-auth"));
        assert!(summary.contains("stripe"));
        assert!(!summary.contains("phaser"));
        assert!(!summary.contains("ioredis"));
        assert!(!summary.contains("bullmq"));
    }


    #[test]
    fn test_packages_foreign_cleanup_and_stripe_version_preservation_v51() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v51_pkgs_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/utils")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app")).unwrap();
        std::fs::write(
            temp_dir.join("src/utils/stripe.ts"),
            "import Stripe from 'stripe';\nexport const stripe = new Stripe(process.env.STRIPE_SECRET_KEY || '');\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/utils/i18n.ts"),
            "import { createI18n } from 'vue-i18n';\nexport const i18n = createI18n({});\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("package.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "name": "test-app",
                "dependencies": {
                    "next": "14.2.35",
                    "react": "^18.2.0",
                    "react-dom": "^18.2.0",
                    "stripe": "^23.1.0",
                    "vue-i18n": "^9.0.0",
                    "svelte": "^4.0.0"
                },
                "devDependencies": {}
            })).unwrap(),
        ).unwrap();

        crate::packages::repair_package_json_dependencies(&temp_dir);

        let updated_pkg: Value = serde_json::from_str(&std::fs::read_to_string(temp_dir.join("package.json")).unwrap()).unwrap();
        let deps = updated_pkg["dependencies"].as_object().unwrap();
        assert!(!deps.contains_key("vue-i18n"), "vue-i18n must be removed from Next.js package.json: {:?}", deps);
        assert!(!deps.contains_key("svelte"), "svelte must be removed from Next.js package.json: {:?}", deps);
        assert_eq!(
            deps.get("stripe").and_then(|v| v.as_str()),
            Some("^23.1.0"),
            "Installed stripe semver lock must not be clobbered by wildcard DB entry: {:?}",
            deps
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


