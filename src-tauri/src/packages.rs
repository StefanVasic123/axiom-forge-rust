use crate::*;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    pub aliases: Vec<String>,
    #[serde(rename = "importExamples")]
    pub import_examples: Vec<String>,
    pub compatibility: serde_json::Map<String, Value>,
    #[serde(rename = "bestPractices")]
    pub best_practices: String,
}

pub fn ensure_packages_db_exists() -> std::path::PathBuf {
    let base_dir = get_base_dir();
    let _ = std::fs::create_dir_all(&base_dir);
    let db_path = base_dir.join("axiom-packages-db.json");
    
    let mut needs_write = !db_path.exists();
    if db_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&db_path) {
            if !content.contains("\"dbVersion\": 3") {
                needs_write = true;
            }
        }
    }
    
    if needs_write {
        let default_db = serde_json::json!({
            "dbVersion": 3,
            "packages": [
                {
                    "name": "resend",
                    "version": "^3.2.0",
                    "aliases": ["@resend/client", "@resend/base", "resend-node", "resend/client"],
                    "importExamples": ["import { Resend } from 'resend';"],
                    "compatibility": {
                        "node": ">=18.0.0",
                        "next": ">=13.0.0"
                    },
                    "bestPractices": "Always instantiate Resend client using: const resend = new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview');"
                },
                {
                    "name": "next-auth",
                    "version": "^4.24.7",
                    "aliases": ["@next-auth/client", "next-auth/client"],
                    "importExamples": [
                        "import { getServerSession } from 'next-auth';",
                        "import NextAuth from 'next-auth';",
                        "import { useSession } from 'next-auth/react';",
                        "import CredentialsProvider from 'next-auth/providers/credentials';",
                        "import GitHubProvider from 'next-auth/providers/github';",
                        "import GoogleProvider from 'next-auth/providers/google';",
                        "import EmailProvider from 'next-auth/providers/email';",
                        "import { getServerSession } from 'next-auth/next';",
                        "export { default } from 'next-auth/middleware';"
                    ],
                    "compatibility": {
                        "node": ">=16.0.0",
                        "next": ">=12.0.0"
                    },
                    "bestPractices": "Use NextAuth(authOptions) in API routes. Use useSession() hook on the client side with SessionProvider."
                },
                {
                    "name": "@auth/prisma-adapter",
                    "version": "^1.0.12",
                    "aliases": [],
                    "importExamples": ["import { PrismaAdapter } from '@auth/prisma-adapter';"],
                    "compatibility": {
                        "next-auth": "^4.24.7",
                        "@prisma/client": "5.22.0"
                    },
                    "bestPractices": "Pass the PrismaClient instance to PrismaAdapter(prisma) as any in NextAuth v4 configuration."
                },
                {
                    "name": "@next-auth/prisma-adapter",
                    "version": "^1.0.7",
                    "aliases": [],
                    "importExamples": ["import { PrismaAdapter } from '@next-auth/prisma-adapter';"],
                    "compatibility": {
                        "next-auth": "^4.24.7",
                        "@prisma/client": "5.22.0"
                    },
                    "bestPractices": "Pass the PrismaClient instance to PrismaAdapter(prisma) in NextAuth configuration."
                },
                {
                    "name": "zod",
                    "version": "^3.23.8",
                    "aliases": ["zod-validator"],
                    "importExamples": ["import { z } from 'zod';"],
                    "compatibility": {},
                    "bestPractices": "Define schemas using z.object({...}). Validate input using schema.safeParse(data)."
                },
                {
                    "name": "react-hook-form",
                    "version": "^7.51.5",
                    "aliases": [],
                    "importExamples": ["import { useForm } from 'react-hook-form';"],
                    "compatibility": {},
                    "bestPractices": "Use register function to bind inputs. Use formState.errors for validations."
                },
                {
                    "name": "@hookform/resolvers",
                    "version": "^3.4.2",
                    "aliases": [],
                    "importExamples": ["import { zodResolver } from '@hookform/resolvers/zod';"],
                    "compatibility": {
                        "react-hook-form": "^7.51.5",
                        "zod": "^3.23.8"
                    },
                    "bestPractices": "Combine with zod to validate react-hook-form: useForm({ resolver: zodResolver(schema) })."
                },
                {
                    "name": "axios",
                    "version": "^1.7.2",
                    "aliases": [],
                    "importExamples": ["import axios from 'axios';"],
                    "compatibility": {},
                    "bestPractices": "Create an Axios instance with base URL for modular requests: axios.create({...})."
                },
                {
                    "name": "lucide-react",
                    "version": "^0.460.0",
                    "aliases": [],
                    "importExamples": ["import { Mail, Settings } from 'lucide-react';"],
                    "compatibility": {},
                    "bestPractices": "Import only necessary icons to enable tree shaking and reduce bundle sizes."
                },
                {
                    "name": "@heroicons/react",
                    "version": "^2.1.5",
                    "aliases": ["heroicons"],
                    "importExamples": ["import { UserIcon } from '@heroicons/react/24/outline';", "import { CheckIcon } from '@heroicons/react/24/solid';"],
                    "compatibility": {},
                    "bestPractices": "Import icons from @heroicons/react/24/outline or @heroicons/react/24/solid."
                },
                {
                    "name": "framer-motion",
                    "version": "^11.2.10",
                    "aliases": [],
                    "importExamples": ["import { motion } from 'framer-motion';"],
                    "compatibility": {},
                    "bestPractices": "Wrap animated components with motion.div. Use AnimatePresence for exit animations."
                },
                {
                    "name": "recharts",
                    "version": "^2.12.7",
                    "aliases": [],
                    "importExamples": ["import { ResponsiveContainer, AreaChart, Area, XAxis, YAxis, Tooltip } from 'recharts';"],
                    "compatibility": {},
                    "bestPractices": "Wrap Recharts charts in ResponsiveContainer to ensure responsiveness."
                },
                {
                    "name": "react-chartjs-2",
                    "version": "^5.2.0",
                    "aliases": [],
                    "importExamples": ["import { Line, Bar, Doughnut } from 'react-chartjs-2';"],
                    "compatibility": {
                        "chart.js": "^4.4.4"
                    },
                    "bestPractices": "Always pair react-chartjs-2 with chart.js."
                },
                {
                    "name": "chart.js",
                    "version": "^4.4.4",
                    "aliases": ["chartjs"],
                    "importExamples": ["import { Chart as ChartJS, CategoryScale, LinearScale, PointElement, LineElement } from 'chart.js';"],
                    "compatibility": {},
                    "bestPractices": "Register scales and elements via ChartJS.register(...)."
                },
                {
                    "name": "@reduxjs/toolkit",
                    "version": "^2.2.7",
                    "aliases": ["redux-toolkit"],
                    "importExamples": ["import { configureStore, createSlice } from '@reduxjs/toolkit';"],
                    "compatibility": {
                        "react-redux": "^9.1.2"
                    },
                    "bestPractices": "Use configureStore and createSlice alongside react-redux Provider."
                },
                {
                    "name": "react-redux",
                    "version": "^9.1.2",
                    "aliases": [],
                    "importExamples": ["import { Provider, useSelector, useDispatch } from 'react-redux';"],
                    "compatibility": {
                        "@reduxjs/toolkit": "^2.2.7"
                    },
                    "bestPractices": "Wrap root state consumers with <Provider store={store}>."
                },
                {
                    "name": "react-icons",
                    "version": "^5.2.1",
                    "aliases": [],
                    "importExamples": ["import { FaMailBulk } from 'react-icons/fa';", "import { IoMdNotifications } from 'react-icons/io';"],
                    "compatibility": {},
                    "bestPractices": "Import icons individually to reduce bundle size."
                },
                {
                    "name": "clsx",
                    "version": "^2.1.1",
                    "aliases": [],
                    "importExamples": ["import clsx from 'clsx';"],
                    "compatibility": {},
                    "bestPractices": "Construct className strings conditionally."
                },
                {
                    "name": "tailwind-merge",
                    "version": "^2.3.0",
                    "aliases": [],
                    "importExamples": ["import { twMerge } from 'tailwind-merge';"],
                    "compatibility": {},
                    "bestPractices": "Merge Tailwind CSS classes without conflicts."
                },
                {
                    "name": "class-variance-authority",
                    "version": "^0.7.0",
                    "aliases": [],
                    "importExamples": ["import { cva } from 'class-variance-authority';"],
                    "compatibility": {},
                    "bestPractices": "Create UI variants easily with cva."
                },
                {
                    "name": "uuid",
                    "version": "^9.0.1",
                    "aliases": [],
                    "importExamples": ["import { v4 as uuidv4 } from 'uuid';"],
                    "compatibility": {},
                    "bestPractices": "Generate random UUID v4."
                },
                {
                    "name": "bcryptjs",
                    "version": "^2.4.3",
                    "aliases": ["bcrypt", "bcryptjs-alias"],
                    "importExamples": ["import bcrypt from 'bcryptjs';"],
                    "compatibility": {},
                    "bestPractices": "Hash passwords asynchronously using bcrypt.hash(pwd, 10)."
                },
                {
                    "name": "jsonwebtoken",
                    "version": "^9.0.2",
                    "aliases": [],
                    "importExamples": ["import jwt from 'jsonwebtoken';"],
                    "compatibility": {},
                    "bestPractices": "Sign and verify JWT tokens securely."
                },
                {
                    "name": "@mui/material",
                    "version": "^5.15.0",
                    "aliases": ["@material-ui/core", "@mui/core"],
                    "importExamples": [
                        "import { Button, AppBar, Toolbar, Typography } from '@mui/material';",
                        "import { styled } from '@mui/material/styles';"
                    ],
                    "compatibility": {
                        "react": "^18.2.0",
                        "@emotion/react": "^11.11.4",
                        "@emotion/styled": "^11.11.5"
                    },
                    "bestPractices": "Import components from @mui/material. Use custom styles with styled utility."
                },
                {
                    "name": "@mui/icons-material",
                    "version": "^5.15.0",
                    "aliases": ["@material-ui/icons"],
                    "importExamples": ["import { Mail, Settings, Notifications } from '@mui/icons-material';"],
                    "compatibility": {
                        "@mui/material": "^5.15.0",
                        "@emotion/react": "^11.11.4",
                        "@emotion/styled": "^11.11.5"
                    },
                    "bestPractices": "Import icons individually or destructured. Make sure @mui/material is installed."
                },
                {
                    "name": "@prisma/client",
                    "version": "5.22.0",
                    "aliases": [],
                    "importExamples": ["import { PrismaClient } from '@prisma/client';"],
                    "compatibility": {
                        "prisma": "5.22.0"
                    },
                    "bestPractices": "Instantiate PrismaClient globally to avoid multiple connection pools in hot reload environments. Prisma client MUST only be imported and used inside server-side code (Next.js API route handlers), never in React hooks or client components ('use client')."
                },
                {
                    "name": "prisma",
                    "version": "5.22.0",
                    "aliases": [],
                    "importExamples": [],
                    "compatibility": {
                        "@prisma/client": "5.22.0"
                    },
                    "bestPractices": "Use local prisma CLI from node_modules/prisma/build/index.js."
                }
            ]
        });
        let _ = std::fs::write(&db_path, serde_json::to_string_pretty(&default_db).unwrap());
    }
    db_path
}

