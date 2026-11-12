use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStage {
    ManifestPlanner,
    PromptAssembly,
    PreSaveReviewer,
    DeterministicSanitizer,
    AutoHealVerifier,
    KatastarMemory,
}

impl AgentStage {
    pub fn id(&self) -> &'static str {
        match self {
            AgentStage::ManifestPlanner => "manifest_planner",
            AgentStage::PromptAssembly => "prompt_assembly",
            AgentStage::PreSaveReviewer => "pre_save_reviewer",
            AgentStage::DeterministicSanitizer => "deterministic_sanitizer",
            AgentStage::AutoHealVerifier => "auto_heal_verifier",
            AgentStage::KatastarMemory => "katastar_memory",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            AgentStage::ManifestPlanner => "Manifest & Phase Planner",
            AgentStage::PromptAssembly => "Prompt & Context Assembly",
            AgentStage::PreSaveReviewer => "Pre-Save Structural Reviewer",
            AgentStage::DeterministicSanitizer => "Deterministic Sanitizer",
            AgentStage::AutoHealVerifier => "Multi-Tier Auto-Heal & Verifier",
            AgentStage::KatastarMemory => "Katastar Episodic Memory",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRule {
    pub id: &'static str,
    pub domain: &'static str,
    pub title: &'static str,
    pub severity: &'static str,
    pub agent_stage: AgentStage,
    pub applies_to_roles: &'static [&'static str],
    pub prompt_directive: &'static str,
    pub validator_summary: &'static str,
    pub sanitizer_fn_name: &'static str,
    pub heal_directive: &'static str,
    pub katastar_signatures: &'static [&'static str],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineAgentNode {
    pub id: &'static str,
    pub name: &'static str,
    pub stage_order: u8,
    pub module_path: &'static str,
    pub description: &'static str,
    pub rule_ids: Vec<&'static str>,
}

pub const AXIOM_POLICY_REGISTRY: &[PolicyRule] = &[
    PolicyRule {
        id: "manifest.required-auth-routes",
        domain: "Navigation & Auth",
        title: "Mandatory App Router Auth Pages (/login, /register)",
        severity: "error",
        agent_stage: AgentStage::ManifestPlanner,
        applies_to_roles: &["manifest", "route", "ui", "middleware"],
        prompt_directive: "When auth, NextAuth, middleware redirects, or protected /dashboard routes exist, always generate /login (`src/app/login/page.tsx`) and /register (`src/app/register/page.tsx`) and point auth links there (NEVER broken `/dashboard/login` or `/dashboard/register`).",
        validator_summary: "Verifies that auth-enabled Next.js manifests contain both `/login` and `/register` page routes matching middleware redirect targets.",
        sanitizer_fn_name: "orchestrator::ensure_nextjs_required_auth_pages",
        heal_directive: "Create missing App Router auth pages at `src/app/login/page.tsx` and `src/app/register/page.tsx` and rewrite any `/dashboard/login` or `/dashboard/register` links.",
        katastar_signatures: &["/login", "/register", "404", "NEXT_NOT_FOUND"],
    },
    PolicyRule {
        id: "middleware.matcher-exclusions",
        domain: "Middleware & Edge",
        title: "Matcher-Layer Static Exclusion Without Redundant Branches",
        severity: "warning",
        agent_stage: AgentStage::PreSaveReviewer,
        applies_to_roles: &["middleware"],
        prompt_directive: "Exclude static assets, `_next/static`, `_next/image`, `favicon.ico`, and `/api` in `export const config = { matcher: ['/((?!api|_next/static|_next/image|_vercel|favicon\\\\.ico|robots\\\\.txt|sitemap\\\\.xml|.*\\\\..*).*)'] }`. NEVER write redundant inline `if (pathname.startsWith('/_next/static'))` checks inside `middleware()`.",
        validator_summary: "Parses `config.matcher` negative-lookahead regex and flags unreachable inline `pathname.startsWith(...)` checks inside `middleware()`.",
        sanitizer_fn_name: "reviewer::sanitize_middleware_principled",
        heal_directive: "Remove unreachable `if (...)` branches inside `middleware()` already excluded by `config.matcher` negative lookahead.",
        katastar_signatures: &["middleware", "unreachable-matcher-branch", "matcher"],
    },
    PolicyRule {
        id: "middleware.edge-safe-runtime",
        domain: "Middleware & Edge",
        title: "Edge Runtime Purity & Redirect-Loop Prevention",
        severity: "error",
        agent_stage: AgentStage::PreSaveReviewer,
        applies_to_roles: &["middleware"],
        prompt_directive: "Middleware runs in Edge Runtime: use ONLY `NextRequest`, `NextResponse` (`next/server`), and `getToken` (`next-auth/jwt`). NEVER import `@prisma/client`, `bcryptjs`, `fs`, or Express, and guard `/login`, `/maintenance`, or `/waitlist` redirects to prevent loops.",
        validator_summary: "Rejects Node/DB imports and unguarded self-redirects inside Edge middleware.",
        sanitizer_fn_name: "sanitizer::sanitize_middleware_redundancy",
        heal_directive: "Replace Node/Prisma imports in `middleware.ts` with JWT cookie token verification via `next-auth/jwt` and exclude the redirect destination path.",
        katastar_signatures: &["edge-unsafe-import", "redirect-loop", "Edge Runtime"],
    },
    PolicyRule {
        id: "auth.nextauth-config-export",
        domain: "Auth & NextAuth",
        title: "Isolated authOptions & NextAuth v4 Provider Imports",
        severity: "error",
        agent_stage: AgentStage::PromptAssembly,
        applies_to_roles: &["auth", "api"],
        prompt_directive: "Export shared `authOptions` ONLY from `@/config/auth` or `@/lib/auth`. NEVER import `authOptions` from `@/app/api/auth/[...nextauth]/route`. Import `CredentialsProvider` from `'next-auth/providers/credentials'`.",
        validator_summary: "Blocks route-to-route imports targeting `/api/auth/[...nextauth]/route` and validates NextAuth v4 provider paths.",
        sanitizer_fn_name: "sanitizer::sanitize_nextauth_imports",
        heal_directive: "Move `authOptions` to `@/config/auth` (or `@/lib/auth`), import it from there, and keep only `{ handler as GET, handler as POST }` exported from `[...nextauth]/route.ts`.",
        katastar_signatures: &["authOptions", "CredentialsProvider", "next-auth"],
    },
    PolicyRule {
        id: "api.app-router-boundary",
        domain: "API Routes",
        title: "Next.js App Router Web Request/Response Boundary",
        severity: "error",
        agent_stage: AgentStage::PreSaveReviewer,
        applies_to_roles: &["api"],
        prompt_directive: "Use Web/App Router handler signatures (`export async function POST(request: NextRequest)`) returning `NextResponse.json(...)`. NEVER use `NextApiRequest`, `NextApiResponse`, or synchronous `request.body`.",
        validator_summary: "Rejects Pages Router types (`NextApiRequest`/`NextApiResponse`), `request.body`, and illegal non-HTTP exports in `src/app/api/**/route.ts`.",
        sanitizer_fn_name: "sanitizer::sanitize_app_router_route_exports",
        heal_directive: "Convert handler to `(request: NextRequest)` + `await request.json()` + `NextResponse.json(...)` and strip non-HTTP helper exports.",
        katastar_signatures: &["NextApiRequest", "NextApiResponse", "app-router-boundary", "req-body"],
    },
    PolicyRule {
        id: "api.try-catch-body-scope",
        domain: "API Routes",
        title: "Hoisted Request Body Parsing Before Outer Try/Catch",
        severity: "error",
        agent_stage: AgentStage::DeterministicSanitizer,
        applies_to_roles: &["api"],
        prompt_directive: "If `catch` logs or falls back using parsed request data, declare `let body: any = {}; try { body = await request.json(); } catch {}` BEFORE the outer `try { ... } catch`. NEVER declare `const body = await request.json()` inside `try` and reference `body` inside `catch` (TS2304).",
        validator_summary: "Detects `try`-scoped `request.json()` bindings referenced inside the paired `catch` block (`TS2304: Cannot find name 'body'`).",
        sanitizer_fn_name: "sanitizer::sanitize_try_catch_body_scope",
        heal_directive: "Hoist `let body: any = {};` above the `try` block and assign `body = await request.json();` inside `try` so `catch` can safely access `body`.",
        katastar_signatures: &["Cannot find name 'body'", "TS2304", "try-catch-body-scope"],
    },
    PolicyRule {
        id: "payments.stripe-sdk-compatibility",
        domain: "Payments & Stripe",
        title: "Modern Stripe SDK (^17.x) & Compatible API Version",
        severity: "error",
        agent_stage: AgentStage::DeterministicSanitizer,
        applies_to_roles: &["package", "api", "payments"],
        prompt_directive: "Use modern `stripe` SDK (`^17.5.0`) and initialize `new Stripe(process.env.STRIPE_SECRET_KEY || 'sk_test_preview', { typescript: true })` WITHOUT stale hardcoded `apiVersion` dates. Keep Checkout/PaymentIntent payloads compliant with modern Stripe v17 types.",
        validator_summary: "Upgrades legacy `< 12` Stripe npm pins to `^17.5.0` and removes stale hardcoded `apiVersion` literal mismatches.",
        sanitizer_fn_name: "sanitizer::sanitize_stripe_client_init",
        heal_directive: "Upgrade `stripe` to `^17.5.0` in `package.json` and remove outdated hardcoded `apiVersion: '202X-XX-XX'` literals from `new Stripe(...)`.",
        katastar_signatures: &["apiVersion", "Stripe", "2020-08-27", "TS2322", "TS2353"],
    },
    PolicyRule {
        id: "ui.root-layout-children-shell",
        domain: "UI & Layouts",
        title: "RootLayout Shell-Only & Mandatory {children} Slot",
        severity: "error",
        agent_stage: AgentStage::DeterministicSanitizer,
        applies_to_roles: &["layout", "ui"],
        prompt_directive: "`src/app/layout.tsx` MUST return `<html><body>...{children}...</body></html>` with `<Navbar />` / `<Footer />` outside `<main>`. NEVER render KPI cards, tables, or domain hooks in RootLayout.",
        validator_summary: "Ensures RootLayout renders `{children}`, hoists `'use client'` cleanly, and forbids leaked page/table markup.",
        sanitizer_fn_name: "sanitizer::sanitize_root_layout_missing_children",
        heal_directive: "Preserve `<html><body>...<main>{children}</main>...</body></html>` in `layout.tsx` and move KPI/domain components into `page.tsx`.",
        katastar_signatures: &["RootLayout", "children", "layout.tsx"],
    },
    PolicyRule {
        id: "ui.format-jsx-in-plain-ts",
        domain: "UI & TypeScript",
        title: "No Raw JSX Markup Inside Plain .ts Modules",
        severity: "error",
        agent_stage: AgentStage::PreSaveReviewer,
        applies_to_roles: &["hook", "types", "lib", "config"],
        prompt_directive: "Plain `.ts` files (including `src/config/*`, `src/hooks/*`, `src/lib/*`) MUST NOT contain raw JSX (`<Tag>...</Tag>`). Return strings/objects or use `React.createElement(...)`.",
        validator_summary: "Flags TS1005/TS1109/TS1161 caused by JSX markup inside `.ts` files and rewrites it to `React.createElement(...)`.",
        sanitizer_fn_name: "sanitizer::sanitize_jsx_in_ts_file",
        heal_directive: "Convert all `<Tag>...</Tag>` JSX expressions in `.ts` files to `React.createElement(...)` calls or pure TS objects.",
        katastar_signatures: &["TS1005", "TS1109", "TS1161", "Unterminated regular expression literal", "jsx-in-plain-ts"],
    },
    PolicyRule {
        id: "ui.primitives-toast-and-enums",
        domain: "UI & Prisma Client Boundary",
        title: "UI Primitive Synthesis (toast/TableHeaderCell) & Client Enum Mocks",
        severity: "error",
        agent_stage: AgentStage::DeterministicSanitizer,
        applies_to_roles: &["ui", "client-page", "hook"],
        prompt_directive: "Import UI primitives (`@/components/ui`, `@/components/ui/use-toast`) consistently, keep `toast`/`useToast` signatures compatible, and NEVER import runtime `@prisma/client` inside `'use client'` files (match Prisma enum literals cleanly).",
        validator_summary: "Synthesizes missing UI primitives including `toast`, `useToast`, `Toast`, `Toaster`, `TableHeaderCell`, and aligns client mock enum strings with `prisma/schema.prisma`.",
        sanitizer_fn_name: "sanitizer::reconcile_client_prisma_enum_mocks",
        heal_directive: "Synthesize missing UI primitives in `src/components/ui*` and cast client mock literals to uppercase Prisma schema enum names without importing runtime `@prisma/client`.",
        katastar_signatures: &["use-toast", "TableHeaderCell", "is not assignable to type", "toast"],
    },
    PolicyRule {
        id: "ui.ux-currency-and-auth-cta",
        domain: "UI & UX Hygiene",
        title: "Double-Currency Prevention & Auth Link Normalization",
        severity: "warning",
        agent_stage: AgentStage::DeterministicSanitizer,
        applies_to_roles: &["ui", "page"],
        prompt_directive: "Store numeric prices or format money via a single currency guard (NEVER `$` + `'$50'` -> `$$50`), and keep auth CTA links pointing directly to `/login` and `/register`.",
        validator_summary: "Normalizes `${val}` JSX interpolation against string-prefixed `$50` mocks and rewrites `/dashboard/login` & `/dashboard/register` hrefs.",
        sanitizer_fn_name: "sanitizer::sanitize_tsx_ux_glitches",
        heal_directive: "Guard `${expr}` currency prefixing against already-formatted `'$50'` strings and point Sign In / Sign Up links to `/login` and `/register`.",
        katastar_signatures: &["$$", "/dashboard/login", "/dashboard/register"],
    },
    PolicyRule {
        id: "css.tailwind-apply-and-braces",
        domain: "CSS & Tailwind",
        title: "Tailwind @apply Validity & Balanced CSS Block Delimiters",
        severity: "error",
        agent_stage: AgentStage::DeterministicSanitizer,
        applies_to_roles: &["css"],
        prompt_directive: "Inside `@apply`, use ONLY valid Tailwind utility classes (NEVER raw CSS functions like `translateX(100%)`; write `transform: translateX(100%);` outside `@apply`). Keep `@layer` blocks non-duplicated with balanced `{}` braces.",
        validator_summary: "Converts raw CSS transform/calc functions out of `@apply`, deduplicates `@layer` blocks, and removes orphan declarations/extra `}`.",
        sanitizer_fn_name: "sanitizer::sanitize_tailwind_globals_css",
        heal_directive: "Move CSS functions (`translateX(...)`) from `@apply` to standard CSS declarations, deduplicate `@layer` blocks, and balance `{}` braces.",
        katastar_signatures: &["globals.css", "Unexpected }", "does not exist", "@apply", "translateX"],
    },
    PolicyRule {
        id: "heal.anti-stagnation-ledger",
        domain: "Self-Healing & Memory",
        title: "Stagnation-Breaker Rollback & Katastar Quality Gate",
        severity: "info",
        agent_stage: AgentStage::AutoHealVerifier,
        applies_to_roles: &["heal", "all"],
        prompt_directive: "Auto-heal tracks per-file error counts in `.axiom/heal-ledger.json`, rolls back neutral/regressive patches, injects targeted structural diagnostics on retry, and excludes generic filler hints or bogus `@types/prisma` entries from Katastar.",
        validator_summary: "Detects unchanged rollback loops (`diagnose_heal_stagnation`) and sanitizes Katastar entries & npm package resolutions.",
        sanitizer_fn_name: "reviewer::diagnose_heal_stagnation",
        heal_directive: "Change the actual failing expression/signature rather than repeating an unchanged cosmetic edit.",
        katastar_signatures: &["rolled_back", "unchanged_after_postprocess", "npm:E404:@types/prisma"],
    },
];

pub fn all_policies() -> &'static [PolicyRule] {
    AXIOM_POLICY_REGISTRY
}

