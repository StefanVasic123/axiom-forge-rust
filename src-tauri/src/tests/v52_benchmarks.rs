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


    #[test]
    fn test_v52_loyaltyhub_css_ts_jsx_middleware_pkg_and_katastar_fixes() {
        // 1. CSS: @apply transform translateX(100%), duplicated @layer components, stray `}`, and orphan declarations
        let broken_css = r#"@keyframes slideIn {
  from {
    transform: translateX(100%);
  }
  to {
    transform: translateX(0);
  }
}
@layer components {
  .slide-in {
    @apply transform translateX(100%);
    animation: slideIn 0.3s ease-in-out forwards;
  }
}

@layer components {
  .slide-in {
    @apply transform translateX(100%);
    animation: slideIn 0.3s ease-in-out forwards;
  }
}
  .slide-in {
    @apply transform translateX(100%);
    animation: slideIn 0.3s ease-in-out forwards;
  }
}
  @apply transform translateX(100%);
  animation: slideIn 0.3s ease-in-out forwards;
}
"#;
        let fixed_css = post_process_generated_file("src/app/globals.css", broken_css, &serde_json::json!({}), true);
        assert!(fixed_css.contains("@tailwind base;"), "Missing @tailwind base:\n{}", fixed_css);
        assert!(!fixed_css.contains("@apply transform translateX"), "Invalid @apply transform translateX must be converted:\n{}", fixed_css);
        assert!(fixed_css.contains("transform: translateX(100%);"), "Expected native CSS transform:\n{}", fixed_css);
        assert_eq!(fixed_css.matches("@layer components").count(), 1, "Duplicate @layer components must be collapsed:\n{}", fixed_css);
        assert_eq!(fixed_css.matches('{').count(), fixed_css.matches('}').count(), "CSS braces must balance cleanly:\n{}", fixed_css);
        assert!(!fixed_css.trim_end().ends_with("forwards;\n}"), "Orphan tail declaration must be dropped:\n{}", fixed_css);

        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules("src/app/globals.css", broken_css, "")).is_err());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules("src/app/globals.css", &fixed_css, "")).is_ok());

        // 2. `.ts` JSX-to-React.createElement conversion (`src/config/i18n.ts`) without breaking TS generics
        let broken_i18n_ts = r#"import { I18nextProvider, initReactI18next } from 'react-i18next';
import i18n from 'i18next';

const locales: Array<string> = ['en', 'es'];

export default function I18nProvider({ children }: { children: React.ReactNode }) {
  return <I18nextProvider i18n={i18n}>{children}</I18nextProvider>;
}
export { I18nProvider, locales };
"#;
        let fixed_i18n = post_process_generated_file("src/config/i18n.ts", broken_i18n_ts, &serde_json::json!({}), false);
        assert!(fixed_i18n.contains("import React from 'react';"), "Must inject React import for createElement:\n{}", fixed_i18n);
        assert!(fixed_i18n.contains("React.createElement(I18nextProvider, { i18n: i18n }, children)"), "Must rewrite JSX to React.createElement:\n{}", fixed_i18n);
        assert!(fixed_i18n.contains("Array<string>"), "Must preserve TS generics:\n{}", fixed_i18n);
        assert!(!fixed_i18n.contains("</I18nextProvider>"), "Closing JSX tag must be gone:\n{}", fixed_i18n);
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules("src/config/i18n.ts", broken_i18n_ts, "")).is_err());
        assert!(rt.block_on(crate::orchestrator::validate_file_with_abstract_rules("src/config/i18n.ts", &fixed_i18n, "")).is_ok());

        // 3. `src/middleware.ts` redundant inline static skip stripped when negative lookahead matcher already excludes `_next/static`
        let redundant_mw = r#"import { NextResponse, NextRequest } from 'next/server';

export async function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;

  // Bypass static asset queries at the matcher layer
  if (pathname.startsWith('/_next/static') || pathname.startsWith('/_next/image') || pathname.includes('.')) {
    return NextResponse.next();
  }

  if (pathname.startsWith('/dashboard')) {
    return NextResponse.redirect(new URL('/login', request.url));
  }

  return NextResponse.next();
}

