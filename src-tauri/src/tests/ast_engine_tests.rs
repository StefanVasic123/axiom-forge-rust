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
        assert!(model_out.contains("status: Status | string;"), "{}", model_out);
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
        assert!(crate::sanitizer::is_banned_external_ui_package("formik"));
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
}