pub fn load_packages_db() -> Vec<PackageMeta> {
    let db_path = ensure_packages_db_exists();
    if let Ok(content) = std::fs::read_to_string(&db_path) {
        if let Ok(db_val) = serde_json::from_str::<Value>(&content) {
            if let Some(arr) = db_val.get("packages").and_then(|p| p.as_array()) {
                let mut pkgs = Vec::new();
                for item in arr {
                    if let Ok(pkg) = serde_json::from_value::<PackageMeta>(item.clone()) {
                        pkgs.push(pkg);
                    }
                }
                return pkgs;
            }
        }
    }
    Vec::new()
}

pub fn is_local_import_specifier(spec: &str) -> bool {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.starts_with('.')
        || trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || trimmed.starts_with("@/")
        || trimmed.starts_with("~/")
        || trimmed == "@"
        || trimmed == "~"
        || trimmed.contains(":\\")
        || trimmed.contains(":/")
    {
        return true;
    }
    let lower = trimmed.to_lowercase();
    const LOCAL_PATH_PREFIXES: &[&str] = &[
        "src/",
        "app/",
        "components/",
        "hooks/",
        "lib/",
        "utils/",
        "models/",
        "styles/",
        "pages/",
        "types/",
        "services/",
        "context/",
        "contexts/",
        "providers/",
        "config/",
        "constants/",
        "prisma/",
    ];
    LOCAL_PATH_PREFIXES.iter().any(|p| lower.starts_with(p))
}

pub fn resolve_and_rewrite_import(imported_path: &str, packages_db: &[PackageMeta]) -> Option<String> {
    let imported_path_trimmed = imported_path.trim();
    if imported_path_trimmed.is_empty() {
        return None;
    }

    if imported_path_trimmed == "bcrypt" {
        return Some("bcryptjs".to_string());
    }
    if imported_path_trimmed == "node-base64" {
        return Some("js-base64".to_string());
    }
    if imported_path_trimmed == "node-sass" {
        return Some("sass".to_string());
    }
    
    // 1. Check if it's a relative import, absolute path, or bare local src/app path in the project
    if is_local_import_specifier(imported_path_trimmed) {
        return None;
    }
    
    // Preserve valid official NextAuth v4 subpaths even against older cached DB entries
    if imported_path_trimmed.starts_with("next-auth/providers/")
        || imported_path_trimmed == "next-auth/react"
        || imported_path_trimmed == "next-auth/next"
        || imported_path_trimmed == "next-auth/middleware"
        || imported_path_trimmed == "next-auth/jwt"
        || imported_path_trimmed == "next-auth/adapters"
    {
        return None;
    }

    // We also skip common local aliases that nextjs/react uses
    let local_aliases = ["@components", "@hooks", "@utils", "@lib", "@styles", "@services", "@context", "@types", "@pages", "@app", "@assets", "@config"];
    for &la in &local_aliases {
        if imported_path_trimmed == la || imported_path_trimmed.starts_with(&format!("{}/", la)) {
            return None;
        }
    }
    
    // 2. Check for exact match with aliases first
    for pkg in packages_db {
        if pkg.aliases.contains(&imported_path_trimmed.to_string()) {
            // Special case for next-auth client side legacy alias
            if pkg.name == "next-auth" && (imported_path_trimmed == "@next-auth/client" || imported_path_trimmed == "next-auth/client") {
                return Some("next-auth/react".to_string());
            }
            return Some(pkg.name.clone());
        }
    }
    
    // 3. Check if the imported path starts with a package name or its aliases followed by "/"
    for pkg in packages_db {
        // Check package name match
        if imported_path_trimmed.starts_with(&format!("{}/", pkg.name)) {
            // Verify if this specific subpath is listed as valid in any import examples
            let mut is_valid_subpath = false;
            for example in &pkg.import_examples {
                if example.contains(imported_path_trimmed) {
                    is_valid_subpath = true;
                    break;
                }
            }
            if !is_valid_subpath {
                // It's a non-existent/hallucinated subpath of a known package (e.g. resend/client)
                // Rewrite to the package name itself
                return Some(pkg.name.clone());
            }
        }
        
        // Check aliases matches
        for alias in &pkg.aliases {
            if imported_path_trimmed.starts_with(&format!("{}/", alias)) {
                // If it starts with an alias, we check if it matches some known subpath, otherwise rewrite to pkg.name
                let mut is_valid_subpath = false;
                for example in &pkg.import_examples {
                    if example.contains(imported_path_trimmed) {
                        is_valid_subpath = true;
                        break;
                    }
                }
                if !is_valid_subpath {
                    // E.g. @resend/client/something -> resend
                    return Some(pkg.name.clone());
                }
            }
        }
    }
    
    None
}

pub fn get_packages_db_summary() -> String {
    let mut pkg_db_summary = "\nAvailable NPM package names, stable versions, import formats, compatibility, and integration best practices. ALWAYS use these exact package names and import syntaxes. Do NOT hallucinate other packages or subpaths:\n".to_string();
    let db_path = get_base_dir().join("axiom-packages-db.json");
    if db_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&db_path) {
            if let Ok(db_val) = serde_json::from_str::<Value>(&content) {
                if let Some(arr) = db_val.get("packages").and_then(|p| p.as_array()) {
                    for item in arr {
                        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        let version = item.get("version").and_then(|v| v.as_str()).unwrap_or("");
                        let examples = item.get("importExamples").and_then(|e| e.as_array());
                        let best_practices = item.get("bestPractices").and_then(|b| b.as_str()).unwrap_or("");
                        let compat = item.get("compatibility").and_then(|c| c.as_object());
                        
                        if !name.is_empty() {
                            pkg_db_summary.push_str(&format!("- Package: '{}' (Version: {})\n", name, version));
                            if let Some(ex_arr) = examples {
                                for ex in ex_arr {
                                    if let Some(ex_str) = ex.as_str() {
                                        pkg_db_summary.push_str(&format!("  * Import Syntax: {}\n", ex_str));
                                    }
                                }
                            }
                            if let Some(c_obj) = compat {
                                if !c_obj.is_empty() {
                                    let mut compat_parts = Vec::new();
                                    for (k, v) in c_obj {
                                        compat_parts.push(format!("{} ({})", k, v.as_str().unwrap_or("*")));
                                    }
                                    pkg_db_summary.push_str(&format!("  * Compatibility: {}\n", compat_parts.join(", ")));
                                }
                            }
                            if !best_practices.is_empty() {
                                pkg_db_summary.push_str(&format!("  * Best Practices: {}\n", best_practices));
                            }
                        }
                    }
                }
            }
        }
    }
    pkg_db_summary
}

pub fn format_relevant_packages_db_summary(
    packages_db: &[PackageMeta],
    pkg_json_deps: &[String],
    context_text: &str,
) -> String {
    let ctx_lower = context_text.to_lowercase();
    let baseline_pkgs = ["next-auth", "@prisma/client", "react-icons", "zod", "lucide-react"];

    let mut direct_hits: Vec<&PackageMeta> = Vec::new();
    let mut project_hits: Vec<&PackageMeta> = Vec::new();

    for pkg in packages_db {
        let name_lower = pkg.name.to_lowercase();
        let in_ctx = ctx_lower.contains(&name_lower)
            || pkg.aliases.iter().any(|a| ctx_lower.contains(&a.to_lowercase()));
        let in_pkg_json = pkg_json_deps.iter().any(|d| d.eq_ignore_ascii_case(&pkg.name));

        if in_ctx {
            direct_hits.push(pkg);
        } else if in_pkg_json || baseline_pkgs.contains(&pkg.name.as_str()) {
            project_hits.push(pkg);
        }
    }

    let mut selected: Vec<(&PackageMeta, bool)> = Vec::new();
    for pkg in direct_hits {
        if selected.len() >= 8 {
            break;
        }
        selected.push((pkg, true));
    }
    for pkg in project_hits {
        if selected.len() >= 8 {
            break;
        }
        if !selected.iter().any(|(p, _)| p.name == pkg.name) {
            selected.push((pkg, false));
        }
    }

    if selected.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "\nRelevant NPM packages for this file (use exact package names and import syntaxes):\n",
    );
    for (pkg, is_direct) in selected {
        out.push_str(&format!("- '{}' ({})", pkg.name, pkg.version));
        if let Some(first_ex) = pkg.import_examples.first() {
            out.push_str(&format!(": {}", first_ex));
        }
        out.push('\n');
        if is_direct {
            for ex in pkg.import_examples.iter().skip(1).take(3) {
                out.push_str(&format!("  * {}\n", ex));
            }
            if !pkg.best_practices.is_empty() {
                out.push_str(&format!("  * Note: {}\n", pkg.best_practices));
            }
        }
    }
    out
}

