use crate::*;

    #[test]
    fn test_post_process_use_client() {
        let content = "import { useState } from 'react';\nexport default function Counter() { const [c, setC] = useState(0); return <div />; }";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("src/app/components/Counter.tsx", content, &metamanifest, false);
        assert!(processed.starts_with("\"use client\";"));

        let content_with = "\"use client\";\nimport { useState } from 'react';";
        let processed_with = post_process_generated_file("src/app/components/Counter.tsx", content_with, &metamanifest, false);
        assert_eq!(processed_with, content_with.to_string());

        let displaced = "import { useState } from 'react';\n\"use client\";\nimport Link from 'next/link';";
        let hoisted = hoist_use_client_directive(displaced);
        assert!(hoisted.starts_with("\"use client\";\nimport { useState }"));
        assert_eq!(hoisted.matches("\"use client\"").count(), 1);
        
        let mui_content = "import { styled } from '@mui/material/styles';\nconst MyDiv = styled('div')({});";
        let processed_mui = post_process_generated_file("src/app/layout.tsx", mui_content, &metamanifest, false);
        assert!(processed_mui.starts_with("\"use client\";"));
    }


    #[test]
    fn test_post_process_use_session_layout() {
        let content = "import { useSession } from 'next-auth/react';\nexport default function RootLayout({ children }) { const { data } = useSession(); return <html><body>{children}</body></html>; }";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("src/app/layout.tsx", content, &metamanifest, false);
        assert!(processed.contains("export default function RootLayout"));
        assert!(processed.contains("<SessionProvider>{children}</SessionProvider>"));
        assert!(processed.starts_with("\"use client\";"));
    }


    #[test]
    fn test_nextjs_middleware_rulepack() {
        let content = r#"import { NextRequest, NextResponse } from 'next/server';

export function middleware(request: NextRequest) {
  return NextResponse.next();
}

export const config = {
  matcher: ['/dashboard/:path*'],
};"#;
        let rules = get_stack_rulepacks("src/middleware.ts", content, None);
        assert!(rules.contains("NEXT.JS MIDDLEWARE & EDGE ROUTING RULES"));
        assert!(rules.contains("PATH-TO-REGEXP MATCHERS"));
        assert!(rules.contains("EDGE RUNTIME BOUNDARY"));
        assert!(rules.contains("SINGLE ROOT/SRC MIDDLEWARE FILE"));
    }


    #[test]
    fn test_inject_axiom_attrs_generics() {
        let input = "const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => { return <form>Hello</form>; };";
        let output = inject_axiom_attrs(input, "src/app/register/page.tsx");
        assert!(!output.contains("<HTMLFormElement data-axiom-component"));
        assert!(output.contains("<form data-axiom-component=\"Page\" data-axiom-file=\"src/app/register/page.tsx\">"));
        
        let input2 = "const [error, setError] = useState<string | null>(null); return <div>Error</div>;";
        let output2 = inject_axiom_attrs(input2, "src/app/register/page.tsx");
        assert!(!output2.contains("<string data-axiom-component"));
        assert!(output2.contains("<div data-axiom-component=\"Page\" data-axiom-file=\"src/app/register/page.tsx\">"));
    }


    #[test]
    fn test_inject_axiom_attrs_context_provider_and_member_expr() {
        // 1. Context Provider should NOT have attributes injected into it
        let input = "return (<CartContext.Provider value={{ cart }}><div>Content</div></CartContext.Provider>);";
        let output = inject_axiom_attrs(input, "src/lib/cartContext.tsx");
        assert!(!output.contains("<CartContext data-axiom-component"));
        assert!(!output.contains("<CartContext.Provider data-axiom-component"));
        assert!(output.contains("<div data-axiom-component=\"CartContext\" data-axiom-file=\"src/lib/cartContext.tsx\">Content</div>"));

        // 2. Member expression components (like Menu.Item) should have attribute placed cleanly on full tag name
        let input2 = "return (<Menu.Item className=\"p-2\"><span>Text</span></Menu.Item>);";
        let output2 = inject_axiom_attrs(input2, "src/components/Menu.tsx");
        assert!(output2.contains("<Menu.Item data-axiom-component=\"Menu\" data-axiom-file=\"src/components/Menu.tsx\" className=\"p-2\">"));
        assert!(!output2.contains("<Menu data-axiom-component"));
    }


    #[test]
    fn test_resolve_app_router_route() {
        assert_eq!(resolve_app_router_route("src/app/page.tsx"), Some("/".to_string()));
        assert_eq!(resolve_app_router_route("src/app/page.jsx"), Some("/".to_string()));
        assert_eq!(resolve_app_router_route("src/app/dashboard/page.tsx"), Some("/dashboard".to_string()));
        assert_eq!(resolve_app_router_route("src/app/dashboard/data-grid/page.tsx"), Some("/dashboard/data-grid".to_string()));
        assert_eq!(resolve_app_router_route("src/app/(auth)/login/page.tsx"), Some("/login".to_string()));
        assert_eq!(resolve_app_router_route("src/app/(dashboard)/analytics/page.tsx"), Some("/analytics".to_string()));
        assert_eq!(resolve_app_router_route("src/app/users/[id]/page.tsx"), Some("/users/[id]".to_string()));
        assert_eq!(resolve_app_router_route("src/components/Navbar.tsx"), None);
        assert_eq!(resolve_app_router_route("src/app/api/auth/route.ts"), None);
    }


    #[test]
    fn test_sanitize_react_icons_imports() {
        let input = "import { HiOutlineChartBar, FaBars } from 'react-icons';\nimport React from 'react';";
        let output = sanitize_react_icons_imports(input);
        assert!(output.contains("import { FaBars } from 'react-icons/fa';"));
        assert!(output.contains("import { HiOutlineChartBar } from 'react-icons/hi';"));
        assert!(!output.contains("from 'react-icons';"));
    }


    #[test]
    fn test_sanitize_app_router_links_with_routes() {
        let valid_routes = vec![
            "/".to_string(),
            "/dashboard".to_string(),
            "/dashboard/data-grid".to_string(),
            "/dashboard/settings".to_string(),
        ];
        let input = r#"<Link href="/data-grid">Data Grid</Link><Link href="/settings-panel">Settings</Link>"#;
        let output = sanitize_app_router_links_with_routes(input, &valid_routes);
        assert!(output.contains(r#"href="/dashboard/data-grid""#));
        assert!(output.contains(r#"href="/dashboard/settings""#));
    }


    #[test]
    fn test_nextjs_react_rulepack_contains_tanstack_query_v5() {
        assert!(NEXTJS_REACT_RULEPACK.contains("TANSTACK REACT QUERY V5 OBJECT SIGNATURE"));
        assert!(NEXTJS_REACT_RULEPACK.contains("useQuery({ queryKey: ['metrics'], queryFn: fetchMetrics })"));
    }



    #[test]
    fn test_sanitize_ts_interfaces() {
        let input = r#"
interface SkeletonProps {
  className?: string;
  data-axiom-component?: string;
  data-axiom-file?: string;
  aria-label?: string;
}

export type ButtonProps = {
  variant?: 'default' | 'outline';
  "data-axiom-component"?: string;
  data-testid?: string;
};
"#;
        let output = sanitize_ts_interfaces(input);
        assert!(!output.contains("data-axiom-component?:"));
        assert!(!output.contains("data-axiom-file?:"));
        assert!(output.contains(r#""aria-label"?: string;"#));
        assert!(output.contains(r#""data-testid"?: string;"#));

        let orphan_input = r#"import React from 'react';
export const Skeleton = () => {
  return (
    <div
      data-axiom-component="Skeleton"
      data-axiom-file="src/components/ui/skeleton.tsx"
    />
  );
};

data-axiom-component="Skeleton"
data-axiom-file="src/components/ui/skeleton.tsx"

export default Skeleton;
"#;
        let orphan_output = sanitize_ts_interfaces(orphan_input);
        assert_eq!(orphan_output.matches(r#"data-axiom-component="Skeleton""#).count(), 1);
        assert_eq!(orphan_output.matches(r#"data-axiom-file="src/components/ui/skeleton.tsx""#).count(), 1);
    }


    #[test]
    fn test_context_isolation_ui_vs_backend_and_auth() {
        let temp_dir = std::env::temp_dir().join("axiom_test_ctx_isolation");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/lib")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/api/auth/[...nextauth]")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();

        std::fs::write(temp_dir.join("prisma/schema.prisma"), "model Project { id String @id }").unwrap();
        std::fs::write(temp_dir.join("src/lib/prisma.ts"), "export const prisma = {};").unwrap();
        std::fs::write(temp_dir.join("src/app/api/auth/[...nextauth]/route.ts"), "export const authOptions = {};").unwrap();
        std::fs::write(temp_dir.join("src/hooks/useProjects.ts"), "export const useProjects = () => [];").unwrap();

        let ui_ctx = get_context_files_for("src/app/dashboard/analytics/page.tsx", &temp_dir);
        assert!(!ui_ctx.contains("schema.prisma"));
        assert!(!ui_ctx.contains("src/lib/prisma.ts"));
        assert!(!ui_ctx.contains("[...nextauth]"));
        assert!(ui_ctx.contains("src/hooks/useProjects.ts"));

        let navbar_ctx = get_context_files_for("src/components/Navbar.tsx", &temp_dir);
        assert!(!navbar_ctx.contains("schema.prisma"));
        assert!(!navbar_ctx.contains("src/hooks/useProjects.ts"));

        let api_ctx = get_context_files_for("src/app/api/projects/route.ts", &temp_dir);
        assert!(api_ctx.contains("schema.prisma"));
        assert!(api_ctx.contains("src/lib/prisma.ts"));
        assert!(!api_ctx.contains("[...nextauth]"));

        let auth_ctx = get_context_files_for("src/app/login/page.tsx", &temp_dir);
        assert!(auth_ctx.contains("[...nextauth]"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_scoped_ui_rules_and_design_archetype() {
        let navbar_rules = get_scoped_ui_rules("src/components/Navbar.tsx");
        assert!(navbar_rules.contains("BRAND LOGO LOCKUP"));
        assert!(navbar_rules.contains("HAMBURGER MENU"));
        assert!(!navbar_rules.contains("SCOPED FOOTER LAYOUT & COMPACTNESS RULES"));
        assert!(!navbar_rules.contains("SCOPED ANALYTICS, KPI & DATA VISUALIZATION RULES"));

        let footer_rules = get_scoped_ui_rules("src/components/Footer.tsx");
        assert!(footer_rules.contains("SCOPED FOOTER LAYOUT & COMPACTNESS RULES"));
        assert!(!footer_rules.contains("BRAND LOGO LOCKUP"));

        let analytics_rules = get_scoped_ui_rules("src/app/dashboard/analytics/page.tsx");
        assert!(analytics_rules.contains("SCOPED ANALYTICS, KPI & DATA VISUALIZATION RULES"));
        assert!(analytics_rules.contains("SCOPED PAGE & DASHBOARD PREVIEW RESILIENCE RULES"));

        let api_rulepacks = get_stack_rulepacks("src/app/api/projects/route.ts", "import { NextResponse } from 'next/server';", None);
        assert!(!api_rulepacks.contains("UI DESIGN & LAYOUT FOUNDATION"));

        let archetype = resolve_design_archetype_prompt("NexusFlow Serbia", "B2B SaaS project management and analytics dashboard", "saas");
        assert!(archetype.contains("Executive B2B SaaS & Collaborative Workspace"));
    }


    #[test]
    fn test_extract_file_path_windows_and_react_hook_imports() {
        let win_line = " ⨯ src\\components\\Navbar.tsx (14:51) @ useState";
        assert_eq!(extract_file_path_from_line(win_line), Some("src\\components\\Navbar.tsx".to_string()));

        let navbar_missing_usestate = r#"import React from 'react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';

const Navbar: React.FC = () => {
  const pathname = usePathname();
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);
  return <nav className="p-4">Brand</nav>;
};
export default Navbar;
"#;
        let processed = post_process_generated_file("src/components/Navbar.tsx", navbar_missing_usestate, &serde_json::json!({}), true);
        assert!(processed.starts_with("\"use client\";"));
        assert!(processed.contains("import React, { useState } from 'react';"));
    }


    #[test]
    fn test_ensure_directory_barrel_files_demanded_barrels() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_barrel_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let src_dir = temp_dir.join("src");
        let app_dir = src_dir.join("app");
        let hooks_dir = src_dir.join("hooks");
        let types_dir = src_dir.join("types");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::create_dir_all(&types_dir).unwrap();

        // Actual hook file in src/hooks
        std::fs::write(hooks_dir.join("useProjects.ts"), "export function useProjects() { return []; }").unwrap();

        // Write a page that imports from @/hooks and @/types and @/utils (which does not exist yet)
        let page_tsx = app_dir.join("page.tsx");
        std::fs::write(&page_tsx, r#"
import React from 'react';
import { useProjects, useAuth } from '@/hooks';
import { UserProfile, DashboardStats } from '@/types';
import { formatCurrency } from '@/utils';

export default function Page() {
    return <div>Test</div>;
}
"#).unwrap();

        ensure_directory_barrel_files(&temp_dir);

        let hooks_index = hooks_dir.join("index.ts");
        let types_index = types_dir.join("index.ts");
        let utils_index = src_dir.join("utils").join("index.ts");

        assert!(hooks_index.exists(), "src/hooks/index.ts should have been created for existing hooks folder");
        assert!(types_index.exists(), "src/types/index.ts should have been created for existing types folder");
        assert!(utils_index.exists(), "src/utils/index.ts should have been created on demand for @/utils");

        let hooks_content = std::fs::read_to_string(&hooks_index).unwrap();
        assert!(hooks_content.contains("export * from './useProjects';"));
        assert!(hooks_content.contains("export const useAuth: any = () => ({} as any);"));

        let types_content = std::fs::read_to_string(&types_index).unwrap();
        assert!(types_content.contains("export type UserProfile = any;"));
        assert!(types_content.contains("export type DashboardStats = any;"));

        let utils_content = std::fs::read_to_string(&utils_index).unwrap();
        assert!(utils_content.contains("export const formatCurrency: any = () => ({} as any);"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_harvester_extracts_and_updates_live_builder_guide() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_harvester_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let prisma_sample = r#"
datasource db {
  provider = "sqlite"
  url = "file:./dev.db"
}

generator client {
  provider = "prisma-client-js"
}

enum Role {
  USER
  ADMIN
}

enum Status {
  ACTIVE
  ARCHIVED
}

model User {
  id        String   @id @default(cuid())
  email     String   @unique
  name      String?
  role      Role     @default(USER)
  projects  Project[]
  createdAt DateTime @default(now())
}

model Project {
  id        String   @id @default(cuid())
  name      String
  status    Status   @default(ACTIVE)
  ownerId   String
  owner     User     @relation(fields: [ownerId], references: [id])
}
"#;

        // 1. Harvest schema.prisma
        harvest_file_contract_to_lbg(&temp_dir, "prisma/schema.prisma", prisma_sample);

        // 2. Harvest API route
        let route_sample = r#"
import { NextResponse, NextRequest } from 'next/server';

export async function GET(request: NextRequest) {
    return NextResponse.json([]);
}

export async function POST(request: NextRequest) {
    return NextResponse.json({ id: '1' });
}
"#;
        harvest_file_contract_to_lbg(&temp_dir, "src/app/api/projects/route.ts", route_sample);

        // 3. Harvest Hook
        let hook_sample = r#"
import { useState, useEffect } from 'react';

export function useProjects() {
    const [projects, setProjects] = useState([]);
    return { projects };
}
"#;
        harvest_file_contract_to_lbg(&temp_dir, "src/hooks/useProjects.ts", hook_sample);

        // Verify live_builder_guide.json
        let lbg_path = temp_dir.join(".axiom").join("live_builder_guide.json");
        assert!(lbg_path.exists());
        let lbg_str = std::fs::read_to_string(&lbg_path).unwrap();
        let lbg: serde_json::Value = serde_json::from_str(&lbg_str).unwrap();

        let shared = lbg.get("sharedContracts").unwrap();
        let db = shared.get("database").unwrap();
        assert!(db.get("models").and_then(|m| m.get("User")).is_some());
        assert!(db.get("models").and_then(|m| m.get("Project")).is_some());
        assert!(db.get("enums").and_then(|e| e.get("Role")).is_some());

        let api = shared.get("api").and_then(|a| a.get("endpoints")).unwrap();
        assert!(api.get("/api/projects").is_some());
        let methods = api.get("/api/projects").and_then(|m| m.as_array()).unwrap();
        assert!(methods.iter().any(|v| v.as_str() == Some("GET")));
        assert!(methods.iter().any(|v| v.as_str() == Some("POST")));

        let hooks = shared.get("hooks").unwrap();
        assert!(hooks.get("useProjects").is_some());

        // Verify prompt formatters
        let db_prompt = format_database_contract_prompt(db);
        assert!(db_prompt.contains("ACTIVE DATABASE MODELS:"));
        assert!(db_prompt.contains("model Project"));
        assert!(db_prompt.contains("model User"));
        assert!(db_prompt.contains("enum Role { USER, ADMIN }"));

        let api_prompt = format_api_endpoints_prompt(api);
        assert!(api_prompt.contains("VERIFIED API ENDPOINTS TO CALL:"));
        assert!(api_prompt.contains("/api/projects"));

        let hooks_prompt = format_hooks_prompt(hooks);
        assert!(hooks_prompt.contains("VERIFIED CUSTOM HOOKS (Import from '@/hooks'):"));
        assert!(hooks_prompt.contains("useProjects"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_custom_hook_and_ui_cot_scopes() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_hook_ui_cot_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::create_dir_all(temp_dir.join(".axiom"));

        // Setup live builder guide with endpoints and hooks
        let lbg = serde_json::json!({
            "projectSpecification": { "category": "saas" },
            "sharedContracts": {
                "database": {},
                "api": {
                    "endpoints": {
                        "/api/projects": ["GET", "POST"]
                    }
                },
                "hooks": {
                    "useProjects": "@/hooks/useProjects"
                }
            }
        });
        std::fs::write(temp_dir.join(".axiom/live_builder_guide.json"), serde_json::to_string_pretty(&lbg).unwrap()).unwrap();

        // 1. Verify custom hook scope
        let hook_prompt = get_expert_system_prompt("src/hooks/useProjects.ts", "nextjs", "", "", "", "", &temp_dir);
        assert!(hook_prompt.contains("Senior React Hooks & State Management Architect"));
        assert!(hook_prompt.contains("CUSTOM REACT HOOK GENERATION CHAIN-OF-THOUGHT"));
        assert!(hook_prompt.contains("Step 1 [Signature & Parameter Safety]"));
        assert!(hook_prompt.contains("Step 4 [Dual Export Compatibility]"));
        assert!(hook_prompt.contains("VERIFIED API ENDPOINTS TO CALL:"));
        assert!(hook_prompt.contains("/api/projects"));
        assert!(!hook_prompt.contains("ROOT LAYOUT ARCHITECTURE RULES"));

        // 2. Verify UI Page route scope gets Default Export Only (never Dual Export)
        let ui_prompt = get_expert_system_prompt("src/app/dashboard/projects/page.tsx", "nextjs", "", "", "", "", &temp_dir);
        assert!(ui_prompt.contains("FRONTEND UI GENERATION CHAIN-OF-THOUGHT"));
        assert!(ui_prompt.contains("Step 1 [Client Boundary & Imports]"));
        assert!(ui_prompt.contains("Step 2 [Domain Mock State]"));
        assert!(ui_prompt.contains("Step 3 [Visual Hierarchy & Layout]"));
        assert!(ui_prompt.contains("Step 4 [Navigation & Default Export Only]"));
        assert!(ui_prompt.contains("DEFAULT EXPORT ONLY:"));
        assert!(!ui_prompt.contains("DUAL EXPORT COMPATIBILITY:"));
        assert!(ui_prompt.contains("VERIFIED API ENDPOINTS TO CALL:"));
        assert!(ui_prompt.contains("/api/projects"));
        assert!(ui_prompt.contains("VERIFIED CUSTOM HOOKS (Import from '@/hooks'):"));
        assert!(ui_prompt.contains("useProjects"));

        // 3. Verify regular UI Component scope still receives Dual Export
        let comp_prompt = get_expert_system_prompt("src/components/Navbar.tsx", "nextjs", "", "", "", "", &temp_dir);
        assert!(comp_prompt.contains("Step 4 [Navigation & Dual Export]"));
        assert!(comp_prompt.contains("DUAL EXPORT COMPATIBILITY:"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_clean_react_navigation_hooks_from_react_imports() {
        // Case 1: Mixed import from react
        let input1 = "import { useRouter, useState, useEffect } from 'react';\nimport { useRouter } from 'next/navigation';";
        let cleaned1 = clean_react_navigation_hooks_from_react_imports(input1);
        assert!(cleaned1.contains("import { useState, useEffect } from 'react';"));
        assert!(cleaned1.contains("import { useRouter } from 'next/navigation';"));
        assert!(!cleaned1.contains("useRouter, useState"));

        // Case 2: Solo useRouter from react
        let input2 = "import { useRouter } from 'react';\nexport default function Page() { return <div />; }";
        let cleaned2 = clean_react_navigation_hooks_from_react_imports(input2);
        assert!(!cleaned2.contains("from 'react'"));

        // Case 3: Default React with useRouter
        let input3 = "import React, { useRouter, useState } from 'react';";
        let cleaned3 = clean_react_navigation_hooks_from_react_imports(input3);
        assert_eq!(cleaned3.trim(), "import React, { useState } from 'react';");

        // Case 4: Full page post-processing
        let page_code = r#"import { useRouter } from 'next/navigation';
import { useRouter, useState, useEffect } from 'react';

export default function AnalyticsPage() {
  const router = useRouter();
  const [data, setData] = useState([]);
  return <div>{data.length}</div>;
}
"#;
        let processed = post_process_generated_file("src/app/real-time-analytics/page.tsx", page_code, &serde_json::json!({}), false);
        // Ensure useRouter is only imported once from next/navigation, and NOT from react
        assert!(processed.contains("import { useRouter } from 'next/navigation';"));
        assert!(processed.contains("import { useState, useEffect } from 'react';"));
        assert!(!processed.contains("useRouter, useState"));
    }


    #[test]
    fn test_hook_use_callback_client_directive_and_ui_metacognitive_prompt() {
        let raw_hook = r#"import { useRouter, useState, useEffect } from 'react';

export function useSuppliers() {
  const router = useRouter();
  const [suppliers, setSuppliers] = useState([]);

  const fetchSuppliers = async () => {
    const res = await fetch('/api/suppliers');
    if (res.ok) setSuppliers(await res.json());
  };

  useEffect(() => {
    fetchSuppliers();
  }, [fetchSuppliers, router]);

  return { suppliers, fetchSuppliers };
}
"#;
        let processed = post_process_generated_file("src/hooks/useSuppliers.tsx", raw_hook, &serde_json::json!({}), false);
        assert!(processed.starts_with("\"use client\";"), "Hook must get 'use client' directive:\n{}", processed);
        assert!(processed.contains("useCallback"), "Action must be memoized with useCallback:\n{}", processed);
        assert!(processed.contains("import { useRouter } from 'next/navigation';"), "useRouter must move to next/navigation:\n{}", processed);
        // Must actually import useCallback alongside useState, useEffect in 'react'
        let react_import_line = processed.lines().find(|l| l.contains("from 'react'")).unwrap();
        assert!(react_import_line.contains("useCallback"), "useCallback MUST be imported from 'react':\n{}", processed);
        assert!(!react_import_line.contains("useRouter"), "useRouter MUST NOT remain in 'react' import:\n{}", processed);

        // Ensure UI generation prompt injects METACOGNITIVE_UI_STATE_RULEPACK for UI files, NOT hooks/API
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_ui_meta_prompt_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let ui_prompt = get_expert_system_prompt("src/components/Navbar.tsx", "nextjs", "", "", "", "", &temp_dir);
        assert!(ui_prompt.contains("METACOGNITIVE UI COMPONENT STATE & INTERACTION PROTOCOL"));
        let hook_prompt = get_expert_system_prompt("src/hooks/useSuppliers.tsx", "nextjs", "", "", "", "", &temp_dir);
        assert!(!hook_prompt.contains("METACOGNITIVE UI COMPONENT STATE & INTERACTION PROTOCOL"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_destructive_code_drop_guard_blocks_wiped_ui() {
        let mut full_tsx = String::from("\"use client\";\nimport React from 'react';\nexport default function DashboardPage() {\n  const title = 'Overview';\n  return (\n    <div className=\"p-6 space-y-4\">\n      <h1>{title}</h1>\n");
        for idx in 0..18 {
            full_tsx.push_str(&format!("      <div className=\"card-{}\"><span className=\"label\">Row {}</span></div>\n", idx, idx));
        }
        full_tsx.push_str("    </div>\n  );\n}\n");

        // 1. Replacing the full component with a minimal stub must be blocked
        let wiped_tsx = "\"use client\";\nexport default function DashboardPage() {\n  return <div className=\"p-4\">Fixed</div>;\n}\n";
        assert!(crate::healer::detect_destructive_code_drop("src/app/dashboard/page.tsx", &full_tsx, wiped_tsx).is_some());

        // 2. Keeping >20 lines of boilerplate while stripping >66% of JSX tags must also be blocked
        let mut stripped_jsx = String::from("\"use client\";\nimport React from 'react';\n");
        for idx in 0..20 {
            stripped_jsx.push_str(&format!("type UnusedHelper{} = string;\n", idx));
        }
        stripped_jsx.push_str("export default function DashboardPage() {\n  return <div>Stub</div>;\n}\n");
        assert!(crate::healer::detect_destructive_code_drop("src/app/dashboard/page.tsx", &full_tsx, &stripped_jsx).is_some());

        // 3. Surgical single-line fix must be allowed
        let surgical_tsx = full_tsx.replace("const title = 'Overview';", "const title: string = 'Overview';");
        assert!(crate::healer::detect_destructive_code_drop("src/app/dashboard/page.tsx", &full_tsx, &surgical_tsx).is_none());

        // 4. Creating a brand-new file (empty before) or editing non-JS files is allowed
        assert!(crate::healer::detect_destructive_code_drop("src/models/NewModel.ts", "", "export interface NewModel { id: string; }\n").is_none());
        assert!(crate::healer::detect_destructive_code_drop("src/app/globals.css", &full_tsx, "body { margin: 0; }").is_none());
    }


    #[test]
    fn test_root_app_layout_hygiene_and_tsconfig() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_root_app_hygiene_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("app")).unwrap();
        std::fs::create_dir_all(temp_dir.join("components")).unwrap();
        std::fs::create_dir_all(temp_dir.join("lib")).unwrap();

        std::fs::write(
            temp_dir.join("tsconfig.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "compilerOptions": {
                    "baseUrl": ".",
                    "paths": { "@/*": ["./src/*"] }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("app/page.tsx"),
            "import { Card } from '~/components/ui';\nimport { cn } from '~/lib/utils';\nexport default function Page() { return <Card className={cn('p-4')}>Hi</Card>; }\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("app/globals.css"),
            "body { margin: 0; }\n.dup { color: red; }\n.dup { color: red; }\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("components/ui.tsx"),
            "import React from 'react';\nexport const Badge = ({ children }: any) => <span>{children}</span>;\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("lib/utils.ts"),
            "export const cn = (...a: string[]) => a.join(' ');\n",
        )
        .unwrap();

        crate::self_healing::run_hygiene_pass_on_project(&temp_dir);

        let ts_val: Value = serde_json::from_str(&std::fs::read_to_string(temp_dir.join("tsconfig.json")).unwrap()).unwrap();
        assert_eq!(ts_val["compilerOptions"]["paths"]["@/*"], serde_json::json!(["./*"]));

        let page_after = std::fs::read_to_string(temp_dir.join("app/page.tsx")).unwrap();
        assert!(page_after.contains("from '@/components/ui'"));
        assert!(page_after.contains("from '@/lib/utils'"));

        let css_after = std::fs::read_to_string(temp_dir.join("app/globals.css")).unwrap();
        assert_eq!(css_after.matches(".dup { color: red; }").count(), 1);

        let ui_after = std::fs::read_to_string(temp_dir.join("components/ui.tsx")).unwrap();
        assert!(ui_after.contains("export const Card ="));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_app_router_route_export_guards_and_template_depth_v48() {
        let page_src = r#"import React from 'react';
import type { Metadata } from 'next';

export const metadata: Metadata = { title: 'Members' };
export type MemberSummary = { id: string; label: string };
export * from './utils';

const sampleTemplate = `literal { inside backtick`;
export const HelperBadge = () => <span>Helper</span>;
export function formatMember(name: string) { return name.trim(); }

export function MembersPage() {
  const inlineObj = { count: 1 };
  const label = `${inlineObj.count} items`;
  return <div>{label}</div>;
}

export { HelperBadge, metadata };
"#;
        let processed_page = post_process_generated_file("src/app/members/page.tsx", page_src, &serde_json::json!({}), false);
        assert!(processed_page.contains("export const metadata: Metadata"));
        assert!(processed_page.contains("export type MemberSummary"));
        assert!(!processed_page.contains("export * from './utils'"));
        assert!(!processed_page.contains("export const HelperBadge"));
        assert!(processed_page.contains("const HelperBadge = () =>"));
        assert!(!processed_page.contains("export function formatMember"));
        assert!(processed_page.contains("function formatMember(name: string)"));
        assert!(!processed_page.contains("export function MembersPage"));
        assert!(processed_page.contains("function MembersPage()"));
        assert!(processed_page.contains("export { metadata };"));
        assert!(!processed_page.contains("export { HelperBadge"));
        assert!(processed_page.contains("export default MembersPage;"));
        assert!(!processed_page.contains("export default HelperBadge;"));

        let layout_src = r#"import React from 'react';
export const metadata = { title: 'Root' };
export function HelperShell() { return <div />; }
export default function RootLayout({ children }: { children: React.ReactNode }) {
  return <html lang="en"><body>{children}</body></html>;
}
"#;
        let processed_layout = post_process_generated_file("src/app/layout.tsx", layout_src, &serde_json::json!({}), false);
        assert!(processed_layout.contains("export const metadata"));
        assert!(!processed_layout.contains("export function HelperShell"));
        assert!(processed_layout.contains("export default function RootLayout"));

        let widget_src = "import React from 'react';\nexport default function Widget() { return <div />; }\n";
        let processed_widget = post_process_generated_file("src/components/Widget.tsx", widget_src, &serde_json::json!({}), false);
        assert!(processed_widget.contains("export { Widget };"));
    }


    #[test]
    fn test_client_hook_boundary_stripping_and_validation_v48() {
        let hook_src = r#""use client";
import { useState } from 'react';
import type { Member } from '@prisma/client';
import { type MemberStatus } from '@prisma/client';
import { prisma } from '../lib/prisma';
import {
  GET,
  POST
} from '@/app/api/events/route';

export function useEvents() {
  const [items, setItems] = useState<Member[]>([]);
  return { items, setItems };
}
"#;
        let cleaned_hook = post_process_generated_file("src/hooks/useEvents.ts", hook_src, &serde_json::json!({}), false);
        assert!(cleaned_hook.contains("import type { Member } from '@prisma/client';"));
        assert!(cleaned_hook.contains("import { type MemberStatus } from '@prisma/client';"));
        assert!(!cleaned_hook.contains("../lib/prisma"));
        assert!(!cleaned_hook.contains("@/lib/prisma"));
        assert!(!cleaned_hook.contains("@/app/api/events/route"));

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let bad_client_api = "\"use client\";\nimport { GET } from '@/app/api/events/route';\nexport default function Panel() { return <div />; }\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/components/Panel.tsx", bad_client_api, "qwen").await.is_err());

            let bad_hook_prisma = "import { prisma } from '@/lib/prisma';\nexport function useEvents() { return { prisma }; }\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/hooks/useEvents.ts", bad_hook_prisma, "qwen").await.is_err());

            let valid_hook_types = "\"use client\";\nimport { useState } from 'react';\nimport type { Member } from '@prisma/client';\nimport { type MemberRole } from '@prisma/client';\nexport function useMembers() { const [list] = useState<Member[]>([]); return { list }; }\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/hooks/useMembers.ts", valid_hook_types, "qwen").await.is_ok());
        });
    }


    #[test]
    fn test_ui_reconciliation_and_wide_props_v49() {
        let narrow = "export interface PillProps {\n  variant?: 'default' | 'success';\n  label?: string;\n}\n";
        let widened = crate::sanitizer::widen_narrow_ui_props_interfaces(narrow);
        assert!(widened.contains("[key: string]: any"), "Narrow UI Props interface must be widened:\n{}", widened);
        let widened_twice = crate::sanitizer::widen_narrow_ui_props_interfaces(&widened);
        assert_eq!(widened, widened_twice, "Widening must be idempotent");

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v49_ui_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/components/cards")).unwrap();
        std::fs::write(
            temp_dir.join("src/components/cards/ItemCard.tsx"),
            "'use client';\nimport { StatusPill } from '../ui/StatusPill';\nexport function ItemCard() { return <StatusPill label=\"OK\" />; }\n",
        )
        .unwrap();

        crate::sanitizer::ensure_ui_primitives_reconciled(&temp_dir);
        let generated_ui = temp_dir.join("src/components/ui/StatusPill.tsx");
        assert!(generated_ui.is_file(), "Relative '../ui/StatusPill' import must synthesize missing UI primitive");
        let ui_src = std::fs::read_to_string(&generated_ui).unwrap();
        assert!(ui_src.contains("export const StatusPill") || ui_src.contains("export function StatusPill"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_auto_import_generics_ambiguity_and_dts_v49() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v49_autoimp_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("node_modules/sample-icons/pack-b")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components/alpha")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components/beta")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/board")).unwrap();

        std::fs::write(
            temp_dir.join("node_modules/sample-icons/package.json"),
            r#"{"name":"sample-icons"}"#,
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("node_modules/sample-icons/pack-b/index.d.ts"),
            "export declare function SampleAddIcon(): JSX.Element;\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/components/UniqueBanner.tsx"),
            "export function UniqueBanner() { return <div />; }\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/components/alpha/DupCard.tsx"),
            "export function DupCard() { return <div />; }\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/components/beta/DupCard.tsx"),
            "export function DupCard() { return <div />; }\n",
        )
        .unwrap();

        let page_file = temp_dir.join("src/app/board/page.tsx");
        let page_src = r#"'use client';
import { useState } from 'react';
export default function BoardPage() {
  const [rows] = useState<RecordType[]>([]);
  const list: Array<ItemRow> = [];
  return (
    <main>
      <UniqueBanner />
      <DupCard />
      <SampleAddIcon />
      <span>{rows.length + list.length}</span>
    </main>
  );
}
"#;
        std::fs::write(&page_file, page_src).unwrap();

        let extra = crate::self_healing::extract_missing_ident_candidates_from_error(
            "ReferenceError: SampleAddIcon is not defined\nReferenceError: window is not defined",
        );
        let updated = crate::self_healing::auto_import_missing_symbols_in_file(
            &temp_dir,
            &page_file,
            "src/app/board/page.tsx",
            &extra,
        )
        .expect("Should auto-import missing SampleAddIcon and UniqueBanner");

        assert!(updated.contains("SampleAddIcon") && updated.contains("sample-icons/pack-b"));
        assert!(updated.contains("UniqueBanner") && updated.contains("@/components/UniqueBanner"));
        assert!(!updated.contains("import { RecordType"), "Must skip TS generic RecordType:\n{}", updated);
        assert!(!updated.contains("import { ItemRow"), "Must skip TS generic ItemRow:\n{}", updated);
        assert!(!updated.contains("DupCard } from"), "Must skip ambiguous local DupCard:\n{}", updated);
        assert!(!updated.contains("import { window"), "Must never auto-import browser global window:\n{}", updated);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_inject_axiom_attrs_utf8_and_self_repair() {
        // 1. Multi-byte UTF-8 punctuation/characters before first JSX tag must NOT shift tag insertion byte offset
        let utf8_tsx = "\"use client\";\n\
// UI note: don’t break — šđčćž ✨\n\
export default function HeroSection() {\n\
  const label = \"Ready — it’s live ✨\";\n\
  return (\n\
    <div className=\"p-6\">\n\
      <span>{label}</span>\n\
    </div>\n\
  );\n\
}\n";
        let injected = crate::sanitizer::inject_axiom_attrs(utf8_tsx, "src/app/page.tsx");
        assert!(
            injected.contains("<div data-axiom-component=\"Page\" data-axiom-file=\"src/app/page.tsx\" className=\"p-6\">"),
            "Expected attributes cleanly inside `<div ...>` after UTF-8 text:\n{}",
            injected
        );
        assert!(!injected.contains("data-axiom-file=\"src/app/page.tsx\" <div"));

        // 2. Post-process must repair previously mangled tag placements (`<secti data-axiom-...` or `data-axiom-... <div`)
        let corrupted_prefix = "\"use client\";\nexport default function Panel() {\n  return (\n    data-axiom-component=\"Page\" data-axiom-file=\"src/app/panel/page.tsx\"<section className=\"box\">\n      <p>Hi</p>\n    </section>\n  );\n}\n";
        let repaired = crate::sanitizer::inject_axiom_attrs(corrupted_prefix, "src/app/panel/page.tsx");
        assert!(
            repaired.contains("<section data-axiom-component=\"Page\" data-axiom-file=\"src/app/panel/page.tsx\" className=\"box\">"),
            "Expected mangled prefix attrs repaired onto `<section>`:\n{}",
            repaired
        );
        assert!(!repaired.contains("data-axiom-file=\"src/app/panel/page.tsx\"<section"));

        let corrupted_prefix_spaced = "\"use client\";\nexport default function PanelSpaced() {\n  return (\n    data-axiom-component=\"Page\" data-axiom-file=\"src/app/panel/page.tsx\"  <section className=\"box\">\n      <p>Hi</p>\n    </section>\n  );\n}\n";
        let repaired_spaced = crate::sanitizer::inject_axiom_attrs(corrupted_prefix_spaced, "src/app/panel/page.tsx");
        assert!(
            repaired_spaced.contains("<section data-axiom-component=\"Page\" data-axiom-file=\"src/app/panel/page.tsx\" className=\"box\">"),
            "Expected spaced mangled prefix attrs repaired onto `<section>`:\n{}",
            repaired_spaced
        );

        let corrupted_split = "\"use client\";\nexport default function SplitBox() {\n  return (\n    <secti data-axiom-component=\"Page\" data-axiom-file=\"src/app/split/page.tsx\"on className=\"box\">\n      <p>Hi</p>\n    </section>\n  );\n}\n";
        let repaired_split = crate::sanitizer::inject_axiom_attrs(corrupted_split, "src/app/split/page.tsx");
        assert!(
            repaired_split.contains("<section data-axiom-component=\"Page\" data-axiom-file=\"src/app/split/page.tsx\" className=\"box\">"),
            "Expected split tag repaired onto `<section>`:\n{}",
            repaired_split
        );
    }


    #[test]
    fn test_barrel_repair_and_ui_children_reconciliation_v51() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v51_barrel_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let src_dir = temp_dir.join("src");
        std::fs::create_dir_all(src_dir.join("components")).unwrap();
        std::fs::create_dir_all(src_dir.join("app/board")).unwrap();

        std::fs::write(
            src_dir.join("components/Navbar.tsx"),
            "'use client';\nexport function Navbar() { return <nav />; }\nexport default Navbar;\n",
        )
        .unwrap();
        std::fs::write(
            src_dir.join("components/ui.tsx"),
            "'use client';\nimport React from 'react';\nexport const Button: React.FC<{ onClick?: () => void; children?: React.ReactNode }> = ({ onClick, children }) => (<button onClick={onClick}>{children}</button>);\nexport const Select: React.FC<{ name: string; onChange: (e: any) => void }> = ({ name, onChange }) => (<select name={name} onChange={onChange}>{children}</select>);\n",
        )
        .unwrap();
        std::fs::write(
            src_dir.join("components/index.ts"),
            "export { default as Navbar } from './Navbar';\nexport { default as Button } from './Button';\nexport { default as Input } from './Input';\n",
        )
        .unwrap();
        std::fs::write(
            src_dir.join("app/board/page.tsx"),
            "import { Button, Select } from '@/components';\nexport default function Board() { return <Button><Select name='s' onChange={() => {}}><option>1</option></Select></Button>; }\n",
        )
        .unwrap();

        crate::sanitizer::ensure_ui_primitives_reconciled(&temp_dir);
        crate::sanitizer::ensure_directory_barrel_files(&temp_dir);

        let updated_barrel = std::fs::read_to_string(src_dir.join("components/index.ts")).unwrap();
        assert!(
            !updated_barrel.contains("./Button") && !updated_barrel.contains("./Input"),
            "Dead re-exports ./Button / ./Input must be stripped from barrel:\n{}",
            updated_barrel
        );
        assert!(
            updated_barrel.contains("export * from './ui';") || updated_barrel.contains("./ui"),
            "Barrel must rescue UI symbols from sibling ui.tsx:\n{}",
            updated_barrel
        );
        assert!(
            updated_barrel.contains("Navbar"),
            "Existing valid sibling export must remain:\n{}",
            updated_barrel
        );

        let updated_ui = std::fs::read_to_string(src_dir.join("components/ui.tsx")).unwrap();
        assert!(
            updated_ui.contains("children?: React.ReactNode") && updated_ui.contains("{ name, onChange, children"),
            "Select arrow component referencing {{children}} must gain children prop & param even with `=>` inside type:\n{}",
            updated_ui
        );

        // Also ensure collect_missing_imports skips dead re-export targets only after barrel cleanup (zero missing)
        let mut missing = Vec::new();
        crate::self_healing::collect_missing_imports(&src_dir, &src_dir, &mut missing);
        assert!(
            !missing.iter().any(|m| m.target_path.to_string_lossy().ends_with("Button.ts") || m.target_path.to_string_lossy().ends_with("Input.ts")),
            "Reconciled barrel must leave zero missing Button/Input targets"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_ui_bare_fc_idempotent_union_and_compound_jsx_v51() {
        // 1. Bare React.FC signatures
        let ui_src = "export const Button: React.FC = ({ onClick, children }) => <button onClick={onClick}>{children}</button>;\nexport const IconBox: React.FC = (props) => <span {...props} />;\nconst Footer: React.FC = () => <footer />;\n";
        let repaired_general = crate::sanitizer::repair_bare_react_fc_signatures(ui_src, false);
        assert!(repaired_general.contains("Button: React.FC<any> = ({ onClick"));
        assert!(repaired_general.contains("const Footer: React.FC = ()"), "Zero-arg Footer outside UI dir should remain untouched");

        let repaired_ui = crate::sanitizer::repair_bare_react_fc_signatures(ui_src, true);
        assert!(repaired_ui.contains("IconBox: React.FC<any> = (props)"));

        // 2. Idempotent (string & {}) literal union widening + Compound Dot-JSX (<Tabs.List>, <Accordion.Item>)
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v51_ui_compound_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let src_dir = temp_dir.join("src");
        std::fs::create_dir_all(src_dir.join("components/ui")).unwrap();
        std::fs::create_dir_all(src_dir.join("app/dashboard")).unwrap();

        std::fs::write(
            src_dir.join("components/ui/badge.tsx"),
            "\"use client\";\nimport React from 'react';\nexport interface BadgeProps {\n  variant?: 'default' | 'success' | 'danger';\n  children?: React.ReactNode;\n}\nexport const Badge: React.FC<BadgeProps> = ({ variant = 'default', children }) => <span data-v={variant}>{children}</span>;\n",
        ).unwrap();

        std::fs::write(
            src_dir.join("components/ui.tsx"),
            "\"use client\";\nimport React from 'react';\nexport const Tabs: React.FC = ({ children }) => <div>{children}</div>;\nexport const Accordion: React.FC = ({ children }) => <div>{children}</div>;\n",
        ).unwrap();

        std::fs::write(
            src_dir.join("app/dashboard/page.tsx"),
            "\"use client\";\nimport { Tabs, Accordion } from '@/components/ui';\nexport default function Dashboard() {\n  return (\n    <Tabs>\n      <Tabs.List><Tabs.Trigger value=\"a\">Tab A</Tabs.Trigger></Tabs.List>\n      <Tabs.Content value=\"a\"><Accordion><Accordion.Item>Hi</Accordion.Item></Accordion></Tabs.Content>\n    </Tabs>\n  );\n}\n",
        ).unwrap();

        crate::sanitizer::ensure_ui_primitives_reconciled(&temp_dir);
        crate::sanitizer::ensure_ui_primitives_reconciled(&temp_dir);

        let badge_out = std::fs::read_to_string(src_dir.join("components/ui/badge.tsx")).unwrap();
        assert_eq!(
            badge_out.matches("(string & {})").count(),
            1,
            "Re-running UI reconciler must be idempotent for union props:\n{}",
            badge_out
        );

        let ui_out = std::fs::read_to_string(src_dir.join("components/ui.tsx")).unwrap();
        assert!(ui_out.contains("export const TabsList"), "Missing synthesized TabsList:\n{}", ui_out);
        assert!(ui_out.contains("export const TabsTrigger"), "Missing synthesized TabsTrigger:\n{}", ui_out);
        assert!(ui_out.contains("export const TabsContent"), "Missing synthesized TabsContent:\n{}", ui_out);
        assert!(ui_out.contains("export const AccordionItem"), "Missing synthesized AccordionItem:\n{}", ui_out);
        assert!(ui_out.contains("(Tabs as any).List = TabsList;"), "Missing runtime Tabs.List assignment:\n{}", ui_out);
        assert!(ui_out.contains("(Accordion as any).Item = AccordionItem;"), "Missing runtime Accordion.Item assignment:\n{}", ui_out);
        assert!(ui_out.contains("Tabs: React.FC<any> & Record<string, any>"), "Missing Tabs Record<string, any> intersection:\n{}", ui_out);
        assert!(ui_out.contains("Accordion: React.FC<any> & Record<string, any>"), "Missing Accordion Record<string, any> intersection:\n{}", ui_out);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


