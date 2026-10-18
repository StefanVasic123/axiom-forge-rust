use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GenerationPhase {
    Config = 1,
    DatabaseOrMain = 2,
    ApiOrState = 3,
    LogicOrHooks = 4,
    ShellOrWidgets = 5,
    PagesOrUi = 6,
}

pub fn get_generation_phase(file_path: &str, project_type: &str) -> GenerationPhase {
    let path_lower = file_path.to_lowercase().replace('\\', "/");
    
    match project_type {
        "nextjs" | "mern" => {
            if path_lower == "package.json" || path_lower == "tsconfig.json" || 
               path_lower == "tailwind.config.js" || path_lower == "postcss.config.js" || 
               path_lower == "next.config.js" || path_lower.contains("config") || path_lower.ends_with(".json") {
                GenerationPhase::Config
            } else if path_lower.contains("schema.prisma") || path_lower.contains("prisma") || path_lower.contains("models/") || path_lower.contains("database") {
                GenerationPhase::DatabaseOrMain
            } else if path_lower.contains("hooks/") || path_lower.contains("utils/") || path_lower.contains("lib/") {
                GenerationPhase::LogicOrHooks
            } else if path_lower.contains("api/") || path_lower.contains("auth") || path_lower.contains("router") {
                GenerationPhase::ApiOrState
            } else if path_lower.contains("layout.tsx") || path_lower.contains("layout.jsx") || path_lower.contains("layout.js") || 
                      path_lower.contains("globals.css") || path_lower.contains("styles/") || path_lower.contains("navigation") {
                GenerationPhase::ShellOrWidgets
            } else {
                GenerationPhase::PagesOrUi
            }
        }
        "electron" | "rust" => {
            if path_lower == "package.json" || path_lower == "cargo.toml" || path_lower.contains("config") {
                GenerationPhase::Config
            } else if path_lower.contains("main.js") || path_lower.contains("main.ts") || path_lower.contains("preload.js") || 
                      path_lower.contains("main.rs") || path_lower.contains("commands") {
                GenerationPhase::DatabaseOrMain
            } else {
                GenerationPhase::PagesOrUi
            }
        }
        "flutter" => {
            if path_lower.contains("pubspec.yaml") || path_lower.contains("config") {
                GenerationPhase::Config
            } else if path_lower.contains("models/") || path_lower.contains("sqlite") || path_lower.contains("db") || 
                      path_lower.contains("api") || path_lower.contains("bloc") || path_lower.contains("provider") {
                GenerationPhase::DatabaseOrMain
            } else {
                GenerationPhase::PagesOrUi
            }
        }
        _ => {
            if path_lower.contains("package.json") || path_lower.contains("cargo.toml") || path_lower.ends_with(".json") {
                GenerationPhase::Config
            } else if path_lower.contains("server") || path_lower.contains("db") || path_lower.contains("api") || path_lower.contains("lib") {
                GenerationPhase::DatabaseOrMain
            } else {
                GenerationPhase::PagesOrUi
            }
        }
    }
}

pub fn get_context_files_for(file_path: &str, proj_dir: &std::path::Path) -> String {
    let mut context = String::new();
    
    let prisma_path = proj_dir.join("prisma/schema.prisma");
    let prisma_path_alt = proj_dir.join("schema.prisma");
    let actual_prisma = if prisma_path.exists() { Some(prisma_path) } else if prisma_path_alt.exists() { Some(prisma_path_alt) } else { None };
    
    if let Some(p_path) = actual_prisma {
        if file_path != "prisma/schema.prisma" && file_path != "schema.prisma" {
            if let Ok(content) = std::fs::read_to_string(&p_path) {
                context.push_str(&format!("\nReference: Database Schema (schema.prisma):\n```prisma\n{}\n```\n", content));
            }
        }
    }
    
    let prisma_lib = proj_dir.join("src/lib/prisma.ts");
    let prisma_lib_alt = proj_dir.join("src/lib/prisma.js");
    let actual_prisma_lib = if prisma_lib.exists() { Some(prisma_lib) } else if prisma_lib_alt.exists() { Some(prisma_lib_alt) } else { None };
    if let Some(pl_path) = actual_prisma_lib {
        if !file_path.contains("prisma") {
            if let Ok(content) = std::fs::read_to_string(&pl_path) {
                context.push_str(&format!("\nReference: Prisma client export (src/lib/prisma.ts):\n```typescript\n{}\n```\n", content));
            }
        }
    }
    
    let auth_route = proj_dir.join("src/app/api/auth/[...nextauth]/route.ts");
    let auth_route_alt = proj_dir.join("src/app/api/auth/[...nextauth]/route.js");
    let actual_auth_route = if auth_route.exists() { Some(auth_route) } else if auth_route_alt.exists() { Some(auth_route_alt) } else { None };
    if let Some(ar_path) = actual_auth_route {
        if file_path.contains("hooks") || file_path.contains("components") || file_path.contains("app") {
            if let Ok(content) = std::fs::read_to_string(&ar_path) {
                context.push_str(&format!("\nReference: NextAuth configuration (src/app/api/auth/[...nextauth]/route.ts):\n```typescript\n{}\n```\n", content));
            }
        }
    }
    
    if file_path.contains("components") || (file_path.contains("app") && file_path.ends_with("page.tsx")) {
        let hooks_dir = proj_dir.join("src/hooks");
        if hooks_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(hooks_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                context.push_str(&format!("\nReference: Hook (src/hooks/{}):\n```typescript\n{}\n```\n", name, content));
                            }
                        }
                    }
                }
            }
        }
    }
    
    context
}

