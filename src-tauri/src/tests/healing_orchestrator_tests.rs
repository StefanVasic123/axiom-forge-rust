use crate::*;

    #[test]
    fn test_ensure_zod_imports() {
        let route_code = "import { NextResponse } from 'next/server';\n\nconst schema = z.object({ id: z.string() });\n\nexport async function POST() { return NextResponse.json({}); }";
        let with_zod = ensure_zod_imports(route_code);
        assert!(with_zod.contains("import { z } from 'zod';"));

        let processed = post_process_generated_file("src/app/api/subscriptions/route.ts", route_code, &serde_json::json!({}), false);
        assert!(processed.contains("import { z } from 'zod';"));
    }


    #[test]
    fn test_log_throttler_dedup_and_rate_limit() {
        let mut throttler = LogThrottler::new();
        assert!(throttler.filter_line("POST /api/subscriptions 500").is_some());
        assert!(throttler.filter_line("POST /api/subscriptions 500").is_some());
        assert!(throttler.filter_line("POST /api/subscriptions 500").is_none());
        assert!(throttler.filter_line("POST /api/subscriptions 500").is_none());

        let diff = throttler.filter_line("✓ Compiled in 200ms");
        assert!(diff.is_some());
        assert!(diff.unwrap().contains("Prethodna poruka ponovljena ukupno 4 puta"));
    }


    #[test]
    fn test_extract_edits_from_json_fixes_and_code() {
        // Test JSON with "fixes" array containing "code" (exact response from qwen on layout.tsx)
        let json_fixes = serde_json::json!({
            "fixes": [
                {
                    "file": "src/app/layout.tsx",
                    "line": 12,
                    "description": "Unexpected token html",
                    "code": "export default function RootLayout({ children }: { children: React.ReactNode }) { return (<html><body>{children}</body></html>); }"
                }
            ]
        });
        let edits = extract_edits_from_json(&json_fixes, "src/app/layout.tsx");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["path"], "src/app/layout.tsx");
        assert!(edits[0]["replacement"].as_str().unwrap().contains("export default function RootLayout"));

        // Test JSON with arbitrary nested structure and custom key names
        let json_nested = serde_json::json!({
            "response": {
                "solution": {
                    "target_file": "src/app/layout.tsx",
                    "updated_code": "export default function Layout() { return <div>Hello</div>; }"
                }
            }
        });
        let edits_nested = extract_edits_from_json(&json_nested, "src/app/layout.tsx");
        assert_eq!(edits_nested.len(), 1);
        assert_eq!(edits_nested[0]["path"], "src/app/layout.tsx");
        assert!(edits_nested[0]["replacement"].as_str().unwrap().contains("export default function Layout"));

        // Test JSON with outer file and inner "path" containing target code snippet (Navbar.tsx case)
        let json_router_fix = serde_json::json!({
            "file": "src/components/Navbar.tsx",
            "changes": [
                {
                    "type": "replace",
                    "path": "import { useRouter } from 'next/router';",
                    "replacement": "import { useRouter } from 'next/navigation';"
                }
            ]
        });
        let edits_router = extract_edits_from_json(&json_router_fix, "src/components/Navbar.tsx");
        assert_eq!(edits_router.len(), 1);
        assert_eq!(edits_router[0]["path"], "src/components/Navbar.tsx");
        assert_eq!(edits_router[0]["target"], "import { useRouter } from 'next/router';");
        assert_eq!(edits_router[0]["replacement"], "import { useRouter } from 'next/navigation';");

        // Test post_process_generated_file safely unwraps raw JSON envelope into code
        let raw_json_file = "{\n  \"fixes\": [\n    {\n      \"file\": \"src/app/layout.tsx\",\n      \"code\": \"export default function Clean() { return <div>Clean</div>; }\"\n    }\n  ]\n}";
        let processed = post_process_generated_file("src/app/layout.tsx", raw_json_file, &serde_json::json!({}), false);
        assert!(!processed.contains("\"fixes\":"));
        assert!(processed.contains("export default function Clean"));
    }


    #[test]
    fn test_clean_cli_output_and_extract_failing_lines() {
        let raw = "\u{1b}[1;91merror\u{1b}[0m: Invalid field type\n  -->  [4mprisma\\schema.prisma:2[0m\n  -->  prisma\\schema.prisma:2";
        let clean = clean_cli_output_for_llm(raw);
        assert!(!clean.contains("[1;91m"));
        assert!(!clean.contains("[0m"));
        assert!(!clean.contains('\u{1b}'));
        let schema = "model A {\n  userId String @relation(fields: [userId], references: [id])\n}";
        let lines = extract_prisma_error_lines(&clean, schema);
        assert_eq!(lines.matches("Line 2:").count(), 1);
        assert!(lines.contains("userId String @relation"));
    }


    #[test]
    fn test_get_generation_phase() {
        assert_eq!(get_generation_phase("package.json", "nextjs"), GenerationPhase::Config);
        assert_eq!(get_generation_phase("tsconfig.json", "nextjs"), GenerationPhase::Config);
        assert_eq!(get_generation_phase("schema.prisma", "nextjs"), GenerationPhase::DatabaseOrMain);
        assert_eq!(get_generation_phase("src/app/api/auth/[...nextauth]/route.ts", "nextjs"), GenerationPhase::ApiOrState);
        assert_eq!(get_generation_phase("src/hooks/useAuth.ts", "nextjs"), GenerationPhase::LogicOrHooks);
        assert_eq!(get_generation_phase("src/app/layout.tsx", "nextjs"), GenerationPhase::ShellOrWidgets);
        assert_eq!(get_generation_phase("src/app/dashboard/page.tsx", "nextjs"), GenerationPhase::PagesOrUi);

        assert_eq!(get_generation_phase("main.js", "electron"), GenerationPhase::DatabaseOrMain);
        assert_eq!(get_generation_phase("preload.js", "electron"), GenerationPhase::DatabaseOrMain);
        assert_eq!(get_generation_phase("src/components/Counter.tsx", "electron"), GenerationPhase::PagesOrUi);
    }


    #[test]
    fn test_resolve_import_path() {
        let temp_dir = std::env::temp_dir().join("axiom_test_import_resolve");
        let _ = std::fs::create_dir_all(&temp_dir);
        let src_dir = temp_dir.join("src").join("app").join("components");
        let _ = std::fs::create_dir_all(&src_dir);
        
        let card_file = src_dir.join("ProductCard.tsx");
        let _ = std::fs::write(&card_file, "export const ProductCard = () => null;");
        
        // Test absolute resolve
        let resolved = resolve_import_path(&temp_dir, "@/app/components/ProductCard", "src/app/page.tsx");
        assert_eq!(resolved, Some("src/app/components/ProductCard.tsx".to_string()));
        
        // Test relative resolve (canonical path collapses `..` segments cleanly)
        let resolved_rel = resolve_import_path(&temp_dir, "../components/ProductCard", "src/app/pages/page.tsx");
        assert_eq!(resolved_rel, Some("src/app/components/ProductCard.tsx".to_string()));
        
        // Clean up
        let _ = std::fs::remove_file(card_file);
        let _ = std::fs::remove_dir_all(temp_dir);
    }


    #[test]
    fn test_generate_axiom_map() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        
        let src_hooks = temp_dir.join("src/hooks");
        std::fs::create_dir_all(&src_hooks).unwrap();
        
        let hook_content = r#"
import axios from 'axios';
export function useAuth() {
    return "auth";
}
"#;
        std::fs::write(src_hooks.join("useAuth.ts"), hook_content).unwrap();

        let comp_dir = temp_dir.join("src/components");
        std::fs::create_dir_all(&comp_dir).unwrap();
        
        let comp_content = r#"
