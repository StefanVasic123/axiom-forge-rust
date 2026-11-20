#[cfg(test)]
mod tests {
    use crate::ast_engine::{
        extract_exports, extract_hook_call_sites, extract_hook_signatures, extract_imports,
        extract_type_shapes, parse_source, reconcile_hook_contracts_in_project,
        reconcile_model_and_type_contracts_in_project, reconcile_model_and_type_contracts_in_source,
        syntax_issues_for, HookParamKind,
    };
    use crate::sanitizer::{
        ensure_common_formatting_helpers, ensure_multiplatform_entry_routes_bridged,
        hoist_enums_before_usage, sanitize_const_body_reassignment, sanitize_openai_v3_to_v4,
        sanitize_truncated_import_repetition_loop, sanitize_zod_number_float,
        strip_self_conflicting_local_imports,
    };

    #[test]
    fn phase3_ast_extracts_imports_exports_hooks_and_types() {
        let src = r#""use client";
import React, { useState, type FC } from 'react';
import * as Icons from 'react-icons/hi';
export interface Sensor { id: string; airQuality?: number; nested: { a: string } }
export type Reward = { id: string; amount: number };
const useUserRewards = ({ userId, fallbackData }: { userId: string; fallbackData?: Reward[] }) => {
  const [rewards] = useState<Reward[]>(fallbackData ?? []);
  if (!userId) { return { data: [], isLoading: false, }; }
  return { data: rewards, isLoading: false };
};
export function useThing(id: string, opts = {}) { return ({ value: id }); }
export default useUserRewards;
export { useUserRewards, Sensor as S };
export * from './x';
function Page() {
  const { data, isLoading, error } = useUserRewards();
  const { value } = useThing('a', { b: 1 });
  return <div className="p-4">{data.length}</div>;
}
"#;
        let parsed = parse_source("src/app/community/page.tsx", src).expect("parse TSX");
        assert!(syntax_issues_for("src/app/community/page.tsx", src)
            .unwrap()
            .is_empty());

        let imports = extract_imports(&parsed, src);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].source, "react");
        assert_eq!(imports[0].default_local.as_deref(), Some("React"));

        let exports = extract_exports(&parsed, src);
        assert!(exports.named.contains(&"Sensor".to_string()));
        assert!(exports.named.contains(&"Reward".to_string()));
        assert!(exports.named.contains(&"useUserRewards".to_string()));
        assert!(exports.named.contains(&"useThing".to_string()));
        assert_eq!(exports.default_local.as_deref(), Some("useUserRewards"));
        assert_eq!(exports.star_reexports, vec!["./x".to_string()]);

        let sigs = extract_hook_signatures(&parsed, src);
        assert_eq!(sigs.len(), 2);
        let rewards_sig = sigs.iter().find(|s| s.name == "useUserRewards").unwrap();
        assert_eq!(rewards_sig.params.len(), 1);
        assert_eq!(rewards_sig.params[0].kind, HookParamKind::ObjectPattern);
        assert!(!rewards_sig.params[0].has_default);

        let calls = extract_hook_call_sites(&parsed, src);
        let rewards_call = calls.iter().find(|c| c.hook_name == "useUserRewards").unwrap();
        assert_eq!(rewards_call.arg_count, 0);
        assert!(rewards_call.destructured_keys.contains(&"error".to_string()));

        let types = extract_type_shapes(&parsed, src);
        assert!(types.iter().any(|t| t.name == "Sensor" && t.exported));
    }

    #[test]
    fn phase3_ast_reconciles_hook_destructured_defaults_and_return_properties() {
        let temp_dir =
            std::env::temp_dir().join(format!("axiom_ast_hook_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/community")).unwrap();

        let hook_src = r#"import { useState } from 'react';
import { Reward } from '@/models';

const useUserRewards = ({ userId, fallbackData }: { userId: string; fallbackData?: Reward[] }) => {
  const [data] = useState<Reward[]>(fallbackData ?? []);
  return { data, isLoading: false };
};

export default useUserRewards;
"#;
        let caller_src = r#""use client";
import React from 'react';
import useUserRewards from '@/hooks/useUserRewards';

export default function CommunityPage() {
  const { data, isLoading, error, refetch } = useUserRewards();
  return <div>{isLoading ? 'Loading' : data.length}{error}{String(typeof refetch)}</div>;
}
"#;
        let hook_path = temp_dir.join("src/hooks/useUserRewards.ts");
        std::fs::write(&hook_path, hook_src).unwrap();
        std::fs::write(temp_dir.join("src/app/community/page.tsx"), caller_src).unwrap();

        let report = reconcile_hook_contracts_in_project(&temp_dir);
        assert!(!report.edited_files.is_empty());

        let reconciled = std::fs::read_to_string(&hook_path).unwrap();
        assert!(reconciled.contains("= {} as any"));
        assert!(reconciled.contains("error:"));
        assert!(reconciled.contains("refetch:"));
        assert!(syntax_issues_for("src/hooks/useUserRewards.ts", &reconciled)
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn v55_sanitizers_fix_desktop_mobile_and_web_patterns() {
        // 1. Runaway import repetition loop (HeatCycle App.tsx)
        let looped = concat!(
            "import React from 'react';\n",
            "import { a } from './a';\n",
            "import { b } from './b';\n",
            "import { c } from './c';\n",
            "import { a } from './a';\n",
            "import { b } from './b';\n",
            "import { c } from './c';\n",
            "import { a } from './a';\n",
            "import { b } from './b';\n",
            "import { c } from './c';\n",
            "import { a } from './a';\n",
            "import { b } from './b';\n",
            "import { use"
        );
        let cleaned_loop = sanitize_truncated_import_repetition_loop(looped);
        assert!(!cleaned_loop.contains("import { use"));
        assert!(cleaned_loop.contains("export default function AxiomRecoveredComponent"));

        // 2. const body reassignment (EnviroLab Nexus)
        let route_src = "export async function POST(req: Request) {\n  const body = await req.json();\n  body = { ...body, ok: true };\n}\n";
        let fixed_body = sanitize_const_body_reassignment(route_src);
        assert!(fixed_body.contains("let body = await req.json();"));

        // 3. z.number().float() and Stripe.Connect (KioskFlow & EnviroLab)
        let zod_src =
            "const s = z.object({ temp: z.number().float().min(0) });\ntype C = Stripe.Connect;\n";
        let fixed_zod = sanitize_zod_number_float(zod_src);
        assert!(!fixed_zod.contains(".float()"));
        assert!(fixed_zod.contains("Stripe.Account"));

        // 4. Enum hoisted before usage (ZoningCheck TS2450)
        let enum_src = "import React from 'react';\nconst x = ZoneType.RESIDENTIAL;\nexport enum ZoneType {\n  RESIDENTIAL = 'RESIDENTIAL',\n}\n";
        let hoisted = hoist_enums_before_usage(enum_src);
        let enum_pos = hoisted.find("export enum ZoneType").unwrap();
        let usage_pos = hoisted.find("const x = ZoneType.RESIDENTIAL").unwrap();
        assert!(enum_pos < usage_pos);

        // 5. Self-conflicting import stripped (ZoningCheck TS2440)
        let conflict_src =
            "import { ZoneRule, OtherType } from './types';\nexport interface ZoneRule { id: string; }\n";
        let stripped = strip_self_conflicting_local_imports(conflict_src);
        assert!(!stripped.contains("ZoneRule,"));
        assert!(stripped.contains("import { OtherType } from './types';"));

        // 6. OpenAI v3 -> v4 (EcoSensor Commons)
        let oai_src = "import { Configuration, OpenAIApi } from 'openai';\nconst configuration = new Configuration({ apiKey: process.env.OPENAI_API_KEY });\nconst openai = new OpenAIApi(configuration);\n";
        let fixed_oai = sanitize_openai_v3_to_v4(oai_src);
        assert!(fixed_oai.contains("import OpenAI from 'openai';"));
        assert!(fixed_oai.contains("const openai = new OpenAI("));

        // 7. formatDate + formatCurrency (TaxSync Pro & EcoSensor Commons)
        let fmt_src = "\"use client\";\nexport default function P() { return <div>{formatCurrency(10)} {formatDate(new Date())}</div>; }\n";
        let fixed_fmt = ensure_common_formatting_helpers(fmt_src, "src/app/page.tsx");
        assert!(fixed_fmt.contains("const formatCurrency ="));
        assert!(fixed_fmt.contains("const formatDate ="));

        // 8. Desktop Electron 404 bridge (`src/renderer/App.tsx` -> `src/app/page.tsx`)
        let temp_dir =
            std::env::temp_dir().join(format!("axiom_bridge_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src/renderer")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/api")).unwrap();
        std::fs::write(
            temp_dir.join("src/renderer/App.tsx"),
            "export default function App() { return <div>Desktop Ready</div>; }\n",
        )
        .unwrap();
        ensure_multiplatform_entry_routes_bridged(&temp_dir);
        assert!(temp_dir.join("src/app/page.tsx").is_file());
        assert!(temp_dir.join("src/app/layout.tsx").is_file());
        let bridged_page = std::fs::read_to_string(temp_dir.join("src/app/page.tsx")).unwrap();
        assert!(bridged_page.contains("@/renderer/App"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn reconciles_explicit_return_type_interface_and_return_object_without_overriding() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v56_hook_iface_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/register")).unwrap();

        let hook_src = r#"import { useState } from 'react';
export interface User { id: string; email: string; }
export interface AuthState {
  data: User | null;
  isLoading: boolean;
  error: string | null;
}

const useAuth = (): AuthState => {
  const [data] = useState<User | null>(null);
  const handleLogin = async () => {};
  const handleLogout = async () => {};
  return {
    data,
    isLoading: false,
    error: null,
    login: handleLogin,
    logout: handleLogout,
  };
};

export default useAuth;
"#;
        let caller_src = r#""use client";
import React from 'react';
import useAuth from '@/hooks/useAuth';

export default function RegisterPage() {
  const { login, register } = useAuth();
  return <button onClick={() => { login(); register(); }}>Go</button>;
}
"#;
        let hook_path = temp_dir.join("src/hooks/useAuth.ts");
        std::fs::write(&hook_path, hook_src).unwrap();
        std::fs::write(temp_dir.join("src/app/register/page.tsx"), caller_src).unwrap();

        let report = reconcile_hook_contracts_in_project(&temp_dir);
        assert!(!report.edited_files.is_empty());

        let reconciled = std::fs::read_to_string(&hook_path).unwrap();
        // Set-difference additive only: existing strict declarations must remain untouched
        assert!(reconciled.contains("data: User | null;"), "{}", reconciled);
        assert!(reconciled.contains("isLoading: boolean;"), "{}", reconciled);
        assert!(reconciled.contains("error: string | null;"), "{}", reconciled);
        // Returned + demanded missing keys added as optional `?: any;`
        assert!(reconciled.contains("login?: any;"), "{}", reconciled);
        assert!(reconciled.contains("logout?: any;"), "{}", reconciled);
        assert!(reconciled.contains("register?: any;"), "{}", reconciled);
        // Return object also synchronized with caller-demanded `register`
        assert!(reconciled.contains("register:"), "{}", reconciled);
        assert!(syntax_issues_for("src/hooks/useAuth.ts", &reconciled)
            .unwrap()
            .is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn reconciles_model_interfaces_with_index_signature_and_enum_widening() {
        let model_src = r#"export enum Status {
  PENDING,
  COMPLETED,
  CANCELLED,
}

export interface Kitchen {
  id: string;
  name: string;
}

export interface Subscription {
  id: string;
  status: Status;
}

export interface ETA_Prediction {
  id: string;
  kitchen: Kitchen;
  status: Status;
}
"#;
        let updated = reconcile_model_and_type_contracts_in_source(model_src, true)
            .expect("Expected model contract reconciliation");
        assert!(updated.contains("[key: string]: any;"), "{}", updated);
        assert!(updated.contains("status: Status | string;"), "{}", updated);
        assert!(syntax_issues_for("src/models/index.ts", &updated)
            .unwrap()
            .is_empty());
        // Idempotent on second pass when no UI JSX/string relation mismatch exists
        assert!(reconcile_model_and_type_contracts_in_source(&updated, true).is_none());

        // Project-level pass also widens relation object `kitchen: Kitchen` when UI renders `{item.kitchen}`
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v56_model_rel_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src/models")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();
        std::fs::write(temp_dir.join("src/models/index.ts"), model_src).unwrap();
        std::fs::write(
            temp_dir.join("src/components/EtaList.tsx"),
            "export default function EtaList({ items }: any) { return <div>{items.map((item: any) => <span key={item.id}>{item.kitchen}</span>)}</div>; }\n",
        )
        .unwrap();

        let count = reconcile_model_and_type_contracts_in_project(&temp_dir);
        assert_eq!(count, 1);
        let model_out = std::fs::read_to_string(temp_dir.join("src/models/index.ts")).unwrap();
        assert!(model_out.contains("kitchen?: any;"), "{}", model_out);
        assert!(model_out.contains("status?: Status | string;"), "{}", model_out);
        assert!(model_out.contains("[key: string]: any;"), "{}", model_out);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn phase5_v57_general_rules_zero_regression_suite() {
        // 1. Enum -> const + [key: string]: any + type (and numeric enum preserved untouched)
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v57_suite_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src/types")).unwrap();
        std::fs::create_dir_all(temp_dir.join("app/(tabs)")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();
        std::fs::write(
            temp_dir.join("tsconfig.json"),
            r#"{"compilerOptions":{"strict":false}}"#,
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/types/index.ts"),
            "export enum Role { ADMIN, USER }\nexport enum Flags { READ = 1, WRITE = 2 }\nexport interface Robot {\n  id: string;\n  name: string;\n  createdAt: Date;\n  status: Role;\n}\n",
        ).unwrap();

        let edited = reconcile_model_and_type_contracts_in_project(&temp_dir);
        assert_eq!(edited, 1);
        let types_out = std::fs::read_to_string(temp_dir.join("src/types/index.ts")).unwrap();
        assert!(types_out.contains("export const Role:"), "Pure enum should become Prisma-compatible const object:\n{}", types_out);
        assert!(types_out.contains("ADMIN: 'ADMIN'"), "{}", types_out);
        assert!(types_out.contains("export type Role = any;"), "{}", types_out);
        assert!(types_out.contains("export enum Flags { READ = 1, WRITE = 2 }"), "Numeric enum must be preserved:\n{}", types_out);
        assert!(types_out.contains("name?: string;"), "Non-id fields optional under strict:false:\n{}", types_out);
        assert!(types_out.contains("createdAt?: any;"), "Date widened to any:\n{}", types_out);
        // Idempotent second pass
        assert_eq!(reconcile_model_and_type_contracts_in_project(&temp_dir), 0);

        // 2. Dual-root UI primitive reconciliation (root app/(tabs)/ reconciles into src/components/ui.tsx)
        std::fs::write(
            temp_dir.join("app/(tabs)/alerts.tsx"),
            "import { DataTable, EmptyState, LoadingSpinner, Modal } from '@/components/ui';\nexport default function Alerts() { return <Modal isOpen={true}><LoadingSpinner /><EmptyState /><DataTable /></Modal>; }\n",
        ).unwrap();
        crate::sanitizer::ensure_ui_primitives_reconciled(&temp_dir);
        let ui_out = std::fs::read_to_string(temp_dir.join("src/components/ui.tsx")).unwrap();
        for sym in ["DataTable", "EmptyState", "LoadingSpinner", "Modal"] {
            assert!(ui_out.contains(&format!("export const {}", sym)), "Dual-root import {} missing in ui.tsx:\n{}", sym, ui_out);
        }

        // 3. Electron Window ambient declarations
        std::fs::write(
            temp_dir.join("src/components/Bridge.tsx"),
            "export function Bridge() { window.electron.init(); return null; }\n",
        ).unwrap();
        crate::sanitizer::ensure_window_ambient_declarations(&temp_dir);
        let win_dts = std::fs::read_to_string(temp_dir.join("axiom-window-env.d.ts")).unwrap();
        assert!(win_dts.contains("electron?: any;"), "{}", win_dts);
        let _ = std::fs::remove_dir_all(&temp_dir);

        // 4. Tailwind circular @apply sanitizer
        let css_in = "@layer utilities {\n  .transition-all {\n    @apply transition-all duration-300 ease-in-out;\n  }\n  .bg-white {\n    @apply bg-white;\n  }\n  .btn-primary {\n    @apply bg-blue-600 text-white;\n  }\n}\n";
        let css_out = crate::sanitizer::sanitize_tailwind_css(css_in);
        assert!(css_out.contains("@apply duration-300 ease-in-out;"), "Circular token must be removed:\n{}", css_out);
        assert!(!css_out.contains(".bg-white"), "Empty self-referencing rule must be removed:\n{}", css_out);
        assert!(css_out.contains(".btn-primary {\n    @apply bg-blue-600 text-white;\n  }"), "Legitimate custom rule preserved:\n{}", css_out);

        // 5. General import hygiene + react-icons with trailing comment + Electron guard
        let ts_in = "\"use client\";\nimport { fetch, NextResponse } from 'next/server';\nimport { z, zodResolver } from '@hookform/resolvers/zod';\nimport { CredentialsProvider, signIn } from 'next-auth/react';\nimport { HiOutlineSun } from 'react-icons'; // Example icon\nimport { Client } from '@supabase/supabase-js';\nconst url = import.meta.env.VITE_API;\nconst s = new Client(url, 'k');\nexport function cn(...args: ClassValue[]) { return twMerge(...args); }\nwindow.electron.init();\n";
        let ts_hyg = crate::sanitizer::sanitize_react_icons_imports(
            &crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(ts_in, "src/lib/test.tsx"),
        );
        assert!(!ts_hyg.contains("import { fetch"), "fetch stripped from next/server:\n{}", ts_hyg);
        assert!(ts_hyg.contains("import { NextResponse } from 'next/server'"), "{}", ts_hyg);
        assert!(ts_hyg.contains("import { z } from 'zod';") && ts_hyg.contains("import { zodResolver } from '@hookform/resolvers/zod'"), "{}", ts_hyg);
        assert!(!ts_hyg.contains("CredentialsProvider"), "{}", ts_hyg);
        assert!(ts_hyg.contains("import { HiOutlineSun } from 'react-icons/hi'"), "react-icons split even with trailing comment:\n{}", ts_hyg);
        assert!(ts_hyg.contains("import { createClient } from '@supabase/supabase-js'") && ts_hyg.contains("createClient(url, 'k')"), "{}", ts_hyg);
        assert!(ts_hyg.contains("process.env.NEXT_PUBLIC_API"), "{}", ts_hyg);
        assert!(ts_hyg.contains("from 'clsx'") && ts_hyg.contains("from 'tailwind-merge'"), "{}", ts_hyg);
        assert!(ts_hyg.contains("(window as any).electron?.init?.()"), "{}", ts_hyg);

        // 6. Hook FC strip + setter-only toggle state + handleCancel
        let hook_in = "export const useAlerts: React.FC<Props> = ({ a }) => {\n  const [err, setErr] = useState<Error | null>(null);\n  return { err };\n};\n";
        let hook_out = crate::sanitizer::sanitize_custom_hook_functions(hook_in);
        assert!(hook_out.contains("export const useAlerts = ({ a }) =>"), "{}", hook_out);
        assert!(hook_out.contains("useState<any>(null)"), "{}", hook_out);

        let page_in = "const RobotsPage = () => {\n  const [isEditing, setIsEditing] = useState(false);\n  return <div><button onClick={() => setIsFormOpen(true)}>Add</button><button onClick={handleCancel}>Cancel</button></div>;\n};\n";
        let page_out = crate::sanitizer::ensure_undeclared_toggle_states(page_in);
        assert!(page_out.contains("const [isFormOpen, setIsFormOpen] = useState(false);"), "{}", page_out);
        assert!(page_out.contains("const handleCancel = () => { setIsEditing(false); };"), "{}", page_out);
    }

    #[test]
    fn phase6_v58_ui_lock_degeneration_types_and_ast_js_promotion_suite() {
        // 1. Intra-line CamelCase/phrase repetition & EOF truncation detection
        let loop_ident = "const queryPanelsByComplianceCheckComplianceCheckComplianceCheckComplianceCheckComplianceCheckSchema = z.object({});\n";
        assert!(
            crate::self_healing::detect_infinite_loop(loop_ident),
            "Must detect repeating CamelCase identifier loop"
        );
        let loop_comment = "// query panels by compliance check compliance check compliance check compliance check\nconst x = 1;\n";
        assert!(
            crate::self_healing::detect_infinite_loop(loop_comment),
            "Must detect repeating phrase loop inside line"
        );
        let truncated_eof = "import { z } from 'zod';\nconst queryPanelsByStatusSchema";
        assert!(
            crate::ast_engine::detect_truncated_or_degenerate_ts_js("src/app/api/route.ts", truncated_eof).is_some(),
            "Must detect truncated variable declaration at EOF"
        );
        let normal_jsx = "export const Row = () => <tr><td>a</td><td>b</td><td>c</td><td>d</td></tr>;\n";
        assert!(
            !crate::self_healing::detect_infinite_loop(normal_jsx),
            "Must NOT false-positive on repeated table cells"
        );

        // 2. Banned external UI kit filtering & import rewriting
        assert!(crate::sanitizer::is_banned_external_ui_package("@chakra-ui/react"));
        assert!(crate::sanitizer::is_banned_external_ui_package("@mui/material"));
        assert!(crate::sanitizer::is_banned_external_ui_package("@radix-ui/react-input"));
        assert!(crate::sanitizer::is_banned_external_ui_package("antd"));
        assert!(!crate::sanitizer::is_banned_external_ui_package("formik"));
        assert!(!crate::sanitizer::is_banned_external_ui_package("lucide-react"));

        let page_chakra = "\"use client\";\nimport { Avatar } from '@chakra-ui/react';\nexport default function P() { return <Avatar name=\"A\" />; }\n";
        let page_rewritten = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(page_chakra, "app/(tabs)/ProcessAuditSharing.tsx");
        assert!(page_rewritten.contains("import { Avatar } from '@/components/ui';"), "{}", page_rewritten);

        let comp_chakra = "\"use client\";\nimport { Avatar as ChakraAvatar, AvatarBadge } from '@chakra-ui/react';\nconst Avatar = () => <ChakraAvatar><AvatarBadge /></ChakraAvatar>;\n";
        let comp_rewritten = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(comp_chakra, "src/components/Avatar.tsx");
        assert!(!comp_rewritten.contains("@chakra-ui/react"), "{}", comp_rewritten);
        assert!(comp_rewritten.contains("const ChakraAvatar: any ="), "{}", comp_rewritten);
        assert!(comp_rewritten.contains("const AvatarBadge: any ="), "{}", comp_rewritten);

        // 3. Pure React component props (no DOM HTMLAttributes collision) & required props optionalized
        let btn_in = "\"use client\";\nimport React, { ButtonHTMLAttributes } from 'react';\ninterface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {\n  type?: 'primary' | 'secondary';\n  label: string;\n}\nexport const Button: React.FC<ButtonProps> = ({ label, children }) => <button>{label}{children}</button>;\n";
        let btn_out = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(btn_in, "src/components/Button.tsx");
        assert!(!btn_out.contains("extends ButtonHTMLAttributes"), "{}", btn_out);
        assert!(btn_out.contains("label?: string;"), "{}", btn_out);
        assert!(btn_out.contains("children?: any;"), "{}", btn_out);
        assert!(btn_out.contains("[key: string]: any;"), "{}", btn_out);

        // 4. Tuple hook widening & self-recursive useToast cleanup
        let toast_in = "\"use client\";\nimport { useState, useEffect } from 'react';\nconst useToast = (): [any, (o: any) => void] => {\n  const [s, setS] = useState(null);\n  const { toast } = useToast();\n  return [s, setS];\n};\n";
        let toast_out = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(toast_in, "src/hooks/useToast.ts");
        assert!(!toast_out.contains("const { toast } = useToast()"), "{}", toast_out);
        assert!(toast_out.contains("): any =>"), "{}", toast_out);
        assert!(toast_out.contains("Object.assign([s, setS]"), "{}", toast_out);

        // 5. Zod 2-arg .refine & z.nativeEnum undeclared fallback & instanceof on interfaces & duplicate object literal keys
        let auth_in = "\"use client\";\nimport { ComplianceReport } from '@/types';\nconst s = z.object({ a: z.string().refine((val, { parent }) => val === parent.b, 'match'), status: z.nativeEnum(MissingStatus).default(MissingStatus.ACTIVE) });\nconst arr = [{ id: '5', id: '5', ok: true }];\nconst chk = arr[0] instanceof ComplianceReport;\nconst incompleteTrailingVar";
        let auth_out = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(auth_in, "src/app/page.tsx");
        assert!(auth_out.contains("as any"), "{}", auth_out);
        assert!(auth_out.contains("z.string().default('ACTIVE')"), "{}", auth_out);
        assert!(!auth_out.contains("incompleteTrailingVar"), "{}", auth_out);
        assert!(auth_out.contains("Boolean(arr[0])"), "{}", auth_out);
        assert_eq!(auth_out.matches("id: '5'").count(), 1, "Duplicate object key must be removed:\n{}", auth_out);

        // 6. AST `.js` -> `.ts` promotion, `@/types` sync, `lib/api.ts` deduplication & `id?: any` widening
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v58_suite_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src/shared/models")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/types")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/lib")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/settings")).unwrap();
        std::fs::write(temp_dir.join("tsconfig.json"), r#"{"compilerOptions":{"strict":false}}"#).unwrap();

        std::fs::write(
            temp_dir.join("src/shared/models/PredictiveAlert.js"),
            "export interface PredictiveAlert {\n  id: string;\n  panelId: string;\n  metrics: { efficiency: number };\n}\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/types/TBMPerformance.ts"),
            "export interface TBMPerformance {\n  id: string;\n  projectId: string;\n  performanceMetrics: { excavationRate: number };\n}\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/lib/api.ts"),
            "export interface TBMPerformance {\n  id: number;\n  date: string;\n}\nexport async function fetchPerf(): Promise<TBMPerformance[]> { return []; }\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/app/settings/page.tsx"),
            "import { GeologicalCondition } from '@/types/TBMPerformance';\nexport default function Settings() { return null; }\n",
        ).unwrap();

        let promoted = crate::ast_engine::promote_typed_js_files_to_ts_in_project(&temp_dir);
        assert_eq!(promoted, 1);
        assert!(temp_dir.join("src/shared/models/PredictiveAlert.ts").is_file());
        assert!(!temp_dir.join("src/shared/models/PredictiveAlert.js").exists());

        crate::sanitizer::synchronize_global_types_and_dedup_api(&temp_dir);
        crate::ast_engine::reconcile_model_and_type_contracts_in_project(&temp_dir);

        let alert_type = std::fs::read_to_string(temp_dir.join("src/types/PredictiveAlert.ts")).unwrap();
        assert!(alert_type.contains("id?: any;"), "{}", alert_type);
        assert!(alert_type.contains("panelId?: any;"), "{}", alert_type);
        assert!(alert_type.contains("metrics?: any;"), "{}", alert_type);

        let tbm_type = std::fs::read_to_string(temp_dir.join("src/types/TBMPerformance.ts")).unwrap();
        assert!(tbm_type.contains("performanceMetrics?: any;"), "{}", tbm_type);
        assert!(tbm_type.contains("projectId?: any;"), "{}", tbm_type);

        let api_out = std::fs::read_to_string(temp_dir.join("src/lib/api.ts")).unwrap();
        assert!(!api_out.contains("export interface TBMPerformance"), "Duplicate interface must be deduplicated from api.ts:\n{}", api_out);
        assert!(api_out.contains("import { TBMPerformance } from '@/types';"), "{}", api_out);

        let settings_out = std::fs::read_to_string(temp_dir.join("src/app/settings/page.tsx")).unwrap();
        assert!(settings_out.contains("from '@/types'"), "Broken subpath import must be repaired:\n{}", settings_out);

        let types_idx = std::fs::read_to_string(temp_dir.join("src/types/index.ts")).unwrap();
        assert!(types_idx.contains("GeologicalCondition"), "{}", types_idx);
        assert!(types_idx.contains("PredictiveAlert"), "{}", types_idx);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn phase7_v59_routing_unification_complete_services_and_zero_regression_suite() {
        // 1. Preserve react-icons/* subpaths in packages.rs + clean dead unexported icons
        let db = crate::packages::load_packages_db();
        assert_eq!(crate::packages::resolve_and_rewrite_import("react-icons/hi2", &db), None);
        assert_eq!(crate::packages::resolve_and_rewrite_import("react-icons/hi", &db), None);

        // 2. Fix `as any as const` (`TS1355`), `.ts`/`.tsx` import extensions (`TS5097`), mock array `any[]`, and Supabase auth cast
        let ts_in = "\"use client\";\nimport { Alert } from '@/types/index.ts';\nimport Card from '@/components/ui/card.tsx';\nconst INITIAL_MOCK_ITEMS = [{ id: 1, name: 'Node A' }];\nconst r = { role: 'ADMIN' as any as const };\nexport async function doSignUp(c: any) { const { user, error } = await c.auth.signUp({ email: 'a@b.com', password: '123' }); return { user, error }; }\n";
        let ts_out = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(ts_in, "app/(tabs)/alerts.tsx");
        assert!(ts_out.contains("from '@/types';"), "TS5097 extension + /index stripped:\n{}", ts_out);
        assert!(ts_out.contains("from '@/components/ui/card';"), "TS5097 .tsx stripped:\n{}", ts_out);
        assert!(ts_out.contains("const INITIAL_MOCK_ITEMS: any[] = ["), "Mock array typed as any[]:\n{}", ts_out);
        assert!(!ts_out.contains("as any as const"), "TS1355 fixed:\n{}", ts_out);
        assert!(ts_out.contains("((await c.auth.signUp({ email: 'a@b.com', password: '123' })) as any)"), "Supabase auth wrapped:\n{}", ts_out);

        // 3. Inline `React.FC<{ data: AnalyticsData[] }>` prop widening + index signature
        let fc_in = "\"use client\";\nimport React from 'react';\ninterface AnalyticsData { id: number; }\nconst AnalyticsChart: React.FC<{ data: AnalyticsData[] }> = ({ data }) => <div>{data.length}</div>;\nexport default AnalyticsChart;\n";
        let fc_out = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(fc_in, "src/components/AnalyticsChart.tsx");
        assert!(fc_out.contains("data?: AnalyticsData[]"), "{}", fc_out);
        assert!(fc_out.contains("[key: string]: any;"), "{}", fc_out);

        // 4. Temp project test: Mobile `app/` vs `src/app/` unification, Desktop workspace, `AuthState` auto-import, hook rest args, & complete industry pages
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v59_suite_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("app/(tabs)")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(temp_dir.join(".axiom")).unwrap();
        std::fs::create_dir_all(temp_dir.join("node_modules/react-icons/hi2")).unwrap();

        std::fs::write(
            temp_dir.join("package.json"),
            r#"{"name":"tunnel-telemetry-mobile","dependencies":{"next":"14.2.5","react":"18.3.1","react-icons":"^5.7.0","expo":"^51.0.0"}}"#,
        ).unwrap();
        std::fs::write(
            temp_dir.join("node_modules/react-icons/package.json"),
            r#"{"name":"react-icons","exports":{"./hi":{"types":"./hi/index.d.ts"},"./hi2":{"types":"./hi2/index.d.ts"}}}"#,
        ).unwrap();
        std::fs::create_dir_all(temp_dir.join("node_modules/react-icons/hi")).unwrap();
        std::fs::write(temp_dir.join("node_modules/react-icons/hi/index.d.ts"), "export declare const HiOutlineBell: any;\n").unwrap();
        std::fs::write(temp_dir.join("node_modules/react-icons/hi2/index.d.ts"), "export declare const HiOutlineBellAlert: any;\n").unwrap();

        std::fs::write(
            temp_dir.join(".axiom/generation_state.json"),
            r#"{"name":"TunnelSense QA","projectDescription":"Industrial bore sensor telemetry and automated compliance tracking."}"#,
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/hooks/useAuth.ts"),
            "import { useState, useCallback } from 'react';\nexport interface AuthState { user: any; registerUser?: (email: string, password: string) => Promise<void>; }\nexport const useAuth = (): AuthState => {\n  const [user] = useState(null);\n  const registerUser = useCallback(async (email: string, password: string) => { console.log(email, password); }, []);\n  return { user, registerUser };\n};\nexport default useAuth;\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("app/(tabs)/alerts.tsx"),
            "\"use client\";\nimport { HiOutlineBellAlert } from 'react-icons/hi';\nimport { useAuth } from '@/hooks/useAuth';\ninterface P { user?: AuthState['user']; }\nexport default function Alerts({ user }: P) {\n  const { registerUser } = useAuth();\n  return <button onClick={() => registerUser?.('a@b.c', 'p', 'Name', 'ADMIN', 'Bio')}><HiOutlineBellAlert />{user?.email}</button>;\n}\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("app/(tabs)/index.tsx"),
            "\"use client\";\nexport default function MobileHome() { return <div>Mobile Home</div>; }\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/app/layout.tsx"),
            "\"use client\";\nimport Navbar from '@/components/Navbar';\nimport Footer from '@/components/Footer';\nexport default function RootLayout({ children }: { children: React.ReactNode }) { return <html><body><Navbar /><main>{children}</main><Footer /></body></html>; }\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/app/page.tsx"),
            "import { redirect } from 'next/navigation';\nexport default function Home() {\n  return redirect('/dashboard');\n}\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/components/Navbar.tsx"),
            "\"use client\";\nimport React from 'react';\nexport const Navbar: React.FC<any> = ({ children }) => <nav>{children}</nav>;\nexport default Navbar;\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/components/Footer.tsx"),
            "\"use client\";\nimport React from 'react';\nexport const Footer: React.FC<any> = ({ children }) => <footer>{children}</footer>;\nexport default Footer;\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/components/Button.tsx"),
            "\"use client\";\nimport { HiOutlineBellAlert } from 'react-icons/hi';\nexport const Button = () => <button>Click</button>;\n",
        ).unwrap();

        crate::self_healing::run_hygiene_pass_on_project(&temp_dir);

        // Check root `app/` mirror (prevents 404 when root `app/` exists!)
        assert!(temp_dir.join("src/hooks/useUserTier.ts").is_file(), "Shared useUserTier hook must exist");
        assert!(temp_dir.join("app/page.tsx").is_file(), "Root app/page.tsx must exist");
        assert!(temp_dir.join("app/layout.tsx").is_file(), "Root app/layout.tsx must exist");
        assert!(temp_dir.join("app/dashboard/page.tsx").is_file(), "Mobile dashboard bridge must exist");
        assert!(temp_dir.join("app/alerts/page.tsx").is_file(), "Bridged /alerts route must exist");
        assert!(temp_dir.join("app/pricing/page.tsx").is_file(), "Pricing route must exist in root app/");
        assert!(temp_dir.join("app/about/page.tsx").is_file(), "About route must exist in root app/");
        assert!(temp_dir.join("app/account/page.tsx").is_file(), "Account route must exist in root app/");
        assert!(temp_dir.join("app/legal/terms/page.tsx").is_file(), "Terms route must exist in root app/");
        assert!(temp_dir.join("app/legal/privacy/page.tsx").is_file(), "Privacy route must exist in root app/");

        let home_out = std::fs::read_to_string(temp_dir.join("src/app/page.tsx")).unwrap();
        assert!(!home_out.contains("redirect('/dashboard')"), "Thin redirect must be replaced:\n{}", home_out);
        assert!(home_out.contains("How It Works") && home_out.contains("Frequently Asked Questions"), "{}", home_out);
        assert!(home_out.contains("Field Telemetry"), "Industry archetype styling/content must apply:\n{}", home_out);

        let nav_out = std::fs::read_to_string(temp_dir.join("src/components/Navbar.tsx")).unwrap();
        assert!(nav_out.contains("/pricing") && nav_out.contains("/account"), "Empty Navbar upgraded:\n{}", nav_out);

        let foot_out = std::fs::read_to_string(temp_dir.join("src/components/Footer.tsx")).unwrap();
        assert!(foot_out.contains("/legal/terms") && foot_out.contains("/legal/privacy"), "Empty Footer upgraded:\n{}", foot_out);

        let terms_out = std::fs::read_to_string(temp_dir.join("src/app/legal/terms/page.tsx")).unwrap();
        assert!(terms_out.contains("consult a qualified legal team"), "Legal disclaimer required:\n{}", terms_out);

        let use_auth_out = std::fs::read_to_string(temp_dir.join("src/hooks/useAuth.ts")).unwrap();
        assert!(use_auth_out.contains("..._args: any[]"), "Hook method rest args added:\n{}", use_auth_out);

        let alerts_out = std::fs::read_to_string(temp_dir.join("app/(tabs)/alerts.tsx")).unwrap();
        assert!(alerts_out.contains("react-icons/hi2"), "HiOutlineBellAlert moved to hi2:\n{}", alerts_out);
        assert!(alerts_out.contains("AuthState") && alerts_out.contains("@/hooks/useAuth"), "AuthState auto-imported:\n{}", alerts_out);

        let button_out = std::fs::read_to_string(temp_dir.join("src/components/Button.tsx")).unwrap();
        assert!(!button_out.contains("HiOutlineBellAlert"), "Unused unexported icon removed:\n{}", button_out);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_atomic_checkpoint_persistence_and_zero_byte_recovery() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_checkpoint_resilience_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(temp_dir.join(".axiom"));

        let cp_data = serde_json::json!({
            "projectId": "test-resilient-proj",
            "name": "SafetyGuardian Compliance OS",
            "status": "stopped",
            "completedFiles": ["package.json", "src/lib/prisma.ts"]
        });

        // 1. Initial atomic save
        crate::orchestrator::save_generation_checkpoint(&temp_dir, &cp_data);
        let loaded1 = crate::orchestrator::load_generation_checkpoint(&temp_dir);
        assert!(loaded1.is_some(), "Loaded checkpoint should succeed");
        assert_eq!(loaded1.unwrap()["name"], "SafetyGuardian Compliance OS");

        // 2. Second atomic save: creates .bak
        let cp_data_v2 = serde_json::json!({
            "projectId": "test-resilient-proj",
            "name": "SafetyGuardian Compliance OS",
            "status": "stopped",
            "completedFiles": ["package.json", "src/lib/prisma.ts", "src/types/Incident.ts"]
        });
        crate::orchestrator::save_generation_checkpoint(&temp_dir, &cp_data_v2);
        assert!(temp_dir.join(".axiom/generation_state.json.bak").exists(), "Backup checkpoint must exist");

        // 3. Simulate sudden power outage: primary file zero-filled by NTFS
        let zero_bytes = vec![0u8; 4096];
        std::fs::write(temp_dir.join(".axiom/generation_state.json"), &zero_bytes).unwrap();

        // 4. Load must gracefully fallback to .bak instead of failing/crashing
        let recovered = crate::orchestrator::load_generation_checkpoint(&temp_dir);
        assert!(recovered.is_some(), "Must fall back to .bak when main file is zero-filled");
        assert_eq!(recovered.unwrap()["name"], "SafetyGuardian Compliance OS");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn phase8_v60_universal_locator_ast_prop_defaults_and_config_driven_tailwind() {
        // 1. Objective Error-File Locator (`locate_error_source_file`)
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v60_universal_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("packages/fleet-desktop/modules/views")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app")).unwrap();

        std::fs::write(
            temp_dir.join("packages/fleet-desktop/modules/views/TurbineMonitor.tsx"),
            "\"use client\";\nimport React from 'react';\nexport default function TurbineMonitor({ turbines }: any) {\n  return <div>{turbines.length > 0 ? <span>OK</span> : null}</div>;\n}\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/components/TurbineMonitor.tsx"),
            "export default function TurbineMonitor() { return <div>Decoy</div>; }\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/app/page.tsx"),
            "export default function Home() { return <main>Home</main>; }\n",
        ).unwrap();

        let code_frame_err = "Unhandled Runtime Error\nTypeError: Cannot read properties of undefined (reading 'length')\n\nSource\nviews/TurbineMonitor.tsx (4:16) @ length\n\n  2 | import React from 'react';\n> 4 |   return <div>{turbines.length > 0 ? <span>OK</span> : null}</div>;\n";
        let loc_trunc = crate::server::locate_error_source_file(&temp_dir, "views/TurbineMonitor.tsx", code_frame_err)
            .expect("Expected truncated hint + code-frame to locate exact file");
        assert_eq!(loc_trunc.rel_path, "packages/fleet-desktop/modules/views/TurbineMonitor.tsx");

        let code_frame_no_path = "TypeError: Cannot read properties of undefined (reading 'length')\n  2 | import React from 'react';\n> 4 |   return <div>{turbines.length > 0 ? <span>OK</span> : null}</div>;\n";
        let loc_wrong_hint = crate::server::locate_error_source_file(&temp_dir, "src/app/page.tsx", code_frame_no_path)
            .expect("Expected code-frame alone to override completely wrong hint");
        assert_eq!(loc_wrong_hint.rel_path, "packages/fleet-desktop/modules/views/TurbineMonitor.tsx");

        // 2. Universal AST Component Destructured Prop Defaults & AST TS2749
        let comp_in = "\"use client\";\nimport React from 'react';\ninterface DispatchProps {\n  turbines?: any[];\n  shipment?: Shipment;\n  onDispatch?: () => void;\n  title?: string;\n  optionalNote?: string;\n  user?: { email: string };\n  checks?: ComplianceCheck[];\n}\ninterface Shipment { status: string; }\nconst ComplianceCheck = () => <div />;\nconst DispatchPanel: React.FC<DispatchProps> = ({ turbines, shipment, onDispatch, title, optionalNote, user, checks }) => {\n  if (!optionalNote) { onDispatch(); }\n  return (\n    <div>\n      <h1>{title.length > 0 ? title : 'Untitled'}</h1>\n      <span>{shipment.status}</span>\n      <span>{user?.email}</span>\n      {turbines.length > 0 ? <span>{checks?.length}</span> : null}\n    </div>\n  );\n};\nexport default DispatchPanel;\n";
        let comp_sanitized = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(comp_in, "src/components/DispatchPanel.tsx");
        assert!(comp_sanitized.contains("type ComplianceCheck = Record<string, any>;"), "AST TS2749 must catch array type ref:\n{}", comp_sanitized);
        assert!(comp_sanitized.contains("turbines = []"), "Expected turbines array default:\n{}", comp_sanitized);
        assert!(comp_sanitized.contains("shipment = {} as any"), "Expected shipment object default:\n{}", comp_sanitized);
        assert!(comp_sanitized.contains("onDispatch = ((() => {}) as any)"), "Expected onDispatch callback default:\n{}", comp_sanitized);
        assert!(!comp_sanitized.contains("title = []"), "Primitive string .length must NOT get array default:\n{}", comp_sanitized);
        assert!(!comp_sanitized.contains("optionalNote ="), "Guarded falsy prop must NOT be defaulted:\n{}", comp_sanitized);
        assert!(!comp_sanitized.contains("user ="), "Optional-chained prop must NOT be defaulted:\n{}", comp_sanitized);
        assert!(
            syntax_issues_for("src/components/DispatchPanel.tsx", &comp_sanitized).unwrap().is_empty(),
            "Rewritten component must remain valid TSX:\n{}",
            comp_sanitized
        );

        // 3. Config-driven Tailwind CSS reconciliation & compiler-driven strip helper
        std::fs::write(
            temp_dir.join("tailwind.config.js"),
            "module.exports = {\n  theme: {\n    extend: {\n      colors: {\n        'safety-blue': '#2196F3',\n        brand: {\n          500: '#3b82f6',\n          600: '#2563eb',\n        },\n      },\n    },\n  },\n};\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/app/globals.css"),
            "@tailwind base;\n@tailwind components;\n@tailwind utilities;\n.card {\n  @apply bg-brand-500 text-arbitrary-500 border-safety-blue-500 font-heading;\n}\n",
        ).unwrap();
        crate::sanitizer::reconcile_tailwind_css_with_config(&temp_dir);
        let css_after = std::fs::read_to_string(temp_dir.join("src/app/globals.css")).unwrap();
        assert!(css_after.contains("bg-brand-500"), "Configured scale color must be preserved:\n{}", css_after);
        assert!(css_after.contains("border-safety-blue"), "Flat configured color must be rewritten to flat utility:\n{}", css_after);
        assert!(!css_after.contains("text-arbitrary-500"), "Undefined arbitrary color scale must be omitted:\n{}", css_after);
        assert!(css_after.contains("font-heading"), "Non-scale token preserved until compiler reports it:\n{}", css_after);

        let stripped_compiler = crate::sanitizer::strip_tailwind_apply_class(&css_after, "font-heading");
        assert!(!stripped_compiler.contains("font-heading"), "Compiler-reported class must be stripped:\n{}", stripped_compiler);
        assert!(stripped_compiler.contains("bg-brand-500"), "Other valid classes remain:\n{}", stripped_compiler);

        // 4. Universal inspector script parses cleanly inside TSX layout
        let layout_with_inspector = format!(
            "export default function Layout({{ children }}: any) {{ return <html><body>{{children}}\n{}</body></html>; }}",
            crate::sanitizer::get_axiom_inspector_script()
        );
        assert!(
            syntax_issues_for("src/app/layout.tsx", &layout_with_inspector).unwrap().is_empty(),
            "Inspector script must parse cleanly inside layout TSX"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn phase9_v60_clean_contracts_context_and_interface_dedup() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v60_phase9_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/lib")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/types")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/utils")).unwrap();
        std::fs::create_dir_all(temp_dir.join("app/(tabs)")).unwrap();

        // 1. Prisma `@id`/`@unique`/`@@unique` contract visibility
        let schema = r#"