fn infer_file_roles(file_path: &str, has_auth: bool, has_stripe: bool) -> Vec<&'static str> {
    let low = file_path.replace('\\', "/").to_ascii_lowercase();
    let mut roles = Vec::new();
    if low.ends_with(".css") || low.ends_with(".scss") {
        roles.push("css");
        return roles;
    }
    if low == "package.json" || low.ends_with("/package.json") {
        roles.push("package");
        if has_stripe {
            roles.push("payments");
        }
        return roles;
    }
    if low.ends_with("middleware.ts") || low.ends_with("middleware.js") {
        roles.push("middleware");
        if has_auth {
            roles.push("auth");
        }
        return roles;
    }
    if low.contains("/api/") || low.ends_with("/route.ts") || low.ends_with("/route.js") {
        roles.push("api");
        if has_auth || low.contains("auth") || low.contains("register") || low.contains("login") {
            roles.push("auth");
        }
        if has_stripe || low.contains("stripe") || low.contains("payment") || low.contains("checkout") || low.contains("webhook") {
            roles.push("payments");
        }
        return roles;
    }
    if low.ends_with("layout.tsx") || low.ends_with("layout.jsx") {
        roles.push("layout");
        roles.push("ui");
        return roles;
    }
    if low.contains("/hooks/") || low.starts_with("hooks/") {
        roles.push("hook");
        return roles;
    }
    if low.contains("/types/") || low.starts_with("types/") || low.contains("/models/") || low.starts_with("models/") {
        roles.push("types");
        return roles;
    }
    if low.contains("/config/") || low.starts_with("config/") || low.contains("/lib/") || low.starts_with("lib/") {
        roles.push("config");
        roles.push("lib");
        if low.contains("auth") {
            roles.push("auth");
        }
        if has_stripe || low.contains("stripe") {
            roles.push("payments");
        }
        return roles;
    }
    if low.ends_with(".tsx") || low.ends_with(".jsx") {
        roles.push("ui");
        if low.ends_with("page.tsx") || low.ends_with("page.jsx") {
            roles.push("page");
        }
        roles.push("client-page");
        if has_auth {
            roles.push("auth");
        }
        return roles;
    }
    roles
}