import { useAuth } from '@/hooks/useAuth';
export default function Dashboard() {
    const auth = useAuth();
    return "dashboard";
}
export const version = "1.0.0";
"#;
        std::fs::write(comp_dir.join("Dashboard.tsx"), comp_content).unwrap();

        let map_res = generate_axiom_map(&temp_dir);
        assert!(map_res.is_ok());

        let map_path = temp_dir.join("axiom-map.json");
        assert!(map_path.exists());

        let map_content = std::fs::read_to_string(map_path).unwrap();
        let map_val: Value = serde_json::from_str(&map_content).unwrap();

        assert!(map_val.get("files").unwrap().get("src/hooks/useAuth.ts").is_some());
        assert!(map_val.get("files").unwrap().get("src/components/Dashboard.tsx").is_some());

        let dash_entry = map_val.get("files").unwrap().get("src/components/Dashboard.tsx").unwrap();
        assert!(dash_entry.get("exports").unwrap().as_array().unwrap().contains(&serde_json::json!("default")));
        assert!(dash_entry.get("exports").unwrap().as_array().unwrap().contains(&serde_json::json!("version")));
        assert!(dash_entry.get("dependencies").unwrap().as_array().unwrap().contains(&serde_json::json!("src/hooks/useAuth.ts")));

        let _ = std::fs::remove_dir_all(temp_dir);
    }


    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1B[31mx\x1B[0m You're importing a component that needs \u{1b}[36museState\u{1b}[0m.";
        let cleaned = strip_ansi_codes(input);
        assert_eq!(cleaned, "x You're importing a component that needs useState.");
    }


    #[test]
    fn test_extract_file_path_from_line() {
        let input1 = " ⨯ ./src/hooks/useAuth.ts";
        assert_eq!(extract_file_path_from_line(input1), Some("src/hooks/useAuth.ts".to_string()));

        let input2 = "Error: C:\\projects\\app\\src\\components\\Button.tsx:10:15";
        assert_eq!(extract_file_path_from_line(input2), Some("C:\\projects\\app\\src\\components\\Button.tsx".to_string()));

        let input3 = "   ,-[C:\\projects\\app\\src\\hooks\\useAuth.ts:1:1]";
        assert_eq!(extract_file_path_from_line(input3), Some("C:\\projects\\app\\src\\hooks\\useAuth.ts".to_string()));

        let input4 = "  at Home (./src/app/page.tsx:15:98)";
        assert_eq!(extract_file_path_from_line(input4), Some("src/app/page.tsx".to_string()));

        let input5 = "  at Module._compile (C:\\projects\\node_modules\\next\\dist\\server\\require.js:5:10)";
        assert_eq!(extract_file_path_from_line(input5), None);

        let input6 = "  at eval (./.next/server/app/products/page.js:151:1)";
        assert_eq!(extract_file_path_from_line(input6), None);

        let input7 = "Unhandled Runtime Error: Error: The default export is not a React Component in page: \"/\"";
        assert_eq!(extract_file_path_from_line(input7), Some("src/app/page.tsx".to_string()));

        let input8 = "Error: The default export is not a React Component in page: \"/dashboard\"";
        assert_eq!(extract_file_path_from_line(input8), Some("src/app/dashboard/page.tsx".to_string()));

        let input9 = "Error: Handler is not defined in page: \"/api/auth/[...nextauth]\"";
        assert_eq!(extract_file_path_from_line(input9), Some("src/app/api/auth/[...nextauth]/route.ts".to_string()));
    }


    #[test]
    fn test_get_project_path() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        
        // When project is not registered, it should fallback to internal projects dir
        let fallback_path = get_project_path("non_existent_project");
        assert!(fallback_path.ends_with("non_existent_project"));
        
        let _ = std::fs::remove_dir_all(temp_dir);
    }


    #[test]
    fn test_get_stack_rulepacks() {
        // Test 1: React + NextIntl file and error detection
        let file_path = "src/components/header.tsx";
        let content = "import { useTranslations } from 'next-intl'; export function Header() { const t = useTranslations('Header'); }";
        let err = "Error: Failed to call `useTranslations` because the context from `NextIntlClientProvider` was not found.";

        let rules = get_stack_rulepacks(file_path, content, Some(err));
        assert!(rules.contains("REACT JSX SYNTAX, HOOKS & STYLING RULES"));
        assert!(rules.contains("NEVER USE STRING STYLE ATTRIBUTES"));
        assert!(rules.contains("NEVER CALL HOOKS AT MODULE SCOPE"));
        assert!(rules.contains("NEXT-INTL (INTERNATIONALIZATION) RULES"));
        assert!(rules.contains("useTranslations SIGNATURE ENFORCEMENT"));
        assert!(rules.contains("NEXT.JS LINK COMPONENT RULES") == false);

        // Test 2: Next.js Link component
        let link_content = "import Link from 'next/link'; export default function Nav() { return <Link href='/'>Home</Link>; }";
        let link_rules = get_stack_rulepacks("src/components/Nav.tsx", link_content, None);
        assert!(link_rules.contains("NEXT.JS LINK COMPONENT RULES"));
        assert!(link_rules.contains("PREFETCH OPTIMIZATION"));

        // Test 3: Prisma schema
        let prisma_rules = get_stack_rulepacks("prisma/schema.prisma", "datasource db { provider = \"sqlite\" }", None);
        assert!(prisma_rules.contains("PRISMA ORM & DATABASE RULES"));
        assert!(prisma_rules.contains("SERVER-ONLY BOUNDARY"));

        // Test 4: MERN server.js
        let mern_rules = get_stack_rulepacks("server.js", "const express = require('express'); const app = express();", None);
        assert!(mern_rules.contains("MERN STACK (EXPRESS + MONGOOSE + VITE/REACT) RULES"));
        assert!(mern_rules.contains("EXPRESS ASYNC ROUTE SAFETY"));

        // Test 5: React Native / Expo screen
        let rn_rules = get_stack_rulepacks("src/screens/HomeScreen.tsx", "import { View, Text } from 'react-native';", None);
        assert!(rn_rules.contains("REACT NATIVE & EXPO MOBILE RULES"));
        assert!(rn_rules.contains("ABSOLUTE WEB DOM ELEMENT BAN"));
        // React Native should NOT have web React rules injected
        assert!(!rn_rules.contains("REACT JSX SYNTAX, HOOKS & STYLING RULES"));

        // Test 6: Desktop Electron preload
        let desktop_rules = get_stack_rulepacks("preload.js", "const { contextBridge, ipcRenderer } = require('electron');", None);
        assert!(desktop_rules.contains("DESKTOP (ELECTRON & TAURI) RULES"));
        assert!(desktop_rules.contains("ELECTRON CONTEXT ISOLATION"));

        // Test 7: Unrelated plain file
        let plain_rules = get_stack_rulepacks("README.md", "# My Project", None);
        assert!(plain_rules.is_empty());

        // Test 8: Configuration files (tsconfig.json)
        let config_rules = get_stack_rulepacks("tsconfig.json", "{}", None);
        assert!(config_rules.contains("CONFIGURATION & TSCONFIG RULES"));
        assert!(config_rules.contains("TSCONFIG.JSON CONCISENESS & VALIDITY"));

        // Test 9: Middleware on error
        let mw_err_rules = get_stack_rulepacks("src/middleware.ts", "", Some("Error parsing `/dashboard/*`: Invalid middlewares found"));
        assert!(mw_err_rules.contains("NEXT.JS MIDDLEWARE & EDGE ROUTING RULES"));
        assert!(mw_err_rules.contains("PATH-TO-REGEXP MATCHERS"));
    }


    #[test]
    fn test_apply_targeted_edit() {
        let original = "
        <Toolbar>
          <IconButton edge=\"start\">
            <FaBars />
          </IconButton>
          <Typography variant=\"h6\" component=\"div\" sx={{ flexGrow: 1, ml: 1 }}>
            Axiom Dev
          </Typography>
          <IconButton>
            <IoMdNotifications />
          </IconButton>
        </Toolbar>
";

        // LLM generates target compressed onto one line
        let target = "<Typography variant=\"h6\" component=\"div\" sx={{ flexGrow: 1, ml: 1 }}>Axiom Dev</Typography>";
        let replacement = "<Typography variant=\"h6\" component=\"div\" sx={{ flexGrow: 1, ml: 1 }}>Construction Smart System</Typography>";

        let result = apply_targeted_edit(original, target, replacement);
        assert!(result.is_ok(), "Targeted edit should succeed: {:?}", result.err());
        let modified = result.unwrap();
        assert!(modified.contains("Construction Smart System"));
        assert!(!modified.contains("Axiom Dev"));
    }


    #[test]
    fn test_extract_related_project_files_for_error() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_rel_test_{}", uuid::Uuid::new_v4()));
        let comp_dir = temp_dir.join("src/components");
        let _ = std::fs::create_dir_all(&comp_dir);

        let navbar_file = comp_dir.join("Navbar.tsx");
        std::fs::write(&navbar_file, "export default function Navbar() { return <nav>Nav</nav>; }").unwrap();

        let layout_content = "import React from 'react';\nimport Navbar from '@/components/Navbar';\nexport default function RootLayout() { return <Navbar />; }";
        let err_msg = "Attempted import error: '@/components/Navbar' does not contain a default export (imported as 'Navbar').";

        let related = extract_related_project_files_for_error(&temp_dir, layout_content, "src/app/layout.tsx", err_msg);
        assert!(!related.is_empty());
        assert_eq!(related[0].0, "src/components/Navbar.tsx");
        assert!(related[0].1.contains("Navbar"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }


    #[test]
    fn test_apply_targeted_edit_duplicated_links() {
        let content = "
        <nav className=\"hidden md:flex\">
          <Link href=\"/data-grid\" className=\"text-gray-700\">Data Grid</Link>
        </nav>
        <div className=\"mobile-menu md:hidden\">
          <Link href=\"/data-grid\" className=\"text-gray-700\">Data Grid</Link>
        </div>
        ";
        let target = "<Link href=\"/data-grid\" className=\"text-gray-700\">Data Grid</Link>";
        let replacement = "<Link href=\"/dashboard/data-grid\" className=\"text-gray-700\">Data Grid</Link>";

        let result = apply_targeted_edit(content, target, replacement);
        assert!(result.is_ok(), "Specific duplicate target should be replaced across all occurrences");
        let modified = result.unwrap();
        assert!(!modified.contains("href=\"/data-grid\""));
        assert_eq!(modified.matches("href=\"/dashboard/data-grid\"").count(), 2);
    }


    #[test]
    fn test_resolve_best_matching_route() {
        let valid_routes = vec![
            "/".to_string(),
            "/dashboard".to_string(),
            "/dashboard/data-grid".to_string(),
            "/dashboard/detailed-analytics".to_string(),
            "/dashboard/kpi-widgets".to_string(),
            "/dashboard/settings".to_string(),
        ];

        assert_eq!(resolve_best_matching_route("/data-grid", &valid_routes), Some("/dashboard/data-grid".to_string()));
        assert_eq!(resolve_best_matching_route("/detailed-analytics", &valid_routes), Some("/dashboard/detailed-analytics".to_string()));
        assert_eq!(resolve_best_matching_route("/settings-panel", &valid_routes), Some("/dashboard/settings".to_string()));
        assert_eq!(resolve_best_matching_route("/dashboard", &valid_routes), None);
        assert_eq!(resolve_best_matching_route("/", &valid_routes), None);
    }


    #[test]
    fn test_sanitize_unhandled_session_loading() {
        let input = "import { useSession } from 'next-auth/react';\nconst [loading, setLoading] = useState(true);\nuseEffect(() => {\n  if (session) {\n    fetchKpis();\n  }\n}, [session]);";
        let output = sanitize_unhandled_session_loading(input);
        assert!(output.contains("else {"));
        assert!(output.contains("setLoading(false);"));
    }



    #[test]
    fn test_dual_export_synchronization() {
        // Case 1: Button with only default export gets named export { Button }
        let button_input = r#"
import React from 'react';
const Button: React.FC<any> = ({ children }) => <button>{children}</button>;
export default Button;
"#;
        let button_output = post_process_generated_file("src/components/ui/button.tsx", button_input, &serde_json::json!({}), false);
        assert!(button_output.contains("export default Button;"));
        assert!(button_output.contains("export { Button };"));

        // Case 2: Card with named export gets export default Card;
        let card_input = r#"
import React from 'react';
export const Card: React.FC<any> = ({ children }) => <div>{children}</div>;
export const CardHeader: React.FC<any> = ({ children }) => <div>{children}</div>;
"#;
        let card_output = post_process_generated_file("src/components/ui/card.tsx", card_input, &serde_json::json!({}), false);
        assert!(card_output.contains("export const Card:"));
        assert!(card_output.contains("export default Card;"));

        // Case 3: Kebab-case file alert-dialog.tsx gets PascalCase named/default export
        let alert_input = r#"
import React from 'react';
export default function AlertDialog() { return <div />; }
"#;
        let alert_output = post_process_generated_file("src/components/ui/alert-dialog.tsx", alert_input, &serde_json::json!({}), false);
        assert!(alert_output.contains("export default function AlertDialog"));
        assert!(alert_output.contains("export { AlertDialog };"));
    }


    #[test]
    fn test_extract_edits_ignores_explanation_and_supports_deletion() {
        // 1. Explanation mentioning "export " must NEVER be extracted as code replacement
        let payload = serde_json::json!({
            "explanation": "The error indicates that the name `Skeleton` is defined multiple times in the file. This is likely due to an additional export of the `Skeleton` component without a default export.",
            "edits": [
                {
                    "target": "export const Skeleton: React.FC<SkeletonProps> = ({ className = '' }) => {",
                    "replacement": "const Skeleton: React.FC<SkeletonProps> = ({ className = '' }) => {"
                }
            ]
        });
        let edits = extract_edits_from_json(&payload, "src/components/ui/skeleton.tsx");
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0].get("target").and_then(|v| v.as_str()),
            Some("export const Skeleton: React.FC<SkeletonProps> = ({ className = '' }) => {")
        );
        assert_eq!(
            edits[0].get("replacement").and_then(|v| v.as_str()),
            Some("const Skeleton: React.FC<SkeletonProps> = ({ className = '' }) => {")
        );

        // 2. Empty replacement ("") with a valid target must be preserved as a deletion edit
        let deletion_payload = serde_json::json!({
            "explanation": "The error indicates that the name 'Skeleton' is defined multiple times in the file. This is likely due to an additional export of the 'Skeleton' component without a default export.",
            "edits": [
                {
                    "target": "export default Skeleton;",
                    "replacement": ""
                }
            ]
        });
        let del_edits = extract_edits_from_json(&deletion_payload, "src/components/ui/skeleton.tsx");
        assert_eq!(del_edits.len(), 1);
        assert_eq!(del_edits[0].get("target").and_then(|v| v.as_str()), Some("export default Skeleton;"));
        assert_eq!(del_edits[0].get("replacement").and_then(|v| v.as_str()), Some(""));

        // 3. Natural language prose mentioning keywords is not classified as code
        assert!(!looks_like_code_payload(
            "The error indicates that the name `Skeleton` is defined multiple times in the file. This is likely due to an additional export of the `Skeleton` component without a default export."
        ));
    }


    #[test]
    fn test_sanitize_unhandled_session_loading_strips_preview_blockers() {
        let analytics_input = r#"
"use client";
import React from 'react';
import { useSession } from 'next-auth/react';

const AnalyticsPage = () => {
  const { data: session } = useSession();

  if (!session) {
    return <div>Loading...</div>;
  }

  return (
    <div className="max-w-7xl mx-auto p-6">
      <h1>Analytics Overview</h1>
    </div>
  );
};
export default AnalyticsPage;
"#;
        let processed = post_process_generated_file("src/app/dashboard/analytics/page.tsx", analytics_input, &serde_json::json!({}), true);
        assert!(!processed.contains("return <div>Loading...</div>;"));
        assert!(processed.contains(r#"<div data-axiom-component="Page" data-axiom-file="src/app/dashboard/analytics/page.tsx" className="max-w-7xl mx-auto p-6">"#));

        let dash_input = r#"
"use client";
import React from 'react';
import { useSession } from 'next-auth/react';
import Link from 'next/link';

export default function DashboardPage() {
  const { status } = useSession();
  if (status === 'unauthenticated') return <Link href="/login">Log in</Link>;
  return (
    <main className="p-8">
      <h1>Dashboard KPI</h1>
    </main>
  );
}
"#;
        let processed_dash = post_process_generated_file("src/app/dashboard/page.tsx", dash_input, &serde_json::json!({}), true);
        assert!(!processed_dash.contains("return <Link href=\"/login\">Log in</Link>;"));
        assert!(processed_dash.contains("<h1>Dashboard KPI</h1>"));
    }


    #[test]
    fn test_generate_project_metadata_resolves_alias_dependencies() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_meta_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let src_dir = temp_dir.join("src");
        let app_dir = src_dir.join("app");
        let components_dir = src_dir.join("components");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::create_dir_all(&components_dir).unwrap();

        let navbar_file = components_dir.join("Navbar.tsx");
        std::fs::write(&navbar_file, "export default function Navbar() { return <nav>Navbar</nav>; }").unwrap();

        let page_file = app_dir.join("page.tsx");
        std::fs::write(&page_file, "import Navbar from '@/components/Navbar';\nexport default function Page() { return <Navbar />; }").unwrap();

        let files_list = vec![
            serde_json::json!({ "path": "src/app/page.tsx" }),
            serde_json::json!({ "path": "src/components/Navbar.tsx" }),
        ];

        generate_project_metadata(&temp_dir, "test-proj", &files_list);

        let meta_file = temp_dir.join(".axiom").join("metadata.json");
        assert!(meta_file.exists());
        let meta_str = std::fs::read_to_string(&meta_file).unwrap();
        let meta_val: serde_json::Value = serde_json::from_str(&meta_str).unwrap();

        let page_deps = meta_val.get("dependencies").and_then(|d| d.get("src/app/page.tsx")).and_then(|v| v.as_array()).unwrap();
        assert_eq!(page_deps.len(), 1);
        assert_eq!(page_deps[0].as_str(), Some("src/components/Navbar.tsx"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_axiom_contracts_harvester_refresh_and_prompt() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_contracts_test_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join(".axiom")).unwrap();
        // Simulate corrupted/empty stub like v0.1.45 CarbonChain Navigator
        std::fs::write(
            temp_dir.join(".axiom").join("axiom-contracts.json"),
            r#"{"lastUpdated":"2026-01-01T00:00:00Z","totalFilesTracked":0}"#,
        ).unwrap();

        let schema_code = "enum Role {\n  ADMIN\n  MEMBER\n}\nmodel Supplier {\n  id String @id @default(cuid())\n}\n";
        let api_code = "import { NextResponse } from 'next/server';\nexport async function GET() { return NextResponse.json([]); }\n";
        let hook_code = "import { useState, useEffect } from 'react';\nexport function useSuppliers() {\n  const [suppliers, setSuppliers] = useState([]);\n  useEffect(() => { fetch('/api/suppliers'); }, []);\n  return { suppliers };\n}\n";
        let nav_code = "import Link from 'next/link';\nimport { useRouter } from 'next/navigation';\nexport default function Navbar() {\n  const router = useRouter();\n  const [isMenuOpen, setIsMenuOpen] = useState(false);\n  return <nav onClick={() => router.push('/dashboard')}><Link href=\"/suppliers\">Suppliers</Link></nav>;\n}\n";

        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/api/suppliers")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();
        std::fs::write(temp_dir.join("prisma/schema.prisma"), schema_code).unwrap();
        std::fs::write(temp_dir.join("src/app/api/suppliers/route.ts"), api_code).unwrap();
        std::fs::write(temp_dir.join("src/hooks/useSuppliers.tsx"), hook_code).unwrap();
        std::fs::write(temp_dir.join("src/components/Navbar.tsx"), nav_code).unwrap();

        let files_list = vec![
            serde_json::json!({ "path": "prisma/schema.prisma" }),
            serde_json::json!({ "path": "src/app/api/suppliers/route.ts" }),
            serde_json::json!({ "path": "src/hooks/useSuppliers.tsx" }),
            serde_json::json!({ "path": "src/components/Navbar.tsx" }),
        ];

        harvest_file_to_axiom_contracts(&temp_dir, "proj-1", "prisma/schema.prisma", schema_code, &files_list);
        harvest_file_to_axiom_contracts(&temp_dir, "proj-1", "src/app/api/suppliers/route.ts", api_code, &files_list);
        harvest_file_to_axiom_contracts(&temp_dir, "proj-1", "src/hooks/useSuppliers.tsx", hook_code, &files_list);
        harvest_file_to_axiom_contracts(&temp_dir, "proj-1", "src/components/Navbar.tsx", nav_code, &files_list);

        let root_contracts: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(temp_dir.join("axiom-contracts.json")).unwrap()
        ).unwrap();
        assert_eq!(root_contracts["version"], serde_json::json!("1.0"));
        assert_eq!(root_contracts["projectId"], serde_json::json!("proj-1"));
        assert_eq!(root_contracts["totalFilesTracked"], serde_json::json!(4));

        let files_arr = root_contracts["files"].as_array().unwrap();
        assert_eq!(files_arr.len(), 4);

        let hook_entry = files_arr.iter().find(|f| f["path"] == "src/hooks/useSuppliers.tsx").unwrap();
        assert_eq!(hook_entry["role"], serde_json::json!("client-hook"));
        assert!(hook_entry["capabilities"]["states"].get("suppliers").is_some());
        assert!(hook_entry["consumedContracts"]["apiCalls"].as_array().unwrap().iter().any(|v| v == "/api/suppliers"));

        let nav_entry = files_arr.iter().find(|f| f["path"] == "src/components/Navbar.tsx").unwrap();
        let nav_routes = nav_entry["connectedRoutes"].as_array().unwrap();
        assert!(nav_routes.iter().any(|v| v == "/suppliers"));
        assert!(nav_routes.iter().any(|v| v == "/dashboard"));
        assert!(nav_entry["capabilities"]["exports"].as_array().unwrap().iter().any(|v| v == "Navbar"));

        // Prompt injection skips configuration and current file
        let prompt_ctx = format_axiom_contracts_prompt(&temp_dir, "src/app/dashboard/page.tsx");
        assert!(prompt_ctx.contains("CROSS-FILE CONTRACTS REGISTRY"));
        assert!(prompt_ctx.contains("src/hooks/useSuppliers.tsx"));
        assert!(prompt_ctx.contains("/api/suppliers"));

        // Refresh picks up files added later by Import Fixer
        std::fs::write(
            temp_dir.join("src/components/MissingWidget.tsx"),
            "export const MissingWidget = () => <div />;\nexport default MissingWidget;\n",
        ).unwrap();
        refresh_axiom_contracts_for_project(&temp_dir, "proj-1", &files_list);
        let refreshed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(temp_dir.join(".axiom").join("axiom-contracts.json")).unwrap()
        ).unwrap();
        assert_eq!(refreshed["totalFilesTracked"], serde_json::json!(5));
        assert!(refreshed["files"].as_array().unwrap().iter().any(|f| f["path"] == "src/components/MissingWidget.tsx"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_healer_guards_import_rewrite_and_prose_rejection_v47() {
        // 1. Path validation rejects extensionless module specifiers, absolute paths, and spaces
        assert!(!is_valid_file_path("src/models/MaintenanceAlert"));
        assert!(!is_valid_file_path("/abs/path/file.ts"));
        assert!(!is_valid_file_path("src/components/My Card.tsx"));
        assert!(is_valid_file_path("src/models/MaintenanceAlert.ts"));
        assert!(is_valid_file_path(".env.local"));

        // 2. Prose & JSON payload rejection
        assert!(!looks_like_code_payload("The error indicates that the module 'src/models/MaintenanceAlert' cannot be found."));
        assert!(!looks_like_code_payload("1. Ensure the file exists at src/models/MaintenanceAlert.ts\n2. Verify the import path in tsconfig.json"));
        assert!(!looks_like_code_payload("{\"edits\":[]}"));
        assert!(looks_like_code_payload("import type { MaintenanceAlert } from '@/models/MaintenanceAlert';\nexport const x = 1;"));
        assert!(looks_like_code_payload(".btn { color: red; }"));
        assert!(looks_like_code_payload("model Alert {\n  id String @id\n}"));

        // 3. `type: "import"` swap edits must target the consumer file and rewrite `'src/...'` -> `'@/...'`, never creating an extensionless file
        let import_swap_json = serde_json::json!({
            "response": "1. Ensure src/models/MaintenanceAlert.ts exists\n2. Update import",
            "steps": "Check tsconfig.json",
            "edits": [
                {
                    "type": "import",
                    "path": "src/models/MaintenanceAlert",
                    "newPath": "src/models/MaintenanceAlert.ts"
                }
            ]
        });
        let extracted = extract_edits_from_json(&import_swap_json, "src/hooks/useMaintenanceAlerts.ts");
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0]["path"], "src/hooks/useMaintenanceAlerts.ts");
        assert_eq!(extracted[0]["target"], "'src/models/MaintenanceAlert'");
        assert_eq!(extracted[0]["replacement"], "'@/models/MaintenanceAlert'");
    }


    #[test]
    fn test_orchestrator_validator_contracts_and_self_healing_v47() {
        // 1. Deterministic validate_file_with_abstract_rules checks
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let clean_model = "export interface MaintenanceAlert {\n  id: string;\n  severity: 'LOW' | 'HIGH';\n}\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/models/MaintenanceAlert.ts", clean_model, "qwen").await.is_ok());

            let bad_model = "import { PrismaClient } from '@prisma/client';\nexport const db = new PrismaClient();\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/models/MaintenanceAlert.ts", bad_model, "qwen").await.is_err());

            let bad_mw = "import { NextFunction } from 'express';\nexport function middleware() {}\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("middleware.ts", bad_mw, "qwen").await.is_err());

            let bad_client = "\"use client\";\nimport prisma from '@/lib/prisma';\nexport function Panel() { return <div />; }\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/components/Panel.tsx", bad_client, "qwen").await.is_err());

            let prose_leak = "export const a = 1;\nThe error indicates that something is wrong.\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/utils/a.ts", prose_leak, "qwen").await.is_err());

            let mut repeating_css = String::new();
            for _ in 0..12 {
                repeating_css.push_str(".loop-card { padding: 12px; border: 1px solid #ccc; }\n");
            }
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/app/globals.css", &repeating_css, "qwen").await.is_err());

            let unclosed = "export function broken() {\n  if (true) {\n    const obj = {\n      a: 1\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/utils/broken.ts", unclosed, "qwen").await.is_err());
        });

        // 2. Contract harvester: multiline useState, hookCalls, src/ internal deps, and CSS inclusion on refresh
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_orch_sh_v47_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/models")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/utils")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app")).unwrap();

        let hook_code = r#""use client";
