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
            if !content.contains("@mui/material") {
                needs_write = true;
            }
        }
    }
    
    if needs_write {
        let default_db = serde_json::json!({
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
                        "import { useSession } from 'next-auth/react';"
                    ],
                    "compatibility": {
                        "node": ">=16.0.0",
                        "next": ">=12.0.0"
                    },
                    "bestPractices": "Use NextAuth(authOptions) in API routes. Use useSession() hook on the client side with SessionProvider."
                },
                {
                    "name": "@next-auth/prisma-adapter",
                    "version": "^1.0.7",
                    "aliases": [],
                    "importExamples": ["import { PrismaAdapter } from '@next-auth/prisma-adapter';"],
                    "compatibility": {
                        "next-auth": ">=4.0.0",
                        "prisma": ">=4.0.0"
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
                        "react-hook-form": ">=7.0.0"
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
                    "name": "bcrypt",
                    "version": "^5.1.1",
                    "aliases": ["bcryptjs-alias"],
                    "importExamples": ["import bcrypt from 'bcrypt';"],
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
                        "react": ">=17.0.0",
                        "@emotion/react": "^11.0.0",
                        "@emotion/styled": "^11.0.0"
                    },
                    "bestPractices": "Import components from @mui/material. Use custom styles with styled utility."
                },
                {
                    "name": "@mui/icons-material",
                    "version": "^5.15.0",
                    "aliases": ["@material-ui/icons"],
                    "importExamples": ["import { Mail, Settings, Notifications } from '@mui/icons-material';"],
                    "compatibility": {
                        "@mui/material": ">=5.0.0"
                    },
                    "bestPractices": "Import icons individually or destructured. Make sure @mui/material is installed."
                },
                {
                    "name": "@prisma/client",
                    "version": "^5.14.0",
                    "aliases": [],
                    "importExamples": ["import { PrismaClient } from '@prisma/client';"],
                    "compatibility": {
                        "prisma": ">=5.0.0"
                    },
                    "bestPractices": "Instantiate PrismaClient globally to avoid multiple connection pools in hot reload environments. Prisma client MUST only be imported and used inside server-side code (Next.js API route handlers), never in React hooks or client components ('use client')."
                },
                {
                    "name": "prisma",
                    "version": "^5.14.0",
                    "aliases": [],
                    "importExamples": [],
                    "compatibility": {},
                    "bestPractices": "Use prisma developer CLI commands via npx prisma."
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
    
    // 1. Check if it's a relative import or absolute path in the project
    if imported_path_trimmed.starts_with('.') || imported_path_trimmed.starts_with('/') || imported_path_trimmed.starts_with("@/") {
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

pub fn repair_package_json_dependencies(proj_dir: &std::path::Path) {
    let pkg_path = proj_dir.join("package.json");
    if !pkg_path.exists() {
        return;
    }
    
    let packages_db = load_packages_db();
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
                                    if (trimmed.starts_with("import ") || trimmed.starts_with("import{") || trimmed.starts_with("import type") || trimmed.contains("require(")) 
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
                                                    
                                                    // Check if it's an import we should rewrite
                                                    if let Some(correct_path) = resolve_and_rewrite_import(&literal, &packages_db) {
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
                                    if (line.starts_with("import ") || line.starts_with("import{") || line.starts_with("import type") || line.contains("require(")) && (line.contains('\'') || line.contains('"') || line.contains('`')) {
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
                                                    if !literal.is_empty() && !literal.starts_with('.') && !literal.starts_with('/') && !literal.starts_with("@/") {
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
                                                                if !node_builtins.contains(&pkg.as_str()) && pkg != "react" && pkg != "react-dom" && pkg != "next" {
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
                        for (k, v) in [
                            ("next", "14.2.35"),
                            ("react", "^18.2.0"),
                            ("react-dom", "^18.2.0"),
                            ("lucide-react", "^0.460.0"),
                            ("clsx", "^2.1.1"),
                            ("tailwind-merge", "^2.3.0"),
                        ] {
                            if !deps_obj.contains_key(k) {
                                deps_obj.insert(k.to_string(), serde_json::json!(v));
                                changed = true;
                            }
                        }
                        if deps_obj.contains_key("next-auth") && !deps_obj.contains_key("@auth/prisma-adapter") && has_prisma_schema {
                            deps_obj.insert("@auth/prisma-adapter".to_string(), serde_json::json!("^1.0.12"));
                            changed = true;
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
                            if !dev_deps_obj.contains_key(k) {
                                dev_deps_obj.insert(k.to_string(), serde_json::json!(v));
                                changed = true;
                            }
                        }
                    }
                }

                let uses_prisma = has_prisma_schema
                    || pkg_obj.get("dependencies").and_then(|d| d.as_object()).map_or(false, |d| d.contains_key("@prisma/client"));
                if uses_prisma {
                    if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                        if !deps_obj.contains_key("@prisma/client") {
                            deps_obj.insert("@prisma/client".to_string(), serde_json::json!("^5.14.0"));
                            changed = true;
                        }
                    }
                    if let Some(dev_deps_obj) = pkg_obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                        if !dev_deps_obj.contains_key("prisma") {
                            dev_deps_obj.insert("prisma".to_string(), serde_json::json!("^5.14.0"));
                            changed = true;
                        }
                    }
                }
                
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
                for imp in imports_found {
                    // Try to find the version in the database
                    let mut version = None;
                    for pkg in &packages_db {
                        if pkg.name == imp {
                            version = Some(pkg.version.clone());
                            break;
                        }
                    }
                    
                    if let Some(v) = version {
                        if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if !deps_obj.contains_key(&imp) {
                                deps_obj.insert(imp.clone(), serde_json::json!(v));
                                changed = true;
                            }
                        }
                    } else if imp == "prisma" {
                        if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if !deps_obj.contains_key("@prisma/client") {
                                deps_obj.insert("@prisma/client".to_string(), serde_json::json!("^5.14.0"));
                                changed = true;
                            }
                        }
                    } else if imp == "@mui/material" {
                        if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if !deps_obj.contains_key("@mui/material") {
                                deps_obj.insert("@mui/material".to_string(), serde_json::json!("^5.15.0"));
                                changed = true;
                            }
                            if !deps_obj.contains_key("@emotion/react") {
                                deps_obj.insert("@emotion/react".to_string(), serde_json::json!("^11.11.0"));
                                changed = true;
                            }
                            if !deps_obj.contains_key("@emotion/styled") {
                                deps_obj.insert("@emotion/styled".to_string(), serde_json::json!("^11.11.0"));
                                changed = true;
                            }
                        }
                    } else if imp == "@mui/icons-material" {
                        if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if !deps_obj.contains_key("@mui/icons-material") {
                                deps_obj.insert("@mui/icons-material".to_string(), serde_json::json!("^5.15.0"));
                                changed = true;
                            }
                        }
                    } else {
                        if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if !deps_obj.contains_key(&imp) {
                                deps_obj.insert(imp.clone(), serde_json::json!("*"));
                                changed = true;
                            }
                        }
                    }
                    
                    if has_tsconfig {
                        let type_pkg = match imp.as_str() {
                            "bcrypt" => Some(("@types/bcrypt", "^5.0.2")),
                            "bcryptjs" => Some(("@types/bcryptjs", "^2.4.6")),
                            "jsonwebtoken" => Some(("@types/jsonwebtoken", "^9.0.6")),
                            "uuid" => Some(("@types/uuid", "^9.0.8")),
                            "canvas-confetti" => Some(("@types/canvas-confetti", "^1.6.4")),
                            _ => None
                        };
                        if let Some((tp_name, tp_ver)) = type_pkg {
                            if let Some(dev_deps_obj) = pkg_obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                                if !dev_deps_obj.contains_key(tp_name) {
                                    dev_deps_obj.insert(tp_name.to_string(), serde_json::json!(tp_ver));
                                    changed = true;
                                }
                            }
                        }
                    }
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

