use crate::*;

    #[test]
    fn test_app_router_route_guards_and_import_boundary_v52() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v52_app_route_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join(".axiom")).unwrap();

        // 1. App Router pages named models/, types/, hooks/, webhooks/, auth/ must be PagesOrUi (layer 7)
        for p in [
            "src/app/models/page.tsx",
            "src/app/types/page.tsx",
            "src/app/hooks/page.tsx",
            "src/app/webhooks/page.tsx",
            "src/app/auth/page.tsx",
        ] {
            assert_eq!(get_generation_phase(p, "nextjs"), GenerationPhase::PagesOrUi, "Failed phase for {}", p);
            assert_eq!(file_dependency_layer_priority(p), 7, "Failed layer priority for {}", p);
            let prompt = get_expert_system_prompt(p, "nextjs", "", "", "", "", &temp_dir);
            assert!(!prompt.contains("DOMAIN MODEL & TYPE RULES"), "App Router page {} got model/type prompt", p);
            assert!(!prompt.contains("HOOK ARCHITECTURE RULES"), "App Router page {} got hook prompt", p);
            assert!(prompt.contains("METACOGNITIVE UI COMPONENT STATE") || prompt.contains("CORE FRONTEND COMPONENT RULES"));
        }

        assert_eq!(get_generation_phase("src/models/User.ts", "nextjs"), GenerationPhase::DatabaseOrMain);
        assert_eq!(file_dependency_layer_priority("src/models/User.ts"), 1);

        // 2. Forbid importing from App Router route modules (`@/app/**/page`, relative `../models/page`) in non-test TS/JS files
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules(
            "src/app/dashboard/page.tsx",
            "import type { InferenceRequest } from '@/app/models/page';\nexport default function Dashboard() { return <div />; }\n",
            "qwen2.5-coder:7b"
        )).is_err());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules(
            "src/app/dashboard/page.tsx",
            "import { x } from '../models/page';\nexport default function Dashboard() { return <div>{x}</div>; }\n",
            "qwen2.5-coder:7b"
        )).is_err());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules(
            "src/app/dashboard/page.tsx",
            "import type { InferenceRequest } from '@/types/inference';\nexport default function Dashboard() { return <div />; }\n",
            "qwen2.5-coder:7b"
        )).is_ok());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules(
            "src/app/dashboard/page.test.tsx",
            "import ModelsPage from '@/app/models/page';\nexport default ModelsPage;\n",
            "qwen2.5-coder:7b"
        )).is_ok());

        // 2b. App Router route files (`page.tsx`, `layout.tsx`, etc.) MUST export a default React Component
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules(
            "src/app/models/page.tsx",
            "export interface Model {\n  id: string;\n  name: string;\n}\nenum Status {\n  ACTIVE = 'ACTIVE',\n}\n",
            "qwen2.5-coder:7b"
        )).is_err());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules(
            "src/app/models/page.tsx",
            "export default { page: '/models' };\n",
            "qwen2.5-coder:7b"
        )).is_err());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules(
            "src/app/models/page.tsx",
            "function ModelsPage() { return <main />; }\nexport default withAuth(ModelsPage);\n",
            "qwen2.5-coder:7b"
        )).is_ok());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules(
            "src/app/models/page.tsx",
            "function ModelsPage() { return <main />; }\nexport { ModelsPage as default };\n",
            "qwen2.5-coder:7b"
        )).is_ok());

        // 3. `format_axiom_contracts_prompt` omits exports/members on `page-route` entries
        std::fs::write(
            temp_dir.join(".axiom").join("axiom-contracts.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "1.0",
                "projectId": "v52-test",
                "files": [
                    {
                        "path": "src/app/models/page.tsx",
                        "role": "page-route",
                        "capabilities": { "exports": ["default", "InferenceRequest"] },
                        "consumedContracts": { "apiCalls": ["/api/models"] },
                        "connectedRoutes": ["/models"]
                    },
                    {
                        "path": "src/types/inference.ts",
                        "role": "domain-model",
                        "capabilities": { "exports": ["InferenceRequest"] },
                        "consumedContracts": {},
                        "connectedRoutes": []
                    }
                ]
            })).unwrap(),
        ).unwrap();
        let contracts_prompt = format_axiom_contracts_prompt(&temp_dir, "src/app/dashboard/page.tsx");
        assert!(contracts_prompt.contains("src/types/inference.ts [domain-model] exports:[InferenceRequest]"));
        assert!(
            !contracts_prompt.contains("src/app/models/page.tsx [page-route] exports:"),
            "page-route exports must not be advertised in contracts prompt:\n{}",
            contracts_prompt
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_ui_primitives_semantic_tags_and_unimported_spinner_v52() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v52_ui_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/components/ui")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/dashboard")).unwrap();

        // Existing `ui.tsx` with broken legacy Table/Link `<div` catch-alls & `ui/card.tsx` already exporting Card
        std::fs::write(
            temp_dir.join("src/components/ui.tsx"),
            "\"use client\";\nimport React from 'react';\nexport const Button = ({ children, variant = 'primary', size = 'md', className = '', ...props }: any) => <button className={`inline-flex items-center justify-center rounded-lg px-4 py-2 text-sm font-medium transition-colors ${className}`} {...props}>{children}</button>;\nexport const Table = ({ children, className = '', ...props }: any) => <div className={className} {...props}>{children}</div>;\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/components/ui/card.tsx"),
            "\"use client\";\nimport React from 'react';\nexport const Card = ({ children, className = '', ...props }: any) => <div className={className} {...props}>{children}</div>;\nexport default Card;\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/app/dashboard/page.tsx"),
            "\"use client\";\nimport { Table, TableHeader, TableRow, TableHead, TableBody, TableCell, Button } from '@/components/ui';\nexport default function DashboardPage() {\n  return <div><Card><Spinner /></Card><Button asChild><a href=\"/models\">Go</a></Button><Table><TableHeader><TableRow><TableHead>A</TableHead></TableRow></TableHeader><TableBody><TableRow><TableCell>1</TableCell></TableRow></TableBody></Table></div>;\n}\n",
        ).unwrap();

        crate::sanitizer::ensure_ui_primitives_reconciled(&temp_dir);
        let ui_src = std::fs::read_to_string(temp_dir.join("src/components/ui.tsx")).unwrap();
        assert!(ui_src.contains("<table"), "Table must render semantic <table>, got:\n{}", ui_src);
        assert!(ui_src.contains("<thead"), "TableHeader must render <thead>, got:\n{}", ui_src);
        assert!(ui_src.contains("<tr"), "TableRow must render <tr>, got:\n{}", ui_src);
        assert!(ui_src.contains("<th"), "TableHead must render <th>, got:\n{}", ui_src);
        assert!(ui_src.contains("<tbody"), "TableBody must render <tbody>, got:\n{}", ui_src);
        assert!(ui_src.contains("<td"), "TableCell must render <td>, got:\n{}", ui_src);
        assert!(ui_src.contains("asChild = false"), "Button should support asChild, got:\n{}", ui_src);
        assert!(ui_src.contains("export const Spinner"), "Unimported <Spinner /> must be synthesized in ui.tsx:\n{}", ui_src);
        assert!(!ui_src.contains("export const Card"), "Card already exported in ui/card.tsx must NOT be duplicated in ui.tsx:\n{}", ui_src);

        let updated_count = crate::self_healing::auto_import_missing_symbols_in_project(&temp_dir);
        assert!(updated_count >= 1);
        let page_src = std::fs::read_to_string(temp_dir.join("src/app/dashboard/page.tsx")).unwrap();
        assert!(page_src.contains("Spinner") && page_src.contains("@/components/ui"), "Expected Spinner imported:\n{}", page_src);
        assert!(page_src.contains("import Card from '@/components/ui/card'") || page_src.contains("import { Card } from '@/components/ui/card'"), "Expected Card imported from unique ui/card.tsx:\n{}", page_src);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_reconcile_external_icon_imports_via_dts_v52() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v52_icons_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("node_modules/lucide-react/dist")).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/lucide-react/package.json"),
            r#"{"name":"lucide-react","typings":"dist/lucide-react.d.ts"}"#,
        ).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/lucide-react/dist/lucide-react.d.ts"),
            "export declare const Server: any;\nexport declare const Cpu: any;\nexport declare const FileText: any;\nexport declare const Filter: any;\nexport declare const Plus: any;\n",
        ).unwrap();

        let src = "\"use client\";\nimport {\n  FiServer,\n  FileText,\n  Filter,\n  HiOutlinePlus,\n} from 'lucide-react';\nexport default function P() { return <><FiServer /><FileText /><Filter /><HiOutlinePlus /></>; }\n";

        // 1. Only lucide-react installed: FiServer -> Server as FiServer, HiOutlinePlus -> Plus as HiOutlinePlus; FileText & Filter stay untouched!
        let fixed_lucide_only = crate::self_healing::reconcile_external_icon_imports_in_content(&temp_dir, src);
        assert!(fixed_lucide_only.contains("Server as FiServer"), "Expected Server as FiServer alias:\n{}", fixed_lucide_only);
        assert!(fixed_lucide_only.contains("Plus as HiOutlinePlus"), "Expected Plus as HiOutlinePlus alias:\n{}", fixed_lucide_only);
        assert!(fixed_lucide_only.contains("FileText") && !fixed_lucide_only.contains("leText as FileText"));
        assert!(fixed_lucide_only.contains("Filter") && !fixed_lucide_only.contains("lter as Filter"));

        // 2. Both lucide-react & react-icons installed: FiServer moves to react-icons/fi, HiOutlinePlus moves to react-icons/hi
        std::fs::create_dir_all(temp_dir.join("node_modules/react-icons/fi")).unwrap();
        std::fs::create_dir_all(temp_dir.join("node_modules/react-icons/hi")).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/react-icons/package.json"),
            r#"{"name":"react-icons","exports":{"./fi":{"types":"./fi/index.d.ts"},"./hi":{"types":"./hi/index.d.ts"}}}"#,
        ).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/react-icons/fi/index.d.ts"),
            "export declare const FiServer: any;\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/react-icons/hi/index.d.ts"),
            "export declare const HiOutlinePlus: any;\n",
        ).unwrap();

        let fixed_both = crate::self_healing::reconcile_external_icon_imports_in_content(&temp_dir, src);
        assert!(fixed_both.contains("import { FileText, Filter } from 'lucide-react';"), "FileText/Filter must stay in lucide-react:\n{}", fixed_both);
        assert!(fixed_both.contains("import { FiServer } from 'react-icons/fi';"), "FiServer must move to react-icons/fi:\n{}", fixed_both);
        assert!(fixed_both.contains("import { HiOutlinePlus } from 'react-icons/hi';"), "HiOutlinePlus must move to react-icons/hi:\n{}", fixed_both);

        // 3. Cross-reverse: Server imported from react-icons/fi -> moved to lucide-react
        let swapped = "import { Server } from 'react-icons/fi';\nexport default function X() { return <Server />; }\n";
        let fixed_swapped = crate::self_healing::reconcile_external_icon_imports_in_content(&temp_dir, swapped);
        assert!(fixed_swapped.contains("import { Server } from 'lucide-react';"), "Expected Server moved to lucide-react:\n{}", fixed_swapped);
        assert!(!fixed_swapped.contains("react-icons/fi"), "Empty react-icons/fi import should be removed:\n{}", fixed_swapped);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_auth_optional_password_cast_and_nextauth_env_v52() {
        let auth_src = r#"import bcrypt from 'bcryptjs';
// user.password in comment
const label = "user.password";
export async function check(user: any, credentials: any, req: any) {
  user.password = "new";
  if (req.user.password === "x") return false;
  if ((user as any)?.password) {}
  return bcrypt.compare(credentials.password, user.hashedPassword || user.password || '');
}
"#;
        let sanitized = crate::sanitizer::sanitize_optional_user_password_access(auth_src);
        assert!(sanitized.contains("// user.password in comment"), "Comment must stay untouched");
        assert!(sanitized.contains("\"user.password\""), "String literal must stay untouched");
        assert!(sanitized.contains("user.password = \"new\";"), "LHS assignment must stay untouched");
        assert!(sanitized.contains("req.user.password === \"x\""), "Chained member must stay untouched");
        assert!(sanitized.contains("credentials.password, (user as any)?.hashedPassword || (user as any)?.password || ''"), "Expected optional cast:\n{}", sanitized);

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v52_env_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/app/api/auth/[...nextauth]")).unwrap();
        std::fs::write(temp_dir.join(".env"), "DATABASE_URL=\"postgresql://localhost/db\"\n").unwrap();
        std::fs::write(temp_dir.join(".env.local"), "NEXTAUTH_URL=\"http://localhost:3001\"\n").unwrap();

        crate::self_healing::ensure_nextauth_env_vars(&temp_dir);

        let env_root = std::fs::read_to_string(temp_dir.join(".env")).unwrap();
        let env_local = std::fs::read_to_string(temp_dir.join(".env.local")).unwrap();
        assert!(env_root.contains("NEXTAUTH_URL=\"http://localhost:3000\""), "Missing NEXTAUTH_URL in .env:\n{}", env_root);
        assert!(env_root.contains("NEXTAUTH_SECRET="), "Missing NEXTAUTH_SECRET in .env:\n{}", env_root);
        assert!(env_local.contains("NEXTAUTH_URL=\"http://localhost:3001\""), "Custom NEXTAUTH_URL in .env.local must be preserved:\n{}", env_local);
        assert!(env_local.contains("NEXTAUTH_SECRET="), "Missing NEXTAUTH_SECRET in .env.local:\n{}", env_local);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_tier2_next_build_log_extraction_and_extensionless_resolve_v52() {
        // Keep existing `.next/server` invariant
        assert_eq!(
            extract_file_path_from_line("  at eval (./.next/server/app/products/page.js:151:1)"),
            None
        );
        // Map `.next/types/app/...` back to `src/app/...`
        assert_eq!(
            extract_file_path_from_line(".next/types/app/models/page.ts:12:4"),
            Some("src/app/models/page.tsx".to_string())
        );
        assert_eq!(
            extract_file_path_from_line(".next/types/app/api/models/route.ts:5:2"),
            Some("src/app/api/models/route.ts".to_string())
        );
        // Do not skip folders named `/builds/`
        assert_eq!(
            extract_file_path_from_line("src/app/builds/page.tsx:5:1"),
            Some("src/app/builds/page.tsx".to_string())
        );
        // Prerender & default-export React Component log extraction
        assert_eq!(
            extract_file_path_from_line("Error occurred prerendering page \"/models\". Read more: https://nextjs.org/docs/messages/prerender-error"),
            Some("src/app/models/page.tsx".to_string())
        );
        assert_eq!(
            extract_file_path_from_line("Error: The default export is not a React Component in \"/models\""),
            Some("src/app/models/page.tsx".to_string())
        );
        assert_eq!(
            extract_file_path_from_line("  page: '/models'"),
            Some("src/app/models/page.tsx".to_string())
        );
        assert_eq!(
            extract_file_path_from_line("Warning: unexpected token on page: \"introduction\""),
            None
        );
        // Extensionless `typeof import(".../src/app/models/page")` fallback
        assert_eq!(
            extract_file_path_from_line("Type error: Type 'OmitWithTag<typeof import(\"C:/proj/src/app/models/page\"), \"default\">'"),
            Some("src/app/models/page".to_string())
        );

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v52_resolve_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/app/models")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/api/auth/[...nextauth]")).unwrap();
        std::fs::write(temp_dir.join("src/app/models/page.tsx"), "export default function Models() { return null; }\n").unwrap();
        std::fs::write(temp_dir.join("src/app/api/auth/[...nextauth]/route.ts"), "export const GET = () => {};\n").unwrap();

        assert_eq!(
            resolve_project_file_candidate(&temp_dir, "src/app/models/page"),
            Some("src/app/models/page.tsx".to_string())
        );
        assert_eq!(
            resolve_project_file_candidate(&temp_dir, "src/app/api/auth/[...nextauth]/route"),
            Some("src/app/api/auth/[...nextauth]/route.ts".to_string())
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_v52_preview_chunk_url_mapping_and_node_modules_guard() {
        use crate::server::{extract_file_path_from_line, map_next_chunk_url_to_source, resolve_project_file_candidate};

        assert_eq!(
            map_next_chunk_url_to_source("http://localhost:3000/_next/static/chunks/app/layout.js"),
            Some("src/app/layout.tsx".to_string())
        );
        assert_eq!(
            map_next_chunk_url_to_source("http://localhost:3000/_next/static/chunks/app/alerts/page-9a8b7c6d.js?v=1"),
            Some("src/app/alerts/page.tsx".to_string())
        );
        assert_eq!(
            extract_file_path_from_line("Uncaught ChunkLoadError: Loading chunk app/layout failed (timeout: http://localhost:3000/_next/static/chunks/app/layout.js)"),
            Some("src/app/layout.tsx".to_string())
        );
        assert_eq!(
            extract_file_path_from_line("webpack-internal:///(app-pages-browser)/./node_modules/next/dist/compiled/react-server-dom-webpack/cjs/react-server-dom-webpack-client.browser.development.js:44:12"),
            None
        );

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v52_chunk_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/app")).unwrap();
        std::fs::write(temp_dir.join("src/app/layout.tsx"), "export default function RootLayout({ children }: { children: React.ReactNode }) { return <html><body>{children}</body></html>; }\n").unwrap();

        assert_eq!(
            resolve_project_file_candidate(&temp_dir, "http://localhost:3000/_next/static/chunks/app/layout.js"),
            Some("src/app/layout.tsx".to_string())
        );
        assert_eq!(
            resolve_project_file_candidate(&temp_dir, "webpack-internal:///(app-pages-browser)/./node_modules/next/dist/foo.js"),
            None
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_v52_root_layout_children_enforcement_and_drop_recovery() {
        use crate::healer::detect_destructive_code_drop;
        use crate::orchestrator::validate_file_with_abstract_rules;
        use crate::sanitizer::sanitize_root_layout_missing_children;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let broken_dashboard_layout = r#"
'use client';
import Navbar from '@/components/ui/Navbar';
export default function RootLayout() {
  return (
    <html lang="en">
      <body>
        <Navbar />
        <main className="p-6"><h1>Perimeter Sentinel</h1><table><tbody><tr><td>0</td></tr></tbody></table></main>
      </body>
    </html>
  );
}
"#;
        let validation_err = rt
            .block_on(validate_file_with_abstract_rules("src/app/layout.tsx", broken_dashboard_layout, "test-model"))
            .expect_err("RootLayout without {children} must fail validation");
        assert!(validation_err.contains("{children}"));

        let hook_layout = "import { useAlerts } from '@/hooks/useAlerts';\nexport default function RootLayout({ children }: { children: React.ReactNode }) { useAlerts(); return <html><body>{children}</body></html>; }\n";
        assert!(
            rt.block_on(validate_file_with_abstract_rules("src/app/layout.tsx", hook_layout, "test-model")).is_err(),
            "RootLayout importing domain hooks must fail validation"
        );

        let long_broken_before = format!(
            "{}\n{}",
            "export default function RootLayout({ children }: { children: React.ReactNode }) { return <html><body><div>DashboardCard</div><div>KPI</div><div>Chart</div><div>Grid</div><div>Feed</div><div>Panel</div><div>Table</div></body></html>; }",
            "// filler\n".repeat(45)
        );
        let recovered_after = "export default function RootLayout({ children }: { children: React.ReactNode }) {\n  return <html lang=\"en\"><body>{children}</body></html>;\n}\n";
        assert_eq!(
            detect_destructive_code_drop("src/app/layout.tsx", &long_broken_before, recovered_after),
            None,
            "Restoring missing {{children}} in layout.tsx must be exempt from destructive drop guard (even when {{ children }} was only in props destructure!)"
        );

        let broken_with_props_only = r#"
import Navbar from '@/components/ui/Navbar';
export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        <Navbar />
        <main className="p-6"><h1>Perimeter Sentinel</h1></main>
      </body>
    </html>
  );
}
"#;
        assert!(
            rt.block_on(validate_file_with_abstract_rules("src/app/layout.tsx", broken_with_props_only, "test-model")).is_err(),
            "RootLayout that declares {{ children }} prop without rendering it in JSX must fail validation"
        );
        let repaired = sanitize_root_layout_missing_children(broken_with_props_only, "src/app/layout.tsx", true);
        assert!(repaired.contains("</main>") && repaired.contains("{children}"), "Sanitizer must restore {{children}} inside JSX:\n{}", repaired);
        assert!(repaired.contains("Navbar"), "Navbar shell should remain intact:\n{}", repaired);
    }


    #[test]
    fn test_v52_stripe_v23_dts_exports_and_webhook_guidance() {
        use crate::self_healing::{inspect_dts_exports_for_file, resolve_external_dts_path};

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v52_stripe_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("node_modules/stripe/types/api")).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/stripe/package.json"),
            r#"{"name":"stripe","version":"23.1.0","exports":{"import":{"types":"./types/index.d.ts","default":"./esm/stripe.esm.node.js"}}}"#,
        ).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/stripe/types/index.d.ts"),
            "export class Stripe { constructor(key: string, config?: any); }\nexport default Stripe;\n",
        ).unwrap();

        let dts_path = resolve_external_dts_path(&temp_dir, "stripe");
        assert!(dts_path.is_some(), "Conditional root package.json exports without '.' key must resolve Stripe .d.ts");

        let broken_webhook = r#"