pub async fn validate_file_with_abstract_rules(
    file_path: &str,
    content: &str,
    model_id: &str,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
        
    let system_prompt = "You are a strict code quality validator. Analyze the provided file path and code content for critical syntax errors, truncated code, or severe architectural violations. Rules:\n\
1. ONLY root layout files (`layout.tsx` / `layout.jsx`) import `globals.css`. Regular pages (`page.tsx`) and components do NOT need to import `globals.css`.\n\
2. In Next.js projects, `src/middleware.ts` or `middleware.ts` MUST use `NextRequest` and `NextResponse` from `'next/server'` and export `function middleware(request: NextRequest)`. It must NEVER use Express (`from 'express'`, `NextFunction`, or `(req, res, next)`).\n\
3. If NextAuth is used, ensure there are no invalid imports for Prisma or NextAuth.\n\
Respond ONLY with a JSON object in this format: { \"valid\": true } or { \"valid\": false, \"reason\": \"Detailed description of the issue\" }.";
    
    let user_prompt = format!("File Path: {}\n\nCode Content:\n```\n{}\n```", file_path, content);
    
    let ollama_req = serde_json::json!({
        "model": model_id,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": &user_prompt }
        ],
        "stream": false,
        "options": build_ollama_options(model_id, 0.1, Some(512))
    });
    
    let res = client.post("http://127.0.0.1:11434/api/chat")
        .json(&ollama_req)
        .send()
        .await;
        
    if let Ok(resp) = res {
        if resp.status().is_success() {
            if let Ok(resp_json) = resp.json::<Value>().await {
                if let Some(content_str) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                    let cleaned = extract_code_content(content_str);
                    if let Ok(parsed) = serde_json::from_str::<Value>(&cleaned) {
                        if let Some(valid) = parsed.get("valid").and_then(|v| v.as_bool()) {
                            if valid {
                                return Ok(());
                            } else {
                                let reason = parsed.get("reason").and_then(|r| r.as_str()).unwrap_or("Unknown validation issue");
                                return Err(reason.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(())
}

pub fn generate_axiom_map(proj_dir: &std::path::Path) -> Result<(), String> {
    use std::fs;
    use std::path::{Path, PathBuf};
    
    let mut files_map = serde_json::Map::new();
    let mut methods_map = serde_json::Map::new();
    
    let mut source_files = Vec::new();
    
    fn scan_dir(dir: &Path, source_files: &mut Vec<PathBuf>) {
        if !dir.exists() { return; }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name != "node_modules" && name != ".next" && name != "build" && name != "dist" {
                        scan_dir(&path, source_files);
                    }
                } else if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                            source_files.push(path);
                        }
                    }
                }
            }
        }
    }
    
    scan_dir(proj_dir, &mut source_files);
    
    for file_path in &source_files {
        let Ok(content) = fs::read_to_string(file_path) else { continue; };
        let Ok(rel_path) = file_path.strip_prefix(proj_dir) else { continue; };
        let rel_path_str = rel_path.to_string_lossy().replace('\\', "/");
        
        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let mut local_deps = Vec::new();
        
        for line in content.lines() {
            let line = line.trim();
            
            if line.starts_with("import ") || line.starts_with("import{") {
                if let Some(from_pos) = line.rfind("from") {
                    let path_part = &line[from_pos + 4..].trim();
                    let cleaned = path_part.trim_matches(|c| c == '\'' || c == '"' || c == ';').trim().to_string();
                    if !cleaned.is_empty() {
                        imports.push(cleaned.clone());
                        
                        let mut resolved_dep = None;
                        if cleaned.starts_with("@/") {
                            let rel_sub = cleaned.replacen("@/", "src/", 1);
                            resolved_dep = Some(proj_dir.join(rel_sub));
                        } else if cleaned.starts_with("./") || cleaned.starts_with("../") {
                            if let Some(parent) = file_path.parent() {
                                resolved_dep = Some(parent.join(&cleaned));
                            }
                        }
                        
                        if let Some(dep_base) = resolved_dep {
                            let extensions = vec!["ts", "tsx", "js", "jsx"];
                            for ext in extensions {
                                let dep_file = dep_base.with_extension(ext);
                                if dep_file.exists() {
                                    if let Ok(dep_rel) = dep_file.strip_prefix(proj_dir) {
                                        local_deps.push(dep_rel.to_string_lossy().replace('\\', "/"));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            if line.starts_with("export ") {
                if line.contains("default") {
                    exports.push("default".to_string());
                } else {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    for (idx, &part) in parts.iter().enumerate() {
                        if (part == "const" || part == "let" || part == "var" || part == "function" || part == "type" || part == "interface" || part == "class") && idx + 1 < parts.len() {
                            let name = parts[idx + 1].trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                            if !name.is_empty() {
                                exports.push(name.to_string());
                            }
                            break;
                        }
                    }
                }
            }
            
            if (line.starts_with("export function ") && line.contains("(")) || 
               (line.starts_with("function ") && line.contains("(")) ||
               (line.contains("const ") && line.contains("=") && line.contains("=>") && line.contains("(")) {
                
                let mut method_name = String::new();
                if line.starts_with("export function ") {
                    if let Some(rest) = line.strip_prefix("export function ") {
                        method_name = rest.split('(').next().unwrap_or("").trim().to_string();
                    }
                } else if line.starts_with("function ") {
                    if let Some(rest) = line.strip_prefix("function ") {
                        method_name = rest.split('(').next().unwrap_or("").trim().to_string();
                    }
                } else if line.contains("const ") {
                    if let Some(start_idx) = line.find("const ") {
                        let after_const = &line[start_idx + 6..];
                        if let Some(eq_idx) = after_const.find('=') {
                            method_name = after_const[..eq_idx].trim().to_string();
                        }
                    }
                }
                
                method_name = method_name.trim_matches(|c: char| !c.is_alphanumeric() && c != '_').to_string();
                
                if !method_name.is_empty() && method_name != "default" {
                    let mut params = Vec::new();
                    if let Some(open_paren) = line.find('(') {
                        if let Some(close_paren) = line.find(')') {
                            let param_sub = &line[open_paren + 1..close_paren];
                            for p in param_sub.split(',') {
                                let p_trimmed = p.trim().to_string();
                                if !p_trimmed.is_empty() {
                                    params.push(p_trimmed);
                                }
                            }
                        }
                    }
                    
                    methods_map.insert(
                        method_name,
                        serde_json::json!({
                            "definedIn": rel_path_str.clone(),
                            "parameters": params,
                            "returns": "any"
                        })
                    );
                }
            }
        }
        
        files_map.insert(
            rel_path_str,
            serde_json::json!({
                "exports": exports,
                "imports": imports,
                "dependencies": local_deps
            })
        );
    }
    
    let axiom_map = serde_json::json!({
        "files": files_map,
        "methods": methods_map
    });
    
    if let Ok(pretty) = serde_json::to_string_pretty(&axiom_map) {
        let _ = fs::write(proj_dir.join("axiom-map.json"), pretty);
    }
    
    Ok(())
}

pub fn get_project_routes_prompt(files: &[Value]) -> String {
    let mut prompt = String::new();
    let mut routes = Vec::new();
    
    for file in files {
        if let Some(path_str) = file.get("path").and_then(|p| p.as_str()) {
            let path_lower = path_str.to_lowercase().replace('\\', "/");
            if path_lower.starts_with("src/app/") && path_lower.ends_with("/page.tsx") {
                let route = path_lower
                    .strip_prefix("src/app/")
                    .unwrap_or(&path_lower)
                    .strip_suffix("/page.tsx")
                    .unwrap_or("");
                if route.is_empty() {
                    routes.push(("/".to_string(), path_str.to_string()));
                } else {
                    routes.push((format!("/{}", route), path_str.to_string()));
                }
            } else if path_lower.starts_with("app/") && path_lower.ends_with("/page.tsx") {
                let route = path_lower
                    .strip_prefix("app/")
                    .unwrap_or(&path_lower)
                    .strip_suffix("/page.tsx")
                    .unwrap_or("");
                if route.is_empty() {
                    routes.push(("/".to_string(), path_str.to_string()));
                } else {
                    routes.push((format!("/{}", route), path_str.to_string()));
                }
            }
        }
    }
    
    if !routes.is_empty() {
        prompt.push_str("\nAvailable Pages/Routes in this Next.js project. You MUST use these exact paths for navigation links:\n");
        for (route, file_path) in routes {
            prompt.push_str(&format!("- Route '{}' -> maps to file '{}'\n", route, file_path));
        }
    }
    
    prompt
}

pub const NEXTJS_REACT_RULEPACK: &str = r#"REACT JSX SYNTAX, HOOKS & STYLING RULES:
1. NEVER USE STRING STYLE ATTRIBUTES: In React/JSX, the 'style' attribute MUST NEVER be a string like style="color: yellow;". Passing a string throws a fatal runtime crash ('The `style` prop expects a mapping from style properties to values, not a string'). ALWAYS use Tailwind CSS utility classes via `className="..."` (e.g., `className="text-yellow-400 font-bold"`) or, if strictly necessary, a JS style object (e.g., `style={{ color: 'yellow' }}`).
2. PREFER TAILWIND CSS CLASSES: When fulfilling styling requests (colors, typography, spacing, padding, margins, flex, grid, hover states), ALWAYS apply Tailwind CSS classes to `className`. Never write inline style strings.
3. TAILWIND DYNAMIC CLASSES: NEVER construct Tailwind classes using runtime string interpolation like className={"text-" + color + "-500"}. Tailwind's compiler statically scans source files; dynamic fragments are stripped in production builds. Always use complete class names or a static mapping object.
4. JSX ATTRIBUTE CONVENTIONS: In JSX, never use pure HTML attribute names: use `className` instead of `class`, `htmlFor` instead of `for`, and camelCase for event handlers (`onClick`, `onChange`, `onSubmit`). Void tags (<img />, <input />, <br />, <hr />, <meta />, <link />) MUST self-close with '/>'.
5. NEVER CALL HOOKS AT MODULE SCOPE: React hooks (e.g., useTranslations, useState, useEffect, useContext, useRef, useSession, useRouter, useParams, etc.) MUST ONLY be called directly inside the body of a React Function Component or a custom hook (a function starting with 'use'). NEVER invoke any hook outside of a component body or in global/module scope.
6. NO CONDITIONAL HOOKS: Hooks must be placed unconditionally at the top level of the component function. Never invoke hooks inside 'if' branches, loops, nested callbacks, or after early returns.
7. 'use client' DIRECTIVE: Any component that uses React hooks, event listeners (e.g. onClick, onChange, onSubmit), or browser APIs (window, localStorage, document) MUST have 'use client' as the very first line of the file. Server Components (default, no directive) may be async and fetch data directly.
8. SERVER-ONLY IMPORT LEAKAGE: Never import @prisma/client, fs, path, crypto, or server secrets into files containing 'use client'. Pass serialized data as props from Server Components or fetch via Route Handlers.
9. NO FAKE CONTEXT BYPASS: Never attempt to bypass a missing React Context by defining mock objects or passing illegal second arguments to context-consuming hooks in the component file. If context is missing, the component must be wrapped by its parent Provider in a layout or parent component.
10. HYDRATION INTEGRITY: NEVER generate different initial markup on server vs client using Date.now(), Math.random(), or unguarded browser-only state. Perform browser-dependent updates after mount inside useEffect to prevent hydration mismatch crashes.
11. IMAGE OPTIMIZATION: Always use `import Image from 'next/image';` with required `width`, `height`, and `alt` props. Never use unoptimized `<img>` tags for static assets.
12. DYNAMIC ROUTE PARAMS: In Next.js 14 App Router, dynamic route params are synchronous objects: type them as `{ params }: { params: { id: string } }`. Do not await params.
13. ROUTE HANDLER SIGNATURES: In `app/api/**/route.ts`, export named async functions per HTTP method (`export async function GET(request: Request) { return NextResponse.json({...}); }`). Never use Pages Router default export `handler(req, res)`. Always validate incoming bodies with Zod before database operations.
14. ENVIRONMENT VARIABLE EXPOSURE: Client components can only read variables prefixed with `NEXT_PUBLIC_` (e.g. `process.env.NEXT_PUBLIC_API_URL`). Server secrets (DATABASE_URL, JWT_SECRET) must remain unprefixed and server-only.
15. SERVER ACTIONS DIRECTIVE: Mutating server actions must have `'use server'` as the first line of their file or function body. Pass actions to `<form action={myAction}>` or invoke via import.
16. CLIENT-ONLY DYNAMIC IMPORTS: `dynamic(() => import('./Chart'), { ssr: false })` is only valid inside `'use client'` files. Never use `ssr: false` directly in Server Components.
17. NEXT.JS APP ROUTER NAVIGATION IMPORTS: In Next.js App Router (`src/app/` or `app/`), NEVER import `useRouter`, `usePathname`, or `useSearchParams` from `'next/router'`. Importing from `'next/router'` throws a fatal runtime crash: "NextRouter was not mounted". ALWAYS import from `'next/navigation'`.
18. NEXTAUTH CREDENTIALS PROVIDER IMPORT: In NextAuth configuration files, `CredentialsProvider` MUST be imported from `'next-auth/providers/credentials'`. NEVER import from `'next-auth'` directly (which imports the NextAuth initialization function instead of the credentials provider factory).
19. APP ROUTER API ISOLATION: In App Router projects, all API routes MUST live under `src/app/api/**/route.ts`. NEVER create parallel duplicate endpoints in `src/pages/api/**` (e.g. `pages/api/register.ts` alongside `app/api/register/route.ts`). Having both causes a fatal Next.js build error: "Conflicting app and page files were found".
20. REACT-ICONS PACKAGE PREFIX ACCURACY: Never mix or cross-import icon names across different families in `react-icons`. Icons with prefix `Io` or `IoMd` (e.g. `IoMdNotifications`) belong strictly in `'react-icons/io'`, `Fa` icons (e.g. `FaBars`) belong in `'react-icons/fa'`, `Md` icons in `'react-icons/md'`, `Fi` in `'react-icons/fi'`. Importing an `Io*` icon from `'react-icons/fa'` causes a fatal build compilation error.
21. PRISMA SINGLETON PATTERN: In `src/lib/prisma.ts`, `prisma` MUST be an instantiated singleton of `PrismaClient`, NOT a function or type. Always export a ready-to-use instance: `import { PrismaClient } from '@prisma/client'; const globalForPrisma = globalThis as unknown as { prisma: PrismaClient | undefined }; export const prisma = globalForPrisma.prisma ?? new PrismaClient(); if (process.env.NODE_ENV !== 'production') globalForPrisma.prisma = prisma; export default prisma;`.
22. MANDATORY DEFAULT EXPORT IN APP ROUTER (LAYOUTS & PAGES): Every file in `src/app/**/layout.tsx` (or `.jsx`) and `src/app/**/page.tsx` (or `.jsx`) MUST have an `export default function` (e.g. `export default function RootLayout({ children }: { children: React.ReactNode })`). NEVER omit `export default` or define only a local `const Layout = ...` without default export. If a file marked with 'use client' lacks a default export, the React Server Components runtime fails to resolve `module.default` through its client proxy and crashes fatally with: "Error: Cannot access default.then on the server. You cannot dot into a client module from a server component."
23. ROOT LAYOUT DOM HIERARCHY & PROVIDER PLACEMENT: In `src/app/layout.tsx`, `<html lang="en">` and `<body>` MUST be the outermost tags returned by `RootLayout`. Global Context Providers (such as `<SessionProvider>`, `<ThemeProvider>`, `<QueryClientProvider>`) MUST ALWAYS be placed INSIDE the `<body>` tag wrapping `{children}`: `return (<html lang="en"><body><SessionProvider>{children}</SessionProvider></body></html>);`. NEVER wrap any Provider outside the `<html>` tag! Wrapping `<html>` inside a Provider or component produces an invalid DOM hierarchy and causes fatal Next.js hydration and server-rendering crashes.
24. CLIENT COMPONENTS CANNOT BE ASYNC: Any component marked with 'use client' MUST NEVER be declared as an async function (e.g. `export default async function Component()`). In React 18 / Next.js 14, only Server Components can be async. Declaring an async client component causes it to return a Promise, prompting the RSC compiler to access `.then` on the client module proxy and throwing "Error: Cannot access default.then on the server". For asynchronous data fetching in client components, always use `useEffect` or standard state hooks with synchronous component signatures.
25. NEXT.JS MIDDLEWARE MATCHER SYNTAX: In `src/middleware.ts` or `src/middleware.js`, route matchers use `path-to-regexp` syntax. Wildcards MUST use named parameters (e.g., `'/dashboard/:path*'`, `'/profile/:path*'`, `'/admin/:path*'`) or regex filters (e.g. `'/((?!api|_next|_vercel|.*\\..*).*)'`). NEVER use glob wildcards like `'/dashboard/*'` or `'/profile/*'`. Glob syntax (`/*`) throws a fatal route parsing crash: "Error parsing `/.../*` Reason: Unexpected MODIFIER, expected END"."#;

pub const CONFIG_FILES_RULEPACK: &str = r#"CONFIGURATION & TSCONFIG RULES:
1. TSCONFIG.JSON CONCISENESS & VALIDITY: `tsconfig.json` MUST be a valid, compact, non-redundant JSON object (maximum 30 lines). NEVER repeat compilerOptions flags or output duplicate keys (`strictNullChecks`, `noImplicitAny`). Redundant repetition loops corrupt the file and crash compilers.
2. TSCONFIG COMPILER OPTIONS SPECIFICATION: Always include: "lib": ["dom", "dom.iterable", "esnext"], "allowJs": true, "skipLibCheck": true, "strict": false, "noEmit": true, "esModuleInterop": true, "module": "esnext", "moduleResolution": "bundler", "resolveJsonModule": true, "isolatedModules": true, "jsx": "preserve", "paths": { "@/*": ["./src/*"] }.
3. TSCONFIG INCLUDES & EXCLUDES: Include array MUST strictly be `["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"]` and exclude MUST be `["node_modules"]`.
4. NEXT.CONFIG.JS FORMAT: Always export `module.exports = nextConfig;` where `nextConfig` is a plain JavaScript object. Never use ES6 `export default` in `.js` config files unless `type: "module"` is set.
5. TAILWIND.CONFIG.JS CONTENT ARRAY: The `content` array must explicitly include `./src/**/*.{js,ts,jsx,tsx,mdx}`."#;

pub const NEXT_INTL_RULEPACK: &str = r#"NEXT-INTL (INTERNATIONALIZATION) RULES:
1. useTranslations SIGNATURE ENFORCEMENT: 'useTranslations' takes only an optional namespace string, e.g. useTranslations('Header') or useTranslations(). NEVER pass an object, dictionary, or message map as a second argument (e.g., `useTranslations('Header', messages)` is STRICTLY INVALID and will crash).
2. TRANSLATIONS STORAGE: Translation key-value pairs must be defined inside JSON files under the messages directory (e.g., src/messages/en.json). Do not declare inline translation message maps inside component code.
3. CLIENT PROVIDER REQUIREMENT: In client components, 'useTranslations' requires NextIntlClientProvider to be mounted in the component tree."#;

pub const NEXT_LINK_RULEPACK: &str = r#"NEXT.JS LINK COMPONENT RULES:
1. NO NESTED ANCHOR TAGS: In Next.js 13+, do NOT place <a> tags inside <Link href="...">. Place className and styling directly on the <Link> component (e.g. `<Link href="/about" className="...">About</Link>`).
2. PREFETCH OPTIMIZATION: Always add `prefetch={false}` to <Link> components (e.g. `<Link href="/dashboard" prefetch={false}>`) to prevent aggressive edge request amplification on Vercel."#;

pub const NEXTJS_MIDDLEWARE_RULEPACK: &str = r#"NEXT.JS MIDDLEWARE & EDGE ROUTING RULES:
1. PATH-TO-REGEXP MATCHERS: In `src/middleware.ts` or root `middleware.ts`, `config.matcher` strictly parses paths using `path-to-regexp`. Wildcards for subpaths MUST use named parameters (e.g. `'/dashboard/:path*'`, `'/profile/:path*'`, `'/admin/:path*'`) or regex filters (e.g. `'/((?!api|_next|_vercel|.*\\..*).*)'`). NEVER write glob wildcards like `'/dashboard/*'` or `'/profile/*'`. Unnamed wildcards (`/*` or `/**`) throw fatal syntax crashes: "Error parsing /.../* Reason: Unexpected MODIFIER, expected END: Invalid middlewares found".
2. SINGLE ROOT/SRC MIDDLEWARE FILE: Next.js allows only ONE middleware file per project located strictly at `src/middleware.ts` (or `middleware.ts` at the root). Never create nested middleware files inside app subfolders.
3. EDGE RUNTIME BOUNDARY: Next.js middleware executes exclusively in the Edge Runtime. NEVER import Node.js native packages (`fs`, `path`, `child_process`, `crypto`) or database drivers/Prisma Client in middleware files. Database operations belong in Server Components, Route Handlers, or Server Actions.
4. WITHAUTH & NEXTAUTH SESSIONS: For NextAuth route protection in middleware, either wrap the handler with `withAuth` from `'next-auth/middleware'` or inspect the JWT via `await getToken({ req: request, secret: process.env.NEXTAUTH_SECRET })`. For unauthenticated access to protected routes, redirect using `NextResponse.redirect(new URL('/login', request.url))`.
5. ASSET BYPASS OPTIMIZATION: Always filter out or bypass static resources (`/_next/static`, `/_next/image`, `/favicon.ico`, and files with extensions like `.png`, `.svg`, `.css`) in the matcher configuration to avoid redundant middleware execution on static assets."#;

pub const MERN_STACK_RULEPACK: &str = r#"MERN STACK (EXPRESS + MONGOOSE + VITE/REACT) RULES:
1. EXPRESS ASYNC ROUTE SAFETY: Wrap every async route handler body in try/catch. Always return explicit status codes: `res.status(200).json(data)` for success, `res.status(201).json(data)` for created, `res.status(400)` for invalid input, `res.status(404)` for missing resources, `res.status(500).json({ error: err.message })` in catch. Immediately return after sending response to prevent 'Cannot set headers after they are sent'.
2. MIDDLEWARE ORDER: Always register `app.use(cors())` and `app.use(express.json())` immediately after `const app = express();`, BEFORE registering any API routes. Never register body parsers after route endpoints.
3. RUNTIME SEPARATION: Never import React components into `server.js`, and never import Mongoose models, `fs`, or Node core modules into `client/src/`. Client communicates strictly via HTTP (`axios.get('/api/...')`).
4. REACT ROUTER DOM V6 STANDARDS: Never use React Router v5 syntax (<Switch>, `<Route component={Comp}>`, `useHistory()`). Use React Router v6: `<Routes><Route path="/..." element={<Component />} /></Routes>` and `const navigate = useNavigate(); navigate('/path')`.
5. DOTENV LOAD ORDER: `require('dotenv').config();` must be the absolute first line executed in `server.js`, before any other local module reads `process.env`.
6. MONGOOSE CONNECTION SINGLETON: Call `mongoose.connect(process.env.MONGO_URI)` once in `server.js` at startup before `app.listen(...)`. Never connect inside individual route files or per request.
7. PASSWORD HASHING BEFORE SAVE: Never store plaintext passwords or hash passwords inside route handlers. Always hash in a Mongoose pre-save hook: `userSchema.pre('save', async function(next) { if (!this.isModified('password')) return next(); this.password = await bcrypt.hash(this.password, 10); next(); });`.
8. JWT AUTH MIDDLEWARE: Centralize authorization in a `protect` middleware that verifies `req.headers.authorization?.split(' ')[1]` with `jwt.verify()`. Never duplicate inline token verification in every route.
9. CONTROLLED INPUT STATE BINDING: In React form controls, `<input value={name} />` MUST always be paired with `onChange={(e) => setName(e.target.value)}`. Never leave a value-bound input without an onChange handler.
10. FILE UPLOAD MIDDLEWARE: For `multipart/form-data` uploads, always use `multer` middleware (e.g. `upload.single('file')`). `express.json()` cannot parse multipart form payloads.
11. POPULATE VS MANUAL JOINS: Use Mongoose `.populate('author', 'name email')` on schema fields with `ref` types. Never perform manual joins using loops with secondary `.findById()` queries.
12. ENVIRONMENT VARIABLE ISOLATION: Client code uses `import.meta.env.VITE_API_URL` (Vite-injected); backend uses `process.env.MONGO_URI` (Node/dotenv). Never cross the two. Handle duplicate-key MongoDB errors (code 11000) with HTTP 409 Conflict."#;

pub const REACT_NATIVE_EXPO_RULEPACK: &str = r#"REACT NATIVE & EXPO MOBILE RULES:
1. ABSOLUTE WEB DOM ELEMENT BAN: NEVER use HTML/DOM tags (<div>, <span>, <p>, <a>, <button>, <h1>-<h6>, <input>, <form>, <ul>, <li>, <img>). Use React Native primitives exclusively: <View>, <Text>, <TouchableOpacity>, <Pressable>, <TextInput>, <ScrollView>, <FlatList>, <SafeAreaView>, <Image>.
2. MANDATORY TEXT WRAPPING: ALL textual content and string variables MUST be wrapped inside a `<Text>` component. Never place raw text strings directly inside `<View>` or other containers.
3. BAN OF BROWSER GLOBALS & STORAGE: Never use window, document, localStorage, sessionStorage, or window.location. Use `@react-native-async-storage/async-storage` for key-value persistence. Use Expo Router or `@react-navigation` for screen navigation.
4. STYLING ARCHITECTURE: Never pass CSS strings (style="display:flex; margin: 10px;") or use web units (px, rem, em, vh, vw). Use `StyleSheet.create({ container: { flex: 1, padding: 16 } })` with unitless numeric dimensions, or NativeWind `className` if configured.
5. NATIVE TOUCH & INPUT EVENTS: Never use onClick, onChange, or onMouseEnter. Use `onPress` for touchables/buttons and `onChangeText` for `<TextInput>` (which receives string value directly).
6. GESTURE HANDLER ROOT REQUIREMENT: The root of the application MUST be wrapped in `<GestureHandlerRootView style={{ flex: 1 }}>` from `react-native-gesture-handler` when using swipeable lists, bottom sheets, or Reanimated.
7. KEYBOARD-AWARE LAYOUT: For screens with form inputs, wrap content in `<KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : 'height'}>` to prevent the software keyboard from covering input fields.
8. IMAGE SOURCE SYNTAX: Use `source={require('./assets/logo.png')}` for local images and `source={{ uri: 'https://...' }}` for remote images. The web `src` prop is invalid in React Native.
9. PLATFORM-SPECIFIC BRANCHING: Use `Platform.OS === 'ios'` or `Platform.select({...})` for OS-specific behavior. Do not assume web navigator APIs exist.
10. ASYNC PERMISSIONS PATTERN: Hardware APIs (camera, location, media library) must always explicitly await permissions first: `const { status } = await Camera.requestCameraPermissionsAsync(); if (status !== 'granted') return;`.
11. FONT & SPLASH SCREEN LOADING: Call `SplashScreen.preventAutoHideAsync()` at module load and gate UI rendering on font load completion: `const [loaded] = useFonts({...}); if (!loaded) return null;`.
12. ICONS IMPORT SOURCE: Always import icons from `@expo/vector-icons` (e.g. `Ionicons`, `MaterialIcons`). Never import web icon packages (e.g. `react-icons`) which depend on the DOM.
13. LIST VIRTUALIZATION: Never render large arrays using ScrollView + .map(). Always use `<FlatList data={items} keyExtractor={(item) => item.id} renderItem={({item}) => <Row item={item} />} />` with a stable unique keyExtractor.
14. SAFE AREA AWARENESS: Wrap top-level screens in `<SafeAreaView style={{ flex: 1 }}>` (from `react-native-safe-area-context`) to avoid rendering behind the device notch, status bar, or home indicator."#;

pub const DESKTOP_RULEPACK: &str = r#"DESKTOP (ELECTRON & TAURI) RULES:
1. ELECTRON CONTEXT ISOLATION: Never call require('electron'), require('fs'), or use ipcRenderer directly inside renderer/React components. In `main.js`, configure `webPreferences: { contextIsolation: true, nodeIntegration: false, preload: path.join(__dirname, 'preload.js') }`.
2. PRELOAD BRIDGE CONTRACT: In `preload.js`, expose narrow allowlisted functions via `contextBridge.exposeInMainWorld('electronAPI', { getData: () => ipcRenderer.invoke('get-data') })`. The frontend invokes via `window.electronAPI.getData()`. Never expose raw Node modules or the entire ipcRenderer object.
3. IPC INVOCATION PAIRING: Use `ipcRenderer.invoke('channel', args)` paired with `ipcMain.handle('channel', async (event, args) => { ... })` for request/response operations. Never leave IPC listeners dangling on destroyed windows.
4. WINDOW LIFECYCLE MANAGEMENT: Always guard window operations with `if (mainWindow && !mainWindow.isDestroyed())`. Clean up IPC event listeners when windows close. Handle `window-all-closed` and `activate` lifecycle events appropriately.
5. FILESYSTEM PATH RESOLUTION: Always resolve OS-safe writable locations using `app.getPath('userData')`, `app.getPath('temp')`, or `app.getPath('documents')`. Never hardcode absolute paths like `C:\\...` or cwd-relative paths that fail in packaged installers.
6. NATIVE MENU CONSTRUCTION: Build application menus in the main process using `Menu.buildFromTemplate([{ label: 'File', submenu: [{ role: 'quit' }] }])` and `Menu.setApplicationMenu(menu)`. Never build HTML `<nav>` menus expecting native OS shortcuts.
7. TAURI COMMAND SIGNATURES: In Rust Tauri commands, never use `.unwrap()` or `panic!()`. Always annotate with `#[tauri::command]` and return `Result<T, String>` where errors are mapped with `.map_err(|e| e.to_string())`.
8. TAURI COMMAND REGISTRATION: Every `#[tauri::command]` function MUST be registered inside `tauri::generate_handler![cmd1, cmd2, ...]` during app builder setup.
9. TAURI STATE MANAGEMENT: Access shared state via `tauri::State<Mutex<AppState>>` registered with `.manage(Mutex::new(AppState::default()))`. Never block the UI thread with long-running synchronous operations.
10. TAURI V2 CAPABILITIES & PERMISSIONS: In Tauri v2, declare all plugin permissions (e.g. `"fs:allow-read-file"`) in `src-tauri/capabilities/*.json`. Calls to unlisted permissions will be rejected by ACL.
11. EVENT SYSTEM VS COMMAND INVOKE: Use `window.emit("channel", payload)` from backend and `listen("channel", callback)` from frontend for backend-pushed notifications (progress, file watchers). Use commands for frontend-initiated calls.
12. NO CROSS-PLATFORM POLLUTION: Never reference `window.electronAPI` inside Tauri applications, and never use Tauri `invoke` inside Electron."#;

pub const PRISMA_DATABASE_RULEPACK: &str = r#"PRISMA ORM & DATABASE RULES:
1. SERVER-ONLY BOUNDARY: Prisma Client (`@prisma/client`) must NEVER be imported or instantiated in client components or files marked with `'use client'`. Database operations belong strictly in Server Components, Route Handlers (`app/api/**/route.ts`), or Server Actions (`'use server'`).
2. CONNECTION POOLING SINGLETON: In development and serverless environments, always cache the PrismaClient instance on `globalThis` to prevent connection pool exhaustion during hot reload: `const globalForPrisma = globalThis as unknown as { prisma: PrismaClient }; export const prisma = globalForPrisma.prisma ?? new PrismaClient(); if (process.env.NODE_ENV !== 'production') globalForPrisma.prisma = prisma;`.
3. RELATION FOREIGN-KEY PLACEMENT: In `schema.prisma`, the scalar foreign key (`authorId String`) and the `@relation(fields: [authorId], references: [id])` attribute MUST be defined on the model that stores the foreign key (the child/many side). The parent model defines only the back-relation array (`posts Post[]`).
4. CASCADING DELETE BEHAVIOR: Always specify referential action on relations when child records should not outlive the parent: `@relation(fields: [authorId], references: [id], onDelete: Cascade)`.
5. OFFLINE & PREVIEW DEV RESILIENCE: Wrap database calls in Route Handlers in try/catch. In catch blocks, return realistic deterministic mock data matching the TypeScript interface so the application remains fully interactive in preview mode even if the database is offline or unmigrated.
6. SCHEMA SYNTAX VS TYPESCRIPT SYNTAX: In `schema.prisma`, optional fields use `name String?` (type first, `?` suffix). In TypeScript interfaces, optional fields use `name?: string` (`?` before colon, lowercase primitive). Never mix the two syntaxes.
7. QUERY KEY CONSTRAINTS: Fields used in `where` clauses of `findUnique`, `update`, or `delete` MUST be marked with `@id`, `@unique`, or part of `@@unique([...])` in `schema.prisma`.
8. ZOD VALIDATION & TYPE SYNC: Parse incoming request bodies with `schema.safeParse(body)` before invoking Prisma mutations. Keep Zod field optionality strictly synchronized with schema nullable fields.
9. ENUM DEFINITION SYNTAX: Define enums as top-level blocks without commas or quotes: `enum Role { ADMIN USER }` and use on fields as `role Role @default(USER)`. Never write TypeScript union strings (`role: "ADMIN" | "USER"`) in `.prisma` files.
10. SEED SCRIPT CONFIGURATION: Declare seed script in `package.json` under `"prisma": { "seed": "ts-node prisma/seed.ts" }` so `npx prisma db seed` locates it reliably.
11. TRANSACTIONAL MULTI-WRITE OPERATIONS: When performing multiple dependent mutations, always use `prisma.$transaction([prisma.order.create({...}), prisma.inventory.update({...})])` to guarantee atomic all-or-nothing execution.
12. INDEX DECLARATION FOR QUERY PERFORMANCE: Always add `@@index([status, createdAt])` at the model level for field combinations frequently used in `where` or `orderBy` clauses.
13. ROUTE HANDLER HTTP METHODS: App Router route handlers in `route.ts` must export named async functions per HTTP verb (`export async function GET(request: Request) { return NextResponse.json({...}); }`). Never use default export `handler(req, res)`."#;

pub fn get_stack_rulepacks(file_path: &str, file_content: &str, error_msg: Option<&str>) -> String {
    let path_lower = file_path.to_lowercase().replace('\\', "/");
    let err_lower = error_msg.unwrap_or("").to_lowercase();
    let content_lower = file_content.to_lowercase();

    let mut rules = Vec::new();

    // 1. Mobile (React Native / Expo) - check first to avoid web DOM confusion
    let is_react_native = path_lower.contains("react-native")
        || path_lower.contains("expo")
        || content_lower.contains("from 'react-native'")
        || content_lower.contains("from \"react-native\"")
        || content_lower.contains("from 'expo")
        || content_lower.contains("stylesheet.create")
        || err_lower.contains("view config not found");

    if is_react_native {
        rules.push(REACT_NATIVE_EXPO_RULEPACK);
    } else {
        // 2. React & Next.js Web Stack
        let is_react_file = path_lower.ends_with(".tsx")
            || path_lower.ends_with(".jsx")
            || path_lower.contains("/app/")
            || path_lower.contains("/pages/")
            || path_lower.contains("/components/")
            || content_lower.contains("'use client'")
            || content_lower.contains("\"use client\"")
            || content_lower.contains("from 'react'")
            || content_lower.contains("from \"react\"")
            || content_lower.contains("usestate")
            || content_lower.contains("useeffect")
            || content_lower.contains("usecontext")
            || content_lower.contains("usetranslations")
            || content_lower.contains("usesession")
            || err_lower.contains("hook")
            || err_lower.contains("react context")
            || err_lower.contains("the `style` prop expects a mapping")
            || err_lower.contains("rendered more hooks");

        if is_react_file {
            rules.push(NEXTJS_REACT_RULEPACK);
        }

        // 3. Next-Intl (Internationalization)
        let is_next_intl = path_lower.contains("i18n")
            || path_lower.contains("messages/")
            || content_lower.contains("next-intl")
            || content_lower.contains("usetranslations")
            || err_lower.contains("nextintl")
            || err_lower.contains("usetranslations");

        if is_next_intl {
            rules.push(NEXT_INTL_RULEPACK);
        }

        // 4. Next.js Link
        let is_next_link = content_lower.contains("from 'next/link'")
            || content_lower.contains("from \"next/link\"")
            || content_lower.contains("<link")
            || err_lower.contains("invalid <link>")
            || err_lower.contains("legacybehavior");

        if is_next_link {
            rules.push(NEXT_LINK_RULEPACK);
        }
    }

    // 5. MERN Stack (Express Backend + Vite / React Router)
    let is_mern = path_lower.contains("server.js")
        || path_lower.contains("/routes/")
        || path_lower.contains("/models/")
        || content_lower.contains("express()")
        || content_lower.contains("from 'express'")
        || content_lower.contains("require('express')")
        || content_lower.contains("from 'react-router-dom'")
        || content_lower.contains("from \"react-router-dom\"")
        || err_lower.contains("cannot set headers");

    if is_mern {
        rules.push(MERN_STACK_RULEPACK);
    }

    // 6. Desktop (Electron & Tauri)
    let is_desktop = path_lower.contains("preload.js")
        || path_lower.contains("main.js")
        || path_lower.contains("electron")
        || path_lower.contains("src-tauri")
        || content_lower.contains("contextbridge")
        || content_lower.contains("ipcmain")
        || content_lower.contains("ipcrenderer")
        || content_lower.contains("#[tauri::command]")
        || err_lower.contains("contextisolation");

    if is_desktop {
        rules.push(DESKTOP_RULEPACK);
    }

    // 7. Prisma & Database Resiliency
    let is_prisma = path_lower.ends_with(".prisma")
        || path_lower.contains("/api/")
        || path_lower.contains("route.ts")
        || path_lower.contains("route.js")
        || content_lower.contains("@prisma/client")
        || content_lower.contains("prisma.")
        || err_lower.contains("prisma");

    if is_prisma {
        rules.push(PRISMA_DATABASE_RULEPACK);
    }

    // 8. Configuration & Project Config Rules
    let is_config = path_lower.ends_with(".json")
        || path_lower.contains("config")
        || path_lower.ends_with("tsconfig.json")
        || err_lower.contains("tsconfig")
        || err_lower.contains("debug failure");

    if is_config {
        rules.push(CONFIG_FILES_RULEPACK);
    }

    // 9. Next.js Middleware & Edge Routing Rules
    let is_middleware = path_lower.contains("middleware.ts")
        || path_lower.contains("middleware.js")
        || content_lower.contains("nextrequest")
        || content_lower.contains("nextresponse")
        || content_lower.contains("withauth")
        || err_lower.contains("invalid middlewares found")
        || err_lower.contains("invalid-route-source")
        || err_lower.contains("unexpected modifier")
        || err_lower.contains("middleware");

    if is_middleware {
        rules.push(NEXTJS_MIDDLEWARE_RULEPACK);
    }

    if rules.is_empty() {
        String::new()
    } else {
        format!("\n\n=== STRICT STACK & LIBRARY CONSTRAINTS ===\n{}\n==========================================\n", rules.join("\n\n"))
    }
}

pub fn get_expert_system_prompt(
    file_path: &str,
    project_type: &str,
    _overrides_desc: &str,
    _pkg_db_summary: &str,
    context_ref: &str,
    _routes_ref: &str,
    proj_dir: &std::path::Path,
) -> String {
    let path_lower = file_path.to_lowercase().replace('\\', "/");
    let proj_type_lower = project_type.to_lowercase();

    // 1. RUST & TAURI SYSTEM SCOPE (.rs, Cargo.toml)
    if path_lower.ends_with(".rs") {
        return format!(
            "You are a Senior Rust Systems Engineer. Write only clean, production-ready Rust code wrapped in markdown code blocks. No explanations.\n\n\
             RUST IMPLEMENTATION RULES:\n\
             - Write idiomatic, memory-safe Rust adhering to ownership, borrowing, and lifetime rules.\n\
             - Implement robust error handling with Result<T, E> and Option<T>, avoiding bare unwrap() in production paths.\n\
             - Use serde::{{Serialize, Deserialize}} for data structures requiring serialization.\n\
             - If this file defines Tauri commands, annotate handlers with #[tauri::command] and return clean Result types.\n\
             - STRICTLY NO JavaScript, TypeScript, HTML, CSS, or Node.js imports in Rust code.\n\
             FILE: {}\n",
            file_path
        );
    }

    if path_lower.ends_with("cargo.toml") {
        return "You are an expert Rust systems architect. Write only valid, production-ready Cargo.toml code wrapped in markdown code blocks. No explanations.\n\n\
                CARGO.TOML RULES:\n\
                - Define valid [package] metadata with edition = \"2021\".\n\
                - Specify verified, stable crate versions under [dependencies] (e.g. serde, tokio, tauri, etc.).\n\
                - NEVER output npm, package.json, or JavaScript syntax in Cargo.toml.\n".to_string();
    }

    // 2. FLUTTER & DART MOBILE SCOPE (.dart, pubspec.yaml)
    if path_lower.ends_with(".dart") {
        return format!(
            "You are a Senior Flutter & Dart Mobile Developer. Write only clean, production-ready Dart code wrapped in markdown code blocks. No explanations.\n\n\
             FLUTTER RULES:\n\
             - Write clean StatelessWidget or StatefulWidget implementations using Material 3 or Cupertino widgets.\n\
             - Structure mobile layouts using Column, Row, ListView, Container, Padding, and SizedBox.\n\
             - Strictly NO HTML DOM tags (no <div>, <span>, <p>, <a>), no React hooks, and no web CSS.\n\
             - Adhere to Dart sound null-safety standards.\n\
             FILE: {}\n",
            file_path
        );
    }

    if path_lower.ends_with("pubspec.yaml") {
        return "You are an expert Flutter architect. Write only valid pubspec.yaml code wrapped in markdown code blocks. No explanations.\n\n\
                PUBSPEC RULES:\n\
                - Specify valid name, description, version, and environment sdk: \">=3.0.0 <4.0.0\".\n\
                - Include flutter sdk dependency under dependencies and flutter: uses-material-design: true.\n".to_string();
    }

    // 3. REACT NATIVE / EXPO MOBILE SCOPE
    let is_mobile_rn = proj_type_lower == "react-native" || proj_type_lower == "expo" || path_lower.contains("app.json");
    if is_mobile_rn && (path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx")) {
        return format!(
            "You are a Senior React Native Mobile Engineer. Write only clean, production-ready React Native / Expo code wrapped in markdown code blocks. No explanations.\n\n\
             REACT NATIVE RULES:\n\
             - MOBILE UI ONLY: NEVER use HTML DOM tags (no <div>, <p>, <span>, <a>, <button>, or <html>). ALWAYS use React Native core components (<View>, <Text>, <TouchableOpacity>, <ScrollView>, <FlatList>, <TextInput>, <SafeAreaView>).\n\
             - STYLING: Use StyleSheet.create({{ ... }}) or NativeWind classes. Do NOT use web CSS stylesheets.\n\
             - NAVIGATION: Use React Navigation or Expo Router conventions (never Next.js Link or Next.js router).\n\
             - CLIENT-SIDE ONLY: Never import @prisma/client or Node.js server modules.\n\
             FILE: {}\n",
            file_path
        );
    }

    // 4. FILE-SCOPED PROMPT: Styles & CSS (.css, .module.css, .scss)
    if path_lower.ends_with(".css") || path_lower.ends_with(".scss") {
        return format!(
            "You are an expert CSS / Styling Engineer. Write only valid, production-ready CSS code wrapped in markdown code blocks. No explanations.\n\n\
             ROLE: Senior UI/UX & CSS Specialist\n\
             STYLING RULES:\n\
             - Write clean, modern, responsive CSS rules with flexbox, grid, smooth transitions, and accessible colors.\n\
             - If this is a CSS module (e.g. *.module.css), define clean class selectors (camelCase or kebab-case) matching component usage.\n\
             - Do NOT import JavaScript, TypeScript, Prisma, or backend libraries.\n\
             - Do NOT generate empty placeholders; provide full, polished styling rules.\n"
        );
    }

    // 5. FILE-SCOPED PROMPT: Web Configuration files
    if path_lower.ends_with("postcss.config.js") || path_lower.ends_with("postcss.config.mjs") {
        return "You are an expert build configuration engineer. Write only valid postcss.config.js code wrapped in markdown code blocks. No explanations.\n\nmodule.exports = {\n  plugins: {\n    tailwindcss: {},\n    autoprefixer: {},\n  },\n};\n".to_string();
    }

    if path_lower.ends_with("tailwind.config.js") || path_lower.ends_with("tailwind.config.ts") {
        return "You are an expert Tailwind CSS configuration engineer. Write only valid tailwind.config.js code wrapped in markdown code blocks. No explanations.\n\n/** @type {import('tailwindcss').Config} */\nmodule.exports = {\n  content: [\n    './src/pages/**/*.{js,ts,jsx,tsx,mdx}',\n    './src/components/**/*.{js,ts,jsx,tsx,mdx}',\n    './src/app/**/*.{js,ts,jsx,tsx,mdx}',\n  ],\n  theme: {\n    extend: {},\n  },\n  plugins: [],\n};\n".to_string();
    }

    if path_lower.ends_with("tsconfig.json") {
        return "You are a TypeScript Configuration Specialist. Output only a valid, concise Next.js tsconfig.json JSON object wrapped in markdown code blocks. Maximum 30 lines. No explanations.\n\n{\n  \"compilerOptions\": {\n    \"lib\": [\"dom\", \"dom.iterable\", \"esnext\"],\n    \"allowJs\": true,\n    \"skipLibCheck\": true,\n    \"strict\": false,\n    \"noEmit\": true,\n    \"esModuleInterop\": true,\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\",\n    \"resolveJsonModule\": true,\n    \"isolatedModules\": true,\n    \"jsx\": \"preserve\",\n    \"incremental\": true,\n    \"paths\": {\n      \"@/*\": [\"./src/*\"]\n    }\n  },\n  \"include\": [\"next-env.d.ts\", \"**/*.ts\", \"**/*.tsx\", \".next/types/**/*.ts\"],\n  \"exclude\": [\"node_modules\"]\n}\n".to_string();
    }

    if path_lower.contains("next.config") {
        return "You are a Next.js Configuration Expert. Write only valid next.config.js code wrapped in markdown code blocks. No explanations.\n\n/** @type {import('next').NextConfig} */\nconst nextConfig = {\n  reactStrictMode: true,\n};\n\nmodule.exports = nextConfig;\n".to_string();
    }

    // Read Live Builder Guide contracts if available
    let lbg_path = proj_dir.join(".axiom").join("live_builder_guide.json");
    let lbg_val: Option<Value> = if lbg_path.exists() {
        fs::read_to_string(&lbg_path).ok().and_then(|c| serde_json::from_str::<Value>(&c).ok())
    } else {
        None
    };

    // 6. FILE-SCOPED PROMPT: Database & Prisma Schema
    if path_lower.contains("schema.prisma") {
        let mut prompt = "You are a Senior Database Architect. Write only the complete, valid schema.prisma code wrapped in markdown code blocks. No explanations.\n\n\
             DATABASE DESIGN RULES:\n\
             - Focus on relational integrity, clean schema definitions, and proper Prisma model annotations.\n\
             - When writing tables, ensure IDs use `@id @default(cuid())` or similar suitable auto-generators.\n\
             - Make sure passwords or security token fields (e.g. `hashedPassword`) are defined as optional `String?` so that OAuth/Magic Link users can be saved without password errors.\n\
             - Always include necessary relation annotations cleanly and explicitly.\n\
             - PRISMA RELATION RULES:\n\
               1. In a one-to-many relation, the `@relation` attribute (specifying `fields` and `references`) MUST ONLY be defined on the model that contains the actual foreign key column (the \"many\" side). The parent model (the \"one\" side, returning an array `Model[]`) MUST NOT specify any `fields` or `references` in its `@relation` attribute.\n\
               2. Every relation must have an opposite relation field defined on both models. For example, if model `Message` has `sender User @relation(fields: [senderId], references: [id])`, then model `User` MUST have `messages Message[]`.\n\
               3. For implicit many-to-many relations, do NOT specify any `fields` or `references` arguments.\n\
               4. The `@relation` attribute MUST ONLY be placed on relational object fields, never on scalar fields.\n\
               5. For NextAuth schema, the VerificationToken model does not have a direct relation back to the User model.\n".to_string();

        if let Some(ref lbg) = lbg_val {
            if let Some(db_models) = lbg.get("sharedContracts").and_then(|c| c.get("database")).and_then(|d| d.get("models")) {
                prompt.push_str(&format!("\nREQUIRED DATABASE MODELS CONTRACT:\n{}\n", db_models));
            }
        }
        return prompt;
    }

    // 7. FILE-SCOPED PROMPT: Backend API Routes & NextAuth
    let is_api_route = path_lower.contains("/api/") || path_lower.contains("route.ts") || path_lower.contains("route.js");
    if is_api_route {
        let mut prompt = "You are a Senior Backend Developer. Write only clean, production-ready Next.js Route Handler code wrapped in markdown code blocks. No explanations.\n\n\
             BACKEND API DEVELOPMENT RULES:\n\
             - Implement clean Next.js App Router Route Handlers (exporting GET, POST, PUT, DELETE functions).\n\
             - Wrap database calls in robust try-catch blocks. Implement fallback mock datasets inside catch clauses so the application remains reviewable even if the database is offline.\n\
             - Strictly validate requests (e.g. using standard parsing or Zod) and return clean, typed JSON responses with appropriate HTTP status codes.\n\
             - STRICT IMPORT RESOLUTION: Every external dependency ('zod', 'bcryptjs', etc.) MUST be explicitly imported at the top.\n".to_string();

        if path_lower.contains("/api/auth/") || path_lower.contains("nextauth") {
            prompt.push_str(
                "\nAUTHENTICATION SECURITY & HYBRID PROVIDER RULES (SCOPED TO AUTH ROUTE):\n\
                 - Export GET and POST handlers: `const handler = NextAuth(authOptions); export { handler as GET, handler as POST };`.\n\
                 - Use `strategy: 'jwt'` sessions for NextAuth.\n\
                 - HYBRID DEV/PROD PROVIDERS: Dynamically use `CredentialsProvider` (email+password verified with `bcryptjs`) during development (`process.env.NODE_ENV !== 'production'`), and support `EmailProvider` (Resend SMTP) in production (`process.env.NODE_ENV === 'production'`) ONLY when `process.env.EMAIL_SERVER_HOST` or `process.env.RESEND_API_KEY` is configured, falling back to `CredentialsProvider` if SMTP keys are missing.\n\
                 - CredentialsProvider MUST be imported from 'next-auth/providers/credentials', NEVER from 'next-auth' directly.\n\
                 - Never hardcode environment secrets (use `process.env.NEXTAUTH_SECRET || 'dev-secret'`).\n"
            );
        }

        if path_lower.contains("stripe") || path_lower.contains("webhook") || path_lower.contains("checkout") {
            prompt.push_str(
                "\nPAYMENTS & STRIPE DOMAIN RULES (SCOPED TO BILLING API):\n\
                 - Guard Stripe initialization if `process.env.STRIPE_SECRET_KEY` is missing in local preview, returning a graceful mock/preview JSON response instead of crashing.\n\
                 - Verify webhook signatures cleanly when `STRIPE_WEBHOOK_SECRET` is present.\n"
            );
        }

        if let Some(ref lbg) = lbg_val {
            if let Some(api_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("api")) {
                prompt.push_str(&format!("\nAPI ENDPOINTS CONTRACT:\n{}\n", api_contracts));
            }
            if let Some(db_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("database")) {
                prompt.push_str(&format!("\nDATABASE MODELS REFERENCE:\n{}\n", db_contracts));
            }
        }
        return prompt;
    }

    // 7b. FILE-SCOPED PROMPT: Next.js Edge Middleware (middleware.ts / middleware.js)
    if path_lower.ends_with("middleware.ts") || path_lower.ends_with("middleware.js") {
        return format!(
            "You are a Senior Next.js Edge Architecture Engineer. Write only clean, production-ready Next.js Middleware code wrapped in markdown code blocks. No explanations.\n\n{}\n\
             - NEVER import from 'express' (no Request, Response, NextFunction from 'express'). Always import `{{ NextResponse }}` and `type {{ NextRequest }}` from `'next/server'`.\n\
             - Always export `function middleware(request: NextRequest)` and `export const config = {{ matcher: [...] }}`.\n",
            NEXTJS_MIDDLEWARE_RULEPACK
        );
    }

    // 8. FILE-SCOPED PROMPT: Web Client UI Components, Pages & Layouts (.tsx, .jsx)
    let is_react_frontend = path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx");
    if is_react_frontend {
        let mut prompt = "You are an expert React and Tailwind UI Engineer. Write only clean, complete, production-ready React code wrapped in markdown code blocks. No explanations.\n\n".to_string();

        if path_lower.ends_with("layout.tsx") || path_lower.ends_with("layout.jsx") {
            prompt.push_str(
                "ROOT LAYOUT ARCHITECTURE RULES:\n\
                 - DOM HIERARCHY: Outermost elements returned MUST be `<html lang=\"en\"><body className=\"min-h-screen flex flex-col bg-gray-50 text-gray-900\">...</body></html>`.\n\
                 - GLOBAL PROVIDERS: Wrap all global providers inside `<body>` around `{children}`. If NextAuth is configured, wrap with `<SessionProvider>`.\n\
                 - PERSISTENT NAVIGATION & FOOTER: Always import and mount `<Navbar />` (from `@/components/Navbar`) above main content, wrap `{children}` in `<main className=\"flex-grow flex flex-col w-full\">{children}</main>`, and mount `<Footer />` at the bottom.\n\
                 - MANDATORY DEFAULT EXPORT: RootLayout MUST have `export default function RootLayout({ children }: { children: React.ReactNode })`.\n\
                 - Mark the layout file with 'use client'; at line 1.\n\n"
            );
        } else if path_lower == "src/app/page.tsx" || path_lower == "src/app/page.jsx" || path_lower == "app/page.tsx" || path_lower == "app/page.jsx" {
            prompt.push_str(
                "ROOT LANDING PAGE DESIGN RULES:\n\
                 - This is the HOME/LANDING PAGE (`/`). It must NEVER be a blank screen or bare login form.\n\
                 - HERO SECTION: Include a high-impact Hero banner with compelling headline, engaging description, and clear CTA buttons tailored specifically to the project's business idea.\n\
                 - CORE FEATURE SHOWCASE: Show primary interactive domain workflows, product cards, or metric widgets.\n\
                 - VALUE PROPOSITIONS: Display a responsive grid of key benefits/badges.\n\
                 - DATA RESILIENCY: Fetch data with an immediate fallback to a rich realistic mockup array.\n\
                 - MANDATORY TAILWIND CSS: Every element must use modern Tailwind CSS classes.\n\n"
            );
        } else {
            prompt.push_str(
                "FRONTEND COMPONENT RULES:\n\
                 - PRISMA SERVER-ONLY RULE: You are on the CLIENT side. NEVER import `@prisma/client` in React components. Fetch data from `/api/...` endpoints.\n\
                 - STRICT TYPE DEFINITIONS: Any TypeScript types/interfaces used MUST be declared in the file or imported.\n\
                 - NEXT.JS LINK RULE: Do NOT place <a> tags inside <Link>. Put className directly on <Link>, and add `prefetch={false}`.\n\
                 - NO DEAD IMPORTS: Do not import components unless they exist; implement inline if needed.\n\
                 - Use modern Tailwind CSS with responsive flex/grid layouts, nice borders, shadows, and hover states.\n\
                 - NO MANUAL AXIOM ATTRS: Never declare or use 'data-axiom-component' or 'data-axiom-file' props.\n\n"
            );
        }

        // Scoped Domain Contexts for Frontend Pages & Components
        if path_lower.contains("/login") || path_lower.contains("/register") || path_lower.contains("/signup") || path_lower.contains("auth") {
            prompt.push_str(
                "SCOPED AUTH UI RULES (ACTIVE ONLY FOR AUTH FILES):\n\
                 - Ensure the Login/Register UI works out-of-the-box locally without any SMTP server configuration.\n\
                 - Render the email + password form locally (calling `signIn('credentials', { email, password, redirect: false })`) and support passwordless magic-link email form in production.\n\
                 - If defining an auth hook or provider (e.g. `useAuth`), export BOTH named (`export { useAuth }`) and default (`export default useAuth`) exports.\n\n"
            );
        }

        if path_lower.contains("dashboard") || path_lower.contains("analytics") || path_lower.contains("metrics") {
            prompt.push_str(
                "SCOPED ANALYTICS & DASHBOARD RULES (ACTIVE ONLY FOR DASHBOARD/ANALYTICS FILES):\n\
                 - Build domain-specific KPI summary cards, interactive tabs/filters, and data visualizations directly matching the project's business domain.\n\
                 - Pre-populate state with realistic domain mockup data so charts, tables, and workflows are immediately interactive even before API records are created.\n\n"
            );
        }

        if path_lower.contains("pricing") || path_lower.contains("checkout") || path_lower.contains("billing") {
            prompt.push_str(
                "SCOPED BILLING & PRICING UI RULES (ACTIVE ONLY FOR PRICING/CHECKOUT FILES):\n\
                 - Display clear tier comparisons, billing cycle toggles (monthly/annual), and interactive checkout triggers with graceful local preview feedback.\n\n"
            );
        }

        if let Some(ref lbg) = lbg_val {
            if let Some(spec) = lbg.get("projectSpecification") {
                if let Some(style) = spec.get("importExportStyle").and_then(|s| s.as_str()) {
                    prompt.push_str(&format!("- IMPORT/EXPORT RULE: Use {} exports.\n", style));
                }
                if let Some(reqs) = spec.get("categoryRequirements") {
                    if let Some(principles) = reqs.get("designPrinciples").and_then(|p| p.as_array()) {
                        prompt.push_str("- Design Principles:\n");
                        for p in principles {
                            if let Some(ps) = p.as_str() { prompt.push_str(&format!("  * {}\n", ps)); }
                        }
                    }
                }
            }
            if let Some(api_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("api")).and_then(|a| a.get("endpoints")) {
                prompt.push_str(&format!("- Available API Endpoints to Fetch:\n{}\n", api_contracts));
            }
        }

        if !context_ref.is_empty() {
            prompt.push_str(context_ref);
        }
        return prompt;
    }

    // 9. GENERAL UTILITY / FALLBACK SCOPE (e.g. src/utils/..., src/lib/...)
    let mut prompt = "You are an expert software engineer. Write only clean, production-ready code wrapped in markdown code blocks. No explanations.\n\n".to_string();
    if proj_type_lower == "rust" || proj_type_lower == "tauri" {
        prompt.push_str(&format!("ROLE: Senior Rust Systems Engineer\nFILE: {}\n", file_path));
    } else if proj_type_lower == "flutter" {
        prompt.push_str(&format!("ROLE: Senior Flutter & Dart Mobile Developer\nFILE: {}\n", file_path));
    } else {
        prompt.push_str(&format!("ROLE: Expert TypeScript / JavaScript Developer\nFILE: {}\n", file_path));
    }
    if !context_ref.is_empty() {
        prompt.push_str(context_ref);
    }
    prompt
}

pub async fn run_orchestrator(
    window: tauri::Window,
    task_id: String,
    manifest_id: String,
    project_id: String,
    token: String,
    model_id: String,
    host: Option<String>,
) -> Result<(), String> {
    debug_log_to_file(format!(
        "[Orchestrator] Starting run_orchestrator. task_id: {}, manifest_id: {}, project_id: {}, model: {}, host: {:?}",
        task_id, manifest_id, project_id, model_id, host
    ));
    let emit_progress = |phase: &str, msg: &str, progress: f64| {
        debug_log_to_file(format!(
            "[Orchestrator Progress] Emit event - phase: {}, message: {}, progress: {}%",
            phase, msg, progress
        ));
        let _ = window.emit("task:progress", serde_json::json!({
            "taskId": task_id,
            "projectId": project_id,
            "phase": phase,
            "message": msg,
            "progress": progress
        }));
    };

    emit_progress("fetch-manifest", "Initializing generation pipeline...", 1.0);

    // 1. Fetch Manifest
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let manifest: Value;
    if manifest_id == "TEST_LOCAL" {
        manifest = serde_json::json!({
            "manifestId": "TEST_LOCAL",
            "platform": "web",
            "projectType": "nextjs",
            "resolvedStack": "Next.js + Tailwind",
            "metadata": {
                "name": "CamperConnect",
                "description": "Web app for campers to connect by location and trade gear"
            },
            "files": [
                {
                    "path": "package.json",
                    "description": "Setup package.json with next, react, react-dom, tailwindcss, and lucide-react.",
                    "language": "json"
                },
                {
                    "path": "src/app/globals.css",
                    "description": "Standard Tailwind CSS directives and a camping-themed color palette.",
                    "language": "css"
                },
                {
                    "path": "src/app/layout.jsx",
                    "description": "Root layout for Next.js App Router. Include a navigation bar with 'Map' and 'Trade Gear' links.",
                    "language": "javascript"
                },
                {
                    "path": "src/app/page.jsx",
                    "description": "Landing page for CamperConnect. Show a hero section encouraging campers to connect locally.",
                    "language": "javascript"
                },
                {
                    "path": "src/app/map/page.jsx",
                    "description": "Page to display campers on a map by location. Mock the map UI using a placeholder div and list 3 nearby campers.",
                    "language": "javascript"
                },
                {
                    "path": "src/app/trading/page.jsx",
                    "description": "Trading post page where campers can list gear for trade. Display a grid of 4 mock items (e.g., tent, stove).",
                    "language": "javascript"
                }
            ]
        });
    } else {
        let api_base = host.as_deref().unwrap_or("https://idea-analyzer-ten.vercel.app");
        let url = format!("{}/api/axiom/manifest/{}", api_base, manifest_id);
        
        let res = client.get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            return Err(format!("Server response: {}", res.status()));
        }

        let json: Value = res.json().await.map_err(|e| e.to_string())?;
        manifest = json.get("data").cloned().ok_or("No data in manifest response")?;
    }
    let files = manifest.get("files").and_then(|f| f.as_array()).ok_or("No files in manifest")?;
    let name_val = manifest.get("name")
        .or_else(|| manifest.get("projectName"))
        .or_else(|| manifest.get("title"))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("name")))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("title")));
    let name = name_val.and_then(|n| n.as_str()).unwrap_or("Unknown Project");
    let desc_val = manifest.get("description")
        .or_else(|| manifest.get("projectDescription"))
        .or_else(|| manifest.get("idea"))
        .or_else(|| manifest.get("content"))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("description")))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("idea")));
    let project_description = desc_val.and_then(|d| d.as_str()).unwrap_or("");
    let project_type = detect_project_type(&manifest);

    // Immediately register/update the project in projects.json with "generating" status
    // so it appears on the Dashboard right away with its real name and description.
    let initial_proj = serde_json::json!({
        "id": project_id,
        "name": name,
        "description": project_description,
        "status": "generating",
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "updatedAt": chrono::Utc::now().to_rfc3339(),
    });
    let _ = project_save(initial_proj);

    let mut metamanifest = manifest.get("metamanifest").cloned().unwrap_or(serde_json::json!({}));
    
    // Load default metamanifest definitions based on project type
    let mut default_metamanifest = serde_json::Value::Null;
    if project_type == "nextjs" {
        default_metamanifest = serde_json::json!({
            "rootFiles": ["package.json", "tsconfig.json", "next.config.js", "tailwind.config.js", "postcss.config.js", "next-env.d.ts"],
            "rootFilesMoveFromSrc": true,
            "requiredFilesTemplates": [
                {
                    "path": "package.json",
                    "description": "Standard package.json for Next.js project with Tailwind CSS.",
                    "language": "json",
                    "mergeDependencies": {
                        "dependencies": {
                            "next": "14.2.35",
                            "react": "^18.2.0",
                            "react-dom": "^18.2.0",
                            "next-auth": "^4.24.7",
                            "@auth/prisma-adapter": "^1.0.12",
                            "@prisma/client": "5.22.0",
                            "lucide-react": "^0.460.0",
                            "clsx": "^2.1.1",
                            "tailwind-merge": "^2.3.0",
                            "class-variance-authority": "^0.7.0",
                            "tailwindcss-animate": "^1.0.7",
                            "zod": "^3.23.8",
                            "react-hook-form": "^7.51.5",
                            "bcryptjs": "^2.4.3",
                            "jsonwebtoken": "^9.0.2"
                        },
                        "devDependencies": {
                            "prisma": "5.22.0",
                            "tailwindcss": "^3.4.0",
                            "postcss": "^8.4.32",
                            "autoprefixer": "^10.4.16",
                            "typescript": "^5.3.3",
                            "@types/react": "^18.2.45",
                            "@types/react-dom": "^18.2.18",
                            "@types/node": "^20.10.5",
                            "@types/bcryptjs": "^2.4.6",
                            "@types/jsonwebtoken": "^9.0.6"
                        },
                        "scripts": {
                            "dev": "next dev",
                            "build": "next build",
                            "start": "next start"
                        }
                    }
                },
                {
                    "path": "tailwind.config.js",
                    "description": "Configuration file for Tailwind CSS.",
                    "language": "javascript",
                    "defaultContent": "/** @type {import('tailwindcss').Config} */\nmodule.exports = {\n  content: [\n    \"./src/**/*.{js,ts,jsx,tsx,mdx}\",\n    \"./app/**/*.{js,ts,jsx,tsx,mdx}\",\n    \"./components/**/*.{js,ts,jsx,tsx,mdx}\",\n    \"./pages/**/*.{js,ts,jsx,tsx,mdx}\"\n  ],\n  theme: {\n    extend: {},\n  },\n  plugins: [],\n}"
                },
                {
                    "path": "postcss.config.js",
                    "description": "Configuration file for PostCSS.",
                    "language": "javascript",
                    "defaultContent": "module.exports = {\n  plugins: {\n    tailwindcss: {},\n    autoprefixer: {},\n  },\n}"
                },
                {
                    "path": "tsconfig.json",
                    "description": "TypeScript compiler settings.",
                    "language": "json",
                    "defaultContent": "{\n  \"compilerOptions\": {\n    \"lib\": [\"dom\", \"dom.iterable\", \"esnext\"],\n    \"allowJs\": true,\n    \"skipLibCheck\": true,\n    \"strict\": false,\n    \"noEmit\": true,\n    \"esModuleInterop\": true,\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\",\n    \"resolveJsonModule\": true,\n    \"isolatedModules\": true,\n    \"jsx\": \"preserve\",\n    \"incremental\": true,\n    \"plugins\": [\n      {\n        \"name\": \"next\"\n      }\n    ],\n    \"paths\": {\n      \"@/*\": [\"./src/*\"]\n    }\n  },\n  \"include\": [\"next-env.d.ts\", \"**/*.ts\", \"**/*.tsx\", \".next/types/**/*.ts\"],\n  \"exclude\": [\"node_modules\"]\n}"
                },
                {
                    "path": "next.config.js",
                    "description": "Next.js configuration.",
                    "language": "javascript",
                    "defaultContent": "/** @type {import('next').NextConfig} */\nconst nextConfig = {\n  reactStrictMode: true,\n  swcMinify: true,\n};\nmodule.exports = nextConfig;"
                },
                {
                    "path": "src/app/globals.css",
                    "description": "Global CSS file containing Tailwind directives.",
                    "language": "css",
                    "defaultContent": "@tailwind base;\n@tailwind components;\n@tailwind utilities;\n\n"
                },
                {
                    "path": "src/middleware.ts",
                    "description": "Resource-optimized middleware that bypasses static asset queries.",
                    "language": "typescript",
                    "defaultContent": r#"import { withAuth } from "next-auth/middleware";
import { NextRequest, NextResponse } from "next/server";

const authMiddleware = withAuth(
  function middleware(req: NextRequest) {
    return NextResponse.next();
  },
  {
    callbacks: {
      authorized: ({ token }) => !!token,
    },
    pages: {
      signIn: "/login",
    },
  }
);

export default function middleware(req: NextRequest) {
  const { pathname } = req.nextUrl;

  // 1. Skip middleware for API, internal assets, and static files with extensions
  if (
    pathname.includes("/api/") ||
    pathname.includes("/_next/") ||
    pathname.includes("/_vercel/") ||
    pathname.includes(".")
  ) {
    return NextResponse.next();
  }

  // 2. Protect dashboard, admin and profile routes
  const isProtectedPage = 
    pathname.startsWith("/dashboard") || 
    pathname.startsWith("/admin") || 
    pathname.startsWith("/profile");

  if (isProtectedPage) {
    return (authMiddleware as any)(req);
  }

  return NextResponse.next();
}

export const config = {
  matcher: ["/((?!api|_next|_vercel|.*\\..*).*)"]
};"#
                },
                {
                    "path": "prisma/schema.prisma",
                    "description": "Pre-configured Prisma schema for SQLite database.",
                    "language": "prisma",
                    "defaultContent": r#"generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "sqlite"
  url      = "file:./dev.db"
}

model User {
  id             String    @id @default(cuid())
  name           String?
  email          String    @unique
  emailVerified  DateTime?
  image          String?
  hashedPassword String?
  role           String    @default("USER")
  createdAt      DateTime  @default(now())
  updatedAt      DateTime  @updatedAt
  accounts       Account[]
  sessions       Session[]
}

model Account {
  id                String  @id @default(cuid())
  userId            String
  type              String
  provider          String
  providerAccountId String
  refresh_token     String?
  access_token      String?
  expires_at        Int?
  token_type        String?
  scope             String?
  id_token          String?
  session_state     String?
  user              User    @relation(fields: [userId], references: [id], onDelete: Cascade)

  @@unique([provider, providerAccountId])
}

model Session {
  id           String   @id @default(cuid())
  sessionToken String   @unique
  userId       String
  expires      DateTime
  user         User     @relation(fields: [userId], references: [id], onDelete: Cascade)
}

model VerificationToken {
  identifier String
  token      String   @unique
  expires    DateTime

  @@unique([identifier, token])
}"#
                },
                {
                    "path": "src/lib/prisma.ts",
                    "description": "PrismaClient singleton generator.",
                    "language": "typescript",
                    "defaultContent": r#"import { PrismaClient } from "@prisma/client";

const globalForPrisma = globalThis as unknown as { prisma: PrismaClient | undefined };

export const prisma =
  globalForPrisma.prisma ??
  new PrismaClient({
    log: ["query"],
  });

if (process.env.NODE_ENV !== "production") globalForPrisma.prisma = prisma;

export default prisma;"#
                },
                {
                    "path": "src/app/api/auth/[...nextauth]/route.ts",
                    "description": "NextAuth App Router route handler.",
                    "language": "typescript",
                    "defaultContent": r#"import NextAuth from "next-auth";
import CredentialsProvider from "next-auth/providers/credentials";
import { PrismaAdapter } from "@auth/prisma-adapter";
import bcrypt from "bcryptjs";
import prisma from "@/lib/prisma";

export const authOptions = {
  adapter: PrismaAdapter(prisma) as any,
  providers: [
    CredentialsProvider({
      name: "credentials",
      credentials: {
        email: { label: "Email", type: "email" },
        password: { label: "Password", type: "password" },
      },
      async authorize(credentials) {
        if (!credentials?.email || !credentials?.password) return null;
        try {
          const user = await prisma.user.findUnique({ where: { email: credentials.email } });
          if (!user) return null;
          const passwordToCompare = user.hashedPassword || (user as any).password || "";
          const passwordMatch = await bcrypt.compare(credentials.password, passwordToCompare);
          if (!passwordMatch) return null;
          return { id: String(user.id), email: user.email, name: user.name, role: user.role };
        } catch (error) {
          console.error("Auth error in CredentialsProvider:", error);
          return null;
        }
      },
    }),
  ],
  session: { strategy: "jwt" as const },
  callbacks: {
    async jwt({ token, user }: any) {
      if (user) {
        token.id = user.id;
        token.role = user.role;
      }
      return token;
    },
    async session({ session, token }: any) {
      if (session.user && token) {
        session.user.id = token.id;
        session.user.role = token.role;
      }
      return session;
    },
  },
  pages: {
    signIn: "/login",
  },
  secret: process.env.NEXTAUTH_SECRET || "axiom-dev-secret-change-in-production",
};

const handler = NextAuth(authOptions);
export { handler as GET, handler as POST };"#
                },
                {
                    "path": "src/app/api/register/route.ts",
                    "description": "User registration App Router API endpoint.",
                    "language": "typescript",
                    "defaultContent": r#"import { NextResponse } from "next/server";
import bcrypt from "bcryptjs";
import prisma from "@/lib/prisma";
import { z } from "zod";

const registerSchema = z.object({
  name: z.string().min(2).max(50),
  email: z.string().email(),
  password: z.string().min(8)
});

export async function POST(request: Request) {
  try {
    const body = await request.json();
    const { name, email, password } = registerSchema.parse(body);

    const existingUser = await prisma.user.findUnique({ where: { email } });
    if (existingUser) {
      return NextResponse.json({ message: "Email already in use" }, { status: 400 });
    }

    const hashedPassword = await bcrypt.hash(password, 10);

    const newUser = await prisma.user.create({
      data: {
        name,
        email,
        hashedPassword,
        role: "USER"
      }
    });

    return NextResponse.json(
      { message: "User registered successfully", user: { id: newUser.id, name: newUser.name, email: newUser.email } },
      { status: 201 }
    );
  } catch (error: any) {
    console.error("Registration error:", error);
    return NextResponse.json({ message: error.message || "Internal server error" }, { status: 500 });
  }
}"#
                },
                {
                    "path": "src/components/Navbar.tsx",
                    "description": "Modern responsive Navbar using prefetch-disabled links for Vercel optimization.",
                    "language": "typescript",
                    "defaultContent": r#""use client";

import React from "react";
import Link from "next/link";
import { useSession, signOut } from "next-auth/react";

const Navbar: React.FC = () => {
  const { data: session } = useSession();

  return (
    <nav className="bg-slate-900 text-white shadow-md w-full">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex items-center justify-between h-16">
          <div className="flex items-center">
            <Link href="/" prefetch={false} className="text-xl font-bold tracking-wider text-indigo-400">
              AxiomApp
            </Link>
            <div className="hidden md:block">
              <div className="ml-10 flex items-baseline space-x-4">
                <Link href="/dashboard" prefetch={false} className="px-3 py-2 rounded-md text-sm font-medium hover:bg-slate-800 transition-colors">
                  Dashboard
                </Link>
                <Link href="/profile" prefetch={false} className="px-3 py-2 rounded-md text-sm font-medium hover:bg-slate-800 transition-colors">
                  Profile
                </Link>
              </div>
            </div>
          </div>
          <div className="flex items-center space-x-4">
            {session ? (
              <div className="flex items-center space-x-4">
                <span className="text-sm text-slate-300">{session.user?.name || session.user?.email}</span>
                <button
                  onClick={() => signOut({ callbackUrl: "/" })}
                  className="bg-indigo-600 hover:bg-indigo-700 text-white px-4 py-2 rounded-md text-sm font-medium transition-colors"
                >
                  Logout
                </button>
              </div>
            ) : (
              <div className="flex space-x-2">
                <Link href="/login" prefetch={false} className="bg-slate-800 hover:bg-slate-700 text-white px-4 py-2 rounded-md text-sm font-medium transition-colors">
                  Sign In
                </Link>
                <Link href="/register" prefetch={false} className="bg-indigo-600 hover:bg-indigo-700 text-white px-4 py-2 rounded-md text-sm font-medium transition-colors">
                  Register
                </Link>
              </div>
            )}
          </div>
        </div>
      </div>
    </nav>
  );
};

export default Navbar;"#
                },
                {
                    "path": "src/components/Footer.tsx",
                    "description": "Clean footer utilizing prefoot-disabled links.",
                    "language": "typescript",
                    "defaultContent": r#"import React from "react";
import Link from "next/link";

const Footer: React.FC = () => {
  return (
    <footer className="bg-slate-900 text-slate-400 border-t border-slate-800 py-8 mt-auto w-full">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex flex-col md:flex-row justify-between items-center space-y-4 md:space-y-0">
          <div>
            <p className="text-sm">&copy; {new Date().getFullYear()} AxiomApp. All rights reserved.</p>
          </div>
          <div className="flex space-x-6">
            <Link href="/privacy" prefetch={false} className="text-sm hover:text-indigo-400 transition-colors">
              Privacy Policy
            </Link>
            <Link href="/terms" prefetch={false} className="text-sm hover:text-indigo-400 transition-colors">
              Terms of Service
            </Link>
          </div>
        </div>
      </div>
    </footer>
  );
};

export default Footer;"#
                },
                {
                    "path": "src/app/layout.tsx",
                    "description": "Root layout wrapped in SessionProvider.",
                    "language": "typescript",
                    "defaultContent": r#""use client";

import React from "react";
import { SessionProvider } from "next-auth/react";
import Navbar from "@/components/Navbar";
import Footer from "@/components/Footer";
import "./globals.css";

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <head>
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
      </head>
      <body className="flex flex-col min-h-screen bg-slate-950 text-slate-100">
        <SessionProvider>
          <Navbar />
          <main className="flex-grow flex flex-col w-full">{children}</main>
          <Footer />
        </SessionProvider>
      </body>
    </html>
  );
}"#
                }
            ],
            "injectTailwindDirectives": {
                "targetCssFiles": ["globals.css", "styles.css"]
            }
        });
    } else if project_type == "mern" {
        default_metamanifest = serde_json::json!({
            "rootFiles": ["package.json"],
            "rootFilesMoveFromSrc": true,
            "requiredFilesTemplates": [
                {
                    "path": "package.json",
                    "description": "Root package.json for MERN stack workspace.",
                    "language": "json",
                    "mergeDependencies": {
                        "dependencies": {
                            "express": "^4.19.2",
                            "cors": "^2.8.5",
                            "dotenv": "^16.4.5",
                            "jsonwebtoken": "^9.0.2",
                            "bcryptjs": "^2.4.3"
                        },
                        "devDependencies": {
                            "nodemon": "^3.1.0",
                            "concurrently": "^8.2.2"
                        },
                        "scripts": {
                            "dev": "concurrently \"npm run server\" \"npm run client\"",
                            "server": "nodemon src/server.js",
                            "client": "npm run dev --prefix client",
                            "start": "node src/server.js"
                        }
                    }
                },
                {
                    "path": "src/server.js",
                    "description": "Entry point for Express backend.",
                    "language": "javascript"
                },
                {
                    "path": "client/package.json",
                    "description": "Package settings for MERN frontend.",
                    "language": "json",
                    "mergeDependencies": {
                        "dependencies": {
                            "react": "^18.3.1",
                            "react-dom": "^18.3.1",
                            "react-router-dom": "^6.22.3",
                            "axios": "^1.6.8"
                        },
                        "devDependencies": {
                            "vite": "^5.2.0",
                            "tailwindcss": "^3.4.1"
                        }
                    }
                }
            ]
        });
    } else if project_type == "electron" {
        default_metamanifest = serde_json::json!({
            "rootFiles": ["package.json"],
            "rootFilesMoveFromSrc": true,
            "requiredFilesTemplates": [
                {
                    "path": "package.json",
                    "description": "Package settings for Electron desktop application.",
                    "language": "json",
                    "mergeDependencies": {
                        "devDependencies": {
                            "electron": "^30.0.0"
                        },
                        "scripts": {
                            "start": "electron ."
                        }
                    }
                },
                {
                    "path": "main.js",
                    "description": "Main process file for Electron app.",
                    "language": "javascript"
                },
                {
                    "path": "preload.js",
                    "description": "Preload script exposing secure APIs to renderer process.",
                    "language": "javascript"
                }
            ]
        });
    }

    // Deep merge default_metamanifest into metamanifest
    if !default_metamanifest.is_null() {
        if !metamanifest.is_object() {
            metamanifest = serde_json::json!({});
        }
        if let Some(mm_obj) = metamanifest.as_object_mut() {
            let dm_obj = default_metamanifest.as_object().unwrap();

            // 1. Merge rootFiles
            if let Some(dm_rf) = dm_obj.get("rootFiles").and_then(|rf| rf.as_array()) {
                if !mm_obj.contains_key("rootFiles") {
                    mm_obj.insert("rootFiles".to_string(), serde_json::json!(dm_rf));
                } else if let Some(mm_rf) = mm_obj.get_mut("rootFiles").and_then(|rf| rf.as_array_mut()) {
                    for default_file in dm_rf {
                        if let Some(df_str) = default_file.as_str() {
                            let df_lower = df_str.to_lowercase();
                            let exists = mm_rf.iter().any(|f| f.as_str().map(|s| s.to_lowercase()) == Some(df_lower.clone()));
                            if !exists {
                                mm_rf.push(default_file.clone());
                            }
                        }
                    }
                }
            }

            // 2. Merge requiredFilesTemplates
            if let Some(dm_templates) = dm_obj.get("requiredFilesTemplates").and_then(|t| t.as_array()) {
                if !mm_obj.contains_key("requiredFilesTemplates") {
                    mm_obj.insert("requiredFilesTemplates".to_string(), serde_json::json!(dm_templates));
                } else if let Some(mm_templates) = mm_obj.get_mut("requiredFilesTemplates").and_then(|t| t.as_array_mut()) {
                    for default_temp in dm_templates {
                        if let Some(dt_path) = default_temp.get("path").and_then(|p| p.as_str()) {
                            let dt_path_lower = dt_path.to_lowercase();
                            let exists = mm_templates.iter().any(|t| t.get("path").and_then(|p| p.as_str()).map(|s| s.to_lowercase()) == Some(dt_path_lower.clone()));
                            if !exists {
                                mm_templates.push(default_temp.clone());
                            }
                        }
                    }
                }
            }

            // 3. Merge rootFilesMoveFromSrc
            if !mm_obj.contains_key("rootFilesMoveFromSrc") {
                if let Some(val) = dm_obj.get("rootFilesMoveFromSrc") {
                    mm_obj.insert("rootFilesMoveFromSrc".to_string(), val.clone());
                }
            }

            // 4. Merge injectTailwindDirectives
            if !mm_obj.contains_key("injectTailwindDirectives") {
                if let Some(val) = dm_obj.get("injectTailwindDirectives") {
                    mm_obj.insert("injectTailwindDirectives".to_string(), val.clone());
                }
            }
        }
    }

    emit_progress("fetch-manifest", "Manifest secured and ready.", 10.0);

    // Load tech version overrides from settings
    let settings_path = get_base_dir().join("axiom-settings.json");
    let settings: Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    
    let overrides = settings.get("techOverrides");
    let mut overrides_desc = String::new();
    if let Some(ovs) = overrides.and_then(|o| o.as_object()) {
        for (k, v) in ovs {
            if let Some(v_str) = v.as_str() {
                if !v_str.is_empty() {
                    overrides_desc.push_str(&format!("- {}: {}\n", k, v_str));
                }
            }
        }
    }
    if overrides_desc.is_empty() {
        overrides_desc.push_str("- Use default stable versions\n");
    }

    // Ensure local Ollama server is running before manifest expansion (especially important on cold-start deep links)
    let _ = ollama_start_server().await;

    // Manifest Expansion Phase
    emit_progress("manifest-expansion", "Expanding manifest structure with local AI...", 12.0);
    
    let resolved_stack = manifest.get("resolvedStack").and_then(|s| s.as_str()).unwrap_or("Next.js");
    let platform = manifest.get("platform").and_then(|p| p.as_str()).unwrap_or("web");
    
    let original_files_json = serde_json::to_string_pretty(&files).unwrap_or_else(|_| "[]".to_string());
    
    let expansion_prompt = format!(
        "You are a Senior Project Architect. We are generating a complete {} application named '{}' using {}.\n\
         Project Idea & Core Domain Description: \"{}\"\n\
         The user requested a fully complete, production-ready implementation tailored specifically to this business domain.\n\
         Version Overrides requested by user:\n\
         {}\n\n\
         Original files skeleton in the manifest:\n\
         {}\n\n\
         Identify all missing files, routes, state management, components, configuration files, or database schemas needed to make this project complete.\n\
         CRITICAL DOMAIN RULE: Ensure the pages, dashboard, API routes, and Prisma schema models directly implement the core business features described in the Project Idea (not just generic user authentication).\n\
         IMPORTANT ARCHITECTURAL RULE: If the project uses next-auth (NextAuth) for authentication, you MUST always include 'src/app/api/register/route.ts' in the expanded files list to handle new user registration (creating user record using bcryptjs), since NextAuth does not natively provide a user registration route.\n\
         Output a JSON array containing the expanded, complete list of files. For each file, provide 'path', 'description', and 'language'.\n\
         Ensure all files from the original list are preserved (but you may update their descriptions to match the technology overrides and core domain features).\n\
         Respond ONLY with a valid JSON array. Do not include any introductory or concluding text, or markdown code blocks.",
         platform, name, resolved_stack, project_description, overrides_desc, original_files_json
    );

    let mut files_to_generate = files.clone();
    
    let manifest_runtime_cfg = get_ollama_runtime_config(&model_id);
    let ollama_req = serde_json::json!({
        "model": &model_id,
        "messages": [
            { "role": "system", "content": "You are a Senior Project Architect. Output only raw JSON. No markdown." },
            { "role": "user", "content": expansion_prompt }
        ],
        "stream": true,
        "options": build_ollama_options(&model_id, 0.2, Some(manifest_runtime_cfg.manifest_num_predict))
    });

    let mut expansion_success = false;
    let mut expansion_attempts = 0;
    let mut max_expansion_attempts = 3;
    let mut last_accumulated_response = String::new();

    while expansion_attempts < max_expansion_attempts && !expansion_success {
        expansion_attempts += 1;
        if expansion_attempts > 1 {
            debug_log_to_file(format!(
                "[Orchestrator] Retrying manifest expansion (Attempt {}/{})",
                expansion_attempts, max_expansion_attempts
            ));
            emit_progress("manifest-expansion", &format!("Retrying manifest expansion (attempt {}/{})...", expansion_attempts, max_expansion_attempts), 12.0);
        }

        let req_fut = client.post("http://127.0.0.1:11434/api/chat")
            .json(&ollama_req)
            .send();

        // 120-second timeout for manifest expansion request (initial connection)
        let resp_res = tokio::time::timeout(std::time::Duration::from_secs(120), req_fut).await;

        match resp_res {
            Ok(Ok(mut resp)) => {
                if resp.status().is_success() {
                    let mut accumulated_response = String::new();
                    let mut chars_received = 0;
                    let mut byte_buf = Vec::new();
                    let stream_start = std::time::Instant::now();
                    #[allow(unused_assignments)]
                    let mut last_chunk_time = stream_start;
                    let mut is_done = false;
                    let mut chunk_error = false;

                    loop {
                        // We want a chunk timeout of 30 seconds
                        let chunk_fut = resp.chunk();
                        let chunk_res = tokio::time::timeout(std::time::Duration::from_secs(30), chunk_fut).await;

                        match chunk_res {
                            Ok(Ok(Some(chunk))) => {
                                last_chunk_time = std::time::Instant::now();
                                byte_buf.extend_from_slice(&chunk);

                                // Process any complete lines in the buffer
                                while let Some(pos) = byte_buf.iter().position(|&b| b == b'\n') {
                                    let line_bytes = byte_buf.drain(..=pos).collect::<Vec<u8>>();
                                    if let Ok(line_str) = std::str::from_utf8(&line_bytes) {
                                        let line_trimmed = line_str.trim();
                                        if !line_trimmed.is_empty() {
                                            if let Ok(val) = serde_json::from_str::<Value>(line_trimmed) {
                                                if let Some(content) = val.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                                                    accumulated_response.push_str(content);
                                                    chars_received += content.len();
                                                }
                                                if let Some(done) = val.get("done").and_then(|d| d.as_bool()) {
                                                    if done {
                                                        is_done = true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if is_done {
                                    break;
                                }
                            }
                            Ok(Ok(None)) => {
                                // Stream ended
                                break;
                            }
                            Ok(Err(e)) => {
                                debug_log_to_file(format!("[Orchestrator] Error reading stream chunk (Attempt {}): {}", expansion_attempts, e));
                                chunk_error = true;
                                break;
                            }
                            Err(_) => {
                                debug_log_to_file(format!("[Orchestrator] Chunk read timed out after 30s (Attempt {})", expansion_attempts));
                                chunk_error = true;
                                break;
                            }
                        }

                        // Check global timeout for this attempt
                        let elapsed = stream_start.elapsed();
                        if elapsed.as_secs() > 120 {
                            if chars_received == 0 {
                                debug_log_to_file(format!("[Orchestrator] Stream has not started generating any content after 120s. Aborting attempt {}.", expansion_attempts));
                                chunk_error = true;
                                break;
                            } else {
                                // System has started writing the extended manifest. Check if it's progressing.
                                if last_chunk_time.elapsed().as_secs() > 30 {
                                    debug_log_to_file(format!("[Orchestrator] Stream stalled (no chunks for 30s) after 120s limit. Aborting attempt {}.", expansion_attempts));
                                    chunk_error = true;
                                    break;
                                } else if elapsed.as_secs() > 360 {
                                    debug_log_to_file(format!("[Orchestrator] Stream exceeded maximum allowed time of 360s. Aborting attempt {}.", expansion_attempts));
                                    chunk_error = true;
                                    break;
                                }
                            }
                        }
                    }

                    last_accumulated_response = accumulated_response.clone();

                    if !chunk_error {
                        let cleaned_json = extract_code_content(&accumulated_response);
                        if let Ok(expanded_files) = serde_json::from_str::<Vec<Value>>(&cleaned_json) {
                            if !expanded_files.is_empty() {
                                // Merge to ensure all original manifest files are preserved
                                let mut merged = expanded_files.clone();
                                for orig in files.iter() {
                                    if let Some(orig_path) = orig.get("path").and_then(|p| p.as_str()) {
                                        let exists = merged.iter().any(|f| {
                                            f.get("path").and_then(|p| p.as_str()) == Some(orig_path)
                                        });
                                        if !exists {
                                            merged.push(orig.clone());
                                        }
                                    }
                                }
                                files_to_generate = merged;
                                debug_log_to_file(format!(
                                    "[Orchestrator] Expanded and merged file list from {} to {} files using local Ollama (Attempt {}).",
                                    files.len(), files_to_generate.len(), expansion_attempts
                                ));
                                emit_progress("manifest-expansion", &format!("Manifest expanded to {} files.", files_to_generate.len()), 14.0);
                                expansion_success = true;
                            }
                        } else {
                            debug_log_to_file(format!(
                                "[Orchestrator] Failed to parse expanded manifest JSON (Attempt {}). Content: {}",
                                expansion_attempts, cleaned_json
                            ));
                            if expansion_attempts == 3 && chars_received > 0 {
                                debug_log_to_file("[Orchestrator] Attempt 3 failed to parse but generated content. Granting attempt 4.".to_string());
                                max_expansion_attempts = 4;
                            }
                        }
                    } else {
                        if expansion_attempts == 3 && chars_received > 0 {
                            debug_log_to_file("[Orchestrator] Attempt 3 stalled/errored but generated content. Granting attempt 4.".to_string());
                            max_expansion_attempts = 4;
                        }
                    }
                } else {
                    debug_log_to_file(format!("[Orchestrator] Manifest expansion Ollama status error (Attempt {}): {}", expansion_attempts, resp.status()));
                }
            }
            Ok(Err(e)) => {
                debug_log_to_file(format!("[Orchestrator] Manifest expansion request error (Attempt {}): {}", expansion_attempts, e));
                let _ = ollama_start_server().await;
            }
            Err(_) => {
                debug_log_to_file(format!("[Orchestrator] Manifest expansion initial connection timed out after 120s (Attempt {})", expansion_attempts));
            }
        }
    }

    if !expansion_success {
        debug_log_to_file(format!(
            "[Orchestrator] CRITICAL: Manifest expansion failed after {} attempts.\nLast accumulated raw content:\n{}",
            expansion_attempts, last_accumulated_response
        ));
        emit_progress("manifest-expansion", "Manifest expansion failed. Using original manifest files.", 14.0);
    }

    normalize_project_structure(&mut files_to_generate, &metamanifest, &project_type);

    if project_type == "nextjs" {
        let has_root_page = files_to_generate.iter().any(|f| {
            if let Some(p) = f.get("path").and_then(|path| path.as_str()) {
                let p_lower = p.to_lowercase().replace('\\', "/");
                p_lower == "src/app/page.tsx" || p_lower == "src/app/page.jsx" || p_lower == "src/app/page.js" || p_lower == "src/app/page.ts" ||
                p_lower == "app/page.tsx" || p_lower == "app/page.jsx" || p_lower == "app/page.js" || p_lower == "app/page.ts"
            } else {
                false
            }
        });
        
        if !has_root_page {
            let mut has_dashboard = false;
            let mut has_auth = false;
            let mut has_public = false;
            
            for f in &files_to_generate {
                if let Some(p) = f.get("path").and_then(|path| path.as_str()) {
                    let p_lower = p.to_lowercase().replace('\\', "/");
                    if p_lower.contains("dashboard/page") || p_lower.contains("admin/page") {
                        has_dashboard = true;
                    } else if p_lower.contains("auth/login") || p_lower.contains("login/page") || p_lower.contains("signin/page") {
                        has_auth = true;
                    } else if p_lower.contains("products/page") || p_lower.contains("store/page") || p_lower.contains("landing/page") {
                        has_public = true;
                    }
                }
            }
            
            let proj_desc = manifest.get("metadata").and_then(|m| m.get("description")).and_then(|d| d.as_str()).unwrap_or("");
            let proj_name = manifest.get("metadata").and_then(|m| m.get("name")).and_then(|n| n.as_str()).unwrap_or(name);
            
            let (ui_flow, file_desc, default_content) = if has_public || proj_desc.to_lowercase().contains("landing") || proj_desc.to_lowercase().contains("marketing") {
                ("Landing Page",
                 format!("Root landing page/homepage for '{}'. Render a premium, beautifully designed start screen introducing the application features, navigation header, and visual links to core screens. Description: {}.", proj_name, proj_desc),
                 format!("import React from 'react';\nimport Link from 'next/link';\n\nexport default function HomePage() {{\n  return (\n    <div className=\"min-h-screen bg-slate-900 text-white flex flex-col justify-between\" data-axiom-component=\"HomePage\" data-axiom-file=\"src/app/page.tsx\">\n      <header className=\"border-b border-slate-800 py-4 px-6 flex justify-between items-center\">\n        <h1 className=\"text-xl font-bold tracking-tight bg-gradient-to-r from-blue-400 to-indigo-500 bg-clip-text text-transparent\">{}</h1>\n        <div className=\"space-x-4\">\n          <Link href=\"/auth/login\" className=\"text-sm font-medium hover:text-blue-400 transition\">Sign In</Link>\n          <Link href=\"/dashboard\" className=\"text-sm bg-blue-600 hover:bg-blue-500 text-white px-4 py-2 rounded-md font-medium transition\">Dashboard</Link>\n        </div>\n      </header>\n      <main className=\"max-w-4xl mx-auto px-6 py-20 flex-grow flex flex-col justify-center text-center\">\n        <h2 className=\"text-5xl font-extrabold tracking-tight mb-6 bg-gradient-to-r from-white via-slate-200 to-slate-400 bg-clip-text text-transparent leading-tight\">\n          Welcome to {}\n        </h2>\n        <p className=\"text-lg text-slate-400 mb-10 max-w-xl mx-auto\">\n          {}\n        </p>\n        <div className=\"flex justify-center gap-4\">\n          <Link href=\"/dashboard\" className=\"bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-500 hover:to-indigo-500 text-white px-8 py-3 rounded-lg font-semibold shadow-lg shadow-blue-500/20 transition\">\n            Go to Dashboard\n          </Link>\n          <Link href=\"/auth/login\" className=\"border border-slate-700 hover:border-slate-600 px-8 py-3 rounded-lg font-semibold transition\">\n            Sign In\n          </Link>\n        </div>\n      </main>\n      <footer className=\"border-t border-slate-800 py-6 text-center text-sm text-slate-500\">\n        &copy; 2026 {}. All rights reserved.\n      </footer>\n    </div>\n  );\n}}", proj_name, proj_name, proj_desc, proj_name)
                )
            } else if has_auth && !has_dashboard {
                ("Auth-First Flow",
                 format!("Root page of the application '{}'. UI Flow: Auth-First. The application requires authentication to access. Verify session or render a beautiful gateway card directing the user to the Sign In page (/auth/login). Description: {}.", proj_name, proj_desc),
                 format!("import React from 'react';\nimport Link from 'next/link';\n\nexport default function HomePage() {{\n  return (\n    <div className=\"min-h-screen bg-slate-900 text-white flex flex-col justify-center items-center p-6\" data-axiom-component=\"HomePage\" data-axiom-file=\"src/app/page.tsx\">\n      <div className=\"max-w-md w-full bg-slate-800 border border-slate-700 rounded-2xl p-8 text-center shadow-xl\">\n        <h1 className=\"text-2xl font-bold mb-4\">{}</h1>\n        <p className=\"text-slate-400 mb-6\">Authentication is required to access the application features.</p>\n        <Link href=\"/auth/login\" className=\"inline-block w-full bg-blue-600 hover:bg-blue-500 text-white font-semibold py-3 px-6 rounded-lg transition\">\n          Sign In to Account\n        </Link>\n      </div>\n    </div>\n  );\n}}", proj_name)
                )
            } else {
                ("Dashboard-First Flow",
                 format!("Root page of the application '{}'. UI Flow: Dashboard-First. The app is a workspace/dashboard. The homepage should perform a server-side redirect to the Dashboard (/dashboard) using Next.js 'redirect'. Description: {}.", proj_name, proj_desc),
                 "import { redirect } from 'next/navigation';\n\nexport default function HomePage() {\n  redirect('/dashboard');\n}".to_string()
                )
            };
            
            debug_log_to_file(format!("[Orchestrator] Dynamic UI Flow detected: {}. Adding custom page.tsx template.", ui_flow));
            
            // Insert it into files_to_generate
            files_to_generate.push(serde_json::json!({
                "path": "src/app/page.tsx",
                "role": "logic",
                "language": "typescript",
                "description": file_desc,
                "defaultContent": default_content
            }));
        }
    }

    let has_tailwind = files_to_generate.iter().any(|f| {
        if let Some(p) = f.get("path").and_then(|p| p.as_str()) {
            let p_lower = p.to_lowercase();
            p_lower.contains("tailwind") || p_lower.contains("globals.css")
        } else {
            false
        }
    }) || project_type == "nextjs";

    // Sort files_to_generate by generation phase
    files_to_generate.sort_by_key(|file_spec| {
        let file_path = file_spec.get("path").and_then(|p| p.as_str()).unwrap_or("");
        get_generation_phase(file_path, &project_type)
    });

    emit_progress("generate-files", "Checking local AI engine...", 15.0);

    // 2. Generation Loop
    let total_files = files_to_generate.len() as f64;
    let proj_dir = get_project_path(&project_id);

    // Initialize and write the Live Builder Guide (LBG) to disk
    let axiom_dir = proj_dir.join(".axiom");
    let _ = fs::create_dir_all(&axiom_dir);
    let lbg_path = axiom_dir.join("live_builder_guide.json");
    
    if let Some(lbg_val) = manifest.get("liveBuilderGuide") {
        if let Ok(lbg_content) = serde_json::to_string_pretty(lbg_val) {
            let _ = fs::write(&lbg_path, lbg_content);
        }
    } else if !lbg_path.exists() {
        // Fallback: Generate a default LBG guide locally if not sent by the Web App
        let default_lbg = serde_json::json!({
            "projectSpecification": {
                "stack": resolved_stack,
                "importExportStyle": "named",
                "importPathStyle": "absolute-alias",
                "typescriptStrictness": "strict-false",
                "namingConventions": {
                    "components": "PascalCase",
                    "hooks": "camelCase",
                    "routes": "kebab-case",
                    "models": "PascalCase"
                }
            },
            "sharedContracts": {
                "database": {
                    "models": {}
                },
                "api": {
                    "endpoints": {}
                },
                "hooks": {}
            },
            "generationPipeline": []
        });
        if let Ok(lbg_content) = serde_json::to_string_pretty(&default_lbg) {
            let _ = fs::write(&lbg_path, lbg_content);
        }
    }

    for (i, file_spec) in files_to_generate.iter().enumerate() {
        let file_path = file_spec.get("path").and_then(|p| p.as_str()).unwrap_or("unknown.txt");
        let desc = file_spec.get("description").and_then(|d| d.as_str()).unwrap_or("");
        let lang = file_spec.get("language").and_then(|l| l.as_str()).unwrap_or("txt");

        let msg = format!("Generating {}/{} - {}", i + 1, files_to_generate.len(), file_path);
        let current_progress = 25.0 + ((i as f64) / total_files) * 55.0;
        emit_progress("generate-files", &msg, current_progress);

        let overrides_desc_str = if !overrides_desc.trim().is_empty() {
            format!("\nUse the following version/stack preferences if applicable:\n{}", overrides_desc)
        } else {
            "".to_string()
        };

        let pkg_db_summary = get_packages_db_summary();
        let context_ref = get_context_files_for(file_path, &proj_dir);
        let routes_ref = get_project_routes_prompt(&files_to_generate);

        let mut system_prompt = get_expert_system_prompt(
            file_path,
            &project_type,
            &overrides_desc_str,
            &pkg_db_summary,
            &context_ref,
            &routes_ref,
            &proj_dir,
        );

        if file_path == "package.json" || file_path.ends_with("/package.json") {
            let mut pkg_extra = " IMPORTANT: When generating package.json, always use modern, stable dependency versions".to_string();
            if let Some(nextjs_v) = overrides.and_then(|o| o.get("nextjs")).and_then(|n| n.as_str()) {
                if !nextjs_v.is_empty() {
                    pkg_extra.push_str(&format!(" (specifically Next.js {} and React {})", nextjs_v, overrides.and_then(|o| o.get("react")).and_then(|r| r.as_str()).unwrap_or("18")));
                }
            }
            pkg_extra.push_str(". Do NOT use outdated versions.");
            system_prompt.push_str(&pkg_extra);
        }
        let user_prompt = if project_description.is_empty() {
            format!("Project: {}\nGenerate {} code for file: {}\n\nDescription: {}", name, lang, file_path, desc)
        } else {
            format!(
                "Project: {} ({})\nGenerate {} code for file: {}\n\nFile Specification: {}\n\nIMPORTANT: Ensure all UI text, domain entities, metrics, and business logic reflect the Project description above rather than generic placeholders.",
                name, project_description, lang, file_path, desc
            )
        };

        let mut processed_content = String::new();
        let mut file_gen_success = false;
        let mut attempts = 0;
        let max_attempts = 3;
        let mut current_user_prompt = user_prompt.clone();

        while attempts < max_attempts && !file_gen_success {
            attempts += 1;
            if attempts > 1 {
                debug_log_to_file(format!(
                    "[Orchestrator] Retrying file generation for {} (Attempt {}/{})",
                    file_path, attempts, max_attempts
                ));
                emit_progress(
                    "generate-files",
                    &format!("Retrying {}/{} (attempt {}/{}) - {}", i + 1, files_to_generate.len(), attempts, max_attempts, file_path),
                    current_progress
                );
            }

            let ollama_req = serde_json::json!({
                "model": model_id.clone(), 
                "messages": [
                    { "role": "system", "content": &system_prompt },
                    { "role": "user", "content": &current_user_prompt }
                ],
                "stream": true,
                "options": build_ollama_options(&model_id, 0.3, Some(4096))
            });

            let req_fut = client.post("http://127.0.0.1:11434/api/chat")
                .json(&ollama_req)
                .send();

            // 180-second connection timeout for local LLMs with CPU offload
            let resp_res = tokio::time::timeout(std::time::Duration::from_secs(180), req_fut).await;

            let mut resp = match resp_res {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    debug_log_to_file(format!("[Orchestrator] Connection error for {} (Attempt {}): {}", file_path, attempts, e));
                    let _ = ollama_start_server().await;
                    continue;
                }
                Err(_) => {
                    debug_log_to_file(format!("[Orchestrator] Connection timeout for {} (Attempt {})", file_path, attempts));
                    continue;
                }
            };

            if !resp.status().is_success() {
                debug_log_to_file(format!("[Orchestrator] Ollama status error {} for {} (Attempt {})", resp.status(), file_path, attempts));
                continue;
            }

            let mut full_content = String::new();
            let mut token_count = 0;
            let mut loop_detected = false;
            let mut stream_stalled = false;

            loop {
                let chunk_fut = resp.chunk();
                // 60-second timeout per streaming chunk
                let chunk_res = tokio::time::timeout(std::time::Duration::from_secs(60), chunk_fut).await;

                let chunk = match chunk_res {
                    Ok(Ok(Some(c))) => c,
                    Ok(Ok(None)) => break, // normal end of stream
                    Ok(Err(e)) => {
                        debug_log_to_file(format!("[Orchestrator] Chunk read error for {} (Attempt {}): {}", file_path, attempts, e));
                        stream_stalled = true;
                        break;
                    }
                    Err(_) => {
                        debug_log_to_file(format!("[Orchestrator] Streaming chunk timeout for {} (Attempt {})", file_path, attempts));
                        stream_stalled = true;
                        break;
                    }
                };

                if let Ok(str_chunk) = std::str::from_utf8(&chunk) {
                    for line in str_chunk.lines() {
                        if line.is_empty() { continue; }
                        if let Ok(parsed) = serde_json::from_str::<Value>(line) {
                            if let Some(content) = parsed.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                                full_content.push_str(content);
                                token_count += 1;

                                if token_count % 50 == 0 {
                                    emit_progress("generate-files", &format!("AI typing {}... ({} tokens)", file_path, token_count), current_progress);
                                }

                                // Detect repetition loop only after 100 tokens every 30 tokens to avoid early false positives
                                if token_count >= 100 && token_count % 30 == 0 && detect_infinite_loop(&full_content) {
                                    debug_log_to_file(format!(
                                        "[Orchestrator] Infinite loop detected for {} (Attempt {}/{} at token {})!",
                                        file_path, attempts, max_attempts, token_count
                                    ));
                                    loop_detected = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if loop_detected {
                    break;
                }
            }

            if loop_detected || stream_stalled {
                continue;
            }

            // Clean markdown backticks
            let cleaned = extract_code_content(&full_content);
            processed_content = post_process_generated_file(file_path, &cleaned, &metamanifest, has_tailwind);
            
            // Run abstract validation agent
            match validate_file_with_abstract_rules(file_path, &processed_content, &model_id).await {
                Ok(()) => {
                    file_gen_success = true;
                }
                Err(validation_error) => {
                    debug_log_to_file(format!(
                        "[Validator] Validation failed for {} (Attempt {}/{}): {}. Retrying with feedback.",
                        file_path, attempts, max_attempts, validation_error
                    ));
                    if attempts < max_attempts {
                        current_user_prompt = format!(
                            "{}\n\nIMPORTANT: The previous attempt failed validation with the following error:\n{}\nPlease correct this error in your new implementation.",
                            user_prompt, validation_error
                        );
                        file_gen_success = false;
                    } else {
                        // Accept anyway to avoid blockages
                        file_gen_success = true;
                    }
                }
            }
        }

        if !file_gen_success {
            if !processed_content.trim().is_empty() {
                debug_log_to_file(format!(
                    "[Orchestrator] Warning: Max attempts reached for {}. Using partially processed content to avoid blocking pipeline.",
                    file_path
                ));
            } else {
                debug_log_to_file(format!(
                    "[Orchestrator] Warning: Max attempts reached for {}. Generating safe fallback stub to avoid blocking pipeline.",
                    file_path
                ));
                let fallback = get_safe_fallback_content(file_path, lang, desc);
                processed_content = post_process_generated_file(file_path, &fallback, &metamanifest, has_tailwind);
            }
        }

        // 3. Save File
        let full_disk_path = proj_dir.join(file_path);
        if let Some(parent) = full_disk_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&full_disk_path, &processed_content);
    }

    // Setup local PostgreSQL database if project uses Prisma
    let has_prisma = proj_dir.join("prisma/schema.prisma").exists() || proj_dir.join("schema.prisma").exists();
    if has_prisma {
        emit_progress("install-deps", "Checking database setup...", 74.0);
        let db_name = name.replace("-", "_").replace(" ", "_").to_lowercase();
        
        #[cfg(target_os = "windows")]
        {
            emit_progress("install-deps", "Configuring local PostgreSQL database...", 74.2);
            match setup_local_postgres(&proj_dir, &db_name).await {
                Ok(db_url) => {
                    update_env_database_url(&proj_dir, &db_url);
                    emit_progress("install-deps", "Local PostgreSQL database configured successfully.", 74.8);
                }
                Err(err) => {
                    emit_progress("install-deps", &format!("PostgreSQL setup failed: {}. Falling back to SQLite...", err), 74.5);
                    let _ = fallback_to_sqlite(&proj_dir);
                }
            }
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            emit_progress("install-deps", "Non-Windows environment detected. Using local SQLite database fallback.", 74.5);
            let _ = fallback_to_sqlite(&proj_dir);
        }
    }

    // Ensure theme file exists if imported in layout files
    ensure_theme_file_exists(&proj_dir);

    // Resolve any broken local imports in generated JS/TS code
    repair_broken_imports(&proj_dir, &model_id).await;

    // Repair package.json dependencies based on actual source code imports
    repair_package_json_dependencies(&proj_dir);

    // Repair Cargo.toml dependencies if they exist
    repair_cargo_toml_dependencies(&proj_dir);

    // Repair pubspec.yaml dependencies if they exist
    repair_pubspec_yaml_dependencies(&proj_dir);

    // Run npm install with automatic self-healing and version locking
    run_npm_install_with_auto_healing(&proj_dir, &window, "", Some(&task_id)).await;

    // Auto-run prisma generate & db push if Prisma schema exists in the project
    let has_prisma = proj_dir.join("prisma/schema.prisma").exists() || proj_dir.join("schema.prisma").exists();
    if has_prisma {
        if let Err(e) = heal_prisma_schema_loop(&proj_dir, &window, &model_id, "install-deps").await {
            debug_log_to_file(format!("[Prisma Critical Error] Orchestrator schema healing failed: {}", e));
        }

        // Run prisma db push to synchronize the database schema
        emit_progress("install-deps", "Syncing database schema...", 79.0);
        let local_prisma_js = proj_dir.join("node_modules").join("prisma").join("build").join("index.js");
        let mut push_cmd = if local_prisma_js.exists() {
            let mut c = std::process::Command::new("node");
            c.arg(&local_prisma_js).arg("db").arg("push").arg("--accept-data-loss").current_dir(&proj_dir);
            c
        } else {
            #[cfg(target_os = "windows")]
            let mut c = std::process::Command::new("cmd");
            #[cfg(target_os = "windows")]
            c.arg("/C").arg("npx").arg("-y").arg("prisma@5.22.0").arg("db").arg("push").arg("--accept-data-loss").current_dir(&proj_dir);
            
            #[cfg(not(target_os = "windows"))]
            let mut c = std::process::Command::new("npx");
            #[cfg(not(target_os = "windows"))]
            c.arg("-y").arg("prisma@5.22.0").arg("db").arg("push").arg("--accept-data-loss").current_dir(&proj_dir);
            c
        };
        
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            push_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        
        if let Ok(mut child) = push_cmd.spawn() {
            let _ = child.wait();
        }
    }

    // Run self-healing compiler loop to automatically correct any compilation or syntax errors
    perform_self_healing_loop(&proj_dir, &model_id, &files_to_generate).await;

    // Generate Axiom Map (axiom-map.json)
    if let Err(e) = generate_axiom_map(&proj_dir) {
        debug_log_to_file(format!("[Axiom Map] Greška pri kreiranju axiom-map.json: {}", e));
    }

    // Generate project dependency graph metadata
    generate_project_metadata(&proj_dir, &project_id, &files_to_generate);
    generate_axiom_features_json(&proj_dir, name, &files_to_generate);

    // 4. Save Project State
    emit_progress("save-project", "Saving project files...", 80.0);
    
    let new_proj = serde_json::json!({
        "id": project_id,
        "name": name,
        "description": project_description,
        "status": "ready",
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "updatedAt": chrono::Utc::now().to_rfc3339(),
    });
    let _ = project_save(new_proj);

    emit_progress("complete", "Generation completed successfully!", 100.0);

    Ok(())
}

#[tauri::command]
pub async fn task_start_generation(
    window: tauri::Window,
    manifest_id: String,
    project_id: Option<String>,
    token: String,
    model_id: Option<String>,
    host: Option<String>,
) -> Result<String, String> {
    let p_id = project_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let task_id = format!("task-{}", uuid::Uuid::new_v4());
    let model = model_id.clone().unwrap_or_else(|| "qwen2.5-coder:7b".to_string());
    
    debug_log_to_file(format!(
        "[IPC Rust Command] task_start_generation called. manifest_id: {}, project_id: {:?}, model_id: {:?}, host: {:?}",
        manifest_id, project_id, model_id, host
    ));

    let t_id = task_id.clone();
    let p_id_clone = p_id.clone();
    let host_clone = host.clone();
    tokio::spawn(async move {
        debug_log_to_file(format!("[IPC Rust Command] Spawned tokio worker task for task_id: {}", t_id));
        let err_proj_id = p_id_clone.clone();
        if let Err(e) = run_orchestrator(window.clone(), t_id.clone(), manifest_id, p_id_clone, token, model, host_clone).await {
            debug_log_to_file(format!("[IPC Rust Command] run_orchestrator failed for task_id {}: {}", t_id, e));
            let _ = window.emit("task:progress", serde_json::json!({
                "taskId": t_id,
                "projectId": err_proj_id,
                "phase": "error",
                "message": e,
                "progress": 0.0
            }));
        } else {
            debug_log_to_file(format!("[IPC Rust Command] run_orchestrator succeeded for task_id: {}", t_id));
        }
    });

    Ok(task_id)
}