model Creator {
  id    String @id @default(cuid())
  email String @unique
  strategies EngagementStrategy[]
}
model EngagementStrategy {
  id           String @id @default(cuid())
  creatorId    String
  strategyName String
  creator      Creator @relation(fields: [creatorId], references: [id])
  @@unique([creatorId, strategyName])
}
"#;
        std::fs::write(temp_dir.join("prisma/schema.prisma"), schema).unwrap();
        std::fs::write(temp_dir.join("src/lib/prisma.ts"), "export const prisma = {} as any;\n").unwrap();

        let contract = crate::orchestrator::extract_prisma_schema_contract(schema);
        let creator_uniq = contract["models"]["Creator"]["uniqueFields"].as_array().unwrap();
        let creator_uniq_strs: Vec<&str> = creator_uniq.iter().filter_map(|v| v.as_str()).collect();
        assert!(creator_uniq_strs.contains(&"id"));
        assert!(creator_uniq_strs.contains(&"email"));

        let strat_uniq = contract["models"]["EngagementStrategy"]["uniqueFields"].as_array().unwrap();
        let strat_uniq_strs: Vec<&str> = strat_uniq.iter().filter_map(|v| v.as_str()).collect();
        assert!(strat_uniq_strs.contains(&"id"));
        assert!(strat_uniq_strs.contains(&"[creatorId, strategyName]"));
        assert!(!strat_uniq_strs.contains(&"creatorId"), "Non-unique FK must NOT be listed as unique");

        let summary = crate::self_healing::extract_prisma_schema_summary_for_healing(&temp_dir);
        assert!(summary.contains("unique(@id/@unique): [id, [creatorId, strategyName]]"), "{}", summary);

        // 2. Scoped Pre-Generation Context (`get_context_files_for_with_desc`)
        std::fs::write(
            temp_dir.join("src/hooks/useEngagementMetrics.ts"),
            "export function useEngagementMetrics() { return { data: [], isLoading: false, error: null as any }; }\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/types/EngagementMetrics.ts"),
            "export interface EngagementMetrics { id?: string | number; likes?: number; [key: string]: any; }\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/types/fault.ts"),
            "export interface Fault { id?: string | number; description?: string; [key: string]: any; }\n",
        ).unwrap();
        std::fs::write(
            temp_dir.join("src/types/NarrowStrict.ts"),
            "export interface NarrowStrict { code: string; }\n",
        ).unwrap();

        let mobile_ctx = crate::orchestrator::get_context_files_for_with_desc(
            "app/(tabs)/EngagementTracking.tsx",
            "Mobile tab screen displaying engagement metrics",
            &temp_dir,
        );
        assert!(mobile_ctx.contains("src/hooks/useEngagementMetrics.ts"), "{}", mobile_ctx);
        assert!(mobile_ctx.contains("src/types/EngagementMetrics.ts"), "{}", mobile_ctx);
        assert!(!mobile_ctx.contains("schema.prisma"), "Mobile UI must NEVER get schema.prisma:\n{}", mobile_ctx);

        let backend_util_ctx = crate::orchestrator::get_context_files_for_with_desc(
            "src/utils/engagement.ts",
            "Prisma query helpers for creator analytics and upsert operations",
            &temp_dir,
        );
        assert!(backend_util_ctx.contains("schema.prisma"), "DB util must get schema.prisma:\n{}", backend_util_ctx);
        assert!(backend_util_ctx.contains("src/lib/prisma.ts"), "{}", backend_util_ctx);

        let client_util_ctx = crate::orchestrator::get_context_files_for_with_desc(
            "src/lib/utils.ts",
            "Tailwind className merge helper",
            &temp_dir,
        );
        assert!(!client_util_ctx.contains("schema.prisma"), "Pure client util must NOT get Prisma:\n{}", client_util_ctx);

        // 3. AST Deduplication of Local Duplicate Domain Interfaces against `@/types`
        std::fs::write(
            temp_dir.join("src/components/FaultAlert.tsx"),
            "\"use client\";\nimport React, { useState } from 'react';\ninterface Fault {\n  id: string;\n  description: string;\n}\nexport interface FaultProps { label?: string; }\nexport interface KeptExported { code: string; }\ninterface NarrowStrict {\n  code: string;\n}\nexport default function FaultAlert(_p: FaultProps) {\n  const [faults] = useState<Fault[]>([]);\n  const k: NarrowStrict = { code: 'x' };\n  return <div>{faults.length}{k.code}</div>;\n}\n",
        ).unwrap();

        let dedup_count = crate::ast_engine::deduplicate_local_domain_interfaces_against_types_in_project(&temp_dir);
        assert_eq!(dedup_count, 1);

        let updated_fault_alert = std::fs::read_to_string(temp_dir.join("src/components/FaultAlert.tsx")).unwrap();
        assert!(updated_fault_alert.contains("import type { Fault } from '@/types/fault';"), "{}", updated_fault_alert);
        assert!(!updated_fault_alert.contains("interface Fault {"), "Duplicate local interface should be removed:\n{}", updated_fault_alert);
        assert!(updated_fault_alert.contains("export interface FaultProps"), "Props interface must remain:\n{}", updated_fault_alert);
        assert!(updated_fault_alert.contains("interface NarrowStrict"), "Counterpart without [key: string]: any must NOT be deleted:\n{}", updated_fault_alert);
        assert!(
            syntax_issues_for("src/components/FaultAlert.tsx", &updated_fault_alert).unwrap().is_empty(),
            "Updated component must remain valid TSX:\n{}",
            updated_fault_alert
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn phase10_v61_universal_postcss_semantic_ast_ui_and_middleware_suite() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v61_phase10_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app")).unwrap();

        // 1. PostCSS quote-agnostic extraction & inline/multiline @apply cleanup
        let postcss_err = "Syntax error: The `whitespace-no-wrap` class does not exist.\nThe 'foo-bar' class does not exist.\nThe \"baz-qux\" class does not exist.";
        let extracted_cls = crate::sanitizer::extract_missing_tailwind_classes_from_error(postcss_err);
        assert_eq!(extracted_cls, vec!["whitespace-no-wrap", "foo-bar", "baz-qux"]);

        let css_in = ".table { @apply w-full min-w-max table-auto text-left whitespace-no-wrap; }\n.only-bad {\n  @apply whitespace-no-wrap;\n}\n";
        let css_out = crate::sanitizer::strip_tailwind_apply_class(css_in, "whitespace-no-wrap");
        assert!(!css_out.contains("whitespace-no-wrap"), "Must strip inline & standalone failing class:\n{}", css_out);
        assert!(css_out.contains(".table { @apply w-full min-w-max table-auto text-left; }"), "Must preserve sibling valid utilities inline:\n{}", css_out);
        assert!(!css_out.contains("@apply ;"), "Emptied standalone @apply line must be dropped cleanly:\n{}", css_out);

        // 2. Semantic build error JSON parser
        let fenced_json = "```json\n{\"has_error\": true, \"error_kind\": \"css\", \"file_path\": \"src/app/globals.css\", \"error_summary\": \"Unknown class whitespace-no-wrap\", \"missing_css_class\": \".whitespace-no-wrap\"}\n```";
        let parsed_diag = crate::self_healing::parse_semantic_build_diagnosis(fenced_json).unwrap();
        assert!(parsed_diag.has_error);
        assert_eq!(parsed_diag.error_kind, "css");
        assert_eq!(parsed_diag.file_path.as_deref(), Some("src/app/globals.css"));
        assert_eq!(parsed_diag.missing_css_class.as_deref(), Some("whitespace-no-wrap"));

        let no_err_diag = crate::self_healing::parse_semantic_build_diagnosis("{\"has_error\": false, \"file_path\": \"null\"}").unwrap();
        assert!(!no_err_diag.has_error);
        assert_eq!(no_err_diag.file_path, None);
        assert!(crate::self_healing::parse_semantic_build_diagnosis("not json at all").is_none());

        // 3. AST unused TS2305 named import cleanup
        let ts2305_msg = "src/renderer/App.tsx(1,10): error TS2305: Module '\"@stripe/stripe-js\"' has no exported member 'Status'.";
        let targets = crate::self_healing::extract_ts2305_missing_imports(ts2305_msg);
        assert_eq!(targets, vec![(Some("@stripe/stripe-js".to_string()), "Status".to_string())]);

        let unused_src = "import { Status } from '@stripe/stripe-js';\nexport default function App() {\n  {/* Filter by Status */}\n  return <div>Filter by Status</div>;\n}\n";
        let cleaned_unused = crate::ast_engine::remove_unused_named_import_specifiers("src/renderer/App.tsx", unused_src, &targets).unwrap();
        assert!(!cleaned_unused.contains("Status } from"), "Unused TS2305 import line must be removed:\n{}", cleaned_unused);
        assert!(cleaned_unused.contains("Filter by Status"), "JSX text must stay intact:\n{}", cleaned_unused);

        let partial_src = "import { Status, loadStripe } from '@stripe/stripe-js';\nexport const p = loadStripe('pk_test');\n";
        let cleaned_partial = crate::ast_engine::remove_unused_named_import_specifiers("src/lib/stripe.ts", partial_src, &targets).unwrap();
        assert!(cleaned_partial.contains("import { loadStripe } from '@stripe/stripe-js';"), "Used sibling import must remain:\n{}", cleaned_partial);

        let used_type_src = "import { Status } from '@stripe/stripe-js';\nexport const fn = (x: Status) => x;\n";
        assert!(
            crate::ast_engine::remove_unused_named_import_specifiers("src/lib/stripe.ts", used_type_src, &targets).is_none(),
            "Must NOT remove import identifier when referenced in a type annotation"
        );

        // 4. AST-safe enum mock normalization (never touches TS interfaces/unions)
        std::fs::write(
            temp_dir.join("prisma/schema.prisma"),
            "enum Priority { HIGH MEDIUM LOW }\n",
        ).unwrap();
        let hook_src = "\"use client\";\nexport interface MaintenanceAlert {\n  id: string;\n  priority: 'HIGH' | 'MEDIUM' | 'LOW';\n}\nconst sampleMock = { priority: 'high' };\nexport const demo = sampleMock;\n";
        let rec_hook = crate::sanitizer::reconcile_client_prisma_enum_mocks(&temp_dir, "src/hooks/useMaintenanceAlerts.ts", hook_src);
        assert!(rec_hook.contains("priority: 'HIGH' | 'MEDIUM' | 'LOW';"), "Interface union property must remain untouched:\n{}", rec_hook);
        assert!(rec_hook.contains("priority: 'HIGH' as any"), "Runtime object literal mock must normalize:\n{}", rec_hook);
        assert!(
            syntax_issues_for("src/hooks/useMaintenanceAlerts.ts", &rec_hook).unwrap().is_empty(),
            "Enum-reconciled hook must remain syntactically valid TS:\n{}",
            rec_hook
        );

        // 5. Spread-safe try/catch body hoisting
        let spread_route = "export async function POST(request: Request) {\n  try {\n    const body = await request.json();\n    return Response.json(body);\n  } catch (error) {\n    return Response.json({ id: 'mock', ...body }, { status: 201 });\n  }\n}\n";
        let hoisted_spread = crate::sanitizer::sanitize_try_catch_body_scope(spread_route);
        assert!(hoisted_spread.contains("let body: any = {};"), "Spread `...body` in catch must hoist body declaration:\n{}", hoisted_spread);

        let req_body_route = "export async function POST(request: Request) {\n  try {\n    const body = await request.json();\n    return Response.json(body);\n  } catch (error) {\n    return Response.json({ err: req.body }, { status: 500 });\n  }\n}\n";
        let ignored_prop = crate::sanitizer::sanitize_try_catch_body_scope(req_body_route);
        assert!(!ignored_prop.contains("let body: any = {};"), "Property access `req.body` must NOT falsely trigger hoist:\n{}", ignored_prop);

        // 6. Interactive & contrast UI primitive stubs
        std::fs::write(
            temp_dir.join("src/app/page.tsx"),
            "\"use client\";\nimport { Checkbox } from '@/components/ui/checkbox';\nimport { Switch } from '@/components/ui/switch';\nimport { Input } from '@/components/ui/input';\nimport { useToast } from '@/components/ui/use-toast';\nexport default function Home() {\n  const toast = useToast();\n  toast({ title: 'Ready' });\n  return <div><Input /><Checkbox checked={true} onCheckedChange={() => {}} /><Switch onCheckedChange={() => {}} /></div>;\n}\n",
        ).unwrap();
        crate::sanitizer::ensure_ui_primitives_reconciled(&temp_dir);
        let cb_src = std::fs::read_to_string(temp_dir.join("src/components/ui/checkbox.tsx")).unwrap();
        assert!(cb_src.contains("onCheckedChange"), "Checkbox stub must wire onCheckedChange:\n{}", cb_src);
        let sw_src = std::fs::read_to_string(temp_dir.join("src/components/ui/switch.tsx")).unwrap();
        assert!(sw_src.contains("onCheckedChange") && sw_src.contains("aria-checked"), "Switch stub must wire toggle & aria-checked:\n{}", sw_src);
        let in_src = std::fs::read_to_string(temp_dir.join("src/components/ui/input.tsx")).unwrap();
        assert!(in_src.contains("bg-white") && in_src.contains("text-slate-900") && in_src.contains("placeholder:text-slate-400"), "Input stub must enforce conditional light contrast:\n{}", in_src);
        let ut_src = std::fs::read_to_string(temp_dir.join("src/components/ui/use-toast.tsx")).unwrap();
        assert!(ut_src.contains("return Object.assign(fn, { toast: fn"), "useToast must be both callable and destructurable:\n{}", ut_src);

        // 7. Edge middleware public-root protection guard
        let rt = tokio::runtime::Runtime::new().unwrap();
        let bad_catchall_mw = r#"
import { NextRequest, NextResponse } from 'next/server';
import { getToken } from 'next-auth/jwt';
export async function middleware(request: NextRequest) {
  const pathname = request.nextUrl.pathname;
  if (pathname.startsWith('/login') || pathname.startsWith('/register')) return NextResponse.next();
  const token = await getToken({ req: request });
  if (!token) return NextResponse.redirect(new URL('/login', request.url));
  if (token && pathname === '/login') return NextResponse.redirect(new URL('/dashboard', request.url));
  return NextResponse.next();
}
export const config = { matcher: ['/((?!api|_next/static|_next/image|favicon.ico).*)'] };
"#;
        assert!(crate::orchestrator::middleware_blocks_public_root(bad_catchall_mw));
        assert!(
            rt.block_on(crate::orchestrator::validate_file_with_project_context(&temp_dir, "src/middleware.ts", bad_catchall_mw, "qwen")).is_err(),
            "Catch-all middleware blocking `/` must be rejected when public root page exists"
        );

        let good_prefix_mw = r#"
import { NextRequest, NextResponse } from 'next/server';
import { getToken } from 'next-auth/jwt';
export async function middleware(request: NextRequest) {
  const pathname = request.nextUrl.pathname;
  if (pathname.startsWith('/dashboard/') || pathname.startsWith('/admin/')) {
    const token = await getToken({ req: request });
    if (!token) return NextResponse.redirect(new URL('/login', request.url));
  }
  return NextResponse.next();
}
export const config = { matcher: ['/((?!api|_next/static|_next/image|favicon.ico).*)'] };
"#;
        assert!(!crate::orchestrator::middleware_blocks_public_root(good_prefix_mw));
        assert!(
            rt.block_on(crate::orchestrator::validate_file_with_project_context(&temp_dir, "src/middleware.ts", good_prefix_mw, "qwen")).is_ok(),
            "Prefix-scoped middleware must pass"
        );

        let good_matcher_mw = r#"
import { NextRequest, NextResponse } from 'next/server';
export function middleware(_req: NextRequest) {
  return NextResponse.redirect(new URL('/login', _req.url));
}
export const config = { matcher: ['/dashboard/:path*', '/settings/:path*'] };
"#;
        assert!(!crate::orchestrator::middleware_blocks_public_root(good_matcher_mw));
        assert!(
            rt.block_on(crate::orchestrator::validate_file_with_project_context(&temp_dir, "src/middleware.ts", good_matcher_mw, "qwen")).is_ok(),
            "Matcher-scoped middleware must pass"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn phase11_v62_universal_auth_prisma_hooks_openai_and_katastar_suite() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_v62_phase11_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/api/maintenance")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/api/register")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/shared")).unwrap();

        // 1. OpenAI v3 -> v4 with `export const`
        let oai_v3_export = "import { Configuration, OpenAIApi } from 'openai';\n\nconst openaiConfig = new Configuration({\n  apiKey: process.env.OPENAI_API_KEY,\n});\n\nexport const openaiClient = new OpenAIApi(openaiConfig);\n";
        let oai_v4 = crate::sanitizer::sanitize_openai_v3_to_v4(oai_v3_export);
        assert!(oai_v4.contains("import OpenAI from 'openai';"), "{}", oai_v4);
        assert!(oai_v4.contains("export const openaiClient = new OpenAI("), "{}", oai_v4);
        assert!(!oai_v4.contains("Configuration") && !oai_v4.contains("OpenAIApi"), "{}", oai_v4);

        // 2. RootLayout useSession neutralization must ALSO strip window.location.href='/login'
        let root_layout_trap = "\"use client\";\nimport { useEffect } from 'react';\nimport { useSession } from 'next-auth/react';\nexport default function RootLayout({ children }: { children: React.ReactNode }) {\n  const { data: session, status } = useSession();\n  useEffect(() => {\n    if (status === 'unauthenticated') {\n      window.location.href = '/login';\n    }\n  }, [session, status]);\n  return <html><body>{children}</body></html>;\n}\n";
        let safe_layout = crate::sanitizer::sanitize_root_layout_direct_use_session(root_layout_trap);
        assert!(!safe_layout.contains("useSession("), "{}", safe_layout);
        assert!(!safe_layout.contains("window.location.href = '/login'"), "Must neutralize infinite login redirect loop:\n{}", safe_layout);

        // 3. `import type { DefectData } from '@/lib/prisma'` redirected to `@/types` for client/hooks
        let bad_hook_imp = "\"use client\";\nimport type { DefectData } from '@/lib/prisma';\nexport const x: DefectData[] = [];\n";
        let fixed_hook_imp = crate::sanitizer::normalize_prisma_schema_imports(bad_hook_imp, true);
        assert!(fixed_hook_imp.contains("from '@/types'"), "{}", fixed_hook_imp);
        assert!(!fixed_hook_imp.contains("@/lib/prisma"), "{}", fixed_hook_imp);

        // Server prisma singleton import must NOT be touched
        let good_srv_imp = "import { prisma } from '@/lib/prisma';\nexport const fn = () => prisma;\n";
        assert_eq!(crate::sanitizer::normalize_prisma_schema_imports(good_srv_imp, false), good_srv_imp);

        // 4. AST hook array param defaults (`useCostManagement(fallbackData?: T[])`)
        let raw_hook = "\"use client\";\nimport { useState } from 'react';\nexport interface CostData { id: number; }\nconst useCostManagement = <T extends CostData>(fallbackData?: T[] | any, options?: { [key: string]: any }) => {\n  const [data, setData] = useState<T[]>(fallbackData);\n  return { data };\n};\nexport default useCostManagement;\n";
        let safe_hook = crate::ast_engine::ensure_hook_array_param_defaults_in_source("hooks/useCostManagement.ts", raw_hook).unwrap();
        assert!(safe_hook.contains("fallbackData: T[] | any = [] as any"), "{}", safe_hook);
        assert!(safe_hook.contains("Array.isArray(fallbackData) ? fallbackData : []"), "{}", safe_hook);
        assert!(syntax_issues_for("hooks/useCostManagement.ts", &safe_hook).unwrap().is_empty(), "{}", safe_hook);

        // 5. AST Prisma inline enum relaxation + `api/` auto-import of Prisma enums
        let schema = "enum Status { ACTIVE INACTIVE }\nenum Priority { HIGH MEDIUM LOW }\nmodel Workflow {\n  id String @id\n  title String\n  status Status\n  priority Priority\n}\nmodel MaintenanceAlert {\n  id String @id\n  priority Priority\n  status Status\n}\n";
        std::fs::write(temp_dir.join("prisma/schema.prisma"), schema).unwrap();
        let enum_map = crate::ast_engine::extract_prisma_model_enum_fields(schema);
        let wf_route = "import { prisma } from '@/lib/prisma';\nexport async function POST(body: any) {\n  return prisma.workflow.create({\n    data: {\n      title: body.title,\n      status: body.status,\n      priority: body.priority,\n    },\n  });\n}\n";
        let relaxed_wf = crate::ast_engine::relax_prisma_inline_enum_fields_in_source("src/app/api/workflow/route.ts", wf_route, &enum_map).unwrap();
        assert!(relaxed_wf.contains("status: body.status as any"), "{}", relaxed_wf);
        assert!(relaxed_wf.contains("priority: body.priority as any"), "{}", relaxed_wf);
        assert!(relaxed_wf.contains("title: body.title,"), "Non-enum scalar must stay untouched:\n{}", relaxed_wf);

        let maint_route_path = temp_dir.join("src/app/api/maintenance/route.ts");
        std::fs::write(
            &maint_route_path,
            "import { prisma } from '@/lib/prisma';\nexport async function POST(body: any) {\n  return prisma.maintenanceAlert.create({ data: { priority: body.priority as Priority, status: body.status as Status } });\n}\n",
        ).unwrap();
        let auto_cnt = crate::self_healing::auto_import_missing_symbols_in_project(&temp_dir);
        assert!(auto_cnt >= 1, "api/ route must be scanned for Prisma enum auto-imports");
        let maint_after = std::fs::read_to_string(&maint_route_path).unwrap();
        assert!(maint_after.contains("@prisma/client") && maint_after.contains("Priority") && maint_after.contains("Status"), "{}", maint_after);

        // 6. Middleware auth-route loop guard + register contract + 1-hop client->Prisma guard
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mw_blocks_login = "import { NextRequest, NextResponse } from 'next/server';\nimport { getToken } from 'next-auth/jwt';\nexport async function middleware(request: NextRequest) {\n  const pathname = request.nextUrl.pathname;\n  if (pathname === '/' || pathname.startsWith('/pricing')) return NextResponse.next();\n  const token = await getToken({ req: request });\n  if (!token) return NextResponse.redirect(new URL('/login', request.url));\n  return NextResponse.next();\n}\nexport const config = { matcher: ['/((?!api|_next/static|_next/image|favicon.ico).*)'] };\n";
        assert!(crate::orchestrator::middleware_blocks_auth_routes(mw_blocks_login));
        assert!(rt.block_on(crate::orchestrator::validate_file_with_project_context(&temp_dir, "src/middleware.ts", mw_blocks_login, "qwen")).is_err());

        let mw_allows_auth = "import { NextRequest, NextResponse } from 'next/server';\nimport { getToken } from 'next-auth/jwt';\nexport async function middleware(request: NextRequest) {\n  const pathname = request.nextUrl.pathname;\n  if (pathname === '/' || pathname.startsWith('/login') || pathname.startsWith('/register')) return NextResponse.next();\n  const token = await getToken({ req: request });\n  if (!token) return NextResponse.redirect(new URL('/login', request.url));\n  return NextResponse.next();\n}\nexport const config = { matcher: ['/((?!api|_next/static|_next/image|favicon.ico).*)'] };\n";
        assert!(!crate::orchestrator::middleware_blocks_auth_routes(mw_allows_auth));

        std::fs::write(temp_dir.join("src/app/api/register/route.ts"), "export async function POST() {}\n").unwrap();
        let bad_reg = "\"use client\";\nimport { signIn } from 'next-auth/react';\nexport default function Register() { const go = () => signIn('credentials', {}); return <button onClick={go}>Go</button>; }\n";
        assert!(rt.block_on(crate::orchestrator::validate_file_with_project_context(&temp_dir, "src/app/register/page.tsx", bad_reg, "qwen")).is_err());

        std::fs::write(temp_dir.join("src/shared/database.js"), "import prisma from '@/lib/prisma';\nexport const getAll = () => prisma.workflow.findMany();\n").unwrap();
        let bad_client_db = "\"use client\";\nimport * as db from '@/shared/database';\nexport default function W() { return <div>{String(db)}</div>; }\n";
        assert!(rt.block_on(crate::orchestrator::validate_file_with_project_context(&temp_dir, "src/renderer/components/W.tsx", bad_client_db, "qwen")).is_err());

        // 7. Katastar scrubs CUID project IDs so PostCSS errors group into one signature
        let scrubbed = crate::katastar::scrub_sensitive_text("Syntax error: C:\\Users\\PC\\AppData\\Local\\axiom-forge\\projects\\cmwnn3uou0003abwe04b7vorp\\src\\app\\globals.css The `ring-primary` class does not exist.");
        assert!(scrubbed.contains("projects\\<PROJ>\\src\\app\\globals.css"), "{}", scrubbed);
        assert!(!scrubbed.contains("cmwnn3uou0003abwe04b7vorp"), "{}", scrubbed);

        // 8. Inverted Zod coerce + NextResponse.status().json()
        let bad_api = "import { NextResponse } from 'next/server';\nimport { z } from 'zod';\nconst s = z.object({ day: z.date().coerce(), n: z.number().coerce() });\nexport async function POST() {\n  return NextResponse.status(400).json({ error: 'Invalid input' });\n}\n";
        let good_api = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(bad_api, "src/app/api/test/route.ts");
        assert!(good_api.contains("z.coerce.date()") && good_api.contains("z.coerce.number()"), "{}", good_api);
        assert!(good_api.contains("NextResponse.json({ error: 'Invalid input' }, { status: 400 })"), "{}", good_api);

        // 9. Prisma include relation case reconciliation + unused TS2307 import cleanup + misimported Prisma enum reroute
        let rel_schema = "enum AppointmentStatus { SCHEDULED COMPLETED CANCELLED }\nmodel Doctor { id String @id }\nmodel Appointment {\n  id String @id\n  Doctor Doctor @relation(fields: [doctorId], references: [id])\n  doctorId String\n  status AppointmentStatus\n}\n";
        let fields_map = crate::ast_engine::extract_prisma_model_field_names(rel_schema);
        let inc_src = "import { prisma } from '@/lib/prisma';\nexport async function GET() { return prisma.appointment.findMany({ include: { doctor: true } }); }\n";
        let inc_fixed = crate::ast_engine::reconcile_prisma_relation_case_in_source("src/app/api/appointments/route.ts", inc_src, &fields_map).unwrap();
        assert!(inc_fixed.contains("include: { Doctor: true }"), "{}", inc_fixed);

        let dead_mod_src = "\"use client\";\nimport { usePatient } from '@/hooks/usePatient';\nexport default function P() { return <div>OK</div>; }\n";
        let dead_mod_fixed = crate::ast_engine::remove_unused_missing_module_imports("src/app/patients/[id]/page.tsx", dead_mod_src, &["@/hooks/usePatient".to_string()]).unwrap();
        assert!(!dead_mod_fixed.contains("@/hooks/usePatient"), "{}", dead_mod_fixed);

        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/appointments")).unwrap();
        std::fs::write(temp_dir.join("prisma/schema.prisma"), rel_schema).unwrap();
        std::fs::write(temp_dir.join("src/hooks/useAppointments.ts"), "export const useAppointments = () => ({ data: [] });\n").unwrap();
        std::fs::write(temp_dir.join("src/app/appointments/page.tsx"), "\"use client\";\nimport { useAppointments, AppointmentStatus } from '@/hooks/useAppointments';\nexport default function P() { return <div>{Object.values(AppointmentStatus).join(',')}</div>; }\n").unwrap();
        let rerouted = crate::self_healing::reconcile_misimported_prisma_enums_in_project(&temp_dir);
        assert!(rerouted >= 1);
        let appt_after = std::fs::read_to_string(temp_dir.join("src/app/appointments/page.tsx")).unwrap();
        assert!(appt_after.contains("import { useAppointments } from '@/hooks/useAppointments'"), "{}", appt_after);
        assert!(appt_after.contains("AppointmentStatus") && appt_after.contains("from '@/types'"), "{}", appt_after);
        let types_after = std::fs::read_to_string(temp_dir.join("src/types/index.ts")).unwrap();
        assert!(types_after.contains("export const AppointmentStatus:") && types_after.contains("SCHEDULED: 'SCHEDULED'"), "{}", types_after);

        // 10. Callback prop default in TS must satisfy Promise-returning signatures
        let async_cb_comp = "\"use client\";\ninterface FormProps { onSubmit?: (d: any) => Promise<void>; }\nconst F = ({ onSubmit }: FormProps) => <button onClick={() => onSubmit({})}>Save</button>;\nexport default F;\n";
        let safe_cb_comp = crate::ast_engine::ensure_component_destructured_prop_defaults_in_source("src/components/F.tsx", async_cb_comp).unwrap();
        assert!(safe_cb_comp.contains("onSubmit = ((() => {}) as any)"), "{}", safe_cb_comp);

        // 11. Prisma inverted `@relation(references: [localFk])` repair + 1:1 opposite strip
        let bad_prisma = "model LogStream {\n  id String @id @default(cuid())\n  incidents Incident[]\n}\nmodel Incident {\n  id String @id @default(cuid())\n  logStreamId String\n  logStream LogStream @relation(references: [logStreamId])\n  aiSummary AiSummary? @relation(references: [incidentId])\n}\nmodel AiSummary {\n  id String @id @default(cuid())\n  incidentId String\n  incident Incident @relation(references: [incidentId])\n}\n";
        let fixed_prisma = crate::sanitizer::sanitize_prisma_relations(bad_prisma);
        assert!(fixed_prisma.contains("logStream LogStream @relation(fields: [logStreamId], references: [id])"), "{}", fixed_prisma);
        assert!(fixed_prisma.contains("incident Incident @relation(fields: [incidentId], references: [id])"), "{}", fixed_prisma);
        assert!(!fixed_prisma.contains("aiSummary AiSummary? @relation(references: [incidentId])"), "{}", fixed_prisma);

        // 12. Legacy openai@3 package.json upgrade + typed empty object literal `as any` + auto-import `toast`
        let pkg_v3 = temp_dir.join("package.json");
        std::fs::write(&pkg_v3, "{\"dependencies\":{\"openai\":\"^3.1.0\",\"next\":\"14.2.35\"}}").unwrap();
        assert!(crate::packages::sanitize_package_json_for_offline(&pkg_v3));
        let pkg_after = std::fs::read_to_string(&pkg_v3).unwrap();
        assert!(pkg_after.contains("\"openai\": \"^4.67.0\""), "{}", pkg_after);

        let typed_empty = "export async function POST() {\n  let body: { logText: string } = {};\n  return body;\n}\n";
        let fixed_empty = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(typed_empty, "src/app/api/analyze/route.ts");
        assert!(fixed_empty.contains("let body: { logText: string } = {} as any;"), "{}", fixed_empty);

        std::fs::create_dir_all(temp_dir.join("src/app/regtest")).unwrap();
        let toast_page = temp_dir.join("src/app/regtest/page.tsx");
        std::fs::write(&toast_page, "\"use client\";\nexport default function R() { const go = () => { toast.loading('...'); toast.success('ok'); }; return <button onClick={go}>Go</button>; }\n").unwrap();
        let _ = crate::self_healing::auto_import_missing_symbols_in_project(&temp_dir);
        let toast_after = std::fs::read_to_string(&toast_page).unwrap();
        assert!(toast_after.contains("toast") && toast_after.contains("@/components/ui/use-toast"), "{}", toast_after);

        // 13. `@react-icons/hi2` -> `react-icons/hi2` + client `import type { Enum } from '@prisma/client'` -> `@/types` (TS1361)
        let bad_icon_imp = "\"use client\";\nimport { HiOutlineTruck } from '@react-icons/hi2';\nexport default function C() { return <HiOutlineTruck />; }\n";
        let fixed_icon_imp = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(bad_icon_imp, "src/components/C.tsx");
        assert!(fixed_icon_imp.contains("from 'react-icons/hi2'"), "{}", fixed_icon_imp);

        let ts1361_page = temp_dir.join("src/app/appointments/ship.tsx");
        std::fs::write(&ts1361_page, "\"use client\";\nimport type { AppointmentStatus } from '@prisma/client';\nexport default function S() { return <div>{AppointmentStatus.SCHEDULED}</div>; }\n").unwrap();
        let rerouted_prisma_enum = crate::self_healing::reconcile_misimported_prisma_enums_in_project(&temp_dir);
        assert!(rerouted_prisma_enum >= 1);
        let ship_after = std::fs::read_to_string(&ts1361_page).unwrap();
        assert!(ship_after.contains("AppointmentStatus") && ship_after.contains("from '@/types'"), "{}", ship_after);
        assert!(!ship_after.contains("@prisma/client"), "{}", ship_after);

        // 14. Web-track functional rules: `/api/auth/[...nextauth]` URL normalization, login `redirect: false` contract, hook `useEffect` mount hydration
        let bad_auth_fetch = "export async function getU() { return fetch('/api/auth/[...nextauth]'); }\n";
        let good_auth_fetch = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(bad_auth_fetch, "src/hooks/useUser.ts");
        assert!(good_auth_fetch.contains("'/api/auth/session'"), "{}", good_auth_fetch);

        let bad_login = "\"use client\";\nimport { signIn } from 'next-auth/react';\nexport default function L() { const go = async () => { await signIn('credentials', { email: 'a', password: 'b', redirect: false }); }; return <button onClick={go}>Login</button>; }\n";
        assert!(rt.block_on(crate::orchestrator::validate_file_with_project_context(&temp_dir, "src/app/login/page.tsx", bad_login, "qwen")).is_err());

        let good_login = "\"use client\";\nimport { signIn } from 'next-auth/react';\nimport { useRouter } from 'next/navigation';\nexport default function L() { const router = useRouter(); const go = async () => { const res = await signIn('credentials', { email: 'a', password: 'b', redirect: false }); if (!res?.error) router.push('/dashboard'); }; return <button onClick={go}>Login</button>; }\n";
        assert!(rt.block_on(crate::orchestrator::validate_file_with_project_context(&temp_dir, "src/app/login/page.tsx", good_login, "qwen")).is_ok());

        let lazy_hook = "\"use client\";\nimport { useState, useCallback } from 'react';\nexport function useItems() { const [data, setData] = useState([]); const load = useCallback(async () => { const r = await fetch('/api/items'); setData(await r.json()); }, []); return { data, load }; }\n";
        assert!(rt.block_on(crate::orchestrator::validate_file_with_project_context(&temp_dir, "src/hooks/useItems.ts", lazy_hook, "qwen")).is_err());

        // 15. Desktop Iteration #1 fixes: npm ENOVERSIONS + prisma-client-js strip + Electron main reconciliation + Prisma optional FK -> optional relation
        let enover_action = crate::packages::analyze_npm_install_failure("npm error code ENOVERSIONS\nnpm error No versions available for prisma-client-js\n");
        assert!(matches!(enover_action, Some(crate::packages::NpmFailureAction::Remove(ref p)) if p == "prisma-client-js"));

        std::fs::write(temp_dir.join("main.js"), "// electron main\n").unwrap();
        let desktop_pkg = temp_dir.join("package.json");
        std::fs::write(&desktop_pkg, "{\"main\":\"src/main.js\",\"dependencies\":{\"next\":\"14.2.35\"},\"devDependencies\":{\"prisma-client-js\":\"^2.37.0\",\"prisma\":\"5.22.0\"}}").unwrap();
        assert!(crate::packages::sanitize_package_json_for_offline(&desktop_pkg));
        let desktop_pkg_after = std::fs::read_to_string(&desktop_pkg).unwrap();
        assert!(!desktop_pkg_after.contains("prisma-client-js"), "{}", desktop_pkg_after);
        assert!(desktop_pkg_after.contains("\"main\": \"main.js\""), "{}", desktop_pkg_after);

        let opt_fk_prisma = "model User {\n  id String @id @default(cuid())\n  alerts CalibrationAlert[]\n}\nmodel CalibrationAlert {\n  id String @id @default(cuid())\n  user User @relation(fields: [userId], references: [id])\n  userId String?\n}\n";
        let fixed_opt_fk = crate::sanitizer::sanitize_prisma_relations(opt_fk_prisma);
        assert!(fixed_opt_fk.contains("user User? @relation(fields: [userId], references: [id])"), "{}", fixed_opt_fk);

        // 16. Desktop Iteration #2 fixes: auto-import `NextAuthOptions` + widen local component domain view-model interfaces
        let raw_auth = "import CredentialsProvider from 'next-auth/providers/credentials';\nexport const authOptions: NextAuthOptions = { providers: [] };\n";
        let fixed_auth = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(raw_auth, "src/lib/auth.ts");
        assert!(fixed_auth.contains("import type { NextAuthOptions } from 'next-auth';"), "{}", fixed_auth);

        let raw_tree = "\"use client\";\ninterface AssemblyNode {\n  id: string;\n  name: string;\n  children?: AssemblyNode[];\n}\ninterface TreeProps { id: string; }\nexport default function T(_p: TreeProps) { return null; }\n";
        let fixed_tree = crate::ast_engine::widen_local_component_view_models_in_source("src/renderer/components/AssemblyTree.tsx", raw_tree).unwrap();
        assert!(fixed_tree.contains("id?: string;") && fixed_tree.contains("[key: string]: any;"), "{}", fixed_tree);
        assert!(fixed_tree.contains("interface TreeProps { id: string; }"), "Props interface must stay untouched:\n{}", fixed_tree);

        // 17. Desktop Iteration #3 fixes: ban `@nextui-org/*` + `@heroui/*`, default `lucide-react` import -> named, strip broken NextAuth jwt.encode stub
        assert!(crate::sanitizer::is_banned_external_ui_package("@nextui-org/link"));
        assert!(crate::sanitizer::is_banned_external_ui_package("@heroui/react"));

        let bad_lucide = "\"use client\";\nimport LucideLock from 'lucide-react';\nexport default function L() { return <LucideLock />; }\n";
        let fixed_lucide = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(bad_lucide, "src/app/login/page.tsx");
        assert!(fixed_lucide.contains("import { LucideLock } from 'lucide-react';"), "{}", fixed_lucide);

        let bad_jwt_auth = "import type { NextAuthOptions } from 'next-auth';\nexport const authOptions: NextAuthOptions = {\n  providers: [],\n  jwt: {\n    secret: process.env.NEXTAUTH_SECRET,\n    encode: async ({ token, secret }) => {\n      return { token };\n    },\n    decode: async ({ token, secret }) => {\n      return token;\n    }\n  },\n};\n";
        let fixed_jwt_auth = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(bad_jwt_auth, "src/lib/auth.ts");
        assert!(!fixed_jwt_auth.contains("return { token }"), "{}", fixed_jwt_auth);

        // 18. Desktop Iteration #5 fixes: banned UI `Link` -> `next/link` (no TS2440 collision), `import type { useAuth }` split, `zodResolver` auto-import
        let nav_nextui = "'use client';\nimport React from 'react';\nimport { Link } from '@nextui-org/link';\nexport default function Navbar() { return <nav><Link href=\"/\">Home</Link></nav>; }\n";
        let fixed_nav = crate::sanitizer::post_process_generated_file("src/components/Navbar.tsx", nav_nextui, &serde_json::json!({}), true);
        assert!(fixed_nav.contains("import Link from 'next/link';"), "{}", fixed_nav);
        assert!(!fixed_nav.contains("const Link:"), "Must NOT emit colliding local const Link stub:\n{}", fixed_nav);

        let bad_hook_type_imp = "\"use client\";\nimport type { AuthContextType, useAuth } from '@/hooks/useAuth';\nexport default function P() { const ctx: AuthContextType | null = useAuth(); return <div>{String(ctx)}</div>; }\n";
        let fixed_hook_type_imp = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(bad_hook_type_imp, "src/app/login/page.tsx");
        assert!(fixed_hook_type_imp.contains("import type { AuthContextType } from '@/hooks/useAuth';"), "{}", fixed_hook_type_imp);
        assert!(fixed_hook_type_imp.contains("import { useAuth } from '@/hooks/useAuth';"), "{}", fixed_hook_type_imp);

        let missing_zod_res = "\"use client\";\nimport { useForm } from 'react-hook-form';\nexport default function F() { useForm({ resolver: zodResolver({} as any) }); return null; }\n";
        let fixed_zod_res = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(missing_zod_res, "src/components/LoginForm.tsx");
        assert!(fixed_zod_res.contains("import { zodResolver } from '@hookform/resolvers/zod';"), "{}", fixed_zod_res);

        // 19. Mobile Iteration #1 fixes: `has_named_export_symbol` respects `as` alias + widen ChangeEvent<HTMLInputElement> with <select>
        let aliased_hook = "const useDefects = () => ({ data: [] });\nexport default useDefects;\nexport { useDefects as useSiteInspections };\n";
        assert!(!crate::sanitizer::has_named_export_symbol(aliased_hook, "useDefects"));
        assert!(crate::sanitizer::has_named_export_symbol(aliased_hook, "useSiteInspections"));
        let dual_fixed = crate::sanitizer::post_process_generated_file("src/hooks/useDefects.ts", aliased_hook, &serde_json::json!({}), true);
        assert!(crate::sanitizer::has_named_export_symbol(&dual_fixed, "useDefects"), "Must export unaliased `useDefects`:\n{}", dual_fixed);

        let select_form = "\"use client\";\nconst P = () => { const onC = (e: React.ChangeEvent<HTMLInputElement>) => e.target.value; return <form><input onChange={onC} /><select onChange={onC}><option>A</option></select></form>; };\nexport default P;\n";
        let fixed_select_form = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(select_form, "src/app/defects/page.tsx");
        assert!(fixed_select_form.contains("ChangeEvent<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>"), "{}", fixed_select_form);

        // 20. Mobile Iteration #3 fixes: runtime `export const` for type-only Prisma enums in `src/types/index.ts` + optional enum fields under strict:false
        let m3_dir = temp_dir.join("mobile3_enum_check");
        std::fs::create_dir_all(m3_dir.join("prisma")).unwrap();
        std::fs::create_dir_all(m3_dir.join("src/types")).unwrap();
        std::fs::create_dir_all(m3_dir.join("src/hooks")).unwrap();
        std::fs::write(
            m3_dir.join("tsconfig.json"),
            r#"{"compilerOptions":{"strict":false}}"#,
        )
        .unwrap();
        std::fs::write(
            m3_dir.join("prisma/schema.prisma"),
            "enum Role {\n  USER\n  COACH\n}\nenum PainLevel {\n  NONE\n  MILD\n  MODERATE\n  SEVERE\n}\n",
        )
        .unwrap();
        std::fs::write(
            m3_dir.join("src/types/index.ts"),
            "type Role = 'USER' | 'COACH';\nexport type PainLevel = 'NONE' | 'MILD' | 'MODERATE' | 'SEVERE';\nexport interface PainEntry {\n  id: string;\n  level: PainLevel;\n}\n",
        )
        .unwrap();
        std::fs::write(
            m3_dir.join("src/hooks/usePainLogs.ts"),
            "export interface PainEntry {\n  id?: any;\n  painLevel: PainLevel | string;\n  [key: string]: any;\n}\n",
        )
        .unwrap();
        let enum_updates = crate::self_healing::reconcile_misimported_prisma_enums_in_project(&m3_dir);
        assert!(enum_updates >= 1);
        let types_after = std::fs::read_to_string(m3_dir.join("src/types/index.ts")).unwrap();
        assert!(types_after.contains("export const PainLevel:"), "Must emit runtime const PainLevel:\n{}", types_after);
        assert!(types_after.contains("export type Role ="), "Must promote unexported type Role:\n{}", types_after);
        assert!(types_after.contains("export const Role:"), "Must emit runtime const Role:\n{}", types_after);
        // Must be idempotent on second run
        assert_eq!(crate::self_healing::reconcile_misimported_prisma_enums_in_project(&m3_dir), 0);

        let _ = crate::ast_engine::reconcile_model_and_type_contracts_in_project(&m3_dir);
        let hook_after = std::fs::read_to_string(m3_dir.join("src/hooks/usePainLogs.ts")).unwrap();
        assert!(hook_after.contains("painLevel?: PainLevel | string"), "Enum union field must be optional under strict:false:\n{}", hook_after);
        let types_after_ast = std::fs::read_to_string(m3_dir.join("src/types/index.ts")).unwrap();
        assert!(types_after_ast.contains("level?: PainLevel"), "Enum field must be optional under strict:false:\n{}", types_after_ast);

        // 21. Mobile Iteration #4 fixes: App Router route.ts forbids `export default` + register requires `POST`; NextUI `<Link size="lg">` prop strip; `@/types` auto-import for API routes + ambiguous client types
        let bad_register_route = "import NextAuth from 'next-auth/next';\nexport default NextAuth({ providers: [] });\n";
        assert!(rt.block_on(crate::orchestrator::validate_file_with_project_context(&temp_dir, "src/app/api/register/route.ts", bad_register_route, "qwen")).is_err());
        let good_register_route = "import { NextRequest, NextResponse } from 'next/server';\nexport async function POST(req: NextRequest) { return NextResponse.json({ ok: true }, { status: 201 }); }\n";
        assert!(rt.block_on(crate::orchestrator::validate_file_with_project_context(&temp_dir, "src/app/api/register/route.ts", good_register_route, "qwen")).is_ok());

        let nextui_sized_link = "'use client';\nimport { Link } from '@nextui-org/link';\nexport default function Nav() { return <Link href=\"/\" size=\"lg\" color=\"primary\" isExternal>Home</Link>; }\n";
        let fixed_sized_link = crate::sanitizer::post_process_generated_file("src/components/Navbar.tsx", nextui_sized_link, &serde_json::json!({}), true);
        assert!(fixed_sized_link.contains("import Link from 'next/link';"), "{}", fixed_sized_link);
        assert!(!fixed_sized_link.contains("size=\"lg\"") && !fixed_sized_link.contains("isExternal"), "Must strip NextUI-only Link props:\n{}", fixed_sized_link);

        let m4_dir = temp_dir.join("mobile4_autoimp_check");
        std::fs::create_dir_all(m4_dir.join("src/types")).unwrap();
        std::fs::create_dir_all(m4_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(m4_dir.join("src/app/api/earnings")).unwrap();
        std::fs::write(
            m4_dir.join("src/types/index.ts"),
            "export interface ShiftSummaryCreateInput { userId?: string; }\nexport type Stop = any;\n",
        )
        .unwrap();
        std::fs::write(
            m4_dir.join("src/hooks/useStops.ts"),
            "export interface Stop { id?: string; }\nexport const useStops = () => ({ stops: [] });\n",
        )
        .unwrap();
        let api_src = "import { NextRequest, NextResponse } from 'next/server';\nexport async function POST(req: NextRequest) { const parsed: ShiftSummaryCreateInput = await req.json(); return NextResponse.json(parsed); }\n";
        let api_fixed = crate::self_healing::auto_import_missing_symbols_in_content(
            &m4_dir,
            &m4_dir.join("src/app/api/earnings/route.ts"),
            "src/app/api/earnings/route.ts",
            api_src,
            &[],
        )
        .unwrap();
        assert!(api_fixed.contains("import type { ShiftSummaryCreateInput } from '@/types';"), "{}", api_fixed);

        let page_src = "\"use client\";\nimport { useState } from 'react';\nexport default function Home() { const [stops] = useState<Stop[]>([]); return <div>{stops.length}</div>; }\n";
        let page_fixed = crate::self_healing::auto_import_missing_symbols_in_content(
            &m4_dir,
            &m4_dir.join("src/app/page.tsx"),
            "src/app/page.tsx",
            page_src,
            &[],
        )
        .unwrap();
        assert!(page_fixed.contains("Stop") && page_fixed.contains("from '@/types'"), "Must disambiguate Stop via @/types:\n{}", page_fixed);

        // 22. Mobile Iteration #5 fixes: invalid npm package names, `EINVALIDPACKAGENAME` healer, `@stripe/stripe-js` clamp, and `src/pages/api/**/route.ts` -> `src/app/api/**/route.ts`
        assert!(!crate::packages::is_valid_npm_package_name("@eslint-config-next"));
        assert!(crate::packages::is_valid_npm_package_name("@stripe/stripe-js"));
        assert!(crate::packages::is_valid_npm_package_name("next-auth"));
        assert!(!crate::packages::is_valid_npm_package_name("Bad Package"));

        let mut bad_pkg = serde_json::json!({
            "dependencies": {
                "@stripe/stripe-js": "^17.5.0",
                "next": "14.2.35"
            },
            "devDependencies": {
                "@eslint-config-next": "latest",
                "typescript": "^5.3.3"
            }
        });
        assert!(crate::packages::sanitize_package_json_value_for_offline(&mut bad_pkg));
        assert!(bad_pkg["devDependencies"].get("@eslint-config-next").is_none(), "{:?}", bad_pkg);
        assert_eq!(bad_pkg["dependencies"]["@stripe/stripe-js"], "^4.10.0");

        let invalid_name_action = crate::packages::analyze_npm_install_failure(
            "npm error code EINVALIDPACKAGENAME\nnpm error Invalid package name \"@eslint-config-next\" of package \"@eslint-config-next@latest\"\n",
        );
        assert!(matches!(invalid_name_action, Some(crate::packages::NpmFailureAction::Remove(ref p)) if p == "@eslint-config-next"));

        let mut manifest_files = vec![
            serde_json::json!({"path": "src/app/layout.tsx", "description": "Root layout"}),
            serde_json::json!({"path": "src/pages/api/batches/[id]/route.ts", "description": "Batches detail API route with full CRUD handler"}),
            serde_json::json!({"path": "src/app/api/batches/[id]/route.ts", "description": "Short"}),
        ];
        crate::sanitizer::deduplicate_manifest_files(&mut manifest_files);
        assert_eq!(manifest_files.len(), 2, "{:?}", manifest_files);
        assert!(!manifest_files.iter().any(|f| f["path"].as_str().unwrap_or("").contains("pages/api")), "{:?}", manifest_files);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