pub fn get_relevant_packages_db_summary(
    proj_dir: Option<&std::path::Path>,
    context_text: &str,
) -> String {
    let packages_db = load_packages_db();
    let mut pkg_json_deps: Vec<String> = Vec::new();
    if let Some(dir) = proj_dir {
        let pkg_path = dir.join("package.json");
        if let Ok(content) = std::fs::read_to_string(pkg_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                for key in &["dependencies", "devDependencies"] {
                    if let Some(obj) = val.get(*key).and_then(|d| d.as_object()) {
                        for k in obj.keys() {
                            if !pkg_json_deps.contains(k) {
                                pkg_json_deps.push(k.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    format_relevant_packages_db_summary(&packages_db, &pkg_json_deps, context_text)
}

pub fn repair_package_json_dependencies(proj_dir: &std::path::Path) {
    let pkg_path = proj_dir.join("package.json");
    if !pkg_path.exists() {
        return;
    }
    
    let packages_db = load_packages_db();
    let has_src_dir = proj_dir.join("src").exists();
    let mut imports_found = std::collections::HashSet::new();
    
    // Scan directory and perform alias rewriting inline
    let mut scan_dir = |dir: &std::path::Path| {
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            if let Ok(entries) = std::fs::read_dir(&current) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if name != "node_modules" && name != ".next" && name != ".git" {
                            stack.push(path);
                        }
                    } else if path.is_file() {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let mut file_changed = false;
                                let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                                
                                for line in lines.iter_mut() {
                                    let trimmed = line.trim();
                                    if (trimmed.starts_with("import ") || trimmed.starts_with("import{") || trimmed.starts_with("import type") || trimmed.contains("require(") || trimmed.contains("require.resolve(")) 
                                        && (trimmed.contains('\'') || trimmed.contains('"') || trimmed.contains('`')) 
                                    {
                                        let chars: Vec<char> = line.chars().collect();
                                        let mut i = 0;
                                        while i < chars.len() {
                                            if chars[i] == '\'' || chars[i] == '"' || chars[i] == '`' {
                                                let quote = chars[i];
                                                i += 1;
                                                let start = i;
                                                while i < chars.len() && chars[i] != quote {
                                                    i += 1;
                                                }
                                                if i < chars.len() {
                                                    let literal: String = chars[start..i].iter().collect();
                                                    let literal = literal.trim();
                                                    
                                                    // Rewrite bare `src/...` imports to `@/...` when project uses `src/`
                                                    if has_src_dir && literal.starts_with("src/") {
                                                        let target_alias = format!("@/{}", &literal["src/".len()..]);
                                                        let old_quote = format!("{}{}{}", quote, literal, quote);
                                                        let new_quote = format!("{}{}{}", quote, target_alias, quote);
                                                        *line = line.replace(&old_quote, &new_quote);
                                                        file_changed = true;
                                                    } else if has_src_dir && literal.starts_with("~/") {
                                                        let target_alias = format!("@/{}", &literal["~/".len()..]);
                                                        let old_quote = format!("{}{}{}", quote, literal, quote);
                                                        let new_quote = format!("{}{}{}", quote, target_alias, quote);
                                                        *line = line.replace(&old_quote, &new_quote);
                                                        file_changed = true;
                                                    } else if let Some(correct_path) = resolve_and_rewrite_import(&literal, &packages_db) {
                                                        // Check if it's an npm alias we should rewrite
                                                        let old_quote = format!("{}{}{}", quote, literal, quote);
                                                        let new_quote = format!("{}{}{}", quote, correct_path, quote);
                                                        *line = line.replace(&old_quote, &new_quote);
                                                        file_changed = true;
                                                    }
                                                }
                                            }
                                            i += 1;
                                        }
                                    }
                                }
                                
                                let content_to_parse = if file_changed {
                                    let joined = lines.join("\n");
                                    let _ = std::fs::write(&path, &joined);
                                    joined
                                } else {
                                    content
                                };
                                
                                // Parse import/require paths for package.json dependency gathering
                                for line in content_to_parse.lines() {
                                    let line = line.trim();
                                    if (line.starts_with("import ") || line.starts_with("import{") || line.starts_with("import type") || line.contains("require(") || line.contains("require.resolve(")) && (line.contains('\'') || line.contains('"') || line.contains('`')) {
                                        let chars: Vec<char> = line.chars().collect();
                                        let mut i = 0;
                                        while i < chars.len() {
                                            if chars[i] == '\'' || chars[i] == '"' || chars[i] == '`' {
                                                let quote = chars[i];
                                                i += 1;
                                                let start = i;
                                                while i < chars.len() && chars[i] != quote {
                                                    i += 1;
                                                }
                                                if i < chars.len() {
                                                    let literal: String = chars[start..i].iter().collect();
                                                    let literal = literal.trim();
                                                    if !is_local_import_specifier(literal) {
                                                        let parts: Vec<&str> = literal.split('/').collect();
                                                        if !parts.is_empty() {
                                                            let local_aliases = ["@components", "@hooks", "@utils", "@lib", "@styles", "@services", "@context", "@types", "@pages", "@app", "@assets", "@config"];
                                                            if !local_aliases.contains(&parts[0]) {
                                                                let pkg = if parts[0].starts_with('@') && parts.len() > 1 {
                                                                    format!("{}/{}", parts[0], parts[1])
                                                                } else {
                                                                    parts[0].to_string()
                                                                };
                                                                let node_builtins = ["fs", "path", "http", "https", "os", "crypto", "stream", "util", "events", "assert", "dns", "net", "tls", "querystring", "url", "zlib", "buffer", "child_process", "cluster", "dgram", "process", "readline", "vm"];
                                                                if !pkg.starts_with("node:")
                                                                    && !node_builtins.contains(&pkg.as_str())
                                                                    && pkg != "react"
                                                                    && pkg != "react-dom"
                                                                    && pkg != "next"
                                                                    && !pkg.contains(' ')
                                                                {
                                                                    // Map to correct package if it matches an alias
                                                                    let mut mapped_pkg = pkg.clone();
                                                                    for db_pkg in &packages_db {
                                                                        if db_pkg.aliases.contains(&pkg) {
                                                                            mapped_pkg = db_pkg.name.clone();
                                                                            break;
                                                                        }
                                                                    }
                                                                    imports_found.insert(mapped_pkg);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            i += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    
    let has_tsconfig = proj_dir.join("tsconfig.json").exists();
    let is_nextjs_proj = proj_dir.join("next.config.js").exists()
        || proj_dir.join("next.config.mjs").exists()
        || proj_dir.join("src/app").exists()
        || proj_dir.join("app").exists();
    let has_prisma_schema = proj_dir.join("prisma/schema.prisma").exists()
        || proj_dir.join("schema.prisma").exists();

    // Repair broken fallback stubs in tailwind.config.js and postcss.config.js if present
    let tw_path = proj_dir.join("tailwind.config.js");
    if tw_path.exists() {
        if let Ok(tw_content) = std::fs::read_to_string(&tw_path) {
            if tw_content.contains("export {};") || !tw_content.contains("content:") {
                let valid_tw = "/** @type {import('tailwindcss').Config} */\nmodule.exports = {\n  content: [\n    \"./src/**/*.{js,ts,jsx,tsx,mdx}\",\n    \"./app/**/*.{js,ts,jsx,tsx,mdx}\",\n    \"./components/**/*.{js,ts,jsx,tsx,mdx}\",\n    \"./pages/**/*.{js,ts,jsx,tsx,mdx}\"\n  ],\n  theme: {\n    extend: {},\n  },\n  plugins: [],\n};\n";
                let _ = std::fs::write(&tw_path, valid_tw);
            }
        }
    }
    let postcss_path = proj_dir.join("postcss.config.js");
    if postcss_path.exists() {
        if let Ok(pc_content) = std::fs::read_to_string(&postcss_path) {
            if pc_content.contains("export {};") || !pc_content.contains("tailwindcss") {
                let valid_pc = "module.exports = {\n  plugins: {\n    tailwindcss: {},\n    autoprefixer: {},\n  },\n};\n";
                let _ = std::fs::write(&postcss_path, valid_pc);
            }
        }
    }

    scan_dir(proj_dir);
    
    if let Ok(pkg_content) = std::fs::read_to_string(&pkg_path) {
        if let Ok(mut pkg_json) = serde_json::from_str::<Value>(&pkg_content) {
            if let Some(pkg_obj) = pkg_json.as_object_mut() {
                let mut changed = false;

                if !pkg_obj.contains_key("name") {
                    pkg_obj.insert("name".to_string(), serde_json::json!("axiom-app"));
                    changed = true;
                }
                if !pkg_obj.contains_key("version") {
                    pkg_obj.insert("version".to_string(), serde_json::json!("0.1.0"));
                    changed = true;
                }
                if !pkg_obj.contains_key("private") {
                    pkg_obj.insert("private".to_string(), serde_json::json!(true));
                    changed = true;
                }

                pkg_obj.entry("dependencies").or_insert(serde_json::json!({}));
                pkg_obj.entry("devDependencies").or_insert(serde_json::json!({}));

                if is_nextjs_proj {
                    let scripts = pkg_obj.entry("scripts").or_insert(serde_json::json!({}));
                    if let Some(scripts_obj) = scripts.as_object_mut() {
                        for (k, v) in [("dev", "next dev"), ("build", "next build"), ("start", "next start")] {
                            if !scripts_obj.contains_key(k) {
                                scripts_obj.insert(k.to_string(), serde_json::json!(v));
                                changed = true;
                            }
                        }
                    }

                    if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                        // Force-standardize core Next.js 14 stack versions (overwriting outdated/mismatched LLM versions)
                        for (k, v) in [
                            ("next", "14.2.35"),
                            ("react", "^18.2.0"),
                            ("react-dom", "^18.2.0"),
                            ("lucide-react", "^0.460.0"),
                            ("clsx", "^2.1.1"),
                            ("tailwind-merge", "^2.3.0"),
                        ] {
                            let target_val = serde_json::json!(v);
                            if deps_obj.get(k) != Some(&target_val) {
                                deps_obj.insert(k.to_string(), target_val);
                                changed = true;
                            }
                        }

                        // Remove unused Express or misplaced devDependencies / local dir names from Next.js dependencies
                        if !imports_found.contains("express") && deps_obj.remove("express").is_some() {
                            changed = true;
                        }
                        if deps_obj.remove("tailwindcss").is_some() {
                            changed = true;
                        }
                        for bogus_local in ["src", "prisma"] {
                            if deps_obj.remove(bogus_local).is_some() {
                                changed = true;
                            }
                        }
                        for dir_pkg in ["app", "components", "hooks", "lib", "utils", "models", "styles", "pages", "context", "contexts", "providers"] {
                            if deps_obj.get(dir_pkg).and_then(|v| v.as_str()) == Some("*") {
                                deps_obj.remove(dir_pkg);
                                changed = true;
                            }
                        }
                    }

                    if let Some(dev_deps_obj) = pkg_obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                        for (k, v) in [
                            ("tailwindcss", "^3.4.0"),
                            ("postcss", "^8.4.32"),
                            ("autoprefixer", "^10.4.16"),
                            ("typescript", "^5.3.3"),
                            ("@types/react", "^18.2.45"),
                            ("@types/react-dom", "^18.2.18"),
                            ("@types/node", "^20.10.5"),
                        ] {
                            let target_val = serde_json::json!(v);
                            if dev_deps_obj.get(k) != Some(&target_val) {
                                dev_deps_obj.insert(k.to_string(), target_val);
                                changed = true;
                            }
                        }
                    }
                }

                // Rewrite deprecated/native-C++ packages in dependencies
                if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                    if deps_obj.remove("bcrypt").is_some() {
                        deps_obj.insert("bcryptjs".to_string(), serde_json::json!("^2.4.3"));
                        changed = true;
                    }
                    if deps_obj.remove("node-base64").is_some() {
                        deps_obj.insert("js-base64".to_string(), serde_json::json!("^3.7.7"));
                        changed = true;
                    }
                    if deps_obj.remove("node-sass").is_some() {
                        deps_obj.insert("sass".to_string(), serde_json::json!("^1.77.0"));
                        changed = true;
                    }
                }

                // Clean up any dependencies that are aliases of known packages in the DB
                for pkg in &packages_db {
                    for alias in &pkg.aliases {
                        if let Some(deps) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if deps.remove(alias).is_some() {
                                changed = true;
                            }
                        }
                        if let Some(dev_deps) = pkg_obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                            if dev_deps.remove(alias).is_some() {
                                changed = true;
                            }
                        }
                    }
                }

                // Ensure all imported packages exist in dependencies
                for imp in &imports_found {
                    if imp == "prisma" {
                        if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if !deps_obj.contains_key("@prisma/client") {
                                deps_obj.insert("@prisma/client".to_string(), serde_json::json!("5.22.0"));
                                changed = true;
                            }
                        }
                        continue;
                    }
                    let mut version = None;
                    for pkg in &packages_db {
                        if &pkg.name == imp {
                            version = Some(pkg.version.clone());
                            break;
                        }
                    }
                    if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                        if !deps_obj.contains_key(imp) {
                            let v = version.unwrap_or_else(|| "*".to_string());
                            deps_obj.insert(imp.clone(), serde_json::json!(v));
                            changed = true;
                        }
                    }
                }

                // Standardize versions for all known packages already in dependencies
                if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                    for pkg in &packages_db {
                        if pkg.name == "prisma" {
                            continue;
                        }
                        if deps_obj.contains_key(&pkg.name) {
                            let target_val = serde_json::json!(pkg.version);
                            if deps_obj.get(&pkg.name) != Some(&target_val) {
                                deps_obj.insert(pkg.name.clone(), target_val);
                                changed = true;
                            }
                        }
                    }

                    // Cross-package peer dependency & companion matching
                    for pkg in &packages_db {
                        if deps_obj.contains_key(&pkg.name) {
                            for (peer_name, peer_ver) in &pkg.compatibility {
                                if peer_name == "node" || peer_name == "next" || peer_name == "react" || peer_name == "prisma" {
                                    continue;
                                }
                                if let Some(peer_ver_str) = peer_ver.as_str() {
                                    if !peer_ver_str.starts_with(">=") {
                                        let target_peer = serde_json::json!(peer_ver_str);
                                        if deps_obj.get(peer_name) != Some(&target_peer) {
                                            deps_obj.insert(peer_name.clone(), target_peer);
                                            changed = true;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // If next-auth is used with Prisma, ensure @auth/prisma-adapter is present and locked to v1.x (NextAuth v4 compatible)
                    let has_prisma = has_prisma_schema || deps_obj.contains_key("@prisma/client");
                    if deps_obj.contains_key("next-auth") && has_prisma && !deps_obj.contains_key("@auth/prisma-adapter") && !deps_obj.contains_key("@next-auth/prisma-adapter") {
                        deps_obj.insert("@auth/prisma-adapter".to_string(), serde_json::json!("^1.0.12"));
                        changed = true;
                    }
                }

                // Enforce strict Prisma Lockstep ("5.22.0") across dependencies (@prisma/client) and devDependencies (prisma)
                let uses_prisma = has_prisma_schema
                    || imports_found.contains("@prisma/client")
                    || imports_found.contains("prisma")
                    || pkg_obj.get("dependencies").and_then(|d| d.as_object()).map_or(false, |d| d.contains_key("@prisma/client") || d.contains_key("prisma"))
                    || pkg_obj.get("devDependencies").and_then(|d| d.as_object()).map_or(false, |d| d.contains_key("prisma"));
                if uses_prisma {
                    let locked_prisma = serde_json::json!("5.22.0");
                    if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                        if deps_obj.remove("prisma").is_some() {
                            changed = true;
                        }
                        if deps_obj.get("@prisma/client") != Some(&locked_prisma) {
                            deps_obj.insert("@prisma/client".to_string(), locked_prisma.clone());
                            changed = true;
                        }
                    }
                    if let Some(dev_deps_obj) = pkg_obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                        if dev_deps_obj.get("prisma") != Some(&locked_prisma) {
                            dev_deps_obj.insert("prisma".to_string(), locked_prisma);
                            changed = true;
                        }
                    }
                }

                // Auto-inject matching TypeScript @types/* packages for all dependencies when tsconfig.json exists
                if has_tsconfig {
                    let dep_keys: Vec<String> = pkg_obj
                        .get("dependencies")
                        .and_then(|d| d.as_object())
                        .map(|d| d.keys().cloned().collect())
                        .unwrap_or_default();

                    if let Some(dev_deps_obj) = pkg_obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                        for dep in dep_keys {
                            let type_pkg = match dep.as_str() {
                                "bcrypt" => Some(("@types/bcrypt", "^5.0.2")),
                                "bcryptjs" => Some(("@types/bcryptjs", "^2.4.6")),
                                "jsonwebtoken" => Some(("@types/jsonwebtoken", "^9.0.6")),
                                "uuid" => Some(("@types/uuid", "^9.0.8")),
                                "nodemailer" => Some(("@types/nodemailer", "^6.4.14")),
                                "cors" => Some(("@types/cors", "^2.8.17")),
                                "cookie" => Some(("@types/cookie", "^0.6.0")),
                                "canvas-confetti" => Some(("@types/canvas-confetti", "^1.6.4")),
                                _ => None,
                            };
                            if let Some((tp_name, tp_ver)) = type_pkg {
                                let target_val = serde_json::json!(tp_ver);
                                if dev_deps_obj.get(tp_name) != Some(&target_val) {
                                    dev_deps_obj.insert(tp_name.to_string(), target_val);
                                    changed = true;
                                }
                            }
                        }
                    }
                }
                
                let _ = pkg_obj;
                if sanitize_package_json_value_for_offline(&mut pkg_json) {
                    changed = true;
                }

                if changed {
                    if let Ok(pretty) = serde_json::to_string_pretty(&pkg_json) {
                        let _ = std::fs::write(&pkg_path, pretty);
                    }
                }
            }
        }
    }
}

pub fn _find_404_package(err_log: &str) -> Option<String> {
    if let Some(idx) = err_log.find("requested resource '") {
        let after = &err_log[idx + 20..];
        if let Some(end_idx) = after.find('\'') {
            let resource_str = &after[..end_idx];
            if let Some(at_idx) = resource_str.rfind('@') {
                if at_idx > 0 {
                    return Some(resource_str[..at_idx].to_string());
                }
            }
        }
    }
    if let Some(idx) = err_log.find("https://registry.npmjs.org/") {
        let after = &err_log[idx + 27..];
        let end_idx = after.find(' ').unwrap_or_else(|| after.find('\n').unwrap_or(after.len()));
        let url_pkg = &after[..end_idx];
        let decoded = url_pkg.replace("%2f", "/").replace("%2F", "/");
        if !decoded.is_empty() {
            return Some(decoded);
        }
    }
    None
}

pub fn is_core_framework_package(pkg_name: &str) -> bool {
    matches!(
        pkg_name.trim(),
        "next"
            | "react"
            | "react-dom"
            | "prisma"
            | "@prisma/client"
            | "next-auth"
            | "@auth/prisma-adapter"
            | "@next-auth/prisma-adapter"
            | "typescript"
            | "tailwindcss"
            | "@types/react"
            | "@types/react-dom"
            | "@types/node"
    )
}

pub fn remove_package_from_json(pkg_path: &std::path::Path, pkg_name: &str) -> bool {
    if is_core_framework_package(pkg_name) {
        debug_log_to_file(format!(
            "[Core Lock] Refusing to remove core framework package '{}' from package.json",
            pkg_name
        ));
        return false;
    }
    if let Ok(content) = std::fs::read_to_string(pkg_path) {
        if let Ok(mut json_val) = serde_json::from_str::<Value>(&content) {
            let mut changed = false;
            if let Some(obj) = json_val.as_object_mut() {
                if let Some(deps) = obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                    if deps.remove(pkg_name).is_some() {
                        changed = true;
                    }
                }
                if let Some(dev_deps) = obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                    if dev_deps.remove(pkg_name).is_some() {
                        changed = true;
                    }
                }
            }
            if changed {
                if let Ok(pretty) = serde_json::to_string_pretty(&json_val) {
                    let _ = std::fs::write(pkg_path, pretty);
                    return true;
                }
            }
        }
    }
    false
}

#[derive(Debug, Clone)]
pub enum NpmFailureAction {
    Remove(String),
    SetWildcard(String),
}

pub fn analyze_npm_install_failure(err_log: &str) -> Option<NpmFailureAction> {
    // 1. Check for ETARGET / notarget (version mismatch)
    if let Some(idx) = err_log.find("No matching version found for ") {
        let after = &err_log[idx + 30..];
        let line = after.lines().next().unwrap_or("").trim();
        let token = line.split_whitespace().next().unwrap_or("");
        let mut cleaned = token.to_string();
        while cleaned.ends_with('.') || cleaned.ends_with('\'') || cleaned.ends_with('"') || cleaned.ends_with(';') || cleaned.ends_with(',') {
            cleaned.pop();
        }
        if let Some(at_idx) = cleaned.rfind('@') {
            if at_idx > 0 {
                let pkg_name = &cleaned[..at_idx];
                return Some(NpmFailureAction::SetWildcard(pkg_name.to_string()));
            }
        }
    }

    // 2. Check for E404 (NotFound) - original/legacy format
    if let Some(idx) = err_log.find("requested resource '") {
        let after = &err_log[idx + 20..];
        if let Some(end_idx) = after.find('\'') {
            let resource_str = &after[..end_idx];
            if let Some(at_idx) = resource_str.rfind('@') {
                if at_idx > 0 {
                    return Some(NpmFailureAction::Remove(resource_str[..at_idx].to_string()));
                }
            }
            return Some(NpmFailureAction::Remove(resource_str.to_string()));
        }
    }

    // 3. Check for E404 (NotFound) - registry not found format
    if let Some(idx) = err_log.find(" is not in the npm registry") {
        let before = &err_log[..idx];
        let parts: Vec<&str> = before.split('\'').collect();
        if parts.len() >= 2 {
            let pkg_name = parts[parts.len() - 2];
            return Some(NpmFailureAction::Remove(pkg_name.to_string()));
        }
    }

    // 4. Check for E404 (NotFound) - GET url format
    if let Some(idx) = err_log.find("404 Not Found") {
        let line = err_log[idx..].lines().next().unwrap_or("");
        if let Some(get_idx) = line.find("GET ") {
            let after_get = &line[get_idx + 4..];
            let url_token = after_get.split_whitespace().next().unwrap_or("");
            if let Some(last_slash) = url_token.rfind('/') {
                let pkg_name = &url_token[last_slash + 1..];
                let pkg_name = pkg_name.split('?').next().unwrap_or("").split('#').next().unwrap_or("");
                if !pkg_name.is_empty() && pkg_name != "index" {
                    return Some(NpmFailureAction::Remove(pkg_name.to_string()));
                }
            }
        }
    }

    // 5. Check for installation script failures or compile errors for specific packages in node_modules
    for line in err_log.lines() {
        let clean = line.replace("ERR!", "error").replace("warn", "error").replace("WARN", "error");
        if clean.contains("npm error path") && clean.contains("node_modules") {
            let normalized = clean.replace('\\', "/");
            if let Some(idx) = normalized.rfind("node_modules/") {
                let pkg_part = &normalized[idx + "node_modules/".len()..];
                let parts: Vec<&str> = pkg_part.split('/').collect();
                if !parts.is_empty() {
                    let mut pkg_name = parts[0].trim().to_string();
                    if pkg_name.starts_with('@') && parts.len() > 1 {
                        pkg_name = format!("{}/{}", pkg_name, parts[1].trim());
                    }
                    if !pkg_name.is_empty() {
                        return Some(NpmFailureAction::Remove(pkg_name));
                    }
                }
            }
        }
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpmInstallOutcome {
    Installed,
    OfflinePaused,
    Failed,
}

pub fn is_npm_network_error(err_log: &str) -> bool {
    let lower = err_log.to_lowercase();
    err_log.contains("ENOTFOUND")
        || err_log.contains("EAI_AGAIN")
        || err_log.contains("EAI_NODATA")
        || err_log.contains("ETIMEDOUT")
        || err_log.contains("ECONNREFUSED")
        || err_log.contains("ECONNRESET")
        || err_log.contains("ENETUNREACH")
        || err_log.contains("EHOSTUNREACH")
        || err_log.contains("ERR_SOCKET_TIMEOUT")
        || err_log.contains("ERR_NETWORK")
        || err_log.contains("ENOTCACHED")
        || lower.contains("network error")
        || lower.contains("npm error network")
        || lower.contains("npm err! network")
        || lower.contains("socket hang up")
        || lower.contains("network socket disconnected")
        || lower.contains("getaddrinfo")
        || lower.contains("fetch failed")
        || lower.contains("request timeout")
        || lower.contains("network appears to be offline")
        || lower.contains("in offline mode")
        || lower.contains("only-if-cached")
        || (lower.contains("request to ") && lower.contains("failed, reason:"))
}

pub fn sanitize_package_json_value_for_offline(json_val: &mut Value) -> bool {
    let mut changed = false;
    let bogus_subpaths = [
        "fi", "hi", "hi2", "bi", "fa", "fa6", "md", "io", "io5", "bs", "ai", "ri", "tb", "lu",
        "si", "gi", "ti", "wi", "cg", "vsc", "go", "gr", "rx", "pi", "im", "sl", "tfi", "lia",
    ];
    let banned_always_pkgs = [
        "@typescript-eslint/eslint-plugin",
        "@typescript-eslint/parser",
        "@typescript-eslint/project-service",
        "@typescript-eslint/types",
        "@typescript-eslint/visitor-keys",
        "@typescript-eslint/scope-manager",
        "eslint-config-next",
        "eslint",
        "prettier",
        "jest",
        "ts-jest",
        "@types/jest",
        "react-testing-library",
        "@testing-library/react",
        "@testing-library/jest-dom",
        "@testing-library/user-event",
    ];
    // Next.js bundles its own Webpack pipeline internally; standalone webpack/dev-server
    // entries in a Next.js project are LLM hallucinations that pull incompatible transitive deps
    // (such as `shell-quote@^1.10.0` via `webpack-dev-server`). Non-Next.js projects keep theirs.
    let banned_nextjs_bundler_pkgs = [
        "babel-loader",
        "webpack",
        "webpack-cli",
        "webpack-dev-server",
        "webpack-hot-middleware",
        "html-webpack-plugin",
    ];

    let has_pkg = |val: &Value, section: &str, name: &str| -> bool {
        val.get(section)
            .and_then(|d| d.as_object())
            .map_or(false, |d| d.contains_key(name))
    };
    let has_next_script = json_val
        .get("scripts")
        .and_then(|s| s.as_object())
        .map_or(false, |scripts| {
            scripts.values().any(|v| {
                v.as_str().map_or(false, |s| {
                    s.contains("next dev") || s.contains("next build") || s.contains("next start")
                })
            })
        });
    let is_nextjs_ctx = has_pkg(json_val, "dependencies", "next")
        || has_pkg(json_val, "devDependencies", "next")
        || has_next_script;

    if let Some(obj) = json_val.as_object_mut() {
        let mut had_react_icons_subpath = false;

        for key in &["dependencies", "devDependencies"] {
            if let Some(deps) = obj.get_mut(*key).and_then(|d| d.as_object_mut()) {
                for sub in &bogus_subpaths {
                    if deps.remove(*sub).is_some() {
                        had_react_icons_subpath = true;
                        changed = true;
                    }
                }
                for banned in &banned_always_pkgs {
                    if deps.remove(*banned).is_some() {
                        changed = true;
                    }
                }
                if is_nextjs_ctx {
                    for banned in &banned_nextjs_bundler_pkgs {
                        if deps.remove(*banned).is_some() {
                            changed = true;
                        }
                    }
                }
                let scoped_to_remove: Vec<String> = deps
                    .keys()
                    .filter(|k| {
                        k.starts_with("@typescript-eslint/")
                            || k.starts_with("@babel/")
                            || k.starts_with("@testing-library/")
                            || k.starts_with("eslint-plugin-")
                            || k.starts_with("eslint-config-")
                            || k.ends_with("-icons-react")
                            || k.starts_with("react-icons/")
                    })
                    .cloned()
                    .collect();
                for k in scoped_to_remove {
                    if k.ends_with("-icons-react") || k.starts_with("react-icons/") {
                        had_react_icons_subpath = true;
                    }
                    deps.remove(&k);
                    changed = true;
                }
            }
        }

        if had_react_icons_subpath {
            if let Some(deps) = obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                if !deps.contains_key("react-icons") {
                    deps.insert("react-icons".to_string(), serde_json::json!("^5.0.1"));
                    changed = true;
                }
            }
        }

        // Deduplicate packages present in both dependencies and devDependencies
        let dev_keys: Vec<String> = obj
            .get("devDependencies")
            .and_then(|d| d.as_object())
            .map(|d| d.keys().cloned().collect())
            .unwrap_or_default();

        if !dev_keys.is_empty() {
            let dev_only_tools = [
                "typescript",
                "tailwindcss",
                "postcss",
                "autoprefixer",
                "prisma",
                "@types/node",
                "@types/react",
                "@types/react-dom",
            ];
            for dk in dev_keys {
                if dev_only_tools.contains(&dk.as_str()) || dk.starts_with("@types/") {
                    if let Some(deps) = obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                        if deps.remove(&dk).is_some() {
                            changed = true;
                        }
                    }
                } else {
                    let in_deps = obj
                        .get("dependencies")
                        .and_then(|d| d.as_object())
                        .map_or(false, |d| d.contains_key(&dk));
                    if in_deps {
                        if let Some(dev_deps) = obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                            if dev_deps.remove(&dk).is_some() {
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
    }

    changed
}

pub fn sanitize_package_json_for_offline(pkg_path: &std::path::Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(pkg_path) {
        if let Ok(mut json_val) = serde_json::from_str::<Value>(&content) {
            if sanitize_package_json_value_for_offline(&mut json_val) {
                if let Ok(pretty) = serde_json::to_string_pretty(&json_val) {
                    let _ = std::fs::write(pkg_path, pretty);
                    return true;
                }
            }
        }
    }
    false
}

fn extract_npm_pkg_name_from_spec(raw: &str) -> Option<String> {
    let trimmed = raw
        .trim()
        .trim_matches(|c: char| matches!(c, '\'' | '"' | '`' | ',' | ';' | '.' | '(' | ')' | '[' | ']'));
    if trimmed.is_empty()
        || trimmed.contains(':')
        || trimmed.contains('\\')
        || trimmed.starts_with('.')
        || trimmed.starts_with('/')
        || trimmed.starts_with('-')
        || trimmed.starts_with('_')
    {
        return None;
    }

    let name = if let Some(rest) = trimmed.strip_prefix('@') {
        let slash_idx = rest.find('/')?;
        if slash_idx == 0 {
            return None;
        }
        let after_slash = &rest[slash_idx + 1..];
        let scope = &rest[..slash_idx];
        let pkg_part = match after_slash.find('@') {
            Some(at_idx) => &after_slash[..at_idx],
            None => after_slash,
        };
        if pkg_part.is_empty() || pkg_part.contains('/') {
            return None;
        }
        format!("@{}/{}", scope, pkg_part)
    } else {
        if trimmed.contains('/') {
            return None;
        }
        let pkg_part = match trimmed.find('@') {
            Some(at_idx) => &trimmed[..at_idx],
            None => trimmed,
        };
        if pkg_part.is_empty() {
            return None;
        }
        pkg_part.to_string()
    };

    if name.eq_ignore_ascii_case("root")
        || name.eq_ignore_ascii_case("workspace")
        || name.eq_ignore_ascii_case("npm")
        || name.eq_ignore_ascii_case("node_modules")
    {
        return None;
    }

    let is_valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | '/'));
    if is_valid {
        Some(name)
    } else {
        None
    }
}

fn read_npm_debug_log_tail(err_log: &str) -> Option<String> {
    for line in err_log.lines() {
        let lower = line.to_lowercase();
        if let Some(idx) = lower.find("complete log of this run can be found in:") {
            let path_part = line[idx + "complete log of this run can be found in:".len()..]
                .trim()
                .trim_matches(|c: char| matches!(c, '\'' | '"' | '\r' | '\n'));
            if path_part.is_empty() {
                continue;
            }
            let log_path = std::path::Path::new(path_part);
            if log_path.exists() {
                if let Ok(content) = std::fs::read_to_string(log_path) {
                    const MAX_TAIL_BYTES: usize = 256 * 1024;
                    if content.len() > MAX_TAIL_BYTES {
                        let start = content.len() - MAX_TAIL_BYTES;
                        let safe_start = content
                            .char_indices()
                            .find(|(i, _)| *i >= start)
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        return Some(content[safe_start..].to_string());
                    }
                    return Some(content);
                }
            }
        }
    }
    None
}

pub fn extract_offending_parent_from_npm_log(
    err_log: &str,
    failing_pkg: &str,
    direct_deps: &[String],
    root_name: Option<&str>,
) -> Option<String> {
    let mut full_log = err_log.to_string();
    if let Some(debug_tail) = read_npm_debug_log_tail(err_log) {
        full_log.push('\n');
        full_log.push_str(&debug_tail);
    }

    let mut specific_candidates: Vec<String> = Vec::new();
    let mut while_resolving_candidates: Vec<String> = Vec::new();

    let push_unique = |vec: &mut Vec<String>, candidate: String| {
        if candidate == failing_pkg {
            return;
        }
        if let Some(rn) = root_name {
            if candidate == rn {
                return;
            }
        }
        if !vec.contains(&candidate) {
            vec.push(candidate);
        }
    };

    for line in full_log.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Arborist debug log: `silly placeDep ROOT shell-quote@^1.10.0 OK for: webpack-dev-server@4.15.2 ...`
        // or `silly placeDep node_modules/webpack-dev-server shell-quote@^1.10.0 ...`
        if trimmed.contains("placeDep") && trimmed.contains(failing_pkg) {
            if let Some(for_idx) = trimmed.find(" for: ") {
                let after_for = &trimmed[for_idx + " for: ".len()..];
                if let Some(tok) = after_for.split_whitespace().next() {
                    if let Some(pkg) = extract_npm_pkg_name_from_spec(tok) {
                        push_unique(&mut specific_candidates, pkg);
                    }
                }
            }
            if let Some(nm_idx) = trimmed.rfind("node_modules/") {
                let after_nm = &trimmed[nm_idx + "node_modules/".len()..];
                let path_tok = after_nm.split_whitespace().next().unwrap_or("");
                let parts: Vec<&str> = path_tok.split('/').collect();
                if let Some(first) = parts.first() {
                    let cand = if first.starts_with('@') && parts.len() >= 2 {
                        format!("{}/{}", first, parts[1])
                    } else {
                        (*first).to_string()
                    };
                    if let Some(pkg) = extract_npm_pkg_name_from_spec(&cand) {
                        push_unique(&mut specific_candidates, pkg);
                    }
                }
            }
        }

        if let Some(idx) = trimmed.find("requested by ") {
            let after = &trimmed[idx + "requested by ".len()..];
            if let Some(tok) = after.split_whitespace().next() {
                if let Some(pkg) = extract_npm_pkg_name_from_spec(tok) {
                    push_unique(&mut specific_candidates, pkg);
                }
            }
        }

        let no_prefix = trimmed
            .trim_start_matches("npm error")
            .trim_start_matches("npm ERR!")
            .trim_start_matches("npm warn")
            .trim_start_matches("npm WARN")
            .trim();

        if let Some(after_from) = no_prefix.strip_prefix("from ") {
            if let Some(tok) = after_from.split_whitespace().next() {
                if let Some(pkg) = extract_npm_pkg_name_from_spec(tok) {
                    push_unique(&mut specific_candidates, pkg);
                }
            }
        } else if let Some(from_idx) = no_prefix.find(" from ") {
            let after_from = &no_prefix[from_idx + " from ".len()..];
            if let Some(tok) = after_from.split_whitespace().next() {
                if let Some(pkg) = extract_npm_pkg_name_from_spec(tok) {
                    push_unique(&mut specific_candidates, pkg);
                }
            }
        }

        if let Some(wr_idx) = no_prefix.find("While resolving:") {
            let after_wr = no_prefix[wr_idx + "While resolving:".len()..].trim();
            if let Some(tok) = after_wr.split_whitespace().next() {
                if let Some(pkg) = extract_npm_pkg_name_from_spec(tok) {
                    push_unique(&mut while_resolving_candidates, pkg);
                }
            }
        }
    }

    // 1. Highest confidence: specific parent reference matching a direct dependency
    if !direct_deps.is_empty() {
        if let Some(m) = specific_candidates
            .iter()
            .find(|c| direct_deps.iter().any(|d| d == *c))
        {
            return Some(m.clone());
        }
        // 2. Second highest: `While resolving:` entry that matches a direct dependency (not root project)
        if let Some(m) = while_resolving_candidates
            .iter()
            .find(|c| direct_deps.iter().any(|d| d == *c))
        {
            return Some(m.clone());
        }
    }

    // 3. Fallback: first specific candidate (e.g., transitive parent of transitive dep)
    specific_candidates.into_iter().next()
}

pub fn apply_npm_etarget_heal(
    pkg_path: &std::path::Path,
    pkg_name: &str,
    err_log: Option<&str>,
) -> bool {
    if is_core_framework_package(pkg_name) {
        debug_log_to_file(format!(
            "[Core Lock] Refusing to set core framework package '{}' to '*' in package.json",
            pkg_name
        ));
        return false;
    }

    if let Ok(content) = std::fs::read_to_string(pkg_path) {
        if let Ok(mut json_val) = serde_json::from_str::<Value>(&content) {
            let mut changed = false;
            let mut found_direct = false;
            let wildcard = serde_json::json!("*");

            let root_name = json_val
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let mut direct_deps: Vec<String> = Vec::new();
            for key in &["dependencies", "devDependencies"] {
                if let Some(deps) = json_val.get(*key).and_then(|d| d.as_object()) {
                    for k in deps.keys() {
                        if !direct_deps.contains(k) {
                            direct_deps.push(k.clone());
                        }
                    }
                }
            }

            if let Some(obj) = json_val.as_object_mut() {
                if let Some(deps) = obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                    if let Some(cur) = deps.get(pkg_name) {
                        found_direct = true;
                        if cur != &wildcard {
                            deps.insert(pkg_name.to_string(), wildcard.clone());
                            changed = true;
                        }
                    }
                }
                if let Some(dev_deps) = obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                    if let Some(cur) = dev_deps.get(pkg_name) {
                        found_direct = true;
                        if cur != &wildcard {
                            dev_deps.insert(pkg_name.to_string(), wildcard.clone());
                            changed = true;
                        }
                    }
                }

                // If pkg_name is a transitive dependency not directly listed in package.json:
                // 1) Remove eslint-family packages only when eslint itself is involved.
                // 2) NEVER write global "overrides": { "<pkg>": "*" }! Only write parent-scoped
                //    overrides ("overrides": { "<parent>": { "<pkg>": "*" } }) when the offending
                //    parent package is positively identified from npm logs.
                if !found_direct {
                    let is_eslint_related =
                        pkg_name.contains("eslint") || pkg_name.starts_with("@typescript-eslint/");
                    if is_eslint_related {
                        for key in &["dependencies", "devDependencies"] {
                            if let Some(deps) = obj.get_mut(*key).and_then(|d| d.as_object_mut()) {
                                let keys_to_remove: Vec<String> = deps
                                    .keys()
                                    .filter(|k| {
                                        k.contains("eslint") || k.starts_with("@typescript-eslint/")
                                    })
                                    .cloned()
                                    .collect();
                                for k in keys_to_remove {
                                    deps.remove(&k);
                                    changed = true;
                                }
                            }
                        }
                    }

                    if let Some(parent_pkg) = err_log.and_then(|log| {
                        extract_offending_parent_from_npm_log(
                            log,
                            pkg_name,
                            &direct_deps,
                            root_name.as_deref(),
                        )
                    }) {
                        let overrides_val = obj
                            .entry("overrides")
                            .or_insert_with(|| serde_json::json!({}));
                        if let Some(overrides_obj) = overrides_val.as_object_mut() {
                            match overrides_obj.get_mut(&parent_pkg) {
                                Some(existing_parent_val) if existing_parent_val.is_object() => {
                                    if let Some(parent_map) = existing_parent_val.as_object_mut() {
                                        if parent_map.get(pkg_name) != Some(&wildcard) {
                                            parent_map.insert(pkg_name.to_string(), wildcard);
                                            changed = true;
                                        }
                                    }
                                }
                                None => {
                                    overrides_obj.insert(
                                        parent_pkg,
                                        serde_json::json!({ pkg_name: "*" }),
                                    );
                                    changed = true;
                                }
                                Some(_) => {}
                            }
                        }
                    } else {
                        debug_log_to_file(format!(
                            "[Self-Healing] Transitive dependency '{}' failed without an identifiable parent; refusing to add global wildcard override.",
                            pkg_name
                        ));
                    }
                }
            }
            if changed {
                if let Ok(pretty) = serde_json::to_string_pretty(&json_val) {
                    let _ = std::fs::write(pkg_path, pretty);
                    return true;
                }
            }
        }
    }
    false
}

pub fn set_package_version_to_wildcard(pkg_path: &std::path::Path, pkg_name: &str) -> bool {
    apply_npm_etarget_heal(pkg_path, pkg_name, None)
}

pub fn lock_installed_dependencies(pkg_path: &std::path::Path) {
    let parent_dir = match pkg_path.parent() {
        Some(p) => p,
        None => return,
    };
    if let Ok(content) = std::fs::read_to_string(pkg_path) {
        if let Ok(mut json_val) = serde_json::from_str::<Value>(&content) {
            let mut changed = false;
            if let Some(obj) = json_val.as_object_mut() {
                for deps_key in &["dependencies", "devDependencies"] {
                    if let Some(deps) = obj.get_mut(*deps_key).and_then(|d| d.as_object_mut()) {
                        for (pkg_name, version_val) in deps.iter_mut() {
                            if let Some(version_str) = version_val.as_str() {
                                if version_str == "*" || version_str == "latest" || version_str.is_empty() {
                                    let node_pkg_json_path = parent_dir.join("node_modules").join(pkg_name).join("package.json");
                                    if node_pkg_json_path.exists() {
                                        if let Ok(node_content) = std::fs::read_to_string(&node_pkg_json_path) {
                                            if let Ok(node_val) = serde_json::from_str::<Value>(&node_content) {
                                                if let Some(installed_version) = node_val.get("version").and_then(|v| v.as_str()) {
                                                    let locked_ver = if installed_version.chars().next().map_or(false, |c| c.is_numeric()) {
                                                        format!("^{}", installed_version)
                                                    } else {
                                                        installed_version.to_string()
                                                    };
                                                    debug_log_to_file(format!("[Dependency Installer] Locking {} version to {}", pkg_name, locked_ver));
                                                    *version_val = serde_json::json!(locked_ver);
                                                    changed = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if changed {
                if let Ok(pretty) = serde_json::to_string_pretty(&json_val) {
                    let _ = std::fs::write(pkg_path, pretty);
                }
            }
        }
    }
}

pub async fn ensure_node_toolchain_ready(window: Option<&tauri::Window>, log_event: &str) -> String {
    let emit_msg = |msg: &str, is_err: bool| {
        if let Some(w) = window {
            if !log_event.is_empty() {
                let _ = w.emit(log_event, serde_json::json!({
                    "text": msg.to_string(),
                    "type": if is_err { "error" } else { "info" }
                }));
            }
        }
        debug_log_to_file(format!("[NodeToolchain] {}", msg.trim()));
    };

    let current_path = std::env::var("PATH").unwrap_or_default();

    #[cfg(target_os = "windows")]
    {
        let toolchain_node_dir = get_base_dir().join("toolchain").join("node").join("node-v20.18.1-win-x64");
        let mut candidates = vec![
            toolchain_node_dir.clone(),
            std::path::PathBuf::from(r"C:\Program Files\nodejs"),
            std::path::PathBuf::from(r"C:\Program Files (x86)\nodejs"),
        ];
        if let Some(local_data) = dirs::data_local_dir() {
            candidates.push(local_data.join("Programs").join("nodejs"));
        }
        if let Some(roaming) = dirs::data_dir() {
            candidates.push(roaming.join("nvm"));
        }

        // 1. Check known candidate directories first
        for cand in &candidates {
            if cand.join("node.exe").exists() && cand.join("npm.cmd").exists() {
                let cand_str = cand.to_string_lossy().to_string();
                let new_path = if current_path.to_lowercase().contains(&cand_str.to_lowercase()) {
                    current_path.clone()
                } else {
                    format!("{};{}", cand_str, current_path)
                };
                std::env::set_var("PATH", &new_path);
                return new_path;
            }
        }

        // 2. Check if `where.exe node` succeeds in current PATH
        let mut check_cmd = tokio::process::Command::new("where.exe");
        check_cmd.arg("node").creation_flags(0x08000000);
        if let Ok(out) = check_cmd.output().await {
            if out.status.success() {
                return current_path;
            }
        }

        // 3. Node.js is missing! Automatically download & extract Portable Node.js v20.18.1 LTS
        emit_msg("> [Auto-Setup] Node.js nije pronaÄ‘en na sistemu. Automatski preuzimam i instaliram Node.js v20.18.1 LTS...\n", false);

        let base_toolchain = get_base_dir().join("toolchain").join("node");
        let _ = std::fs::create_dir_all(&base_toolchain);
        let zip_path = base_toolchain.join("node.zip");
        let node_url = "https://nodejs.org/dist/v20.18.1/node-v20.18.1-win-x64.zip";

        let mut download_ok = false;
        if let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(180)).build() {
            if let Ok(resp) = client.get(node_url).send().await {
                if resp.status().is_success() {
                    if let Ok(bytes) = resp.bytes().await {
                        if std::fs::write(&zip_path, &bytes).is_ok() {
                            download_ok = true;
                        }
                    }
                }
            }
        }

        if !download_ok {
            // Fallback to curl.exe
            let mut curl_cmd = tokio::process::Command::new("curl.exe");
            curl_cmd
                .arg("-L")
                .arg("-s")
                .arg("-o")
                .arg(&zip_path)
                .arg(node_url)
                .creation_flags(0x08000000);
            if let Ok(st) = curl_cmd.status().await {
                download_ok = st.success() && zip_path.exists();
            }
        }

        if download_ok {
            emit_msg("> [Auto-Setup] Raspakujem Node.js v20.18.1 LTS okruÅ¾enje...\n", false);
            let mut tar_cmd = tokio::process::Command::new("tar.exe");
            tar_cmd
                .arg("-xf")
                .arg(&zip_path)
                .arg("-C")
                .arg(&base_toolchain)
                .creation_flags(0x08000000);
            let _ = tar_cmd.status().await;
            let _ = std::fs::remove_file(&zip_path);
        }

        if toolchain_node_dir.join("node.exe").exists() {
            let node_bin_str = toolchain_node_dir.to_string_lossy().to_string();
            // Create shims in %LOCALAPPDATA%\Microsoft\WindowsApps so all child shells find node/npm/npx
            if let Some(local_data) = dirs::data_local_dir() {
                let win_apps = local_data.join("Microsoft").join("WindowsApps");
                if win_apps.exists() {
                    let _ = std::fs::write(
                        win_apps.join("node.cmd"),
                        format!("@echo off\r\n\"{}\\node.exe\" %*\r\n", node_bin_str),
                    );
                    let _ = std::fs::write(
                        win_apps.join("npm.cmd"),
                        format!("@echo off\r\nSET \"PATH={};%PATH%\"\r\n\"{}\\npm.cmd\" %*\r\n", node_bin_str, node_bin_str),
                    );
                    let _ = std::fs::write(
                        win_apps.join("npx.cmd"),
                        format!("@echo off\r\nSET \"PATH={};%PATH%\"\r\n\"{}\\npx.cmd\" %*\r\n", node_bin_str, node_bin_str),
                    );
                }
            }

            let new_path = format!("{};{}", node_bin_str, current_path);
            std::env::set_var("PATH", &new_path);
            emit_msg("> [Auto-Setup] Node.js v20.18.1 LTS (node, npm, npx) je uspeÅ¡no instaliran i spreman!\n", false);
            return new_path;
        } else {
            emit_msg("> [Auto-Setup Error] Automatsko preuzimanje Node.js-a nije uspelo. Proverite internet konekciju.\n", true);
        }
    }

    current_path
}

pub async fn run_npm_install_with_auto_healing(
    proj_dir: &std::path::Path,
    window: &tauri::Window,
    log_event: &str,
    task_id_opt: Option<&str>,
) -> NpmInstallOutcome {
    let pkg_path = proj_dir.join("package.json");
    if !pkg_path.exists() {
        return NpmInstallOutcome::Installed;
    }

    let enriched_path = ensure_node_toolchain_ready(Some(window), log_event).await;

    use tauri::Manager;
    let mut project_id = proj_dir.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    if project_id == "src" {
        if let Some(parent) = proj_dir.parent() {
            project_id = parent.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        }
    }

    if let Some(state) = window.try_state::<ServerState>() {
        let child_opt = {
            let mut servers = state.0.lock().unwrap();
            servers.remove(&project_id)
        };
        if let Some(mut child) = child_opt {
            if !log_event.is_empty() {
                let _ = window.emit(log_event, serde_json::json!({
                    "text": format!("> [Self-Healing] Zaustavljanje aktivnog dev servera za projekat '{}' radi spreÄavanja EPERM file lock-a...\n", project_id),
                    "type": "info"
                }));
            }
            #[cfg(target_os = "windows")]
            {
                if let Some(pid) = child.id() {
                    let mut kill_cmd = tokio::process::Command::new("taskkill");
                    kill_cmd.arg("/F").arg("/T").arg("/PID").arg(pid.to_string());
                    kill_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                    let _ = kill_cmd.spawn();
                }
            }
            let _ = child.kill().await;
            let _ = window.emit("server:status", serde_json::json!({
                "status": "stopped"
            }));
        }
    }

    let emit_log = |text: &str, is_error: bool| {
        if !log_event.is_empty() {
            let _ = window.emit(log_event, serde_json::json!({
                "text": text.to_string(),
                "type": if is_error { "error" } else { "info" }
            }));
        }
        debug_log_to_file(format!("[Dependency Installer] {}", text.trim()));
    };

    let emit_progress = |msg: &str, progress: f64| {
        if let Some(task_id) = task_id_opt {
            let _ = window.emit("task:progress", serde_json::json!({
                "taskId": task_id,
                "projectId": project_id,
                "phase": "install-deps",
                "status": "generating",
                "offlineWaiting": false,
                "message": msg,
                "progress": progress
            }));
        }
    };

    emit_progress("Finalizing dependencies...", 75.0);
    emit_log("> Starting dependency installation check...\n", false);

    if sanitize_package_json_for_offline(&pkg_path) {
        emit_log("> [Offline Sanitizer] Uklonjeni konfliktni linter/test paketi i duplikati iz package.json pre offline instalacije.\n", false);
    }

    let mut prefer_offline = true;
    let mut pending_npm_repair: Option<(String, String, String)> = None;
    const MAX_ATTEMPTS: usize = 5;
    for attempt in 1..=MAX_ATTEMPTS {
        emit_progress(&format!("Installing dependencies (attempt {}/{})...", attempt, MAX_ATTEMPTS), 75.0 + (attempt as f64 - 1.0) * 0.6);
        emit_log(
            &format!(
                "> Running npm install ({}), attempt {}/{}...\n",
                if prefer_offline { "prefer-offline" } else { "online" },
                attempt,
                MAX_ATTEMPTS
            ),
            false,
        );

        #[cfg(target_os = "windows")]
        let mut install_cmd = tokio::process::Command::new("cmd");
        #[cfg(target_os = "windows")]
        {
            install_cmd.arg("/C").arg("npm").arg("install").arg("--legacy-peer-deps");
            if prefer_offline {
                install_cmd.arg("--prefer-offline");
            }
            install_cmd.current_dir(proj_dir);
        }

        #[cfg(not(target_os = "windows"))]
        let mut install_cmd = tokio::process::Command::new("npm");
        #[cfg(not(target_os = "windows"))]
        {
            install_cmd.arg("install").arg("--legacy-peer-deps");
            if prefer_offline {
                install_cmd.arg("--prefer-offline");
            }
            install_cmd.current_dir(proj_dir);
        }

        #[cfg(target_os = "windows")]
        install_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

        install_cmd.env("PATH", &enriched_path);
        install_cmd.stdout(std::process::Stdio::piped());
        install_cmd.stderr(std::process::Stdio::piped());

        let mut child = match install_cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                emit_log(&format!("> Failed to start npm install command: {}\n", e), true);
                break;
            }
        };

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let window_log = window.clone();
        let log_event_str = log_event.to_string();
        let stdout_handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stdout).lines();
            let mut collected = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                if !log_event_str.is_empty() {
                    let _ = window_log.emit(&log_event_str, serde_json::json!({
                        "text": format!("> [npm install] {}\n", line),
                        "type": "info"
                    }));
                }
                collected.push(line);
            }
            collected.join("\n")
        });

        let window_err = window.clone();
        let log_event_err_str = log_event.to_string();
        let stderr_handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stderr).lines();
            let mut collected = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                if !log_event_err_str.is_empty() {
                    let is_warn = line.contains("npm warn") || line.contains("npm WARN") || line.contains("npm depr") || line.contains("npm DEPR") || line.contains("npm notice") || line.contains("npm NOTICE");
                    let prefix = if is_warn { "> [npm install warn]" } else { "> [npm install err]" };
                    let log_type = if is_warn { "info" } else { "error" };
                    let _ = window_err.emit(&log_event_err_str, serde_json::json!({
                        "text": format!("{} {}\n", prefix, line),
                        "type": log_type
                    }));
                }
                collected.push(line);
            }
            collected.join("\n")
        });

        let status = child.wait().await;
        let stdout_log = stdout_handle.await.unwrap_or_default();
        let stderr_log = stderr_handle.await.unwrap_or_default();
        let err_log = format!("{}\n{}", stdout_log, stderr_log);

        match status {
            Ok(exit_status) if exit_status.success() => {
                if let Some((prev_eid, _, prev_source)) = pending_npm_repair.take() {
                    crate::katastar::finalize_katastar_outcome(
                        &prev_eid,
                        true,
                        &prev_source,
                        "npm_install",
                        None,
                        None,
                        "",
                    );
                }
                emit_log("> npm install completed successfully.\n", false);
                lock_installed_dependencies(&pkg_path);
                return NpmInstallOutcome::Installed;
            }
            Ok(exit_status) => {
                emit_log(&format!("> npm install failed with exit status: {}\n", exit_status), true);
            }
            Err(e) => {
                emit_log(&format!("> Error waiting for npm install: {}\n", e), true);
            }
        }

        let lock_path = proj_dir.join("package-lock.json");
        let is_network_err = is_npm_network_error(&err_log);
        let cur_sig_info = crate::katastar::normalize_error_signature("npm", &err_log, Some("package.json"));

        if let Some((prev_eid, prev_sig, prev_source)) = pending_npm_repair.take() {
            if !is_network_err {
                let prev_passed = prev_sig != cur_sig_info.error_signature;
                crate::katastar::finalize_katastar_outcome(
                    &prev_eid,
                    prev_passed,
                    &prev_source,
                    "npm_install",
                    None,
                    None,
                    "",
                );
            }
        }

        // If the machine is offline or registry is unreachable (after prefer-offline fallback),
        // preserve package-lock.json and pause cleanly so execution resumes once back online.
        if is_network_err {
            emit_log(
                "> [Offline / Network Error] Nema internet konekcije ili je npm registar nedostupan. Sačuvan je postojeći package-lock.json — nastavljamo sa instalacijom paketa kada budemo ponovo online.\n",
                true,
            );
            return NpmInstallOutcome::OfflinePaused;
        }

        // When --prefer-offline hits a stale cached packument (common cause of false ETARGET / E404
        // for newer transitive versions like shell-quote@^1.10.0), retry online first while keeping
        // package-lock.json intact before weakening version ranges or removing valid packages.
        if prefer_offline {
            prefer_offline = false;
            let eid = crate::katastar::register_repair_attempt(
                "npm",
                &err_log,
                "package.json",
                "deterministic",
                "retry_online_without_prefer_offline",
                None,
                "Retry npm install online without --prefer-offline when cached packument is stale",
                "",
            );
            pending_npm_repair = Some((eid, cur_sig_info.error_signature, "deterministic".to_string()));
            emit_log("> [Self-Healing] Offline npm cache nije uspeo (moguć zastareo cache packument). Prelazim na online npm rezoluciju bez --prefer-offline...\n", false);
            continue;
        }

        if let Some(action) = analyze_npm_install_failure(&err_log) {
            let had_lock = lock_path.exists();
            if had_lock {
                let _ = std::fs::remove_file(&lock_path);
            }
            match action {
                NpmFailureAction::SetWildcard(pkg_name) => {
                    if is_core_framework_package(&pkg_name) {
                        emit_log(
                            &format!(
                                "> [Core Lock] Paket '{}' je zaključani core framework paket i neće biti spušten na '*' u package.json.\n",
                                pkg_name
                            ),
                            true,
                        );
                        break;
                    }
                    let eid = crate::katastar::register_repair_attempt(
                        "npm",
                        &err_log,
                        "package.json",
                        "deterministic",
                        "npm_set_wildcard",
                        None,
                        &format!("Set wildcard or parent-scoped override for failing npm version range on '{}'", pkg_name),
                        "",
                    );
                    pending_npm_repair = Some((eid, cur_sig_info.error_signature, "deterministic".to_string()));
                    if apply_npm_etarget_heal(&pkg_path, &pkg_name, Some(&err_log)) {
                        emit_log(&format!("> [Self-Healing] Package '{}' has no matching version. Applied safe dependency/parent-scoped override in package.json and retrying...\n", pkg_name), false);
                    } else if had_lock {
                        emit_log(&format!("> [Self-Healing] Could not safely rewrite '{}' without affecting global resolution. Removed stale package-lock.json and retrying...\n", pkg_name), false);
                    } else {
                        if let Some((prev_eid, _, prev_source)) = pending_npm_repair.take() {
                            crate::katastar::finalize_katastar_outcome(&prev_eid, false, &prev_source, "npm_install", None, None, "");
                        }
                        emit_log(&format!("> [Self-Healing] Package '{}' could not be resolved safely (core-locked or unknown transitive parent). Aborting redundant retries.\n", pkg_name), true);
                        break;
                    }
                }
                NpmFailureAction::Remove(pkg_name) => {
                    if is_core_framework_package(&pkg_name) {
                        emit_log(
                            &format!(
                                "> [Core Lock] Paket '{}' je zaključani core framework paket i neće biti uklonjen iz package.json.\n",
                                pkg_name
                            ),
                            true,
                        );
                        break;
                    }
                    let eid = crate::katastar::register_repair_attempt(
                        "npm",
                        &err_log,
                        "package.json",
                        "deterministic",
                        "npm_remove_missing_pkg",
                        None,
                        &format!("Remove nonexistent/invalid package '{}' from package.json", pkg_name),
                        "",
                    );
                    pending_npm_repair = Some((eid, cur_sig_info.error_signature, "deterministic".to_string()));
                    if remove_package_from_json(&pkg_path, &pkg_name) {
                        emit_log(&format!("> [Self-Healing] Package '{}' not found in npm registry or failed install. Removing from package.json and retrying...\n", pkg_name), false);
                    } else if had_lock {
                        emit_log(&format!("> [Self-Healing] Failed resolving transitive package '{}'. Removed stale package-lock.json and retrying...\n", pkg_name), false);
                    } else {
                        if let Some((prev_eid, _, prev_source)) = pending_npm_repair.take() {
                            crate::katastar::finalize_katastar_outcome(&prev_eid, false, &prev_source, "npm_install", None, None, "");
                        }
                        emit_log(&format!("> [Self-Healing] Transitive package '{}' could not be resolved or removed from package.json. Aborting redundant retries.\n", pkg_name), true);
                        break;
                    }
                }
            }
        } else if lock_path.exists() {
            let eid = crate::katastar::register_repair_attempt(
                "npm",
                &err_log,
                "package.json",
                "deterministic",
                "npm_remove_stale_lock",
                None,
                "Remove stale package-lock.json and retry npm install",
                "",
            );
            pending_npm_repair = Some((eid, cur_sig_info.error_signature, "deterministic".to_string()));
            emit_log("> [Self-Healing] No specific package error parsed. Deleting package-lock.json and retrying...\n", false);
            let _ = std::fs::remove_file(lock_path);
        } else {
            emit_log("> [Self-Healing] No actionable package fix found and no package-lock.json to clean. Aborting redundant retries.\n", true);
            break;
        }
    }

    if let Some((prev_eid, _, prev_source)) = pending_npm_repair.take() {
        crate::katastar::finalize_katastar_outcome(&prev_eid, false, &prev_source, "npm_install", None, None, "");
    }

    NpmInstallOutcome::Failed
}