pub fn remove_package_from_json(pkg_path: &std::path::Path, pkg_name: &str) -> bool {
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
        let mut cleaned = line.to_string();
        while cleaned.ends_with('.') || cleaned.ends_with('\'') || cleaned.ends_with('"') || cleaned.ends_with(';') {
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

pub fn set_package_version_to_wildcard(pkg_path: &std::path::Path, pkg_name: &str) -> bool {
    if let Ok(content) = std::fs::read_to_string(pkg_path) {
        if let Ok(mut json_val) = serde_json::from_str::<Value>(&content) {
            let mut changed = false;
            if let Some(obj) = json_val.as_object_mut() {
                if let Some(deps) = obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                    if deps.contains_key(pkg_name) {
                        deps.insert(pkg_name.to_string(), serde_json::json!("*"));
                        changed = true;
                    }
                }
                if let Some(dev_deps) = obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                    if dev_deps.contains_key(pkg_name) {
                        dev_deps.insert(pkg_name.to_string(), serde_json::json!("*"));
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
        emit_msg("> [Auto-Setup] Node.js nije pronađen na sistemu. Automatski preuzimam i instaliram Node.js v20.18.1 LTS...\n", false);

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
            emit_msg("> [Auto-Setup] Raspakujem Node.js v20.18.1 LTS okruženje...\n", false);
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
            emit_msg("> [Auto-Setup] Node.js v20.18.1 LTS (node, npm, npx) je uspešno instaliran i spreman!\n", false);
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
) -> bool {
    let pkg_path = proj_dir.join("package.json");
    if !pkg_path.exists() {
        return false;
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
                    "text": format!("> [Self-Healing] Zaustavljanje aktivnog dev servera za projekat '{}' radi sprečavanja EPERM file lock-a...\n", project_id),
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
                "phase": "install-deps",
                "message": msg,
                "progress": progress
            }));
        }
    };

    emit_progress("Finalizing dependencies...", 75.0);
    emit_log("> Starting dependency installation check...\n", false);

    for attempt in 1..=4 {
        emit_progress(&format!("Installing dependencies (attempt {}/4)...", attempt), 75.0 + (attempt as f64 - 1.0) * 0.7);
        emit_log(&format!("> Running npm install, attempt {}/4...\n", attempt), false);

        #[cfg(target_os = "windows")]
        let mut install_cmd = tokio::process::Command::new("cmd");
        #[cfg(target_os = "windows")]
        install_cmd.arg("/C").arg("npm").arg("install").arg("--legacy-peer-deps").current_dir(proj_dir);

        #[cfg(not(target_os = "windows"))]
        let mut install_cmd = tokio::process::Command::new("npm");
        #[cfg(not(target_os = "windows"))]
        install_cmd.arg("install").arg("--legacy-peer-deps").current_dir(proj_dir);

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
                emit_log("> npm install completed successfully.\n", false);
                lock_installed_dependencies(&pkg_path);
                return true;
            }
            Ok(exit_status) => {
                emit_log(&format!("> npm install failed with exit status: {}\n", exit_status), true);
            }
            Err(e) => {
                emit_log(&format!("> Error waiting for npm install: {}\n", e), true);
            }
        }

        if let Some(action) = analyze_npm_install_failure(&err_log) {
            match action {
                NpmFailureAction::SetWildcard(pkg_name) => {
                    emit_log(&format!("> [Self-Healing] Package '{}' has no matching version. Updating its version to '*' in package.json and retrying...\n", pkg_name), false);
                    set_package_version_to_wildcard(&pkg_path, &pkg_name);
                }
                NpmFailureAction::Remove(pkg_name) => {
                    emit_log(&format!("> [Self-Healing] Package '{}' not found in npm registry. Removing from package.json and retrying...\n", pkg_name), false);
                    remove_package_from_json(&pkg_path, &pkg_name);
                }
            }
        } else {
            emit_log("> [Self-Healing] No specific package error parsed. Deleting package-lock.json and retrying...\n", false);
            let lock_path = proj_dir.join("package-lock.json");
            if lock_path.exists() {
                let _ = std::fs::remove_file(lock_path);
            }
        }
    }

    false
}