import Stripe from 'stripe';
import { z } from 'zod';
const schema = z.object({ status: z.nativeEnum(Stripe.PaymentIntent.Status) });
export async function POST(request: Request) {
  const stripe = new Stripe(process.env.STRIPE_SECRET_KEY!, { apiVersion: '2026-02-25.clover' });
  const event = stripe.webhooks.constructEvent(request.body, request.headers.get('stripe-signature')!, process.env.STRIPE_WEBHOOK_SECRET!);
  const sub = await stripe.subscriptions.retrieve('sub_1');
  const priceId = sub.items.data[0].plan.id;
  const inv = await stripe.invoices.retrieve('in_1');
  return Response.json({ pi: inv.payment_intent, priceId });
}
"#;
        let guidance = inspect_dts_exports_for_file(&temp_dir, broken_webhook, "TS2339 TS2322 stripe");
        assert!(guidance.contains("request.text()"), "Expected raw webhook body guidance:\n{}", guidance);
        assert!(guidance.contains("z.enum") || guidance.contains("z.string"), "Expected z.nativeEnum(Stripe.*) guidance:\n{}", guidance);
        assert!(guidance.contains("price"), "Expected SubscriptionItem.plan -> price guidance:\n{}", guidance);
        assert!(guidance.contains("payment_intent"), "Expected Invoice.payment_intent guidance:\n{}", guidance);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_v52_ui_barrel_hook_split_prisma_type_and_generic_array_fix() {
        use crate::sanitizer::ensure_hook_type_exports;
        use crate::self_healing::{auto_import_missing_symbols_in_content, extract_prisma_schema_summary_for_healing};

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v52_barrel_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components/ui")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/alerts")).unwrap();

        std::fs::write(
            temp_dir.join("prisma/schema.prisma"),
            "enum SubscriptionPlan { FREE PRO }\nmodel Alert { id String @id\n title String }\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/components/ui/index.ts"),
            "export { Button } from './Button';\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/components/ui/Button.tsx"),
            "export const Button = () => <button />;\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/hooks/useAlerts.ts"),
            "export function useAlerts() { return { alerts: [] }; }\n",
        ).unwrap();

        let hook_raw = "import { useState } from 'react';\ninterface SurveillanceFeed { id: string; url: string; }\nexport function useDroneFeed<T extends SurveillanceFeed>() { return useState<T[]>([]); }\n";
        let hook_fixed = ensure_hook_type_exports(hook_raw, "src/hooks/useDroneFeed.ts");
        assert!(hook_fixed.contains("export interface SurveillanceFeed"), "Hook local interface used in exported signature must be promoted to export:\n{}", hook_fixed);
        std::fs::write(temp_dir.join("src/hooks/useDroneFeed.ts"), &hook_fixed).unwrap();

        let prisma_summary = extract_prisma_schema_summary_for_healing(&temp_dir);
        assert!(prisma_summary.contains("prisma.<enum>") || prisma_summary.contains("SubscriptionPlan"), "Prisma summary must warn enums have no prisma delegate:\n{}", prisma_summary);

        let page_raw = r#"