export const config = {
  matcher: [
    '/((?!api|_next/static|_next/image|_vercel|favicon\\.ico|.*\\..*).*)'
  ]
};
"#;
        let cleaned_mw = crate::sanitizer::sanitize_middleware_redundancy(redundant_mw, "src/middleware.ts");
        assert!(!cleaned_mw.contains("pathname.startsWith('/_next/static')"), "Redundant static branch must be stripped:\n{}", cleaned_mw);
        assert!(cleaned_mw.contains("pathname.startsWith('/dashboard')"), "Auth branch must remain:\n{}", cleaned_mw);

        // 4. package.json strips `@types/prisma` and legacy `resolutions.typescript: 4.x` under TS 5
        let mut pkg = serde_json::json!({
            "name": "loyaltyhub",
            "dependencies": { "next": "14.2.35", "react": "^18.2.0", "react-dom": "^18.2.0", "@prisma/client": "5.22.0" },
            "devDependencies": { "@types/prisma": "^1.0.0", "prisma": "5.22.0", "typescript": "^5.3.3" },
            "resolutions": { "typescript": "4.9.4" }
        });
        assert!(crate::packages::sanitize_package_json_value_for_offline(&mut pkg));
        assert!(pkg["devDependencies"].get("@types/prisma").is_none(), "@types/prisma must be removed: {:?}", pkg);
        assert!(pkg.get("resolutions").is_none(), "Empty resolutions after dropping stale TS 4.9.4 pin must be removed: {:?}", pkg);

        // 5. Katastar rejects boilerplate Serbian fallback hints
        let temp_k = std::env::temp_dir().join(format!(
            "axiom_v52_katastar_hint_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let k_path = temp_k.join("katastar.json");
        let eid = crate::katastar::register_repair_attempt_at(
            &k_path,
            "compile",
            "src/a.ts(1,1): error TS1005: '>' expected.",
            "src/a.ts",
            "ai",
            "ai_edit",
            None,
            "Popravka generisana od strane AI modela",
            "qwen2.5-coder:7b",
        );
        crate::katastar::finalize_katastar_outcome_at(
            &k_path,
            &eid,
            true,
            "ai",
            "tsc_noemit",
            None,
            Some("Izvučen popravljen kod iz AI odgovora"),
            "qwen2.5-coder:7b",
        );
        let store = crate::katastar::load_katastar_store_at(&k_path);
        assert_eq!(store.entries[0].hint, "", "Boilerplate Serbian fallback hint must NOT pollute katastar");
        let _ = std::fs::remove_dir_all(&temp_k);
    }


    #[test]
    fn test_v53_reviewer_pipeline_status_middleware_and_api_route_forms() {
        use crate::orchestrator::{sync_live_builder_guide_pipeline_status, validate_file_with_abstract_rules};
        use crate::reviewer::{
            check_delimiter_balance, diagnose_heal_stagnation, extract_middleware_matcher_exclusions,
            review_generated_file, sanitize_middleware_principled, ReviewSeverity,
        };

        // 1. LiveBuilderGuide `generationPipeline[*].status` transitions pending -> in_progress -> completed
        let temp_lbg = std::env::temp_dir().join(format!(
            "axiom_v53_lbg_status_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_lbg.join(".axiom")).unwrap();
        let lbg_init = serde_json::json!({
            "generationPipeline": [
                { "phaseId": "dependencies", "files": ["package.json"], "status": "pending" },
                { "phaseId": "auth", "files": ["src/app/api/register/route.ts", "src/app/login/page.tsx"], "status": "pending" }
            ]
        });
        std::fs::write(
            temp_lbg.join(".axiom/live_builder_guide.json"),
            serde_json::to_string_pretty(&lbg_init).unwrap(),
        )
        .unwrap();

        sync_live_builder_guide_pipeline_status(
            &temp_lbg,
            &["package.json".to_string(), "src/app/api/register/route.ts".to_string()],
            false,
        );
        let mid: Value = serde_json::from_str(&std::fs::read_to_string(temp_lbg.join(".axiom/live_builder_guide.json")).unwrap()).unwrap();
        assert_eq!(mid["generationPipeline"][0]["status"], "completed");
        assert_eq!(mid["generationPipeline"][1]["status"], "in_progress");

        sync_live_builder_guide_pipeline_status(&temp_lbg, &[], true);
        let fin: Value = serde_json::from_str(&std::fs::read_to_string(temp_lbg.join(".axiom/live_builder_guide.json")).unwrap()).unwrap();
        assert_eq!(fin["generationPipeline"][0]["status"], "completed");
        assert_eq!(fin["generationPipeline"][1]["status"], "completed");
        let _ = std::fs::remove_dir_all(&temp_lbg);

        // 2. App Router API Route Handler rejects Pages Router types & importing from `@/app/api/.../route`
        let rt = tokio::runtime::Runtime::new().unwrap();
        let bad_pages_api = r#"import { NextApiRequest, NextApiResponse } from 'next';
import { NextResponse } from 'next/server';
export async function POST(request: NextApiRequest, response: NextApiResponse) {
  return NextResponse.json({ ok: true });
}
"#;
        assert!(rt.block_on(validate_file_with_abstract_rules("src/app/api/register/route.ts", bad_pages_api, "")).is_err());

        let bad_route_import = r#"import { NextResponse } from 'next/server';
import { authOptions } from '@/app/api/auth/[...nextauth]/route';
export async function POST() {
  return NextResponse.json({ hasOpts: Boolean(authOptions) });
}
"#;
        assert!(rt.block_on(validate_file_with_abstract_rules("src/app/api/register/route.ts", bad_route_import, "")).is_err());

        // 3. Principled Middleware parser & unreachable-branch sanitizer + Edge-unsafe check
        let mw_with_unreachable = r#"import { NextResponse, NextRequest } from 'next/server';

export async function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;

  if (pathname.startsWith('/_next/static') || pathname.endsWith('.ico')) {
    return NextResponse.next();
  }

  if (pathname.startsWith('/dashboard')) {
    return NextResponse.redirect(new URL('/login', request.url));
  }

  if (pathname.startsWith('/api')) {
    const locale = pathname.split('/')[2];
    if (locale === 'es') {
      return NextResponse.rewrite(new URL(`/${locale}${pathname}`, request.url));
    }
  }

  return NextResponse.next();
}

export const config = {
  matcher: [
    '/((?!api|_next/static|_next/image|_vercel|favicon\\.ico|.*\\..*).*)'
  ]
};
"#;
        let spec = extract_middleware_matcher_exclusions(mw_with_unreachable);
        assert!(spec.has_negative_lookahead);
        assert!(spec.excluded_literals.iter().any(|s| s == "api"));
        assert!(spec.excluded_literals.iter().any(|s| s == "_next/static"));
        let stripped_mw = sanitize_middleware_principled(mw_with_unreachable, "src/middleware.ts");
        assert!(!stripped_mw.contains("pathname.startsWith('/_next/static')"), "Excluded static branch must be removed:\n{}", stripped_mw);
        assert!(!stripped_mw.contains("pathname.startsWith('/api')"), "Excluded unreachable /api branch must be removed:\n{}", stripped_mw);
        assert!(stripped_mw.contains("pathname.startsWith('/dashboard')"), "Reachable auth branch must stay:\n{}", stripped_mw);

        let edge_unsafe_mw = "import prisma from '@/lib/prisma';\nexport function middleware() { return prisma ? null : null; }\n";
        let mw_issues = review_generated_file("src/middleware.ts", edge_unsafe_mw);
        assert!(mw_issues.iter().any(|i| i.severity == ReviewSeverity::Error && i.rule_id == "middleware:edge-unsafe-import"));

        // 4. Delimiter balance check & stagnant auto-heal diagnostic directives
        assert!(check_delimiter_balance("body { color: red; }\n}", true).is_err());
        assert!(check_delimiter_balance("export function f() { return /\\}/; }\n", false).is_ok());
        assert!(check_delimiter_balance("export function f() { return 1; }\n}\n", false).is_err());

        let stag_dirs = diagnose_heal_stagnation(
            "src/config/i18n.ts",
            "export default function P({ children }: any) { return <I18n>{children}</I18n>; }",
            "TS1005 '>' expected.",
            2,
        );
        assert!(stag_dirs.iter().any(|d| d.contains("[format:jsx-in-plain-ts]")));
        assert!(stag_dirs.iter().any(|d| d.contains("[heal:anti-stagnation]")));
    }

    #[test]
    fn test_v54_telemetry_bug_fixes_legacy_dedup_prisma_and_inprogress() {
        use crate::katastar::compute_model_progress_telemetry_at;

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v54_telemetry_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let projects_dir = temp_dir.join("projects");
        for id in ["p46", "p48", "p49", "p52", "p53"] {
            std::fs::create_dir_all(projects_dir.join(format!("{}/src/app", id))).unwrap();
            for i in 0..10 {
                std::fs::write(
                    projects_dir.join(format!("{}/src/app/f{}.tsx", id, i)),
                    "export default function P(){return null;}",
                )
                .unwrap();
            }
        }
        std::fs::create_dir_all(projects_dir.join("p53/.axiom")).unwrap();
        std::fs::write(
            projects_dir.join("p53/.axiom/generation_state.json"),
            r#"{"appVersion":"0.1.53","phase":"generating","status":"generating","warnings":[]}"#,
        )
        .unwrap();

        let projects_json = serde_json::json!([
            // Bug 2: Legacy pre-v0.1.48 without warnings/ledger -> must be unmeasured, NOT 100%
            {
                "id": "p46", "name": "ShoeCraft Legacy", "status": "ready", "createdAt": "2026-11-10T10:36:38Z"
            },
            // Bug 1: Duplicate TS warning lines (68 + 68) -> must count 68 + 1 = 69, NOT 137
            {
                "id": "p48", "name": "BalkanFlow", "status": "ready", "createdAt": "2026-11-10T18:45:34Z",
                "metadata": {
                    "appVersion": "0.1.48",
                    "modelId": "qwen2.5-coder:7b",
                    "warnings": [
                        "TypeScript greške (68): src/app/f0.tsx:9 TS2353",
                        "next build greška: Failed to compile.",
                        "TypeScript provera prijavila 68 grešaka"
                    ]
                }
            },
            // Bug 3: Failed Prisma schema ("Business Analysis Failed") -> must be heavily penalized (<35%)
            {
                "id": "p49", "name": "Business Analysis Failed", "status": "ready", "createdAt": "2026-11-11T12:57:37Z",
                "metadata": {
                    "appVersion": "0.1.49",
                    "modelId": "qwen2.5-coder:7b",
                    "warnings": [
                        "Prisma validation/generation failed (Failed to validate schema)",
                        "TypeScript greške (8): src/app/f0.tsx:60 TS1005",
                        "next build greška: Failed to compile.",
                        "prisma validate nije prošao"
                    ]
                }
            },
            // Good v0.1.52 build -> Δ Best must be positive (>= 0), NOT compared to phantom v46 100%
            {
                "id": "p52", "name": "LoyaltyHub", "status": "ready", "createdAt": "2026-11-12T14:06:53Z",
                "metadata": {
                    "appVersion": "0.1.52",
                    "modelId": "qwen2.5-coder:7b",
                    "warnings": [
                        "TypeScript greške (4): src/app/f0.tsx:41 TS1005",
                        "TypeScript provera prijavila 4 grešaka"
                    ]
                }
            },
            // Bug 4: In-progress generating project -> excluded from versionsTimeline & clean builds
            {
                "id": "p53", "name": "PetSit Generating", "status": "generating", "createdAt": "2026-11-12T15:10:19Z",
                "metadata": {
                    "appVersion": "0.1.53",
                    "modelId": "qwen2.5-coder:7b"
                }
            }
        ]);
        let projects_file = temp_dir.join("projects.json");
        std::fs::write(&projects_file, serde_json::to_string_pretty(&projects_json).unwrap()).unwrap();
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
        )
        .unwrap();

        let projects = telemetry["projects"].as_array().unwrap();
        let p46 = projects.iter().find(|p| p["id"] == "p46").unwrap();
        let p48 = projects.iter().find(|p| p["id"] == "p48").unwrap();
        let p49 = projects.iter().find(|p| p["id"] == "p49").unwrap();
        let p52 = projects.iter().find(|p| p["id"] == "p52").unwrap();
        let p53 = projects.iter().find(|p| p["id"] == "p53").unwrap();

        assert_eq!(p46["telemetryStatus"], "unmeasured");
        assert_eq!(p48["residualErrors"], 69, "TS errors must not be double counted");
        assert_eq!(p52["residualErrors"], 4, "TS errors in p52 must not be double counted");
        assert_eq!(p49["hasPrismaFailure"], true);
        assert!(p49["accuracyPct"].as_f64().unwrap() < 35.0, "Prisma failure must heavily penalize accuracy, got {}", p49["accuracyPct"]);
        assert_eq!(p53["telemetryStatus"], "in_progress");

        let timeline = telemetry["versionsTimeline"].as_array().unwrap();
        // v0.1.53 (in_progress only) must not appear in versionsTimeline; summary must point to v0.1.52
        assert!(timeline.iter().all(|v| v["version"] != "v0.1.53"));
        assert_eq!(telemetry["summary"]["version"], "v0.1.52");
        let row_52 = timeline.iter().find(|v| v["version"] == "v0.1.52").unwrap();
        assert!(row_52["deltaVsBestPct"].as_f64().unwrap() > 0.0, "v0.1.52 must beat v0.1.48/v0.1.49 without phantom 100% baseline");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_v54_auth_routes_injection_and_ux_glitches() {
        use crate::orchestrator::ensure_nextjs_required_auth_pages;
        use crate::sanitizer::sanitize_tsx_ux_glitches;

        let mut manifest_files = vec![
            serde_json::json!({ "path": "src/middleware.ts", "description": "Auth guard" }),
            serde_json::json!({ "path": "src/app/dashboard/bookings/page.tsx", "description": "Protected bookings" }),
        ];
        assert!(ensure_nextjs_required_auth_pages(&mut manifest_files, "PetSit Connect"));
        let paths: Vec<String> = manifest_files
            .iter()
            .filter_map(|v| v.get("path").and_then(|p| p.as_str()).map(|s| s.to_string()))
            .collect();
        assert!(paths.contains(&"src/app/login/page.tsx".to_string()), "Missing /login page: {:?}", paths);
        assert!(paths.contains(&"src/app/register/page.tsx".to_string()), "Missing /register page: {:?}", paths);
        // Idempotent second call
        assert!(!ensure_nextjs_required_auth_pages(&mut manifest_files, "PetSit Connect"));

        let glitch_page = r#"
const services = [{ title: 'Sitting', metrics: { price: '$50' } }];
export default function HomePage() {
  return (
    <div>
      <span>${services[0].metrics.price}</span>
      <button onClick={() => router.push('/dashboard/login')}>Log In</button>
      <a href="/dashboard/register">Sign Up</a>
    </div>
  );
}
"#;
        let fixed = sanitize_tsx_ux_glitches(glitch_page, "src/app/page.tsx");
        assert!(fixed.contains("router.push('/login')"), "Broken /dashboard/login must be rewritten: {}", fixed);
        assert!(fixed.contains("\"/register\""), "Broken /dashboard/register must be rewritten: {}", fixed);
        assert!(fixed.contains("startsWith('$')"), "Double currency $$50 guard missing: {}", fixed);
    }

    #[test]
    fn test_v54_try_catch_body_scope_hoist_and_reviewer() {
        use crate::reviewer::check_try_catch_body_scope;
        use crate::sanitizer::sanitize_try_catch_body_scope;

        let broken_route = r#"import { NextRequest, NextResponse } from 'next/server';
export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    return NextResponse.json(body);
  } catch (error) {
    return NextResponse.json({ fallback: true, attempted: body }, { status: 500 });
  }
}
"#;
        let issues = check_try_catch_body_scope(broken_route, "src/app/api/bookings/route.ts");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].rule_id, "api.try-catch-body-scope");

        let fixed = sanitize_try_catch_body_scope(broken_route);
        assert!(fixed.contains("let body: any = {};\n  try {"), "Expected hoisted `let body: any = {{}}`: {}", fixed);
        assert!(fixed.contains("body = await request.json();"), "Expected assignment inside try: {}", fixed);
        assert!(check_try_catch_body_scope(&fixed, "src/app/api/bookings/route.ts").is_empty(), "Fixed code must pass reviewer: {}", fixed);
    }

    #[test]
    fn test_v54_stripe_legacy_upgrade_and_apiversion_cleanup() {
        use crate::packages::sanitize_package_json_value_for_offline;
        use crate::sanitizer::sanitize_stripe_client_init;

        let mut pkg_legacy = serde_json::json!({
            "dependencies": {
                "next": "14.2.35",
                "stripe": "^8.135.0"
            }
        });
        assert!(sanitize_package_json_value_for_offline(&mut pkg_legacy));
        assert_eq!(pkg_legacy["dependencies"]["stripe"], "^17.5.0");

        let mut pkg_modern = serde_json::json!({
            "dependencies": {
                "next": "14.2.35",
                "stripe": "^23.1.0"
            }
        });
        assert!(!sanitize_package_json_value_for_offline(&mut pkg_modern));
        assert_eq!(pkg_modern["dependencies"]["stripe"], "^23.1.0");

        let stripe_src = "import Stripe from 'stripe';\nconst stripe = new Stripe(process.env.STRIPE_SECRET_KEY || '', { apiVersion: '2022-11-15' });\n";
        let fixed_stripe = sanitize_stripe_client_init(stripe_src);
        assert_eq!(
            fixed_stripe,
            "import Stripe from 'stripe';\nconst stripe = new Stripe(process.env.STRIPE_SECRET_KEY || '');\n"
        );
    }

    #[test]
    fn test_v54_ui_toast_stubs_and_client_prisma_enum_mocks() {
        use crate::sanitizer::{ensure_ui_primitives_reconciled, reconcile_client_prisma_enum_mocks};

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v54_ui_enum_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/dashboard/reviews")).unwrap();

        std::fs::write(
            temp_dir.join("prisma/schema.prisma"),
            "enum BookingStatus { PENDING CONFIRMED COMPLETED }\nenum UserRole { PET_OWNER SITTER }\n",
        )
        .unwrap();

        let page_src = r#""use client";
import { toast } from '@/components/ui/use-toast';
import { TableHeaderCell } from '@/components/ui';

const demo = { status: 'confirmed', role: 'petOwner' };
export default function ReviewsPage() {
  toast({ title: 'Loaded' });
  return <TableHeaderCell>{demo.status}</TableHeaderCell>;
}
"#;
        std::fs::write(temp_dir.join("src/app/dashboard/reviews/page.tsx"), page_src).unwrap();

        ensure_ui_primitives_reconciled(&temp_dir);
        let toast_file = std::fs::read_to_string(temp_dir.join("src/components/ui/use-toast.tsx")).unwrap();
        assert!(toast_file.contains("export const toast"), "toast stub must be synthesized: {}", toast_file);
        let ui_file = std::fs::read_to_string(temp_dir.join("src/components/ui.tsx")).unwrap();
        assert!(ui_file.contains("export const TableHeaderCell"), "TableHeaderCell stub missing: {}", ui_file);

        let reconciled = reconcile_client_prisma_enum_mocks(&temp_dir, "src/app/dashboard/reviews/page.tsx", page_src);
        assert!(reconciled.contains("status: 'CONFIRMED' as any"), "Expected uppercase schema enum match: {}", reconciled);
        assert!(reconciled.contains("role: 'PET_OWNER' as any"), "Expected snake-case schema enum match: {}", reconciled);
        assert!(!reconciled.contains("from '@prisma/client'"), "Must NEVER inject @prisma/client into client files: {}", reconciled);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_v54_policy_registry_snapshot_and_prompt_directives() {
        use crate::policy_registry::{build_policy_registry_snapshot_at, prompt_directives_for_file};

        let api_dir = prompt_directives_for_file("src/app/api/payments/route.ts", true, true);
        assert!(api_dir.contains("[api.try-catch-body-scope]"), "Missing try-catch rule: {}", api_dir);
        assert!(api_dir.contains("[payments.stripe-sdk-compatibility]"), "Missing stripe rule: {}", api_dir);

        let mw_dir = prompt_directives_for_file("src/middleware.ts", true, false);
        assert!(mw_dir.contains("[middleware.matcher-exclusions]"), "Missing matcher rule: {}", mw_dir);

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v54_policy_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let katastar_path = temp_dir.join("katastar.json");
        std::fs::write(
            &katastar_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "version": 1,
                "entries": [
                    {
                        "error_signature": "TS2304: Cannot find name 'body'.",
                        "category": "TypeScript",
                        "recommended_fix_hint": "Hoist body above try",
                        "occurrences": 3,
                        "verified": true,
                        "success_count": 2,
                        "fail_count": 0
                    }
                ]
            }))
            .unwrap(),
        )
        .unwrap();

        let snap = build_policy_registry_snapshot_at(&katastar_path);
        assert!(snap["totalRules"].as_u64().unwrap() >= 12);
        assert_eq!(snap["totalAgents"].as_u64().unwrap(), 6);
        let rules = snap["rules"].as_array().unwrap();
        let try_rule = rules.iter().find(|r| r["id"] == "api.try-catch-body-scope").unwrap();
        assert_eq!(try_rule["katastarMatches"], 1);
        assert_eq!(try_rule["verifiedFixes"], 2);
        assert_eq!(try_rule["successRatePct"], 100.0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }



