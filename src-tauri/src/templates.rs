//! Deterministic file templates (v0.1.63).
//!
//! Infrastructure files (tsconfig, next/postcss/tailwind config, globals.css with design tokens,
//! Prisma singleton, NextAuth options/route, `/api/register`, Electron main/preload) and the
//! shared `src/types/index.ts` contract do NOT need creative output from the local 7B model.
//! They are rendered here from fixed templates plus a structured IR parsed from
//! `prisma/schema.prisma` (models, enums, fields, relations), so they are always syntactically
//! and semantically consistent with the database schema and take ~0 ms instead of minutes.
//!
//! Safety rules:
//! - only `nextjs` and `electron` project types are templated (mobile intentionally excluded),
//! - every template is optional: when its prerequisites are missing (e.g. no parsable schema,
//!   no `User` password field) `render_deterministic_file` returns `None` and the regular LLM
//!   pipeline handles the file,
//! - `AXIOM_DISABLE_TEMPLATES=1` disables everything (A/B comparison).

use std::path::Path;

#[derive(Debug, Clone)]
pub struct PrismaField {
    pub name: String,
    pub ty: String,
    pub optional: bool,
    pub list: bool,
    pub is_relation: bool,
    pub is_enum: bool,
}

#[derive(Debug, Clone)]
pub struct PrismaModel {
    pub name: String,
    pub fields: Vec<PrismaField>,
}

#[derive(Debug, Clone)]
pub struct PrismaEnum {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PrismaIr {
    pub models: Vec<PrismaModel>,
    pub enums: Vec<PrismaEnum>,
}

#[derive(Debug, Clone)]
pub struct DeterministicFile {
    pub content: String,
    /// Extra files that must exist for `content` to compile (path relative to project root, content).
    pub side_files: Vec<(String, String)>,
}

pub fn templates_enabled() -> bool {
    !matches!(
        std::env::var("AXIOM_DISABLE_TEMPLATES").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    )
}

/// Parses `schema.prisma` into a small structured IR (single source of truth for generated contracts).
pub fn parse_prisma_schema(src: &str) -> PrismaIr {
    let mut ir = PrismaIr::default();
    let mut block_kind = String::new();
    let mut block_name = String::new();
    let mut cur_fields: Vec<(String, String, bool, bool)> = Vec::new();
    let mut cur_values: Vec<String> = Vec::new();
    let mut raw_models: Vec<(String, Vec<(String, String, bool, bool)>)> = Vec::new();

    for line in src.lines() {
        let t = line.trim();
        if block_kind.is_empty() {
            let parts: Vec<&str> = t.split_whitespace().collect();
            if parts.len() >= 2 && (parts[0] == "model" || parts[0] == "enum") && t.contains('{') {
                block_kind = parts[0].to_string();
                block_name = parts[1].trim_end_matches('{').to_string();
                cur_fields.clear();
                cur_values.clear();
                if t.ends_with('}') {
                    // empty one-line block
                    block_kind.clear();
                }
            }
            continue;
        }
        if t.starts_with('}') {
            if block_kind == "model" {
                raw_models.push((block_name.clone(), cur_fields.clone()));
            } else {
                ir.enums.push(PrismaEnum { name: block_name.clone(), values: cur_values.clone() });
            }
            block_kind.clear();
            continue;
        }
        if t.is_empty() || t.starts_with("//") || t.starts_with("@@") {
            continue;
        }
        if block_kind == "enum" {
            if let Some(v) = t.split_whitespace().next() {
                if v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    cur_values.push(v.to_string());
                }
            }
        } else {
            let parts: Vec<&str> = t.split_whitespace().collect();
            if parts.len() >= 2 {
                let raw_ty = parts[1];
                let list = raw_ty.contains("[]");
                let optional = raw_ty.contains('?');
                let ty = raw_ty.replace("[]", "").replace('?', "");
                cur_fields.push((parts[0].to_string(), ty, optional, list));
            }
        }
    }

    let enum_names: Vec<String> = ir.enums.iter().map(|e| e.name.clone()).collect();
    let model_names: Vec<String> = raw_models.iter().map(|(n, _)| n.clone()).collect();
    for (name, fields) in raw_models {
        let fields = fields
            .into_iter()
            .map(|(fname, ty, optional, list)| PrismaField {
                is_relation: model_names.contains(&ty),
                is_enum: enum_names.contains(&ty),
                name: fname,
                ty,
                optional,
                list,
            })
            .collect();
        ir.models.push(PrismaModel { name, fields });
    }
    ir
}

fn ts_type_for_field(f: &PrismaField) -> String {
    let base = if f.is_relation {
        "any".to_string()
    } else if f.is_enum {
        format!("{} | string", f.ty)
    } else {
        match f.ty.as_str() {
            "String" => "string".to_string(),
            "Int" | "Float" | "Decimal" | "BigInt" => "number".to_string(),
            "Boolean" => "boolean".to_string(),
            _ => "any".to_string(), // DateTime, Json, Bytes, unsupported
        }
    };
    if f.list {
        if base.contains('|') {
            format!("({})[]", base)
        } else {
            format!("{}[]", base)
        }
    } else {
        base
    }
}

const TYPE_NAME_STOPWORDS: &[&str] = &[
    "Shared", "TypeScript", "Prisma", "Next", "React", "Tailwind", "NextAuth", "Zod", "Interface", "Interfaces",
    "Types", "Type", "Enum", "Enums", "Model", "Models", "Core", "Domain", "Contracts", "Contract", "Stripe",
    "Json", "JSON", "Electron", "Postgre", "PostgreSQL", "Root", "Layout", "Page", "Pages", "API", "Api",
    "Client", "Server", "Hook", "Hooks", "Component", "Components", "Props", "The", "This", "That", "With",
    "Exports", "Export", "Used", "Uses", "Using", "For", "And", "From",
];

/// Renders the shared `src/types/index.ts` from the Prisma IR.
/// `extra_names` are additional PascalCase type names mentioned in the manifest description of the
/// file (e.g. `DashboardStats`); they are emitted as permissive stubs so existing imports never break.
pub fn render_types_index(ir: &PrismaIr, extra_names: &[String]) -> String {
    let mut out = String::from("// Auto-generated deterministically from prisma/schema.prisma — shared view-model contracts.\n\n");
    let mut defined: Vec<String> = Vec::new();

    for e in &ir.enums {
        let type_fields: Vec<String> = e.values.iter().map(|v| format!("readonly {}: '{}'", v, v)).collect();
        let entries: Vec<String> = e.values.iter().map(|v| format!("  {}: '{}',", v, v)).collect();
        out.push_str(&format!(
            "export const {}: {{ {}; [key: string]: any }} = {{\n{}\n}};\nexport type {} = (typeof {})[keyof typeof {}] | string;\n\n",
            e.name,
            type_fields.join("; "),
            entries.join("\n"),
            e.name,
            e.name,
            e.name
        ));
        defined.push(e.name.clone());
    }

    for m in &ir.models {
        out.push_str(&format!("export interface {} {{\n", m.name));
        for f in &m.fields {
            if f.name == "id" && !f.optional {
                out.push_str("  id: any;\n");
            } else {
                out.push_str(&format!("  {}?: {};\n", f.name, ts_type_for_field(f)));
            }
        }
        out.push_str("\n  [key: string]: any;\n}\n\n");
        out.push_str(&format!(
            "export type {m}CreateInput = any;\n\nexport type {m}UpdateInput = any;\n\n",
            m = m.name
        ));
        defined.push(m.name.clone());
        defined.push(format!("{}CreateInput", m.name));
        defined.push(format!("{}UpdateInput", m.name));
    }

    for name in extra_names {
        if defined.iter().any(|d| d == name) {
            continue;
        }
        out.push_str(&format!("export interface {} {{\n  id?: any;\n  [key: string]: any;\n}}\n\n", name));
        defined.push(name.clone());
    }
    out
}

fn extract_extra_type_names(desc: &str, ir: &PrismaIr) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Ok(re) = regex::Regex::new(r"\b[A-Z][a-z0-9]+(?:[A-Z][A-Za-z0-9]*)+\b") else {
        return out;
    };
    for m in re.find_all_compat(desc) {
        let name = m.to_string();
        if TYPE_NAME_STOPWORDS.contains(&name.as_str()) {
            continue;
        }
        if ir.models.iter().any(|x| x.name == name) || ir.enums.iter().any(|x| x.name == name) {
            continue;
        }
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

trait FindAllCompat {
    fn find_all_compat<'a>(&self, hay: &'a str) -> Vec<&'a str>;
}
impl FindAllCompat for regex::Regex {
    fn find_all_compat<'a>(&self, hay: &'a str) -> Vec<&'a str> {
        self.find_iter(hay).map(|m| m.as_str()).collect()
    }
}

fn find_user_password_field(ir: &PrismaIr) -> Option<String> {
    let user = ir.models.iter().find(|m| m.name == "User")?;
    for cand in ["hashedPassword", "passwordHash", "hashed_password", "password"] {
        if user.fields.iter().any(|f| f.name == cand && f.ty == "String") {
            return Some(cand.to_string());
        }
    }
    None
}

fn user_has_field(ir: &PrismaIr, name: &str) -> bool {
    ir.models
        .iter()
        .find(|m| m.name == "User")
        .map_or(false, |u| u.fields.iter().any(|f| f.name == name))
}

pub fn render_auth_options(pw_field: &str, has_role: bool, has_name: bool) -> String {
    let role_expr = if has_role { "user.role ?? \"USER\"" } else { "\"USER\"" };
    let name_expr = if has_name { "user.name ?? null" } else { "null" };
    format!(
        r#"import type {{ NextAuthOptions }} from "next-auth";
import CredentialsProvider from "next-auth/providers/credentials";
import bcrypt from "bcryptjs";
import prisma from "@/lib/prisma";

export const authOptions: NextAuthOptions = {{
  providers: [
    CredentialsProvider({{
      name: "credentials",
      credentials: {{
        email: {{ label: "Email", type: "email" }},
        password: {{ label: "Password", type: "password" }},
      }},
      async authorize(credentials) {{
        if (!credentials?.email || !credentials?.password) return null;
        try {{
          const user: any = await prisma.user.findFirst({{ where: {{ email: credentials.email }} }});
          if (!user) return null;
          const passwordHash: string = user.{pw} || "";
          if (!passwordHash) return null;
          const passwordMatch = await bcrypt.compare(credentials.password, passwordHash);
          if (!passwordMatch) return null;
          return {{ id: String(user.id), email: user.email, name: {name}, role: {role} }} as any;
        }} catch (error) {{
          console.error("Auth error in CredentialsProvider:", error);
          return null;
        }}
      }},
    }}),
  ],
  session: {{ strategy: "jwt" }},
  callbacks: {{
    async jwt({{ token, user }}: any) {{
      if (user) {{
        token.id = user.id;
        token.role = user.role;
      }}
      return token;
    }},
    async session({{ session, token }}: any) {{
      if (session.user && token) {{
        (session.user as any).id = token.id;
        (session.user as any).role = token.role;
      }}
      return session;
    }},
  }},
  pages: {{ signIn: "/login" }},
  secret: process.env.NEXTAUTH_SECRET || "axiom-dev-secret-change-in-production",
}};

export default authOptions;
"#,
        pw = pw_field,
        name = name_expr,
        role = role_expr
    )
}

pub fn render_nextauth_route() -> String {
    "import NextAuth from \"next-auth\";\nimport { authOptions } from \"@/lib/auth\";\n\nconst handler = NextAuth(authOptions);\n\nexport { handler as GET, handler as POST };\n"
        .to_string()
}

pub fn render_register_route(pw_field: &str, has_name: bool) -> String {
    let name_line = if has_name { "        name: displayName,\n" } else { "" };
    format!(
        r#"import {{ NextResponse }} from "next/server";
import bcrypt from "bcryptjs";
import prisma from "@/lib/prisma";
import {{ z }} from "zod";

const registerSchema = z.object({{
  name: z.string().trim().min(1).max(100).optional(),
  email: z.string().email(),
  password: z.string().min(8),
}});

export async function POST(request: Request) {{
  try {{
    const body = await request.json();
    const parsed = registerSchema.safeParse(body);
    if (!parsed.success) {{
      return NextResponse.json(
        {{ message: parsed.error.issues[0]?.message || "Invalid registration payload" }},
        {{ status: 400 }}
      );
    }}
    const {{ name, email, password }} = parsed.data;
    const displayName = name || email.split("@")[0];

    const existingUser = await prisma.user.findFirst({{ where: {{ email }} }});
    if (existingUser) {{
      return NextResponse.json({{ message: "Email already in use" }}, {{ status: 400 }});
    }}

    const hashedPassword = await bcrypt.hash(password, 10);
    const newUser: any = await prisma.user.create({{
      data: {{
{name_line}        email,
        {pw}: hashedPassword,
      }} as any,
    }});

    return NextResponse.json(
      {{ message: "User registered successfully", user: {{ id: newUser.id, name: newUser.name ?? displayName, email: newUser.email }} }},
      {{ status: 201 }}
    );
  }} catch (error: any) {{
    console.error("Registration error:", error);
    return NextResponse.json({{ message: error?.message || "Internal server error" }}, {{ status: 500 }});
  }}
}}
"#,
        name_line = name_line,
        pw = pw_field
    )
}

pub fn render_login_page() -> String {
    r#""use client";

import React, { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { signIn } from "next-auth/react";

export default function LoginPage() {
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setIsLoading(true);
    try {
      const result = await signIn("credentials", {
        email,
        password,
        redirect: false,
      });
      if (result?.error || result?.ok === false) {
        setError("Invalid email or password.");
        return;
      }
      router.push("/dashboard");
      router.refresh();
    } catch {
      setError("Unable to sign in right now. Please try again.");
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-background text-foreground px-4 py-12">
      <div className="w-full max-w-md bg-surface border border-border rounded-2xl shadow-lg p-8 space-y-6">
        <div className="text-center space-y-2">
          <h1 className="text-2xl font-bold tracking-tight text-foreground">Sign in</h1>
          <p className="text-sm text-muted-foreground">Access your workspace dashboard</p>
        </div>

        {error && (
          <div className="rounded-xl bg-danger/10 border border-danger/30 px-4 py-3 text-sm text-danger">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label htmlFor="email" className="block text-sm font-medium text-foreground">
              Email
            </label>
            <input
              id="email"
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@example.com"
              className="w-full px-3.5 py-2.5 rounded-xl border border-border bg-surface text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
            />
          </div>

          <div className="space-y-1.5">
            <label htmlFor="password" className="block text-sm font-medium text-foreground">
              Password
            </label>
            <input
              id="password"
              type="password"
              required
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="Enter your password"
              className="w-full px-3.5 py-2.5 rounded-xl border border-border bg-surface text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
            />
          </div>

          <button
            type="submit"
            disabled={isLoading}
            className="w-full inline-flex items-center justify-center rounded-xl bg-primary text-primary-foreground font-semibold px-4 py-2.5 shadow-sm hover:opacity-90 transition disabled:opacity-50"
          >
            {isLoading ? "Signing in..." : "Sign In"}
          </button>
        </form>

        <p className="text-center text-sm text-muted-foreground">
          Don&apos;t have an account?{" "}
          <Link href="/register" prefetch={false} className="font-semibold text-primary hover:underline">
            Create an account
          </Link>
        </p>
      </div>
    </div>
  );
}
"#
    .to_string()
}

pub fn render_register_page() -> String {
    r#""use client";

import React, { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { signIn } from "next-auth/react";

export default function RegisterPage() {
  const router = useRouter();
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    if (password.length < 8) {
      setError("Password must be at least 8 characters long.");
      return;
    }
    setIsLoading(true);
    try {
      const res = await fetch("/api/register", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: name.trim() || undefined, email, password }),
      });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) {
        setError(data?.message || "Registration failed.");
        return;
      }

      const loginRes = await signIn("credentials", {
        email,
        password,
        redirect: false,
      });
      if (loginRes?.error) {
        router.push("/login");
        return;
      }
      router.push("/dashboard");
      router.refresh();
    } catch {
      setError("Unable to complete registration. Please try again.");
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-background text-foreground px-4 py-12">
      <div className="w-full max-w-md bg-surface border border-border rounded-2xl shadow-lg p-8 space-y-6">
        <div className="text-center space-y-2">
          <h1 className="text-2xl font-bold tracking-tight text-foreground">Create account</h1>
          <p className="text-sm text-muted-foreground">Set up your workspace credentials</p>
        </div>

        {error && (
          <div className="rounded-xl bg-danger/10 border border-danger/30 px-4 py-3 text-sm text-danger">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label htmlFor="name" className="block text-sm font-medium text-foreground">
              Full Name
            </label>
            <input
              id="name"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Alex Rivera"
              className="w-full px-3.5 py-2.5 rounded-xl border border-border bg-surface text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
            />
          </div>

          <div className="space-y-1.5">
            <label htmlFor="email" className="block text-sm font-medium text-foreground">
              Email
            </label>
            <input
              id="email"
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@example.com"
              className="w-full px-3.5 py-2.5 rounded-xl border border-border bg-surface text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
            />
          </div>

          <div className="space-y-1.5">
            <label htmlFor="password" className="block text-sm font-medium text-foreground">
              Password (min. 8 chars)
            </label>
            <input
              id="password"
              type="password"
              required
              minLength={8}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="At least 8 characters"
              className="w-full px-3.5 py-2.5 rounded-xl border border-border bg-surface text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
            />
          </div>

          <button
            type="submit"
            disabled={isLoading}
            className="w-full inline-flex items-center justify-center rounded-xl bg-primary text-primary-foreground font-semibold px-4 py-2.5 shadow-sm hover:opacity-90 transition disabled:opacity-50"
          >
            {isLoading ? "Creating account..." : "Create Account"}
          </button>
        </form>

        <p className="text-center text-sm text-muted-foreground">
          Already have an account?{" "}
          <Link href="/login" prefetch={false} className="font-semibold text-primary hover:underline">
            Sign in
          </Link>
        </p>
      </div>
    </div>
  );
}
"#
    .to_string()
}

const TSCONFIG_TEMPLATE: &str = r#"{
  "compilerOptions": {
    "lib": ["dom", "dom.iterable", "esnext"],
    "allowJs": true,
    "skipLibCheck": true,
    "strict": false,
    "strictNullChecks": false,
    "noEmit": true,
    "esModuleInterop": true,
    "module": "esnext",
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "jsx": "preserve",
    "incremental": true,
    "plugins": [{ "name": "next" }],
    "paths": {
      "@/*": ["./src/*", "./*"]
    }
  },
  "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"],
  "exclude": ["node_modules"]
}
"#;

const NEXT_CONFIG_TEMPLATE: &str = "/** @type {import('next').NextConfig} */\nconst nextConfig = {\n  reactStrictMode: true,\n  swcMinify: true,\n  eslint: { ignoreDuringBuilds: true },\n};\n\nmodule.exports = nextConfig;\n";

const POSTCSS_TEMPLATE: &str = "module.exports = {\n  plugins: {\n    tailwindcss: {},\n    autoprefixer: {},\n  },\n};\n";

const TAILWIND_TEMPLATE: &str = r#"/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./src/**/*.{js,ts,jsx,tsx,mdx}",
    "./app/**/*.{js,ts,jsx,tsx,mdx}",
    "./components/**/*.{js,ts,jsx,tsx,mdx}",
    "./pages/**/*.{js,ts,jsx,tsx,mdx}",
  ],
  theme: {
    extend: {
      // Semantic design tokens: change the CSS variables in globals.css to re-theme the whole app.
      colors: {
        background: "rgb(var(--background) / <alpha-value>)",
        foreground: "rgb(var(--foreground) / <alpha-value>)",
        surface: "rgb(var(--surface) / <alpha-value>)",
        primary: "rgb(var(--primary) / <alpha-value>)",
        "primary-foreground": "rgb(var(--primary-foreground) / <alpha-value>)",
        secondary: "rgb(var(--secondary) / <alpha-value>)",
        accent: "rgb(var(--accent) / <alpha-value>)",
        muted: "rgb(var(--muted) / <alpha-value>)",
        "muted-foreground": "rgb(var(--muted-foreground) / <alpha-value>)",
        border: "rgb(var(--border) / <alpha-value>)",
        success: "rgb(var(--success) / <alpha-value>)",
        warning: "rgb(var(--warning) / <alpha-value>)",
        danger: "rgb(var(--danger) / <alpha-value>)",
      },
    },
  },
  plugins: [],
};
"#;

const GLOBALS_CSS_TEMPLATE: &str = r#"@tailwind base;
@tailwind components;
@tailwind utilities;

:root {
  --background: 248 250 252;
  --foreground: 15 23 42;
  --surface: 255 255 255;
  --primary: 79 70 229;
  --primary-foreground: 255 255 255;
  --secondary: 100 116 139;
  --accent: 14 165 233;
  --muted: 241 245 249;
  --muted-foreground: 100 116 139;
  --border: 226 232 240;
  --success: 16 185 129;
  --warning: 245 158 11;
  --danger: 244 63 94;
}

html {
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}
"#;

const ELECTRON_MAIN_TEMPLATE: &str = "const { app, BrowserWindow } = require('electron');\nconst path = require('path');\n\nlet mainWindow = null;\n\nfunction createWindow() {\n  mainWindow = new BrowserWindow({\n    width: 1280,\n    height: 820,\n    webPreferences: {\n      preload: path.join(__dirname, 'preload.js'),\n      contextIsolation: true,\n      nodeIntegration: false,\n    },\n  });\n\n  const startUrl = process.env.ELECTRON_START_URL || 'http://localhost:3000';\n  mainWindow.loadURL(startUrl);\n\n  mainWindow.on('closed', () => {\n    mainWindow = null;\n  });\n}\n\napp.whenReady().then(createWindow);\n\napp.on('window-all-closed', () => {\n  if (process.platform !== 'darwin') app.quit();\n});\n\napp.on('activate', () => {\n  if (BrowserWindow.getAllWindows().length === 0) createWindow();\n});\n";

const ELECTRON_PRELOAD_TEMPLATE: &str = "const { contextBridge, ipcRenderer } = require('electron');\n\ncontextBridge.exposeInMainWorld('electronAPI', {\n  invoke: (channel, data) => ipcRenderer.invoke(channel, data),\n  on: (channel, callback) => ipcRenderer.on(channel, (_event, ...args) => callback(...args)),\n});\n";

fn read_ir(proj_dir: &Path) -> Option<PrismaIr> {
    let src = std::fs::read_to_string(proj_dir.join("prisma").join("schema.prisma")).ok()?;
    let ir = parse_prisma_schema(&src);
    if ir.models.is_empty() {
        None
    } else {
        Some(ir)
    }
}

pub fn render_providers_component(has_auth: bool) -> String {
    if has_auth {
        concat!(
            "\"use client\";\n\n",
            "import React from \"react\";\n",
            "import { SessionProvider } from \"next-auth/react\";\n\n",
            "export function Providers({ children }: { children: React.ReactNode }) {\n",
            "  return <SessionProvider>{children}</SessionProvider>;\n",
            "}\n\n",
            "export default Providers;\n"
        )
        .to_string()
    } else {
        concat!(
            "\"use client\";\n\n",
            "import React from \"react\";\n\n",
            "export function Providers({ children }: { children: React.ReactNode }) {\n",
            "  return <>{children}</>;\n",
            "}\n\n",
            "export default Providers;\n"
        )
        .to_string()
    }
}

fn read_pkg_meta(proj_dir: &Path) -> (String, String, bool) {
    let mut title = String::from("Axiom App");
    let mut desc = String::from("Generated with Axiom Forge");
    let mut pkg_has_auth = false;
    if let Ok(raw) = std::fs::read_to_string(proj_dir.join("package.json")) {
        pkg_has_auth = raw.contains("\"next-auth\"");
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(n) = val.get("name").and_then(|v| v.as_str()) {
                let t = n.trim();
                if !t.is_empty() && t != "generated-package" {
                    title = t.to_string();
                }
            }
            if let Some(d) = val.get("description").and_then(|v| v.as_str()) {
                let t = d.trim();
                if !t.is_empty() {
                    desc = t.to_string();
                }
            }
        }
    }
    (title, desc, pkg_has_auth)
}

fn project_has_auth(proj_dir: &Path, spec_desc: &str) -> bool {
    let (_, _, pkg_has_auth) = read_pkg_meta(proj_dir);
    if pkg_has_auth || spec_desc.to_lowercase().contains("sessionprovider") {
        return true;
    }
    read_ir(proj_dir)
        .and_then(|ir| find_user_password_field(&ir))
        .is_some()
}

pub fn render_root_layout(proj_dir: &Path, spec_desc: &str) -> String {
    let (title, desc, _) = read_pkg_meta(proj_dir);
    let title_json = serde_json::to_string(&title).unwrap_or_else(|_| "\"Axiom App\"".to_string());
    let desc_json = serde_json::to_string(&desc).unwrap_or_else(|_| "\"Generated with Axiom Forge\"".to_string());
    let spec_low = spec_desc.to_lowercase();
    let has_navbar = spec_low.contains("navbar")
        || proj_dir.join("src/components/Navbar.tsx").is_file()
        || proj_dir.join("components/Navbar.tsx").is_file();
    let has_footer = spec_low.contains("footer")
        || proj_dir.join("src/components/Footer.tsx").is_file()
        || proj_dir.join("components/Footer.tsx").is_file();

    let navbar_imp = if has_navbar { "import Navbar from \"@/components/Navbar\";\n" } else { "" };
    let footer_imp = if has_footer { "import Footer from \"@/components/Footer\";\n" } else { "" };
    let navbar_jsx = if has_navbar { "          <Navbar />\n" } else { "" };
    let footer_jsx = if has_footer { "\n          <Footer />" } else { "" };

    format!(
        "import type {{ Metadata }} from \"next\";\n\
import \"./globals.css\";\n\
import {{ Providers }} from \"@/components/Providers\";\n\
{}{}\n\
export const metadata: Metadata = {{\n  \
  title: {},\n  \
  description: {},\n\
}};\n\n\
export default function RootLayout({{ children }}: {{ children: React.ReactNode }}) {{\n  \
  return (\n    \
    <html lang=\"en\">\n      \
      <body className=\"min-h-screen flex flex-col bg-background text-foreground antialiased\">\n        \
        <Providers>\n{}\
          <main className=\"flex-1 flex flex-col w-full\">{{children}}</main>{}\n        \
        </Providers>\n      \
      </body>\n    \
    </html>\n  \
  );\n\
}}\n",
        navbar_imp, footer_imp, title_json, desc_json, navbar_jsx, footer_jsx
    )
}

/// Returns deterministic content for `file_path`, or `None` when the LLM must generate it.
pub fn render_deterministic_file(
    proj_dir: &Path,
    project_type: &str,
    file_path: &str,
    spec_desc: &str,
) -> Option<DeterministicFile> {
    if !templates_enabled() {
        return None;
    }
    if crate::templates_expo::is_expo_project_type(project_type) {
        return crate::templates_expo::render_expo_deterministic_file(
            proj_dir,
            project_type,
            file_path,
            spec_desc,
        );
    }
    let is_electron = project_type == "electron";
    if project_type != "nextjs" && !is_electron {
        return None;
    }
    let p = file_path.replace('\\', "/").trim_start_matches("./").to_lowercase();
    let plain = |content: &str| Some(DeterministicFile { content: content.to_string(), side_files: Vec::new() });

    match p.as_str() {
        "tsconfig.json" => return plain(TSCONFIG_TEMPLATE),
        "next.config.js" => return plain(NEXT_CONFIG_TEMPLATE),
        "postcss.config.js" => return plain(POSTCSS_TEMPLATE),
        "tailwind.config.js" => return plain(TAILWIND_TEMPLATE),
        "main.js" if is_electron => return plain(ELECTRON_MAIN_TEMPLATE),
        "preload.js" if is_electron => return plain(ELECTRON_PRELOAD_TEMPLATE),
        _ => {}
    }

    if p == "src/app/globals.css" || p == "app/globals.css" {
        return plain(GLOBALS_CSS_TEMPLATE);
    }

    if p == "src/app/layout.tsx" || p == "app/layout.tsx" {
        let has_auth = project_has_auth(proj_dir, spec_desc);
        let providers_path = if p.starts_with("src/") {
            "src/components/Providers.tsx".to_string()
        } else {
            "components/Providers.tsx".to_string()
        };
        return Some(DeterministicFile {
            content: render_root_layout(proj_dir, spec_desc),
            side_files: vec![(providers_path, render_providers_component(has_auth))],
        });
    }

    if p == "src/components/providers.tsx" || p == "components/providers.tsx" {
        let has_auth = project_has_auth(proj_dir, spec_desc);
        return plain(&render_providers_component(has_auth));
    }

    if p == "src/lib/prisma.ts" || p == "lib/prisma.ts" {
        return plain(
            "import { PrismaClient } from \"@prisma/client\";\n\nconst globalForPrisma = globalThis as unknown as { prisma: PrismaClient | undefined };\n\nexport const prisma = globalForPrisma.prisma ?? new PrismaClient();\n\nif (process.env.NODE_ENV !== \"production\") globalForPrisma.prisma = prisma;\n\nexport default prisma;\n",
        );
    }

    if p == "src/types/index.ts" {
        let ir = read_ir(proj_dir)?;
        let extra = extract_extra_type_names(spec_desc, &ir);
        return plain(&render_types_index(&ir, &extra));
    }

    let is_login_page = p == "src/app/login/page.tsx" || p == "app/login/page.tsx";
    let is_register_page = p == "src/app/register/page.tsx" || p == "app/register/page.tsx";
    if is_login_page || is_register_page {
        let ir = read_ir(proj_dir)?;
        find_user_password_field(&ir)?;
        if !user_has_field(&ir, "email") {
            return None;
        }
        return if is_login_page {
            plain(&render_login_page())
        } else {
            plain(&render_register_page())
        };
    }

    let is_auth_options = p == "src/lib/auth.ts" || p == "lib/auth.ts";
    let is_nextauth_route = p.ends_with("api/auth/[...nextauth]/route.ts");
    let is_register_route = p.ends_with("api/register/route.ts");
    if is_auth_options || is_nextauth_route || is_register_route {
        let ir = read_ir(proj_dir)?;
        let pw = find_user_password_field(&ir)?;
        if !user_has_field(&ir, "email") {
            return None;
        }
        let has_role = user_has_field(&ir, "role");
        let has_name = user_has_field(&ir, "name");
        if is_auth_options {
            return plain(&render_auth_options(&pw, has_role, has_name));
        }
        if is_register_route {
            return plain(&render_register_route(&pw, has_name));
        }
        let auth_path = if p.starts_with("src/") { "src/lib/auth.ts" } else { "lib/auth.ts" };
        return Some(DeterministicFile {
            content: render_nextauth_route(),
            side_files: vec![(auth_path.to_string(), render_auth_options(&pw, has_role, has_name))],
        });
    }

    None
}

/// Records which files were produced deterministically (`.axiom/deterministic_files.json`).
pub fn record_deterministic_file(proj_dir: &Path, file_path: &str) {
    let axiom = proj_dir.join(".axiom");
    let _ = std::fs::create_dir_all(&axiom);
    let path = axiom.join("deterministic_files.json");
    let mut list: Vec<String> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if !list.iter().any(|x| x == file_path) {
        list.push(file_path.to_string());
        if let Ok(s) = serde_json::to_string_pretty(&list) {
            let _ = std::fs::write(&path, s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = "generator client {\n  provider = \"prisma-client-js\"\n}\nenum Role {\n  USER\n  ADMIN\n}\nenum PainLevel {\n  NONE\n  MILD\n}\nmodel User {\n  id String @id @default(cuid())\n  email String @unique\n  name String?\n  hashedPassword String?\n  role Role @default(USER)\n  createdAt DateTime @default(now())\n  entries PainEntry[]\n}\nmodel PainEntry {\n  id String @id @default(cuid())\n  level PainLevel\n  score Int\n  note String?\n  tags String[]\n  user User @relation(fields: [userId], references: [id])\n  userId String\n  @@index([userId])\n}\n";

    fn temp_proj(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "axiom_tmpl_{}_{}",
            tag,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(d.join("prisma")).unwrap();
        d
    }

    #[test]
    fn parses_prisma_ir_and_renders_types() {
        let ir = parse_prisma_schema(SCHEMA);
        assert_eq!(ir.enums.len(), 2);
        assert_eq!(ir.models.len(), 2);
        let entry = ir.models.iter().find(|m| m.name == "PainEntry").unwrap();
        assert!(entry.fields.iter().any(|f| f.name == "user" && f.is_relation));
        assert!(entry.fields.iter().any(|f| f.name == "level" && f.is_enum));
        assert!(entry.fields.iter().any(|f| f.name == "tags" && f.list));
        let ts = render_types_index(&ir, &["DashboardStats".to_string()]);
        assert!(ts.contains("export const PainLevel:"), "{}", ts);
        assert!(ts.contains("export type PainLevel ="), "{}", ts);
        assert!(ts.contains("level?: PainLevel | string;"), "{}", ts);
        assert!(ts.contains("score?: number;"), "{}", ts);
        assert!(ts.contains("tags?: string[];"), "{}", ts);
        assert!(ts.contains("user?: any;"), "{}", ts);
        assert!(ts.contains("createdAt?: any;"), "{}", ts);
        assert!(ts.contains("  id: any;"), "{}", ts);
        assert!(ts.contains("export type PainEntryCreateInput = any;"), "{}", ts);
        assert!(ts.contains("export type PainEntryUpdateInput = any;"), "{}", ts);
        assert!(ts.contains("export interface DashboardStats"), "{}", ts);
    }

    #[test]
    fn renders_server_root_layout_with_client_providers_and_preserves_metadata() {
        let dir = temp_proj("server_layout");
        std::fs::write(dir.join("prisma/schema.prisma"), SCHEMA).unwrap();
        std::fs::write(
            dir.join("package.json"),
            serde_json::json!({
                "name": "Acme FinOps",
                "description": "Cloud spend governance suite",
                "dependencies": { "next": "14.2.35", "next-auth": "^4.24.7" }
            })
            .to_string(),
        )
        .unwrap();

        let det = render_deterministic_file(
            &dir,
            "nextjs",
            "src/app/layout.tsx",
            "Root layout wrapping SessionProvider, Navbar, main content, and Footer.",
        )
        .unwrap();
        assert!(!det.content.contains("\"use client\""));
        assert!(!det.content.contains("'use client'"));
        assert!(det.content.contains("export const metadata: Metadata = {"));
        assert!(det.content.contains("\"Acme FinOps\""));
        assert!(det.content.contains("<Providers>"));
        assert!(det.content.contains("<Navbar />"));
        assert!(det.content.contains("<Footer />"));

        let prov = det
            .side_files
            .iter()
            .find(|(p, _)| p == "src/components/Providers.tsx")
            .expect("missing Providers.tsx side file");
        assert!(prov.1.starts_with("\"use client\";"));
        assert!(prov.1.contains("<SessionProvider>{children}</SessionProvider>"));

        let mm = serde_json::json!({
            "requiredFilesTemplates": [{
                "path": "package.json",
                "mergeDependencies": { "dependencies": { "next-auth": "^4.24.7" } }
            }]
        });
        let post = crate::sanitizer::post_process_generated_file("src/app/layout.tsx", &det.content, &mm, false);
        assert!(!post.contains("\"use client\""));
        assert!(!post.contains("<SessionProvider"));
        assert!(post.contains("export const metadata: Metadata = {"));
        assert!(post.contains("<Providers>"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renders_infra_and_auth_templates_only_when_prereqs_hold() {
        let dir = temp_proj("auth");
        std::fs::write(dir.join("prisma/schema.prisma"), SCHEMA).unwrap();

        let tw = render_deterministic_file(&dir, "nextjs", "tailwind.config.js", "").unwrap();
        assert!(tw.content.contains("rgb(var(--primary) / <alpha-value>)"));
        let css = render_deterministic_file(&dir, "nextjs", "src/app/globals.css", "").unwrap();
        assert!(css.content.contains("--primary:") && !css.content.contains("@apply"));

        let route = render_deterministic_file(&dir, "nextjs", "src/app/api/auth/[...nextauth]/route.ts", "").unwrap();
        assert!(route.content.contains("handler as GET, handler as POST"));
        assert!(!route.content.contains("export const authOptions"), "route.ts must not export authOptions");
        assert_eq!(route.side_files.len(), 1);
        assert!(route.side_files[0].1.contains("user.hashedPassword"));

        let reg = render_deterministic_file(&dir, "nextjs", "src/app/api/register/route.ts", "").unwrap();
        assert!(reg.content.contains("export async function POST") && !reg.content.contains("export default"));
        assert!(reg.content.contains("hashedPassword: hashedPassword"));

        let login_pg = render_deterministic_file(&dir, "electron", "src/app/login/page.tsx", "").unwrap();
        assert!(login_pg.content.contains("signIn(\"credentials\"") && login_pg.content.contains("redirect: false") && login_pg.content.contains("bg-primary"));
        let reg_pg = render_deterministic_file(&dir, "nextjs", "src/app/register/page.tsx", "").unwrap();
        assert!(reg_pg.content.contains("/api/register") && reg_pg.content.contains("signIn(\"credentials\"") && reg_pg.content.contains("bg-surface"));

        // Unsupported stacks are excluded from web/desktop templates, Expo ignores Tailwind/Next, and Electron-only files need electron type.
        assert!(render_deterministic_file(&dir, "flutter", "tsconfig.json", "").is_none());
        assert!(render_deterministic_file(&dir, "react_native", "tailwind.config.js", "").is_none());
        assert!(render_deterministic_file(&dir, "nextjs", "main.js", "").is_none());
        assert!(render_deterministic_file(&dir, "electron", "main.js", "").is_some());

        // No password field in User -> auth templates (including login/register pages) must fall back to the LLM.
        let schema_no_pw = SCHEMA.replace("  hashedPassword String?\n", "");
        std::fs::write(dir.join("prisma/schema.prisma"), schema_no_pw).unwrap();
        assert!(render_deterministic_file(&dir, "nextjs", "src/app/api/register/route.ts", "").is_none());
        assert!(render_deterministic_file(&dir, "nextjs", "src/app/login/page.tsx", "").is_none());
        assert!(render_deterministic_file(&dir, "nextjs", "src/app/register/page.tsx", "").is_none());

        // Missing schema -> types must fall back to the LLM.
        std::fs::remove_file(dir.join("prisma/schema.prisma")).unwrap();
        assert!(render_deterministic_file(&dir, "nextjs", "src/types/index.ts", "").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prevents_duplicate_providers_import_across_side_effect_css_and_hygiene_pass() {
        let with_semi = "import \"./globals.css\";\nimport Providers from \"@/components/Providers\";\nexport default function RootLayout({ children }: { children: React.ReactNode }) { return <Providers>{children}</Providers>; }\n";
        let known_semi = crate::self_healing::collect_declared_or_imported_idents(with_semi);
        assert!(known_semi.contains("Providers"), "AST must detect default Providers import after side-effect import");

        let no_semi = "import './globals.css'\nimport Providers from '@/components/Providers'\nexport default function RootLayout({ children }: { children: React.ReactNode }) { return <Providers>{children}</Providers> }\n";
        let known_no_semi = crate::self_healing::collect_declared_or_imported_idents(no_semi);
        assert!(known_no_semi.contains("Providers"), "AST must detect ASI import without semicolon");

        let merged = crate::self_healing::merge_or_insert_import_statement(
            with_semi,
            "@/components/Providers",
            "Providers",
            false,
        );
        assert_eq!(merged, with_semi, "must not insert duplicate named Providers import");

        let dir = temp_proj("layout_hygiene");
        std::fs::create_dir_all(dir.join("src/app")).unwrap();
        std::fs::create_dir_all(dir.join("src/components")).unwrap();
        let det = render_deterministic_file(
            &dir,
            "nextjs",
            "src/app/layout.tsx",
            "Root layout wrapping SessionProvider and children.",
        )
        .unwrap();
        assert!(det.content.contains("import { Providers } from \"@/components/Providers\";"));
        std::fs::write(dir.join("src/app/layout.tsx"), &det.content).unwrap();
        for (side_rel, side_src) in &det.side_files {
            std::fs::write(dir.join(side_rel), side_src).unwrap();
        }
        crate::self_healing::run_hygiene_pass_on_project(&dir);
        let after = std::fs::read_to_string(dir.join("src/app/layout.tsx")).unwrap();
        let provider_imp_count = after.matches("@/components/Providers").count();
        assert_eq!(provider_imp_count, 1, "expected exactly 1 Providers import, got:\n{}", after);
        assert!(after.contains("export const metadata: Metadata = {"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preserves_use_user_tier_contracts_strict_null_checks_and_cleans_pages_api() {
        let dir = temp_proj("strict_null_and_tier");
        std::fs::create_dir_all(dir.join("src/app/api/doctors")).unwrap();
        std::fs::create_dir_all(dir.join("src/pages/api/doctors")).unwrap();
        std::fs::create_dir_all(dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(dir.join("src/app/account")).unwrap();

        std::fs::write(
            dir.join("package.json"),
            r#"{"name":"clinic","dependencies":{"next":"14.2.35","react":"18.2.0"}}"#,
        )
        .unwrap();
        let ts_det = render_deterministic_file(&dir, "nextjs", "tsconfig.json", "").unwrap();
        assert!(ts_det.content.contains("\"strictNullChecks\": false"));
        // Also verify ensure_valid_tsconfig backfills strictNullChecks: false on an older strict:false tsconfig
        std::fs::write(
            dir.join("tsconfig.json"),
            r#"{"compilerOptions":{"strict":false,"baseUrl":"."},"include":["**/*.ts","**/*.tsx"]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("src/app/api/doctors/route.ts"), "export async function GET() { return new Response('ok'); }\n").unwrap();
        std::fs::write(dir.join("src/pages/api/doctors/[id].ts"), "export default function handler() {}\n").unwrap();
        std::fs::write(
            dir.join("src/hooks/useUserTier.ts"),
            concat!(
                "export type UserTierId = 'Starter' | 'Pro';\n",
                "export interface TierLimits {\n  seats: number;\n  monthlyOperations: number;\n}\n",
                "export interface UserTierState {\n  tier: UserTierId;\n  limits: TierLimits;\n  setTier: (nextTier: UserTierId) => void;\n}\n",
                "export const useUserTier = (): UserTierState => ({ tier: 'Starter', limits: { seats: 3, monthlyOperations: 5000 }, setTier: () => {} });\n"
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join("src/app/account/page.tsx"),
            "\"use client\";\nimport { useUserTier } from '@/hooks/useUserTier';\nexport default function AccountPage() { const { limits } = useUserTier(); return <div>{limits.seats}</div>; }\n",
        )
        .unwrap();

        crate::self_healing::run_hygiene_pass_on_project(&dir);

        assert!(!dir.join("src/pages/api").exists(), "stale src/pages/api should be removed");
        assert!(!dir.join("src/pages").exists(), "empty src/pages should be removed");
        let ts_raw = std::fs::read_to_string(dir.join("tsconfig.json")).unwrap();
        let ts_val: serde_json::Value = serde_json::from_str(&ts_raw).unwrap();
        assert_eq!(ts_val["compilerOptions"]["strictNullChecks"], serde_json::json!(false));

        let tier_src = std::fs::read_to_string(dir.join("src/hooks/useUserTier.ts")).unwrap();
        assert!(tier_src.contains("limits: TierLimits;"), "useUserTier limits must stay required:\n{}", tier_src);
        assert!(tier_src.contains("seats: number;"), "TierLimits seats must stay required:\n{}", tier_src);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn redirects_react_hook_form_imports_out_of_zod() {
        let mixed = concat!(
            "\"use client\";\n",
            "import { z, useForm, Controller } from \"zod\";\n",
            "export function SupplierForm() { return useForm(); }\n"
        );
        let fixed_mixed = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(mixed, "src/components/SupplierForm.tsx");
        assert!(
            fixed_mixed.contains("import { z } from \"zod\";"),
            "expected z kept in zod import, got:\n{}",
            fixed_mixed
        );
        assert!(
            !fixed_mixed.contains("useForm } from \"zod\"") && fixed_mixed.contains("react-hook-form"),
            "expected useForm & Controller moved to react-hook-form, got:\n{}",
            fixed_mixed
        );
        assert!(
            fixed_mixed.contains("useForm") && fixed_mixed.contains("Controller"),
            "expected both symbols preserved, got:\n{}",
            fixed_mixed
        );

        let only_hook = concat!(
            "\"use client\";\n",
            "import { useForm } from \"zod\";\n",
            "export function FormOnly() { return useForm(); }\n"
        );
        let fixed_only = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(only_hook, "src/components/FormOnly.tsx");
        assert!(
            !fixed_only.contains("from \"zod\"") && fixed_only.contains("import { useForm } from 'react-hook-form';"),
            "expected empty zod import removed and react-hook-form inserted, got:\n{}",
            fixed_only
        );
    }

    #[test]
    fn deduplicates_exported_hook_interfaces_and_widens_z_infer_models() {
        let dir = temp_proj("dedup_hook_and_zinfer");
        std::fs::create_dir_all(dir.join("src/types")).unwrap();
        std::fs::create_dir_all(dir.join("src/models")).unwrap();
        std::fs::create_dir_all(dir.join("src/hooks")).unwrap();

        std::fs::write(
            dir.join("src/types/index.ts"),
            "export interface Revision {\n  id: any;\n  version?: string;\n  [key: string]: any;\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/hooks/useRevisions.ts"),
            concat!(
                "import { useState } from 'react';\n",
                "export interface Revision {\n",
                "  id?: any;\n",
                "  version?: string;\n",
                "}\n",
                "export function useRevisions() {\n",
                "  const [revisions] = useState<Revision[]>([]);\n",
                "  return { revisions };\n",
                "}\n"
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join("src/models/Appointment.ts"),
            concat!(
                "import { z } from 'zod';\n",
                "export const appointmentSchema = z.object({ id: z.string() });\n",
                "export type Appointment = z.infer<typeof appointmentSchema>;\n"
            ),
        )
        .unwrap();

        crate::ast_engine::deduplicate_local_domain_interfaces_against_types_in_project(&dir);
        crate::ast_engine::reconcile_model_and_type_contracts_in_project(&dir);

        let hook_after = std::fs::read_to_string(dir.join("src/hooks/useRevisions.ts")).unwrap();
        assert!(
            hook_after.contains("import type { Revision } from '@/types';"),
            "expected import type Revision from @/types, got:\n{}",
            hook_after
        );
        assert!(
            hook_after.contains("export type { Revision };"),
            "expected re-export type Revision, got:\n{}",
            hook_after
        );
        assert!(
            !hook_after.contains("export interface Revision"),
            "expected duplicate export interface Revision removed, got:\n{}",
            hook_after
        );

        let model_after = std::fs::read_to_string(dir.join("src/models/Appointment.ts")).unwrap();
        assert!(
            model_after.contains("z.infer<typeof appointmentSchema> & Record<string, any>"),
            "expected z.infer widened with Record<string, any>, got:\n{}",
            model_after
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn adds_companion_type_for_const_enum_objects_idempotently() {
        let src = concat!(
            "export const AppointmentStatus: { SCHEDULED: 'SCHEDULED'; COMPLETED: 'COMPLETED' } = {\n",
            "  SCHEDULED: 'SCHEDULED',\n",
            "  COMPLETED: 'COMPLETED',\n",
            "};\n"
        );
        let once = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(src, "src/utils/dateUtils.ts");
        assert!(
            once.contains("export type AppointmentStatus = (typeof AppointmentStatus)[keyof typeof AppointmentStatus] | string;"),
            "expected companion AppointmentStatus type, got:\n{}",
            once
        );
        let twice = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(&once, "src/utils/dateUtils.ts");
        assert_eq!(
            twice.matches("export type AppointmentStatus").count(),
            1,
            "expected idempotent single companion type, got:\n{}",
            twice
        );

        // Non-enum config / existing type should remain untouched
        let untouched_src = concat!(
            "export type Preset = 'A' | 'B';\n",
            "export const Preset = { A: 'A', B: 'B' };\n",
            "export const ThemeColors = { PRIMARY: '#000', SECONDARY: '#fff' };\n"
        );
        assert_eq!(
            crate::ast_engine::ensure_companion_types_for_const_enums_in_source("src/utils/theme.ts", untouched_src),
            None
        );
    }
}
