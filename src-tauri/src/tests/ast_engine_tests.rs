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
}