/// Returns concise, scoped `[ruleId]` prompt directives for a single file from the central PolicyRegistry.
pub fn prompt_directives_for_file(file_path: &str, has_auth: bool, has_stripe: bool) -> String {
    let roles = infer_file_roles(file_path, has_auth, has_stripe);
    if roles.is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = Vec::new();
    for rule in AXIOM_POLICY_REGISTRY {
        if rule.agent_stage == AgentStage::AutoHealVerifier && !roles.contains(&"all") {
            continue;
        }
        if rule.applies_to_roles.iter().any(|r| roles.contains(r)) {
            lines.push(format!("- [{}] {}", rule.id, rule.prompt_directive));
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "CANONICAL AXIOM POLICY REGISTRY DIRECTIVES:\n{}\n",
        lines.join("\n")
    )
}

pub fn pipeline_agents() -> Vec<PipelineAgentNode> {
    let stages = [
        (
            AgentStage::ManifestPlanner,
            1,
            "src-tauri/src/orchestrator.rs",
            "Plans dependency-ordered build phases and guarantees required App Router auth & landing routes.",
        ),
        (
            AgentStage::PromptAssembly,
            2,
            "src-tauri/src/policy_registry.rs + orchestrator.rs",
            "Compiles scoped file prompts, LBG database/API contracts, and canonical PolicyRegistry rules.",
        ),
        (
            AgentStage::PreSaveReviewer,
            3,
            "src-tauri/src/reviewer.rs",
            "Performs token-aware brace balance, Edge middleware matcher verification, and route boundary audits before saving.",
        ),
        (
            AgentStage::DeterministicSanitizer,
            4,
            "src-tauri/src/sanitizer.rs + packages.rs",
            "Applies zero-LLM deterministic repairs (try/catch hoisting, Stripe SDK compatibility, CSS @apply, client enum mocks).",
        ),
        (
            AgentStage::AutoHealVerifier,
            5,
            "src-tauri/src/self_healing.rs + healer.rs",
            "Runs Tier 0 hygiene, Tier 1 `tsc --noEmit` prioritized repair with rollback/stagnation breaker, and Tier 2 `next build`.",
        ),
        (
            AgentStage::KatastarMemory,
            6,
            "src-tauri/src/katastar.rs",
            "Records two-phase verified fixes, tracks ruleId error frequencies, and curates SFT fine-tuning datasets.",
        ),
    ];

    stages
        .into_iter()
        .map(|(stage, order, mod_path, desc)| {
            let rule_ids = AXIOM_POLICY_REGISTRY
                .iter()
                .filter(|r| r.agent_stage == stage)
                .map(|r| r.id)
                .collect();
            PipelineAgentNode {
                id: stage.id(),
                name: stage.display_name(),
                stage_order: order,
                module_path: mod_path,
                description: desc,
                rule_ids,
            }
        })
        .collect()
}

pub fn build_policy_registry_snapshot_at(katastar_path: &Path) -> Value {
    #[derive(Default)]
    struct RuleMetrics {
        matches: u64,
        verified_fixes: u64,
        failed_fixes: u64,
        occurrences: u64,
    }

    let mut per_rule: BTreeMap<&'static str, RuleMetrics> = BTreeMap::new();
    for rule in AXIOM_POLICY_REGISTRY {
        per_rule.insert(rule.id, RuleMetrics::default());
    }

    let katastar_val: Option<Value> = std::fs::read_to_string(katastar_path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());

    if let Some(entries) = katastar_val.as_ref().and_then(|v| v.get("entries")).and_then(|e| e.as_array()) {
        for entry in entries {
            let sig = entry
                .get("error_signature")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let cat = entry.get("category").and_then(|v| v.as_str()).unwrap_or("");
            let hint = entry
                .get("recommended_fix_hint")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let explicit_rule_id = entry.get("rule_id").and_then(|v| v.as_str()).unwrap_or("");
            let haystack = format!("{} {} {}", sig, cat, hint).to_ascii_lowercase();
            let occ = entry.get("occurrences").and_then(|v| v.as_u64()).unwrap_or(1);
            let verified = entry.get("verified").and_then(|v| v.as_bool()).unwrap_or(false);
            let fail_cnt = entry.get("fail_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let success_cnt = entry.get("success_count").and_then(|v| v.as_u64()).unwrap_or(0);

            for rule in AXIOM_POLICY_REGISTRY {
                let is_match = (!explicit_rule_id.is_empty() && explicit_rule_id == rule.id)
                    || rule
                        .katastar_signatures
                        .iter()
                        .any(|pat| haystack.contains(&pat.to_ascii_lowercase()));
                if is_match {
                    if let Some(m) = per_rule.get_mut(rule.id) {
                        m.matches += 1;
                        m.occurrences += occ;
                        if verified || success_cnt > 0 {
                            m.verified_fixes += success_cnt.max(if verified { 1 } else { 0 });
                        }
                        m.failed_fixes += fail_cnt;
                    }
                }
            }
        }
    }

    let rules_json: Vec<Value> = AXIOM_POLICY_REGISTRY
        .iter()
        .map(|rule| {
            let m = per_rule.get(rule.id);
            let matches = m.map_or(0, |x| x.matches);
            let verified = m.map_or(0, |x| x.verified_fixes);
            let failed = m.map_or(0, |x| x.failed_fixes);
            let occurrences = m.map_or(0, |x| x.occurrences);
            let total_outcomes = verified + failed;
            let success_rate_pct = if total_outcomes > 0 {
                ((verified as f64 / total_outcomes as f64) * 100.0 * 10.0).round() / 10.0
            } else {
                0.0
            };
            json!({
                "id": rule.id,
                "domain": rule.domain,
                "title": rule.title,
                "severity": rule.severity,
                "agentStage": rule.agent_stage.id(),
                "agentStageLabel": rule.agent_stage.display_name(),
                "appliesToRoles": rule.applies_to_roles,
                "promptDirective": rule.prompt_directive,
                "validatorSummary": rule.validator_summary,
                "sanitizerFnName": rule.sanitizer_fn_name,
                "healDirective": rule.heal_directive,
                "katastarSignatures": rule.katastar_signatures,
                "katastarMatches": matches,
                "occurrences": occurrences,
                "verifiedFixes": verified,
                "failedFixes": failed,
                "successRatePct": success_rate_pct,
            })
        })
        .collect();

    let agents = pipeline_agents();
    let agents_json: Vec<Value> = agents
        .iter()
        .map(|agent| {
            let mut agent_matches = 0u64;
            let mut agent_verified = 0u64;
            for rid in &agent.rule_ids {
                if let Some(m) = per_rule.get(rid) {
                    agent_matches += m.matches;
                    agent_verified += m.verified_fixes;
                }
            }
            json!({
                "id": agent.id,
                "name": agent.name,
                "stageOrder": agent.stage_order,
                "modulePath": agent.module_path,
                "description": agent.description,
                "ruleIds": agent.rule_ids,
                "ruleCount": agent.rule_ids.len(),
                "katastarMatches": agent_matches,
                "verifiedFixes": agent_verified,
            })
        })
        .collect();

    json!({
        "totalRules": AXIOM_POLICY_REGISTRY.len(),
        "totalAgents": agents.len(),
        "agents": agents_json,
        "rules": rules_json,
    })
}

#[tauri::command]
pub fn get_policy_registry() -> Result<Value, String> {
    Ok(build_policy_registry_snapshot_at(&crate::katastar::get_katastar_path()))
}