import { useState, useCallback } from 'react';
import type { MaintenanceAlert } from 'src/models/MaintenanceAlert';
import { useAuth } from '@/hooks/useAuth';

export function useMaintenanceAlerts() {
  const [alerts, setAlerts] = useState<MaintenanceAlert[]>(
    []
  );
  const { user } = useAuth();
  const reload = useCallback(() => {
    fetch('/api/maintenance');
  }, []);
  return { alerts, setAlerts, user, reload };
}
"#;
        std::fs::write(temp_dir.join("src/models/MaintenanceAlert.ts"), "export interface MaintenanceAlert { id: string; }\n").unwrap();
        std::fs::write(temp_dir.join("src/hooks/useMaintenanceAlerts.ts"), hook_code).unwrap();
        std::fs::write(temp_dir.join("src/app/globals.css"), "@tailwind base;\nbody { color: #111; }\n").unwrap();

        crate::orchestrator::harvest_file_to_axiom_contracts(
            &temp_dir,
            "proj-v47",
            "src/hooks/useMaintenanceAlerts.ts",
            hook_code,
            &[serde_json::json!({ "path": "src/models/MaintenanceAlert.ts" })],
        );
        crate::orchestrator::refresh_axiom_contracts_for_project(&temp_dir, "proj-v47", &[]);

        let contracts_val: Value = serde_json::from_str(
            &std::fs::read_to_string(temp_dir.join(".axiom/axiom-contracts.json")).unwrap(),
        )
        .unwrap();
        let files_arr = contracts_val["files"].as_array().unwrap();
        let hook_entry = files_arr.iter().find(|f| f["path"] == "src/hooks/useMaintenanceAlerts.ts").unwrap();
        assert_eq!(hook_entry["capabilities"]["states"]["alerts"]["type"], "MaintenanceAlert[]");
        assert!(hook_entry["consumedContracts"]["hookCalls"].as_array().unwrap().iter().any(|h| h == "useAuth"));
        assert!(hook_entry["dependencies"]["internal"].as_array().unwrap().iter().any(|d| d == "src/models/MaintenanceAlert.ts"));
        assert!(files_arr.iter().any(|f| f["path"] == "src/app/globals.css"));

        // 3. scan_and_fix_imports must NOT self-import `src/utils/iot.ts` -> `@/utils/iot`, while fixing unique non-self matches
        std::fs::write(
            temp_dir.join("src/utils/iot.ts"),
            "import { missing } from './nonexistent/iot';\nexport const ok = true;\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/components/ProductCard.tsx"),
            "export const ProductCard = () => null;\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/app/page.tsx"),
            "import { ProductCard } from './wrong/ProductCard';\nexport default function Home() { return <ProductCard />; }\n",
        )
        .unwrap();

        let mut mod_index = std::collections::HashMap::new();
        let src_root = temp_dir.join("src");
        crate::self_healing::index_modules(&src_root, &src_root, &mut mod_index);
        crate::self_healing::scan_and_fix_imports(&src_root, &src_root, &mod_index);

        let iot_after = std::fs::read_to_string(temp_dir.join("src/utils/iot.ts")).unwrap();
        assert!(!iot_after.contains("@/utils/iot"));
        let page_after = std::fs::read_to_string(temp_dir.join("src/app/page.tsx")).unwrap();
        assert!(page_after.contains("@/components/ProductCard"));

        // 4. parse_tsc_diagnostics & file_dependency_layer_priority
        let tsc_out = "src\\hooks\\useMaintenanceAlerts.ts(4,5): error TS2307: Cannot find module 'src/models/MaintenanceAlert'.\r\nsrc/app/page.tsx(10,12): error TS2322: Type 'string' is not assignable.";
        let diags = crate::self_healing::parse_tsc_diagnostics(tsc_out);
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].file, "src/hooks/useMaintenanceAlerts.ts");
        assert_eq!(diags[0].line, 4);
        assert_eq!(diags[0].col, 5);
        assert_eq!(diags[0].code, "TS2307");

        assert!(crate::self_healing::file_dependency_layer_priority("prisma/schema.prisma")
            < crate::self_healing::file_dependency_layer_priority("src/models/Alert.ts"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/models/Alert.ts")
            < crate::self_healing::file_dependency_layer_priority("src/lib/prisma.ts"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/lib/prisma.ts")
            < crate::self_healing::file_dependency_layer_priority("src/components/ui.tsx"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/components/ui.tsx")
            < crate::self_healing::file_dependency_layer_priority("src/hooks/useAlerts.ts"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/hooks/useAlerts.ts")
            < crate::self_healing::file_dependency_layer_priority("src/app/api/alerts/route.ts"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/app/api/alerts/route.ts")
            < crate::self_healing::file_dependency_layer_priority("src/components/Navbar.tsx"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/components/Navbar.tsx")
            < crate::self_healing::file_dependency_layer_priority("src/app/page.tsx"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_stack_rulepacks_narrowing_no_false_mern_or_config() {
        let model_rules = get_stack_rulepacks(
            "src/models/MaintenanceAlert.ts",
            "export interface MaintenanceAlert {\n  id: string;\n  severity: string;\n}\n",
            None,
        );
        assert!(!model_rules.contains("MERN STACK"));
        assert!(!model_rules.contains("CONFIGURATION & TSCONFIG RULES"));

        let root_model_rules = get_stack_rulepacks(
            "models/Invoice.ts",
            "export interface Invoice { total: number; }",
            None,
        );
        assert!(!root_model_rules.contains("MERN STACK"));

        let config_modal_rules = get_stack_rulepacks(
            "src/components/ConfigModal.tsx",
            "import React from 'react'; export function ConfigModal() { return <div />; }",
            None,
        );
        assert!(!config_modal_rules.contains("CONFIGURATION & TSCONFIG RULES"));

        let next_cfg_rules = get_stack_rulepacks("next.config.js", "module.exports = {};", None);
        assert!(next_cfg_rules.contains("CONFIGURATION & TSCONFIG RULES"));
    }


    #[test]
    fn test_healer_import_rewrite_respects_explicit_path_with_from_to() {
        let cross_file_import_fix = serde_json::json!({
            "edits": [
                {
                    "type": "import",
                    "path": "src/components/Chart.tsx",
                    "from": "react-icons",
                    "to": "react-icons/hi"
                }
            ]
        });
        let edits = extract_edits_from_json(&cross_file_import_fix, "src/app/page.tsx");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["path"], "src/components/Chart.tsx");
        assert_eq!(edits[0]["target"], "'react-icons'");
        assert_eq!(edits[0]["replacement"], "'react-icons/hi'");
    }


    #[test]
    fn test_parse_batch_file_fixes_response_variants() {
        let obj_envelope = r#"{"files": [{"path": "./src/a.ts", "content": "export const a = 1;"}]}"#;
        let res1 = crate::self_healing::parse_batch_file_fixes_response(obj_envelope, &["src/a.ts".to_string()]);
        assert_eq!(res1, vec![("src/a.ts".to_string(), "export const a = 1;".to_string())]);

        let arr_envelope = r#"[{"path": "src/b.tsx", "content": "export const B = () => <div />;"}]"#;
        let res2 = crate::self_healing::parse_batch_file_fixes_response(arr_envelope, &[]);
        assert_eq!(res2.len(), 1);
        assert_eq!(res2[0].0, "src/b.tsx");

        let edits_envelope = r#"{"edits": [{"file": "src/c.ts", "code": "export type C = string;"}]}"#;
        let res3 = crate::self_healing::parse_batch_file_fixes_response(edits_envelope, &[]);
        assert_eq!(res3, vec![("src/c.ts".to_string(), "export type C = string;".to_string())]);

        let map_envelope = r#"{"src/d.ts": "export const d = 4;"}"#;
        let res4 = crate::self_healing::parse_batch_file_fixes_response(map_envelope, &[]);
        assert_eq!(res4, vec![("src/d.ts".to_string(), "export const d = 4;".to_string())]);

        let raw_single = "export const e = 5;\n";
        let res5 = crate::self_healing::parse_batch_file_fixes_response(raw_single, &["src/e.ts".to_string()]);
        assert_eq!(res5.len(), 1);
        assert_eq!(res5[0].0, "src/e.ts");

        // Multi-file batch must NOT assign single raw blob blindly to multiple expected files
        let res6 = crate::self_healing::parse_batch_file_fixes_response(
            raw_single,
            &["src/e.ts".to_string(), "src/f.ts".to_string()],
        );
        assert!(res6.is_empty());
    }


    #[test]
    fn test_dts_export_inspection_and_wildcard_bailout_v48() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v48_dts_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let nm = temp_dir.join("node_modules");

        // 1. Clean package with exports.types + depth-1 relative `export *`
        let wk_dir = nm.join("widget-kit").join("dist");
        std::fs::create_dir_all(&wk_dir).unwrap();
        std::fs::write(
            nm.join("widget-kit/package.json"),
            r#"{"name":"widget-kit","exports":{".":{"types":"./dist/index.d.ts"}}}"#,
        )
        .unwrap();
        std::fs::write(
            wk_dir.join("index.d.ts"),
            "export declare function createWidget(): void;\nexport interface WidgetOptions { id: string; }\nexport * from './extra';\n",
        )
        .unwrap();
        std::fs::write(
            wk_dir.join("extra.d.ts"),
            "export declare const WIDGET_VERSION: string;\n",
        )
        .unwrap();

        // 2. Depth > 1 wildcard chain (must trigger Bailout Guard -> incomplete = true -> omitted)
        let dw_dir = nm.join("deep-wild-kit");
        std::fs::create_dir_all(&dw_dir).unwrap();
        std::fs::write(dw_dir.join("index.d.ts"), "export declare const TOP_ONLY: number;\nexport * from './sub';\n").unwrap();
        std::fs::write(dw_dir.join("sub.d.ts"), "export * from './nested';\n").unwrap();

        // 3. External wildcard re-export (must trigger Bailout Guard -> omitted)
        let ew_dir = nm.join("ext-wild-kit");
        std::fs::create_dir_all(&ew_dir).unwrap();
        std::fs::write(ew_dir.join("index.d.ts"), "export declare const LOCAL_SYM: boolean;\nexport * from 'other-pkg';\n").unwrap();

        let source_code = "import { badWidget } from 'widget-kit';\nimport { x } from 'deep-wild-kit';\nimport { y } from 'ext-wild-kit';\n";
        let err_ctx = "error TS2305: Module '\"widget-kit\"' has no exported member 'badWidget'.";
        let out = crate::self_healing::inspect_dts_exports_for_file(&temp_dir, source_code, err_ctx);

        assert!(out.contains("widget-kit"));
        assert!(out.contains("createWidget"));
        assert!(out.contains("WidgetOptions"));
        assert!(out.contains("WIDGET_VERSION"));

        // When only incomplete packages are queried directly, Bailout Guard returns empty instead of partial list
        let incomplete_src = "import { x } from 'deep-wild-kit';\nimport { y } from 'ext-wild-kit';\n";
        let incomplete_err = "error TS2305: has no exported member";
        let bail_out = crate::self_healing::inspect_dts_exports_for_file(&temp_dir, incomplete_src, incomplete_err);
        assert!(bail_out.trim().is_empty(), "Incomplete wildcard .d.ts must bail out:\n{}", bail_out);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_deduplication_and_features_json_v49() {
        let mut files = vec![
            serde_json::json!({ "path": "src\\hooks\\useItems.ts", "description": "short" }),
            serde_json::json!({ "path": "src/hooks/useItems.ts", "description": "Complete typed hook managing item CRUD and loading states" }),
            serde_json::json!({ "path": "./src/hooks/useItems.ts", "description": "" }),
            serde_json::json!({ "path": "src/app/api/items/route.ts", "description": "REST API route for items" }),
        ];
        crate::sanitizer::deduplicate_manifest_files(&mut files);
        assert_eq!(files.len(), 2, "Normalized duplicate manifest paths must collapse into 2 unique entries");
        let hook_entry = files
            .iter()
            .find(|f| f["path"].as_str() == Some("src/hooks/useItems.ts"))
            .unwrap();
        assert!(hook_entry["description"].as_str().unwrap().contains("Complete typed hook"));

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v49_feat_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join(".axiom")).unwrap();
        let dup_input = vec![
            serde_json::json!({ "path": "src/hooks/useItems.ts" }),
            serde_json::json!({ "path": "src\\hooks\\useItems.ts" }),
            serde_json::json!({ "path": "src/app/api/items/route.ts" }),
            serde_json::json!({ "path": "src/app/items/page.tsx" }),
        ];
        crate::editor_ai::generate_axiom_features_json(&temp_dir, "sample-app", &dup_input);
        let feat_val: Value = serde_json::from_str(&std::fs::read_to_string(temp_dir.join("axiom-features.json")).unwrap()).unwrap();
        assert_eq!(feat_val["totalFiles"].as_u64(), Some(3), "totalFiles must count deduplicated unique files");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_auxiliary_json_maps_and_contracts_v49() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v49_aux_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/lib/helpers")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/api/shipments")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/shipments")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();

        let schema_src = r#"
enum RecordState {
  DRAFT
  ACTIVE
}

model Shipment {
  id String @id @default(cuid())
  title String
  weight Float
  state RecordState @default(DRAFT)
}
"#;
        std::fs::write(temp_dir.join("prisma/schema.prisma"), schema_src).unwrap();
        std::fs::write(
            temp_dir.join("src/lib/helpers/index.ts"),
            "export const helperA = (x: string) => x.trim();\nexport const helperB = () => 1;\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/lib/format.ts"),
            "import {\n  helperA,\n  helperB,\n} from './helpers';\nexport function formatTitle(raw: string) {\n  if (raw.length > 0) {\n    return helperA(raw);\n  }\n  return String(helperB());\n}\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/hooks/useShipments.ts"),
            "'use client';\nimport { useState } from 'react';\nimport { formatTitle } from '../lib/format';\nexport function useShipments() {\n  const [shipments, setShipments] = useState<string[]>([]);\n  const reloadShipments = () => formatTitle('ok');\n  return { shipments, setShipments, reloadShipments };\n}\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/app/api/shipments/route.ts"),
            "import { NextResponse } from 'next/server';\nexport async function GET(request: Request) {\n  return NextResponse.json({ url: request.url });\n}\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/app/shipments/page.tsx"),
            "import { ShipmentBoard } from '@/components/ShipmentBoard';\nexport default function ShipmentsPage() { return <ShipmentBoard />; }\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/components/ShipmentBoard.tsx"),
            "'use client';\nimport { useShipments } from '@/hooks/useShipments';\nexport function ShipmentBoard() { const { shipments } = useShipments(); return <div>{shipments.length}</div>; }\n",
        )
        .unwrap();

        crate::orchestrator::generate_axiom_map(&temp_dir).unwrap();
        let map_val: Value = serde_json::from_str(&std::fs::read_to_string(temp_dir.join("axiom-map.json")).unwrap()).unwrap();
        let fmt_deps = map_val["files"]["src/lib/format.ts"]["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>();
        assert!(
            fmt_deps.contains(&"src/lib/helpers/index.ts"),
            "Multiline + barrel import must resolve in axiom-map.json: {:?}",
            fmt_deps
        );
        let method_keys: Vec<&str> = map_val["methods"]
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        assert!(
            !method_keys.iter().any(|k| k.starts_with('(')),
            "Malformed `(request` token must never appear in methods: {:?}",
            method_keys
        );
        assert_eq!(map_val["routes"]["/shipments"]["file"].as_str(), Some("src/app/shipments/page.tsx"));

        // generate_project_metadata with empty files_list must still scan disk and resolve relative `../` imports
        crate::healer::generate_project_metadata(&temp_dir, "proj_v49", &[]);
        let meta_val: Value = serde_json::from_str(&std::fs::read_to_string(temp_dir.join(".axiom/metadata.json")).unwrap()).unwrap();
        let hook_deps = meta_val["dependencies"]["src/hooks/useShipments.ts"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>();
        assert!(
            hook_deps.contains(&"src/lib/format.ts"),
            "Disk-scanned metadata.json must resolve ../lib/format: {:?}",
            hook_deps
        );
        assert!(meta_val["dependencies"].get("src/components/ShipmentBoard.tsx").is_some());

        // refresh_axiom_contracts_for_project must capture capabilities.members and surface schema models/enums
        crate::orchestrator::refresh_axiom_contracts_for_project(&temp_dir, "proj_v49", &[]);
        let contracts_val: Value = serde_json::from_str(
            &std::fs::read_to_string(temp_dir.join(".axiom/axiom-contracts.json")).unwrap(),
        )
        .unwrap();
        let hook_members = contracts_val["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"].as_str() == Some("src/hooks/useShipments.ts"))
            .and_then(|f| f["capabilities"]["members"].as_array())
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>();
        assert!(hook_members.contains(&"shipments"));
        assert!(hook_members.contains(&"reloadShipments"));

        let contracts_prompt = crate::orchestrator::format_axiom_contracts_prompt(&temp_dir, "src/components/ShipmentBoard.tsx");
        assert!(contracts_prompt.contains("models:[Shipment]"), "Schema models must appear:\n{}", contracts_prompt);
        assert!(contracts_prompt.contains("enums:[RecordState]"), "Schema enums must appear:\n{}", contracts_prompt);
        assert!(contracts_prompt.contains("members:[shipments"), "Hook members must appear:\n{}", contracts_prompt);

        // Database contract + planned routes summary
        let db_contract = crate::orchestrator::extract_prisma_schema_contract(schema_src);
        assert_eq!(db_contract["models"]["Shipment"]["delegate"].as_str(), Some("shipment"));
        let db_prompt = crate::orchestrator::format_database_contract_prompt(&db_contract);
        assert!(db_prompt.contains("prisma.shipment"));
        assert!(db_prompt.contains("requiredOnCreate"));
        assert!(db_prompt.contains("RecordState"));

        let planned = crate::orchestrator::extract_planned_api_routes_summary(&[
            serde_json::json!({ "path": "src/app/api/shipments/route.ts", "description": "Manage shipments" }),
        ]);
        assert!(planned.contains("/api/shipments"));
        assert!(planned.contains("PLANNED API ROUTES REQUIRING DATABASE SCHEMA SUPPORT"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_semantic_import_edits_and_noop_strip_v49() {
        let ai_json = serde_json::json!({
            "file": "src/app/board/page.tsx",
            "edits": [
                { "type": "import", "module": "vendor-icons/v2", "namedImports": ["VendorPlusIcon"] },
                { "type": "replace", "search": "VendorPlusIcon", "replacement": "VendorPlusIcon" }
            ]
        });
        let edits = crate::healer::extract_edits_from_json(&ai_json, "src/app/board/page.tsx");
        assert_eq!(edits.len(), 1, "No-op search==replacement must be stripped, keeping only ensure_import");
        assert_eq!(edits[0]["op"].as_str(), Some("ensure_import"));
        assert_eq!(edits[0]["module"].as_str(), Some("vendor-icons/v2"));

        // Deletion edit (target non-empty -> replacement "") and explicit from/to path rewrite stay intact
        let misc_json = serde_json::json!({
            "edits": [
                { "path": "src/a.ts", "target": "staleCode();", "replacement": "" },
                { "type": "import", "path": "src/b.ts", "from": "old/path", "to": "@/new/path" }
            ]
        });
        let misc_edits = crate::healer::extract_edits_from_json(&misc_json, "src/a.ts");
        assert_eq!(misc_edits.len(), 2);
        assert!(misc_edits.iter().all(|e| e.get("op").is_none()));

        // Test .d.ts auto-redirection from wrong subpath v1 -> v2 + `'use client'` preservation
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v49_sem_imp_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("node_modules/vendor-icons/v1")).unwrap();
        std::fs::create_dir_all(temp_dir.join("node_modules/vendor-icons/v2")).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/vendor-icons/package.json"),
            r#"{"name":"vendor-icons"}"#,
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("node_modules/vendor-icons/v1/index.d.ts"),
            "export declare function OtherLegacyIcon(): any;\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("node_modules/vendor-icons/v2/index.d.ts"),
            "export declare function VendorPlusIcon(): any;\n",
        )
        .unwrap();

        let wrong_subpath_edit = serde_json::json!({
            "op": "ensure_import",
            "path": "src/app/board/page.tsx",
            "module": "vendor-icons/v1",
            "namedImports": ["VendorPlusIcon"]
        });
        let page_src = "'use client';\nimport React from 'react';\nexport default function BoardPage() { return <VendorPlusIcon />; }\n";
        let updated = crate::healer::apply_semantic_ensure_import_edit(
            &temp_dir,
            "src/app/board/page.tsx",
            &wrong_subpath_edit,
            page_src,
        )
        .unwrap();
        assert!(updated.trim_start().starts_with("'use client';"));
        assert!(
            updated.contains("from 'vendor-icons/v2'") || updated.contains("from \"vendor-icons/v2\""),
            "Should auto-redirect wrong subpath via .d.ts:\n{}",
            updated
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_project_aware_api_route_validator_v49() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v49_val_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::write(
            temp_dir.join("prisma/schema.prisma"),
            "enum WorkState {\n  OPEN\n  DONE\n}\nmodel WorkItem {\n  id String @id\n  title String\n  state WorkState @default(OPEN)\n}\n",
        )
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();

        // 1. Hallucinated prisma delegate must fail
        let bad_delegate = r#"
import { NextResponse } from 'next/server';
import { prisma } from '@/lib/prisma';
export async function GET() {
  const rows = await prisma.fakeEntity.findMany();
  return NextResponse.json(rows);
}
"#;
        assert!(
            rt.block_on(crate::orchestrator::validate_file_with_project_context(
                &temp_dir,
                "src/app/api/work/route.ts",
                bad_delegate,
                "qwen2.5-coder:7b",
            ))
            .is_err(),
            "Must reject hallucinated prisma.fakeEntity delegate"
        );

        // 2. Unimported Prisma enum must fail
        let bad_enum = r#"
import { NextResponse } from 'next/server';
import { prisma } from '@/lib/prisma';
export async function GET() {
  const rows = await prisma.workItem.findMany({ where: { state: WorkState.OPEN } });
  return NextResponse.json(rows);
}
"#;
        assert!(
            rt.block_on(crate::orchestrator::validate_file_with_project_context(
                &temp_dir,
                "src/app/api/work/route.ts",
                bad_enum,
                "qwen2.5-coder:7b",
            ))
            .is_err(),
            "Must reject unimported Prisma enum WorkState"
        );

        // 3. URLSearchParams direct property access (`searchParams.itemId`) must fail
        let bad_sp = r#"
import { NextResponse } from 'next/server';
export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);
  return NextResponse.json({ id: searchParams.itemId });
}
"#;
        assert!(
            rt.block_on(crate::orchestrator::validate_file_with_project_context(
                &temp_dir,
                "src/app/api/work/route.ts",
                bad_sp,
                "qwen2.5-coder:7b",
            ))
            .is_err(),
            "Must reject searchParams.itemId on URLSearchParams"
        );

        // 4. `try { const payload = await request.json(); } catch { return NextResponse.json(payload); }` must fail
        let bad_catch_scope = r#"
import { NextResponse } from 'next/server';
export async function POST(request: Request) {
  try {
    const payload = await request.json();
    return NextResponse.json(payload);
  } catch {
    return NextResponse.json({ fallback: payload }, { status: 500 });
  }
}
"#;
        assert!(
            rt.block_on(crate::orchestrator::validate_file_with_project_context(
                &temp_dir,
                "src/app/api/work/route.ts",
                bad_catch_scope,
                "qwen2.5-coder:7b",
            ))
            .is_err(),
            "Must reject try-scoped payload referenced in catch block"
        );

        // 5. Double request.json() inside paired try/catch must fail
        let bad_double_json = r#"
import { NextResponse } from 'next/server';
export async function POST(request: Request) {
  try {
    const payload = await request.json();
    return NextResponse.json(payload);
  } catch {
    const retryBody = await request.json();
    return NextResponse.json(retryBody, { status: 500 });
  }
}
"#;
        assert!(
            rt.block_on(crate::orchestrator::validate_file_with_project_context(
                &temp_dir,
                "src/app/api/work/route.ts",
                bad_double_json,
                "qwen2.5-coder:7b",
            ))
            .is_err(),
            "Must reject double request.json() across try and catch"
        );

        // 6. Valid route with separate POST + PUT handlers, valid delegate + imported enum + comment URL must pass
        let valid_route = r#"
import { NextResponse } from 'next/server';
import { WorkState } from '@prisma/client';
import { prisma } from '@/lib/prisma';
// External spec reference: https://example.com/api-docs
export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);
  const filterId = searchParams.get('id');
  const rows = await prisma.workItem.findMany({ where: { state: WorkState.OPEN } });
  return NextResponse.json({ filterId, rows });
}
export async function POST(request: Request) {
  const body = await request.json();
  return NextResponse.json(body);
}
export async function PUT(request: Request) {
  const body = await request.json();
  return NextResponse.json(body);
}
"#;
        assert!(
            rt.block_on(crate::orchestrator::validate_file_with_project_context(
                &temp_dir,
                "src/app/api/work/route.ts",
                valid_route,
                "qwen2.5-coder:7b",
            ))
            .is_ok(),
            "Valid route with separate POST/PUT and comment URL must pass"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_manifest_sanitization_and_hygiene_safeguards() {
        // 1. Sentinel name resolution
        assert_eq!(
            crate::orchestrator::resolve_manifest_project_name(
                Some("Business Analysis Failed"),
                "WidgetForge is a modular platform for operational workflows",
                "cmwc999999"
            ),
            "WidgetForge"
        );
        assert_eq!(
            crate::orchestrator::resolve_manifest_project_name(Some("Analysis Failed"), "", "abc123def"),
            "Axiom Project abc123"
        );
        assert_eq!(
            crate::orchestrator::resolve_manifest_project_name(Some("CleanApp"), "desc", "id1"),
            "CleanApp"
        );

        // 2. LBG API non-path pruning & typedFields/fields pruning for dangling relation types
        let lbg = serde_json::json!({
            "sharedContracts": {
                "database": {
                    "enums": {
                        "ItemState": ["OPEN", "CLOSED"]
                    },
                    "models": {
                        "Station": {
                            "fields": ["id", "name", "status", "bookings"],
                            "typedFields": ["id: String", "name: String", "status: ItemState", "bookings: GhostBooking[]"],
                            "relations": ["bookings: GhostBooking[]", "facility: Facility"]
                        },
                        "Facility": {
                            "fields": ["id", "title"],
                            "typedFields": ["id: String", "title: String"],
                            "relations": ["stations: Station[]"]
                        }
                    }
                },
                "api": {
                    "endpoints": {
                        "/api/stations": ["GET", "POST"],
                        "POST /api/facility": ["POST"],
                        "Analytics and Reporting": ["GET"],
                        "Booking Module": ["POST"]
                    }
                }
            }
        });
        let clean_lbg = crate::orchestrator::sanitize_live_builder_guide_contracts(&lbg);
        let ep_obj = clean_lbg["sharedContracts"]["api"]["endpoints"].as_object().unwrap();
        assert!(ep_obj.contains_key("/api/stations"));
        assert!(ep_obj.contains_key("POST /api/facility"));
        assert!(!ep_obj.contains_key("Analytics and Reporting"));
        assert!(!ep_obj.contains_key("Booking Module"));

        let station = &clean_lbg["sharedContracts"]["database"]["models"]["Station"];
        let st_fields: Vec<&str> = station["fields"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        let st_typed: Vec<&str> = station["typedFields"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert!(!st_fields.contains(&"bookings"), "Expected dangling field removed: {:?}", st_fields);
        assert!(st_fields.contains(&"status"));
        assert!(!st_typed.iter().any(|s| s.contains("GhostBooking")), "Expected dangling typedField removed: {:?}", st_typed);
        assert!(st_typed.contains(&"status: ItemState"));

        let api_prompt = crate::orchestrator::format_api_endpoints_prompt(&lbg["sharedContracts"]["api"]);
        assert!(api_prompt.contains("/api/stations"));
        assert!(!api_prompt.contains("Analytics and Reporting"));

        // 3. Prisma schema import rewrite + client type preservation + bcrypt injection
        let hook_with_schema_imp = "\"use client\";\nimport { WorkItem, type ItemState } from '@/prisma/schema';\nexport function useWork() { return [] as WorkItem[]; }\n";
        let hook_out = post_process_generated_file("src/hooks/useWork.ts", hook_with_schema_imp, &serde_json::json!({}), false);
        assert!(
            hook_out.contains("import type { WorkItem, ItemState } from '@prisma/client';"),
            "Expected clean `import type` from @prisma/client without double `type`:\n{}",
            hook_out
        );

        let route_with_schema_imp = "import { NextResponse } from 'next/server';\nimport { prisma } from '@/prisma/schema';\nexport async function POST(req: Request) {\n  const hashed = await bcrypt.hash('secret', 10);\n  return NextResponse.json({ hashed, ok: !!prisma });\n}\n";
        let route_out = post_process_generated_file("src/app/api/register/route.ts", route_with_schema_imp, &serde_json::json!({}), false);
        assert!(route_out.contains("from '@/lib/prisma'"), "Expected @/lib/prisma rewrite:\n{}", route_out);
        assert!(route_out.contains("import bcrypt from 'bcryptjs';"), "Expected bcrypt auto-import:\n{}", route_out);

        // 4. Client/hook direct `prisma.<model>` & deprecated NextAuth v3 validation rejections
        let rt = tokio::runtime::Runtime::new().unwrap();
        let bad_hook = "\"use client\";\nexport function useItems() { return prisma.workItem.findMany(); }\n";
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules("src/hooks/useItems.ts", bad_hook, "qwen2.5-coder:7b")).is_err());

        let bad_nextauth = "import Providers from 'next-auth/providers';\nexport const authOptions = { providers: [Providers.Credentials({})] };\n";
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules("src/config/auth.ts", bad_nextauth, "qwen2.5-coder:7b")).is_err());

        let ok_nextauth = "import type { NextAuthOptions } from 'next-auth';\nimport CredentialsProvider from 'next-auth/providers/credentials';\nexport const authOptions: NextAuthOptions = { providers: [CredentialsProvider({ name: 'cred', credentials: {}, async authorize() { return null; } })] };\n";
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules("src/config/auth.ts", ok_nextauth, "qwen2.5-coder:7b")).is_ok());

        // 5. NextAuth exact specifier matching (must reject deprecated barrel `'next-auth/providers'`)
        let db = vec![PackageMeta {
            name: "next-auth".to_string(),
            version: "^4.24.7".to_string(),
            aliases: vec!["next-auth/client".to_string()],
            import_examples: vec![
                "import CredentialsProvider from 'next-auth/providers/credentials';".to_string(),
                "import { useSession } from 'next-auth/react';".to_string(),
            ],
            compatibility: serde_json::Map::new(),
            best_practices: String::new(),
        }];
        assert_eq!(resolve_and_rewrite_import("next-auth/providers", &db), Some("next-auth".to_string()));
        assert_eq!(resolve_and_rewrite_import("next-auth/providers/credentials", &db), None);

        // 6. collect_missing_imports skips `@/prisma/schema` & relative `prisma/schema` while keeping real missing components
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v50_miss_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let src_dir = temp_dir.join("src");
        std::fs::create_dir_all(src_dir.join("hooks")).unwrap();
        std::fs::write(
            src_dir.join("hooks/useDemo.ts"),
            "import type { Row } from '@/prisma/schema';\nimport type { Alt } from '../../prisma/schema';\nimport { MissingWidget } from '@/components/MissingWidget';\nexport function useDemo() { return [] as Row[]; }\n",
        )
        .unwrap();
        let mut missing = Vec::new();
        crate::self_healing::collect_missing_imports(&src_dir, &src_dir, &mut missing);
        assert!(
            !missing.iter().any(|m| m.target_path.to_string_lossy().replace('\\', "/").contains("prisma/schema")),
            "Import Fixer must never synthesize prisma/schema module"
        );
        assert!(
            missing.iter().any(|m| m.target_path.to_string_lossy().replace('\\', "/").ends_with("components/MissingWidget.ts")),
            "Real missing local component must still be collected"
        );
        let _ = std::fs::remove_dir_all(&temp_dir);

        // 7. sanitize_package_json_value_for_offline strips stale React 17 pins in Next 14+ projects
        let mut pkg_val = serde_json::json!({
            "dependencies": {
                "next": "14.2.35",
                "react": "^18.2.0",
                "react-dom": "^18.2.0"
            },
            "resolutions": {
                "react": "^17.0.2",
                "react-dom": "^17.0.2"
            },
            "overrides": {
                "react": "17.0.2",
                "other-pkg": "1.2.3"
            }
        });
        assert!(crate::packages::sanitize_package_json_value_for_offline(&mut pkg_val));
        assert!(pkg_val.get("resolutions").is_none(), "Empty resolutions should be removed: {:?}", pkg_val);
        assert!(pkg_val["overrides"].get("react").is_none());
        assert_eq!(pkg_val["overrides"]["other-pkg"].as_str(), Some("1.2.3"));
    }


    #[test]
    fn test_rejects_line_range_edits_v51() {
        // 1. startLine/endLine without verbatim target must be rejected rather than producing empty target ""
        let coord_lines_payload = serde_json::json!({
            "explanation": "Guard nullable array before .map()",
            "edits": [
                {
                    "path": "src/app/board/page.tsx",
                    "startLine": 30,
                    "endLine": 30,
                    "replacement": "{(items ?? []).map(item => ("
                }
            ]
        });
        assert!(
            extract_edits_from_json(&coord_lines_payload, "src/app/board/page.tsx").is_empty(),
            "Line-range coordinate edit without 'target' must be rejected"
        );

        // 2. start/end object/range and bare line+fragment without target must also be rejected
        let coord_range_payload = serde_json::json!({
            "file": "src/app/board/page.tsx",
            "line": 30,
            "start": { "line": 30, "column": 12 },
            "end": { "line": 30, "column": 30 },
            "replacement": "{(items ?? []).map(item => ("
        });
        assert!(
            extract_edits_from_json(&coord_range_payload, "src/app/board/page.tsx").is_empty(),
            "Object coordinate edit without 'target' must be rejected"
        );
        let bare_line_fragment_payload = serde_json::json!({
            "file": "src/app/board/page.tsx",
            "line": 30,
            "replacement": "{(items ?? []).map(item => ("
        });
        assert!(
            extract_edits_from_json(&bare_line_fragment_payload, "src/app/board/page.tsx").is_empty(),
            "Bare line-numbered fragment without 'target' must be rejected"
        );

        // 3. Explicit verbatim target + replacement (even alongside line metadata) & semantic import/deletion edits remain valid
        let mixed_valid_payload = serde_json::json!({
            "edits": [
                {
                    "path": "src/app/board/page.tsx",
                    "line": 30,
                    "target": "{items.map(item => (",
                    "replacement": "{(Array.isArray(items) ? items : []).map(item => ("
                },
                {
                    "path": "src/app/board/page.tsx",
                    "type": "import",
                    "module": "react-icons/hi2",
                    "namedImports": ["HiOutlineCheck"]
                },
                {
                    "path": "src/components/Widget.tsx",
                    "target": "export default Unused;",
                    "replacement": ""
                }
            ]
        });
        let valid_edits = extract_edits_from_json(&mixed_valid_payload, "src/app/board/page.tsx");
        assert_eq!(valid_edits.len(), 3, "Expected all 3 valid edits preserved: {:?}", valid_edits);

        // 4. Complete top-level decl helper distinguishes complete declarations from partial JSX/expression snippets
        assert!(crate::healer::is_complete_top_level_decl("interface TicketRow {\n  id: string;\n}"));
        assert!(crate::healer::is_complete_top_level_decl("type TicketState = 'OPEN' | 'CLOSED';"));
        assert!(!crate::healer::is_complete_top_level_decl("{(items ?? []).map(item => ("));
        assert!(!crate::healer::is_complete_top_level_decl("<div className=\"p-4\">{title}</div>"));
    }


    #[test]
    fn test_filter_runaway_manifest_expansion_v51() {
        let orig = vec![
            serde_json::json!({ "path": "src/app/page.tsx", "language": "typescript" }),
            serde_json::json!({ "path": "src/components/Navbar.tsx", "language": "typescript" }),
            serde_json::json!({ "path": "src/translations/en.json", "language": "json" }),
        ];

        let mut expanded = orig.clone();
        // Foreign Vue/Svelte files added by hallucinating expansion
        expanded.push(serde_json::json!({ "path": "src/components/Widget.vue", "language": "vue" }));
        expanded.push(serde_json::json!({ "path": "src/components/Banner.svelte", "language": "svelte" }));
        // Non-route folders accidentally duplicated inside src/app/
        expanded.push(serde_json::json!({ "path": "src/app/components/Card.tsx", "language": "typescript" }));
        expanded.push(serde_json::json!({ "path": "src/app/utils/format.ts", "language": "typescript" }));
        // Runaway 20-locale translation matrix
        for i in 0..20 {
            expanded.push(serde_json::json!({
                "path": format!("src/translations/locale-{}.json", i),
                "language": "json"
            }));
        }

        let filtered = crate::sanitizer::filter_runaway_manifest_expansion(&orig, expanded, "nextjs");
        let paths: Vec<String> = filtered
            .iter()
            .filter_map(|v| v.get("path").and_then(|p| p.as_str()).map(|s| s.to_string()))
            .collect();

        assert!(paths.contains(&"src/app/page.tsx".to_string()));
        assert!(paths.contains(&"src/components/Navbar.tsx".to_string()));
        assert!(paths.contains(&"src/translations/en.json".to_string()));
        assert!(!paths.iter().any(|p| p.ends_with(".vue") || p.ends_with(".svelte")), "Foreign framework files must be dropped: {:?}", paths);
        assert!(paths.contains(&"src/components/Card.tsx".to_string()), "Expanded src/app/components/* must canonicalize to src/components/*: {:?}", paths);
        assert!(paths.contains(&"src/utils/format.ts".to_string()), "Expanded src/app/utils/* must canonicalize to src/utils/*: {:?}", paths);
        let extra_locales = paths.iter().filter(|p| p.starts_with("src/translations/locale-")).count();
        assert_eq!(extra_locales, 2, "Runaway expanded locale JSONs must be capped at 2, got: {:?}", paths);
    }


    #[test]
    fn test_inspect_dts_exports_stripe_dynamic_v51() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v51_stripe_dts_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("node_modules/stripe/esm/resources")).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/stripe/package.json"),
            r#"{"name":"stripe","version":"23.1.0","types":"types/index.d.ts"}"#,
        ).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/stripe/esm/apiVersion.d.ts"),
            "export declare const ApiVersion = \"2026-10-28.endive\";\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/stripe/esm/resources/Subscriptions.d.ts"),
            "export declare class SubscriptionResource {\n  cancel(id: string): Promise<any>;\n}\n",
        ).unwrap();

        let src = "import Stripe from 'stripe';\nexport const stripe = new Stripe(process.env.STRIPE_SECRET_KEY!, { apiVersion: '2020-08-27' });\nexport async function killSub(id: string) { return stripe.subscriptions.del(id); }\n";
        let err = "src/utils/stripe.ts(2,55): error TS2322: Type '\"2020-08-27\"' is not assignable to type 'LatestApiVersion'.\nsrc/utils/stripe.ts(3,64): error TS2339: Property 'del' does not exist on type 'SubscriptionResource'.";
        let notes = crate::self_healing::inspect_dts_exports_for_file(&temp_dir, src, err);
        assert!(notes.contains("2026-10-28.endive"), "Expected dynamic disk ApiVersion in notes:\n{}", notes);
        assert!(notes.contains("stripe.subscriptions.cancel"), "Expected verified Subscriptions.cancel note:\n{}", notes);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