import { Button, useAlerts } from '@/components/ui';
import { useDroneFeed, SurveillanceFeed } from '@/hooks/useDroneFeed';
export default function AlertsPage({ items }: { items: Alert[] }) {
  const { alerts } = useAlerts();
  const [feeds] = useDroneFeed<SurveillanceFeed[]>([]);
  return <div><Button />{alerts.length}{feeds.length}{items.length}</div>;
}
"#;
        let abs_page = temp_dir.join("src/app/alerts/page.tsx");
        std::fs::write(&abs_page, page_raw).unwrap();
        let fixed_page = auto_import_missing_symbols_in_content(
            &temp_dir,
            &abs_page,
            "src/app/alerts/page.tsx",
            page_raw,
            &["Alert".to_string()],
        ).expect("Page should be healed by auto_import_missing_symbols_in_content");

        assert!(fixed_page.contains("import { useAlerts } from '@/hooks/useAlerts'"), "useAlerts must be split out of @/components/ui:\n{}", fixed_page);
        assert!(fixed_page.contains("import { Button } from '@/components/ui'"), "Button stays in @/components/ui:\n{}", fixed_page);
        assert!(fixed_page.contains("from '@prisma/client'") && fixed_page.contains("Alert"), "Alert must be type-imported from @prisma/client:\n{}", fixed_page);
        assert!(fixed_page.contains("useDroneFeed<SurveillanceFeed>(") && !fixed_page.contains("useDroneFeed<SurveillanceFeed[]>"), "Redundant array suffix on generic hook extending single item must be stripped:\n{}", fixed_page);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_v52_middleware_metacognitive_handbook_and_glob_rejection() {
        use crate::orchestrator::{get_middleware_metacognitive_rules, validate_file_with_abstract_rules};

        let rt = tokio::runtime::Runtime::new().unwrap();
        let glob_middleware = r#"
import { NextResponse } from 'next/server';
export function middleware() { return NextResponse.next(); }
export const config = { matcher: ['/dashboard/*', '/admin/**'] };
"#;
        assert!(
            rt.block_on(validate_file_with_abstract_rules("src/middleware.ts", glob_middleware, "test-model")).is_err(),
            "Middleware with invalid glob /* instead of :path* must be rejected"
        );

        let valid_middleware = r#"
import { NextResponse } from 'next/server';
export function middleware() { return NextResponse.next(); }
export const config = { matcher: ['/dashboard/:path*', '/((?!api|_next/static|_next/image|favicon.ico|.*\\..*).*)'] };
"#;
        assert!(
            rt.block_on(validate_file_with_abstract_rules("src/middleware.ts", valid_middleware, "test-model")).is_ok(),
            "Clean two-layer middleware must pass validation"
        );

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v52_mw_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/app/api/webhooks/stripe")).unwrap();
        std::fs::write(
            temp_dir.join("package.json"),
            r#"{"dependencies":{"next":"14.2.35","next-auth":"4.24.0","stripe":"23.1.0","next-intl":"3.0.0"}}"#,
        ).unwrap();
        std::fs::write(temp_dir.join("src/app/api/webhooks/stripe/route.ts"), "export async function POST() {}\n").unwrap();

        let meta_rules = get_middleware_metacognitive_rules(Some(&temp_dir), "");
        assert!(meta_rules.contains("next-intl"), "Expected multi-locale rule:\n{}", meta_rules);
        assert!(meta_rules.contains("/api/webhooks"), "Expected Stripe webhook bypass rule:\n{}", meta_rules);
        assert!(meta_rules.contains("MAINTENANCE_MODE"), "Expected maintenance gate rule:\n{}", meta_rules);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    #[test]
    fn test_v52_comparative_exponential_accuracy_equation_and_sft_export() {
        use crate::katastar::{compute_model_progress_telemetry_at, export_mini_llm_training_dataset_at};

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v52_telemetry_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let projects_dir = temp_dir.join("projects");
        let export_dir = temp_dir.join("exports");
        std::fs::create_dir_all(projects_dir.join("p50/.axiom")).unwrap();
        std::fs::create_dir_all(projects_dir.join("p51/.axiom")).unwrap();
        std::fs::create_dir_all(projects_dir.join("p52/.axiom")).unwrap();
        std::fs::create_dir_all(projects_dir.join("p50/src/app")).unwrap();
        std::fs::create_dir_all(projects_dir.join("p51/src/app")).unwrap();
        std::fs::create_dir_all(projects_dir.join("p52/src/app")).unwrap();

        for i in 0..10 {
            std::fs::write(projects_dir.join(format!("p50/src/app/f{}.tsx", i)), "export default function X(){return null;}").unwrap();
            std::fs::write(projects_dir.join(format!("p51/src/app/f{}.tsx", i)), "export default function X(){return null;}").unwrap();
            std::fs::write(projects_dir.join(format!("p52/src/app/f{}.tsx", i)), "export default function X(){return null;}").unwrap();
        }

        // v0.1.50: 59 errors across 4 files; v0.1.51: 30 errors across 2 files; v0.1.52: 0 errors (clean build)
        let projects_json = serde_json::json!([
            {
                "id": "p50", "name": "Demo v50", "createdAt": "2026-11-10T12:00:00Z",
                "metadata": { "appVersion": "0.1.50", "modelId": "qwen2.5-coder:7b", "warnings": ["TypeScript provera prijavila 59 grešaka: src/app/f0.tsx src/app/f1.tsx src/app/f2.tsx src/app/f3.tsx"] }
            },
            {
                "id": "p51", "name": "Demo v51", "createdAt": "2026-11-11T12:00:00Z",
                "metadata": { "appVersion": "0.1.51", "modelId": "qwen2.5-coder:7b", "warnings": ["TypeScript provera prijavila 30 grešaka: src/app/f0.tsx src/app/f1.tsx"] }
            },
            {
                "id": "p52", "name": "Demo v52", "createdAt": "2026-11-12T12:00:00Z",
                "metadata": { "appVersion": "0.1.52", "modelId": "qwen2.5-coder:7b", "warnings": [] }
            }
        ]);
        let projects_file = temp_dir.join("projects.json");
        std::fs::write(&projects_file, serde_json::to_string_pretty(&projects_json).unwrap()).unwrap();

        std::fs::write(
            projects_dir.join("p52/.axiom/heal-ledger.json"),
            r#"{"files":{"src/app/layout.tsx":{"resolved":["Restored missing {children} in RootLayout"],"unresolved":[],"attempts":[{"round":1,"tier":"tier1","strategy":"sanitize_root_layout","outcome":"verified","summary":"Added {children}"}]}}}"#,
        ).unwrap();

        let katastar_path = temp_dir.join("katastar.json");
        std::fs::write(&katastar_path, r#"{"version":1,"entries":[]}"#).unwrap();
        let settings_path = temp_dir.join("settings.json");
        std::fs::write(&settings_path, r#"{"ollamaModel":"qwen2.5-coder:7b"}"#).unwrap();

        let telemetry = compute_model_progress_telemetry_at(
            &projects_file,
            &projects_dir,
            &katastar_path,
            &settings_path,
            Some("qwen2.5-coder:7b"),
        ).unwrap();

        let timeline = telemetry["versionsTimeline"].as_array().unwrap();
        assert_eq!(timeline.len(), 3);
        let acc_50 = timeline[0]["rawAccuracyPct"].as_f64().unwrap();
        let acc_51 = timeline[1]["rawAccuracyPct"].as_f64().unwrap();
        let acc_52 = timeline[2]["rawAccuracyPct"].as_f64().unwrap();
        assert!(acc_51 > acc_50, "v0.1.51 ({}) must beat v0.1.50 ({})", acc_51, acc_50);
        assert!(acc_52 > acc_51, "v0.1.52 ({}) must beat v0.1.51 ({})", acc_52, acc_51);
        assert!(timeline[2]["deltaVsPrevPct"].as_f64().unwrap() > 0.0);
        assert!(timeline[2]["deltaVsBestPct"].as_f64().unwrap() > 0.0);

        let exp_res = export_mini_llm_training_dataset_at(&projects_dir, &katastar_path, &export_dir, "qwen2.5-coder:3b").unwrap();
        assert!(exp_res["sampleCount"].as_u64().unwrap() >= 4);
        assert!(export_dir.join("axiom_sft_dataset.jsonl").is_file());
        assert!(export_dir.join("Modelfile").is_file());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


