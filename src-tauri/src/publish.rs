use crate::ollama::get_token;
use crate::project::{get_project_path, get_projects_file};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tauri::Emitter;

const GITHUB_API_BASE: &str = "https://api.github.com";
const NEON_API_BASE: &str = "https://console.neon.tech/api/v2";
const VERCEL_API_BASE: &str = "https://api.vercel.com";
const CLOUDFLARE_API_BASE: &str = "https://api.cloudflare.com/client/v4";

// ==================== TYPES ====================

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PublishStateFile {
    pub github_repo_name: Option<String>,
    pub github_repo_url: Option<String>,
    pub github_username: Option<String>,
    pub neon_project_id: Option<String>,
    pub neon_project_name: Option<String>,
    pub vercel_project_id: Option<String>,
    pub vercel_project_name: Option<String>,
    pub deploy_url: Option<String>,
    pub custom_domain: Option<String>,
    pub domain_verified: bool,
    pub nextauth_secret: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DnsRecordInstruction {
    pub record_type: String, // "A" | "CNAME" | "TXT"
    pub name: String,
    pub value: String,
    pub purpose: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PublishPreflight {
    pub project_id: String,
    pub suggested_repo_name: String,
    pub has_github_token: bool,
    pub has_vercel_token: bool,
    pub has_neon_token: bool,
    pub has_cloudflare_token: bool,
    pub has_prisma: bool,
    pub is_expo: bool,
    pub detected_env_keys: Vec<String>,
    pub existing_state: Option<PublishStateFile>,
    pub compliance_summary: Option<crate::compliance::CompliancePreflightSummary>,
    pub cache_summary: Option<crate::cache_template::CachePreflightSummary>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PublishRequest {
    pub project_id: String,
    pub repo_name: Option<String>,
    #[serde(default)]
    pub is_private_repo: bool,
    /// "neon" | "custom_url" | "skip"
    #[serde(default = "default_db_mode")]
    pub db_mode: String,
    pub custom_database_url: Option<String>,
    pub neon_region: Option<String>,
    #[serde(default = "default_true")]
    pub run_prisma_push: bool,
    pub custom_domain: Option<String>,
    #[serde(default)]
    pub auto_cloudflare_dns: bool,
    #[serde(default = "default_true")]
    pub sync_local_env_keys: bool,
    pub vercel_team_id: Option<String>,
    #[serde(default)]
    pub upstash_redis_rest_url: Option<String>,
    #[serde(default)]
    pub upstash_redis_rest_token: Option<String>,
    #[serde(default)]
    pub redis_url: Option<String>,
}

fn default_db_mode() -> String {
    "neon".to_string()
}
fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PublishResult {
    pub success: bool,
    pub github_url: Option<String>,
    pub deploy_url: Option<String>,
    pub custom_domain_url: Option<String>,
    pub neon_project_id: Option<String>,
    pub vercel_project_name: Option<String>,
    pub prisma_pushed: bool,
    pub domain_verified: bool,
    pub cloudflare_dns_configured: bool,
    pub dns_records: Vec<DnsRecordInstruction>,
    pub env_keys_synced: Vec<String>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DomainVerifyResult {
    pub success: bool,
    pub domain: String,
    pub verified: bool,
    pub misconfigured: bool,
    pub dns_records: Vec<DnsRecordInstruction>,
    pub message: String,
}

// ==================== HELPERS: SLUGS, STATE, ENV, VERCEL READINESS ====================

pub fn sanitize_repo_slug(raw: &str) -> String {
    let mut slug: String = raw
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let trimmed = slug.trim_matches('-').to_string();
    let base = if trimmed.is_empty() {
        "axiom-app".to_string()
    } else if trimmed.len() > 52 {
        trimmed[..52].trim_end_matches('-').to_string()
    } else {
        trimmed
    };
    base
}

fn load_publish_state(proj_dir: &Path) -> Option<PublishStateFile> {
    let p = proj_dir.join(".axiom").join("publish-state.json");
    let raw = fs::read_to_string(p).ok()?;
    serde_json::from_str::<PublishStateFile>(&raw).ok()
}

fn save_publish_state(proj_dir: &Path, state: &PublishStateFile) {
    let dir = proj_dir.join(".axiom");
    let _ = fs::create_dir_all(&dir);
    let p = dir.join("publish-state.json");
    if let Ok(pretty) = serde_json::to_string_pretty(state) {
        let _ = fs::write(p, pretty);
    }
}

fn update_projects_json_metadata(project_id: &str, state: &PublishStateFile, deployed_ok: bool) {
    let file_path = get_projects_file();
    if !file_path.exists() {
        return;
    }
    let Ok(raw) = fs::read_to_string(&file_path) else {
        return;
    };
    let Ok(mut projects) = serde_json::from_str::<Vec<Value>>(&raw) else {
        return;
    };

    if let Some(proj) = projects
        .iter_mut()
        .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(project_id))
    {
        if let Some(obj) = proj.as_object_mut() {
            if deployed_ok {
                obj.insert("status".to_string(), json!("deployed"));
            }
            obj.insert("updatedAt".to_string(), json!(chrono::Utc::now().to_rfc3339()));
            let meta = obj
                .entry("metadata".to_string())
                .or_insert_with(|| json!({}));
            if let Some(m) = meta.as_object_mut() {
                if let Some(ref gh) = state.github_repo_url {
                    m.insert("githubUrl".to_string(), json!(gh));
                }
                if let Some(ref dep) = state.deploy_url {
                    m.insert("deployUrl".to_string(), json!(dep));
                }
                if let Some(ref dom) = state.custom_domain {
                    m.insert("customDomain".to_string(), json!(dom));
                    m.insert("customDomainVerified".to_string(), json!(state.domain_verified));
                }
                if let Some(ref nid) = state.neon_project_id {
                    m.insert("neonProjectId".to_string(), json!(nid));
                }
                if let Some(ref vpn) = state.vercel_project_name {
                    m.insert("vercelProjectName".to_string(), json!(vpn));
                }
            }
        }
        if let Ok(pretty) = serde_json::to_string_pretty(&projects) {
            let _ = fs::write(&file_path, pretty);
        }
    }
}

/// Extracts non-empty, non-placeholder, non-localhost environment variables from `.env.local` / `.env`
/// excluding managed keys (`DATABASE_URL`, `DIRECT_URL`, `NEXTAUTH_SECRET`, `NEXTAUTH_URL`).
pub fn extract_safe_local_env_vars(proj_dir: &Path) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    for fname in [".env", ".env.local"] {
        let p = proj_dir.join(fname);
        let Ok(raw) = fs::read_to_string(&p) else {
            continue;
        };
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let Some((k_raw, v_raw)) = trimmed.split_once('=') else {
                continue;
            };
            let key = k_raw.trim().trim_start_matches("export ").trim();
            if key.is_empty()
                || !key
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                continue;
            }
            if matches!(
                key,
                "DATABASE_URL" | "DIRECT_URL" | "NEXTAUTH_SECRET" | "NEXTAUTH_URL" | "NODE_ENV" | "PORT"
            ) {
                continue;
            }
            let val = v_raw.trim().trim_matches('"').trim_matches('\'').trim();
            if val.is_empty() {
                continue;
            }
            let low = val.to_lowercase();
            if low.contains("localhost")
                || low.contains("127.0.0.1")
                || low.starts_with("your_")
                || low.starts_with("sk-placeholder")
                || low == "changeme"
                || low == "placeholder"
            {
                continue;
            }
            vars.insert(key.to_string(), val.to_string());
        }
    }
    vars
}

/// Ensures the generated project is safe and ready to push to GitHub and build on Vercel:
/// 1. Writes/augments `.gitignore` so `node_modules`, `.next`, `.env*`, and `.axiom` are never pushed.
/// 2. Ensures `package.json` runs `prisma generate` on `postinstall` and before `next build` when Prisma exists,
///    and strips broken LLM `peerDependencies`.
/// 3. Ensures App Router `src/app/api/**/route.ts` files declare `export const dynamic = 'force-dynamic';`
///    so Next.js 14 does not statically prerender DB API routes during Vercel build.
pub fn prepare_project_for_vercel_publish(proj_dir: &Path) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    let has_prisma = proj_dir.join("prisma").join("schema.prisma").is_file();

    // 1. Ensure .gitignore protects secrets and heavy build folders
    let gitignore_path = proj_dir.join(".gitignore");
    let required_ignores = [
        "node_modules/",
        ".next/",
        "out/",
        "dist/",
        ".vercel/",
        ".axiom/",
        ".env",
        ".env.local",
        ".env.*.local",
        "*.tsbuildinfo",
        ".expo/",
        ".expo-verify/",
    ];
    let existing_gi = fs::read_to_string(&gitignore_path).unwrap_or_default();
    let existing_lines: Vec<&str> = existing_gi.lines().map(|l| l.trim()).collect();
    let mut missing_gi = Vec::new();
    for entry in required_ignores {
        let alt = entry.trim_end_matches('/');
        if !existing_lines.iter().any(|l| *l == entry || *l == alt) {
            missing_gi.push(entry);
        }
    }
    if !missing_gi.is_empty() {
        let mut updated = existing_gi;
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        if updated.is_empty() {
            updated.push_str("# Axiom Forge Publish Safety .gitignore\n");
        }
        for entry in &missing_gi {
            updated.push_str(entry);
            updated.push('\n');
        }
        fs::write(&gitignore_path, updated).map_err(|e| format!("Failed writing .gitignore: {}", e))?;
        notes.push(format!("Updated .gitignore (+{} protected entries)", missing_gi.len()));
    }

    // 2. Ensure package.json has Vercel-safe Prisma scripts
    let pkg_path = proj_dir.join("package.json");
    if pkg_path.is_file() {
        if let Ok(raw) = fs::read_to_string(&pkg_path) {
            if let Ok(mut pkg) = serde_json::from_str::<Value>(&raw) {
                let mut pkg_changed = false;
                if has_prisma {
                    if let Some(scripts) = pkg.get_mut("scripts").and_then(|s| s.as_object_mut()) {
                        let postinstall = scripts
                            .get("postinstall")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !postinstall.contains("prisma generate") {
                            scripts.insert("postinstall".to_string(), json!("prisma generate"));
                            pkg_changed = true;
                        }
                        let build_cmd = scripts
                            .get("build")
                            .and_then(|v| v.as_str())
                            .unwrap_or("next build");
                        if build_cmd.contains("next build") && !build_cmd.contains("prisma generate") {
                            scripts.insert(
                                "build".to_string(),
                                json!(format!("prisma generate && {}", build_cmd)),
                            );
                            pkg_changed = true;
                        }
                    }
                }
                if pkg.get("peerDependencies").is_some() {
                    if let Some(obj) = pkg.as_object_mut() {
                        obj.remove("peerDependencies");
                        pkg_changed = true;
                    }
                }
                if pkg_changed {
                    if let Ok(pretty) = serde_json::to_string_pretty(&pkg) {
                        let _ = fs::write(&pkg_path, pretty);
                        notes.push("Updated package.json for Vercel Prisma build".to_string());
                    }
                }
            }
        }
    }

    // 3. Ensure Next.js App Router API routes declare `export const dynamic = 'force-dynamic';`
    let api_dir = proj_dir.join("src").join("app").join("api");
    if api_dir.is_dir() {
        let mut updated_routes = 0usize;
        fn walk_api(dir: &Path, updated_routes: &mut usize) {
            let Ok(entries) = fs::read_dir(dir) else { return };
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk_api(&p, updated_routes);
                } else if p.is_file() {
                    let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if fname == "route.ts" || fname == "route.js" {
                        if let Ok(c) = fs::read_to_string(&p) {
                            if !c.contains("export const dynamic") {
                                let next = format!("export const dynamic = 'force-dynamic';\n{}", c);
                                if fs::write(&p, next).is_ok() {
                                    *updated_routes += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        walk_api(&api_dir, &mut updated_routes);
        if updated_routes > 0 {
            notes.push(format!(
                "Added force-dynamic to {} API route(s) for Vercel serverless build",
                updated_routes
            ));
        }
    }

    Ok(notes)
}

pub fn build_dns_instructions(domain: &str, vercel_domain_resp: &Value) -> Vec<DnsRecordInstruction> {
    let clean = domain.trim().trim_end_matches('.').to_lowercase();
    let labels: Vec<&str> = clean.split('.').filter(|s| !s.is_empty()).collect();
    let mut records = Vec::new();

    if labels.len() <= 2 {
        // Apex domain (e.g., example.com)
        records.push(DnsRecordInstruction {
            record_type: "A".to_string(),
            name: "@".to_string(),
            value: "76.76.21.21".to_string(),
            purpose: "Point apex domain to Vercel Edge Network".to_string(),
        });
    } else {
        // Subdomain (e.g., app.example.com -> host "app")
        let sub = labels[..labels.len() - 2].join(".");
        records.push(DnsRecordInstruction {
            record_type: "CNAME".to_string(),
            name: sub,
            value: "cname.vercel-dns.com".to_string(),
            purpose: "Point subdomain to Vercel Edge Network".to_string(),
        });
    }

    if let Some(verifs) = vercel_domain_resp.get("verification").and_then(|v| v.as_array()) {
        for v in verifs {
            let rtype = v.get("type").and_then(|x| x.as_str()).unwrap_or("TXT");
            let rname = v.get("domain").and_then(|x| x.as_str()).unwrap_or("_vercel");
            let rval = v.get("value").and_then(|x| x.as_str()).unwrap_or("");
            if !rval.is_empty() {
                records.push(DnsRecordInstruction {
                    record_type: rtype.to_string(),
                    name: rname.to_string(),
                    value: rval.to_string(),
                    purpose: "Vercel domain ownership verification".to_string(),
                });
            }
        }
    }

    records
}

fn generate_nextauth_secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

// ==================== EXTERNAL REST CLIENTS (TESTABLE WITH BASE URLS) ====================

async fn run_git_cmd(proj_dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(args).current_dir(proj_dir);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);
    let out = cmd
        .output()
        .await
        .map_err(|e| format!("Failed executing git {:?}: {}", args, e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Err(if !stderr.is_empty() { stderr } else { stdout })
    }
}

pub async fn neon_provision_database_at(
    client: &reqwest::Client,
    neon_base: &str,
    neon_token: &str,
    project_name: &str,
    region_id: &str,
    existing_neon_project_id: Option<&str>,
) -> Result<(String, String, String), String> {
    // Returns (neon_project_id, pooled_database_url, direct_database_url)
    let base = neon_base.trim_end_matches('/');

    let (neon_project_id, mut pooled_opt, mut direct_opt) = if let Some(existing_id) = existing_neon_project_id {
        (existing_id.to_string(), None, None)
    } else {
        let create_url = format!("{}/projects", base);
        let resp = client
            .post(&create_url)
            .bearer_auth(neon_token)
            .json(&json!({
                "project": {
                    "name": project_name,
                    "region_id": region_id,
                    "pg_version": 16
                }
            }))
            .send()
            .await
            .map_err(|e| format!("Neon API request failed: {}", e))?;

        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("Invalid Neon API JSON ({}): {}", status, e))?;
        if !status.is_success() {
            let msg = body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Failed to create Neon PostgreSQL project");
            return Err(format!("Neon API {}: {}", status, msg));
        }

        let pid = body
            .get("project")
            .and_then(|p| p.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Neon API response missing project.id".to_string())?
            .to_string();

        // Neon create response often already includes connection_uris
        let mut initial_direct = None;
        if let Some(uris) = body.get("connection_uris").and_then(|u| u.as_array()) {
            if let Some(uri) = uris.first().and_then(|u| u.get("connection_uri")).and_then(|s| s.as_str()) {
                initial_direct = Some(uri.to_string());
            }
        }
        (pid, None, initial_direct)
    };

    // Fetch pooled URI (ideal for Vercel serverless runtime)
    let pooled_url = format!("{}/projects/{}/connection_uri?pooled=true", base, neon_project_id);
    if let Ok(r) = client.get(&pooled_url).bearer_auth(neon_token).send().await {
        if r.status().is_success() {
            if let Ok(j) = r.json::<Value>().await {
                if let Some(u) = j.get("uri").and_then(|v| v.as_str()) {
                    pooled_opt = Some(u.to_string());
                }
            }
        }
    }

    // Fetch direct URI (required for prisma db push / migrations)
    if direct_opt.is_none() {
        let direct_url = format!("{}/projects/{}/connection_uri?pooled=false", base, neon_project_id);
        if let Ok(r) = client.get(&direct_url).bearer_auth(neon_token).send().await {
            if r.status().is_success() {
                if let Ok(j) = r.json::<Value>().await {
                    if let Some(u) = j.get("uri").and_then(|v| v.as_str()) {
                        direct_opt = Some(u.to_string());
                    }
                }
            }
        }
    }

    let direct = direct_opt
        .clone()
        .or_else(|| pooled_opt.clone())
        .ok_or_else(|| "Could not retrieve PostgreSQL connection URI from Neon".to_string())?;
    let pooled = pooled_opt.unwrap_or_else(|| direct.clone());

    Ok((neon_project_id, pooled, direct))
}

async fn run_prisma_db_push(proj_dir: &Path, direct_db_url: &str) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = tokio::process::Command::new("cmd");
        c.args(["/C", "npx", "--no-install", "prisma", "db", "push", "--skip-generate", "--accept-data-loss"]);
        c.creation_flags(0x08000000);
        c
    };
    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = tokio::process::Command::new("npx");
        c.args(["--no-install", "prisma", "db", "push", "--skip-generate", "--accept-data-loss"]);
        c
    };

    cmd.current_dir(proj_dir)
        .env("DATABASE_URL", direct_db_url)
        .env("DIRECT_URL", direct_db_url);

    let out = tokio::time::timeout(Duration::from_secs(90), cmd.output())
        .await
        .map_err(|_| "prisma db push timed out after 90s".to_string())?
        .map_err(|e| format!("Failed spawning prisma db push: {}", e))?;

    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        Err(format!(
            "prisma db push failed: {}",
            if !stderr.is_empty() { stderr } else { stdout }
        ))
    }
}

pub async fn vercel_upsert_project_and_envs_at(
    client: &reqwest::Client,
    vercel_base: &str,
    vercel_token: &str,
    project_name: &str,
    github_repo: &str,
    team_id: Option<&str>,
    env_vars: &HashMap<String, String>,
) -> Result<(String, String), String> {
    // Returns (vercel_project_id, vercel_project_name)
    let base = vercel_base.trim_end_matches('/');
    let team_qs = team_id
        .filter(|t| !t.trim().is_empty())
        .map(|t| format!("?teamId={}", t.trim()))
        .unwrap_or_default();

    let create_url = format!("{}/v9/projects{}", base, team_qs);
    let create_resp = client
        .post(&create_url)
        .bearer_auth(vercel_token)
        .json(&json!({
            "name": project_name,
            "framework": "nextjs",
            "gitRepository": {
                "type": "github",
                "repo": github_repo
            }
        }))
        .send()
        .await
        .map_err(|e| format!("Vercel project create request failed: {}", e))?;

    let create_status = create_resp.status();
    let create_body: Value = create_resp.json().await.unwrap_or_else(|_| json!({}));

    let (vp_id, vp_name) = if create_status.is_success() {
        let id = create_body
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(project_name)
            .to_string();
        let name = create_body
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(project_name)
            .to_string();
        (id, name)
    } else if create_status.as_u16() == 409 {
        let get_url = format!("{}/v9/projects/{}{}", base, project_name, team_qs);
        let get_resp = client
            .get(&get_url)
            .bearer_auth(vercel_token)
            .send()
            .await
            .map_err(|e| format!("Vercel get existing project failed: {}", e))?;
        let get_body: Value = get_resp.json().await.unwrap_or_else(|_| json!({}));
        let id = get_body
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(project_name)
            .to_string();
        let name = get_body
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(project_name)
            .to_string();
        (id, name)
    } else {
        let msg = create_body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("Failed to create Vercel project");
        if create_status.as_u16() == 403 {
            return Err(format!(
                "Vercel cannot access GitHub repo '{}'. Ensure 'Vercel for GitHub' integration has access (https://vercel.com/dashboard/integrations/github). Detail: {}",
                github_repo, msg
            ));
        }
        return Err(format!("Vercel API {}: {}", create_status, msg));
    };

    // Upsert all environment variables
    let env_url = if team_qs.is_empty() {
        format!("{}/v10/projects/{}/env?upsert=true", base, vp_id)
    } else {
        format!("{}/v10/projects/{}/env{}&upsert=true", base, vp_id, team_qs)
    };

    for (key, value) in env_vars {
        let env_resp = client
            .post(&env_url)
            .bearer_auth(vercel_token)
            .json(&json!({
                "key": key,
                "value": value,
                "type": "encrypted",
                "target": ["production", "preview", "development"]
            }))
            .send()
            .await
            .map_err(|e| format!("Vercel env upsert failed for {}: {}", key, e))?;
        if !env_resp.status().is_success() {
            let st = env_resp.status();
            let err_body = env_resp.text().await.unwrap_or_default();
            return Err(format!(
                "Vercel env '{}' upsert failed (HTTP {}): {}",
                key,
                st,
                err_body.chars().take(240).collect::<String>()
            ));
        }
    }

    Ok((vp_id, vp_name))
}

pub async fn vercel_attach_domain_at(
    client: &reqwest::Client,
    vercel_base: &str,
    vercel_token: &str,
    vercel_project_id: &str,
    domain: &str,
    team_id: Option<&str>,
) -> Result<(bool, Vec<DnsRecordInstruction>), String> {
    let base = vercel_base.trim_end_matches('/');
    let team_qs = team_id
        .filter(|t| !t.trim().is_empty())
        .map(|t| format!("?teamId={}", t.trim()))
        .unwrap_or_default();

    let url = format!("{}/v10/projects/{}/domains{}", base, vercel_project_id, team_qs);
    let resp = client
        .post(&url)
        .bearer_auth(vercel_token)
        .json(&json!({ "name": domain }))
        .send()
        .await
        .map_err(|e| format!("Vercel attach domain failed: {}", e))?;

    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
    if !status.is_success() && status.as_u16() != 409 {
        let msg = body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("Failed attaching custom domain");
        return Err(format!("Vercel domain API {}: {}", status, msg));
    }

    let verified = body
        .get("verified")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let instructions = build_dns_instructions(domain, &body);
    Ok((verified, instructions))
}

async fn cloudflare_configure_dns_at(
    client: &reqwest::Client,
    cf_base: &str,
    cf_token: &str,
    domain: &str,
    records: &[DnsRecordInstruction],
) -> Result<(), String> {
    let clean = domain.trim().trim_end_matches('.').to_lowercase();
    let parts: Vec<&str> = clean.split('.').collect();
    if parts.len() < 2 {
        return Err(format!("Invalid domain for Cloudflare DNS: {}", domain));
    }
    let root_zone = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
    let base = cf_base.trim_end_matches('/');

    let z_resp = client
        .get(format!("{}/zones?name={}", base, root_zone))
        .bearer_auth(cf_token)
        .send()
        .await
        .map_err(|e| format!("Cloudflare zones lookup failed: {}", e))?;
    let z_body: Value = z_resp
        .json()
        .await
        .map_err(|e| format!("Invalid Cloudflare zones JSON: {}", e))?;
    let zone_id = z_body
        .get("result")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .and_then(|z| z.get("id"))
        .and_then(|id| id.as_str())
        .ok_or_else(|| format!("Cloudflare zone '{}' not found for this token", root_zone))?
        .to_string();

    for rec in records {
        let cf_name = if rec.name == "@" {
            root_zone.clone()
        } else if rec.name.ends_with(&root_zone) {
            rec.name.clone()
        } else {
            format!("{}.{}", rec.name, root_zone)
        };

        let _ = client
            .post(format!("{}/zones/{}/dns_records", base, zone_id))
            .bearer_auth(cf_token)
            .json(&json!({
                "type": rec.record_type,
                "name": cf_name,
                "content": rec.value,
                "ttl": 1,
                "proxied": false
            }))
            .send()
            .await
            .map_err(|e| format!("Cloudflare DNS write failed for {}: {}", cf_name, e))?;
    }
    Ok(())
}

// ==================== TAURI COMMANDS ====================

#[tauri::command]
pub fn publish_get_preflight(project_id: String) -> Result<PublishPreflight, String> {
    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err(format!("Project folder not found for '{}'", project_id));
    }

    let has_github_token = get_token("github-token".to_string()).success;
    let has_vercel_token = get_token("vercel-token".to_string()).success;
    let has_neon_token = get_token("neon-token".to_string()).success;
    let has_cloudflare_token = get_token("cloudflare-token".to_string()).success;
    let has_prisma = proj_dir.join("prisma").join("schema.prisma").is_file();
    let is_expo = crate::templates_expo::is_expo_project_dir(&proj_dir);
    let existing_state = load_publish_state(&proj_dir);

    let mut detected_env_keys: Vec<String> = extract_safe_local_env_vars(&proj_dir)
        .keys()
        .cloned()
        .collect();
    detected_env_keys.sort();

    let pkg_name = fs::read_to_string(proj_dir.join("package.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()));

    let suggested_repo_name = existing_state
        .as_ref()
        .and_then(|s| s.github_repo_name.clone())
        .unwrap_or_else(|| {
            let base = pkg_name.unwrap_or_else(|| project_id.clone());
            sanitize_repo_slug(&base)
        });

    let compliance_summary = crate::compliance::build_compliance_preflight_summary(&proj_dir);
    let cache_summary = if is_expo {
        None
    } else {
        Some(crate::cache_template::build_cache_preflight_summary(&proj_dir))
    };

    Ok(PublishPreflight {
        project_id,
        suggested_repo_name,
        has_github_token,
        has_vercel_token,
        has_neon_token,
        has_cloudflare_token,
        has_prisma,
        is_expo,
        detected_env_keys,
        existing_state,
        compliance_summary,
        cache_summary,
    })
}

#[tauri::command]
pub async fn publish_project_fullstack(
    window: tauri::Window,
    req: PublishRequest,
) -> Result<PublishResult, String> {
    let emit_progress = |step: &str, percent: u8, message: &str| {
        let _ = window.emit(
            "publish:progress",
            json!({
                "projectId": req.project_id,
                "step": step,
                "percent": percent,
                "message": message
            }),
        );
    };

    let proj_dir = get_project_path(&req.project_id);
    if !proj_dir.exists() {
        return Err(format!("Project folder not found for '{}'", req.project_id));
    }

    let gh_token = get_token("github-token".to_string())
        .value
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "GitHub token is missing. Add it in Settings first.".to_string())?;
    let vercel_token = get_token("vercel-token".to_string())
        .value
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "Vercel token is missing. Add it in Settings first.".to_string())?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent("Axiom-Forge-Desktop/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let mut state = load_publish_state(&proj_dir).unwrap_or_default();
    let mut warnings: Vec<String> = Vec::new();
    let has_prisma = proj_dir.join("prisma").join("schema.prisma").is_file();

    // STEP 1: Prepare project for Vercel & Push to GitHub
    emit_progress("prepare", 8, "Preparing .gitignore, Prisma scripts, and API routes for Vercel...");
    let prep_notes = prepare_project_for_vercel_publish(&proj_dir)?;
    for n in prep_notes {
        warnings.push(n);
    }

    emit_progress("github", 15, "Authenticating with GitHub...");
    let gh_user_resp = client
        .get(format!("{}/user", GITHUB_API_BASE))
        .bearer_auth(&gh_token)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub auth request failed: {}", e))?;
    if !gh_user_resp.status().is_success() {
        return Err(format!(
            "GitHub authentication failed (HTTP {}). Check your GitHub token in Settings.",
            gh_user_resp.status()
        ));
    }
    let gh_user: Value = gh_user_resp.json().await.map_err(|e| e.to_string())?;
    let gh_username = gh_user
        .get("login")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Could not read GitHub username".to_string())?
        .to_string();

    let repo_name = req
        .repo_name
        .as_deref()
        .map(sanitize_repo_slug)
        .filter(|s| !s.is_empty())
        .or_else(|| state.github_repo_name.clone())
        .unwrap_or_else(|| sanitize_repo_slug(&req.project_id));

    emit_progress("github", 22, &format!("Ensuring GitHub repository {}/{}...", gh_username, repo_name));
    let create_repo_resp = client
        .post(format!("{}/user/repos", GITHUB_API_BASE))
        .bearer_auth(&gh_token)
        .header("Accept", "application/vnd.github+json")
        .json(&json!({
            "name": repo_name,
            "description": format!("Published via Axiom Forge ({})", req.project_id),
            "private": req.is_private_repo,
            "auto_init": false
        }))
        .send()
        .await
        .map_err(|e| format!("GitHub repo creation failed: {}", e))?;

    let create_repo_status = create_repo_resp.status();
    if !create_repo_status.is_success() && create_repo_status.as_u16() != 422 {
        let body = create_repo_resp.text().await.unwrap_or_default();
        return Err(format!(
            "GitHub repo creation error (HTTP {}): {}",
            create_repo_status,
            body.chars().take(240).collect::<String>()
        ));
    }

    emit_progress("github", 30, "Committing and pushing code to GitHub (main)...");
    if !proj_dir.join(".git").exists() {
        run_git_cmd(&proj_dir, &["init"]).await?;
        let _ = run_git_cmd(&proj_dir, &["branch", "-M", "main"]).await;
    }
    let _ = run_git_cmd(&proj_dir, &["config", "user.email", "axiom-forge@localhost"]).await;
    let _ = run_git_cmd(&proj_dir, &["config", "user.name", "Axiom Forge"]).await;
    // Untrack any accidentally committed env files so secrets never leak to GitHub
    let _ = run_git_cmd(&proj_dir, &["rm", "--cached", "--ignore-unmatch", ".env", ".env.local"]).await;
    run_git_cmd(&proj_dir, &["add", "-A"]).await?;
    let _ = run_git_cmd(&proj_dir, &["commit", "-m", "Axiom Forge — Full-Stack Publish"]).await;

    let clean_remote = format!("https://github.com/{}/{}.git", gh_username, repo_name);
    let auth_remote = format!("https://{}:{}@github.com/{}/{}.git", gh_username, gh_token, gh_username, repo_name);
    if run_git_cmd(&proj_dir, &["remote", "get-url", "origin"]).await.is_ok() {
        let _ = run_git_cmd(&proj_dir, &["remote", "set-url", "origin", &auth_remote]).await;
    } else {
        let _ = run_git_cmd(&proj_dir, &["remote", "add", "origin", &auth_remote]).await;
    }
    let push_res = run_git_cmd(&proj_dir, &["push", "-u", "origin", "main", "--force"]).await;
    // Always scrub token from .git/config immediately after push
    let _ = run_git_cmd(&proj_dir, &["remote", "set-url", "origin", &clean_remote]).await;
    push_res.map_err(|e| format!("GitHub push failed: {}", e))?;

    let github_url = format!("https://github.com/{}/{}", gh_username, repo_name);
    state.github_repo_name = Some(repo_name.clone());
    state.github_repo_url = Some(github_url.clone());
    state.github_username = Some(gh_username.clone());
    save_publish_state(&proj_dir, &state);

    // STEP 2: Provision PostgreSQL Database (Neon API v2 or Custom URL) & Push Prisma Schema
    let mut pooled_db_url: Option<String> = None;
    let mut direct_db_url: Option<String> = None;
    let mut prisma_pushed = false;

    if has_prisma && req.db_mode != "skip" {
        if req.db_mode == "neon" {
            let neon_token = get_token("neon-token".to_string())
                .value
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| "Neon API token is missing. Add it in Settings or switch DB mode to Custom PostgreSQL URL.".to_string())?;
            let region = req
                .neon_region
                .as_deref()
                .filter(|r| !r.trim().is_empty())
                .unwrap_or("aws-eu-central-1");

            emit_progress("database", 42, "Provisioning Neon PostgreSQL database...");
            let (neon_id, pooled, direct) = neon_provision_database_at(
                &client,
                NEON_API_BASE,
                &neon_token,
                &repo_name,
                region,
                state.neon_project_id.as_deref(),
            )
            .await?;
            state.neon_project_id = Some(neon_id);
            state.neon_project_name = Some(repo_name.clone());
            pooled_db_url = Some(pooled);
            direct_db_url = Some(direct);
            save_publish_state(&proj_dir, &state);
        } else if req.db_mode == "custom_url" {
            let custom = req
                .custom_database_url
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| s.starts_with("postgres://") || s.starts_with("postgresql://"))
                .ok_or_else(|| "Invalid custom PostgreSQL URL (must start with postgresql:// or postgres://).".to_string())?;
            pooled_db_url = Some(custom.to_string());
            direct_db_url = Some(custom.to_string());
        }

        if req.run_prisma_push {
            if let Some(ref direct) = direct_db_url {
                emit_progress("prisma", 52, "Syncing Prisma schema to PostgreSQL (prisma db push)...");
                match run_prisma_db_push(&proj_dir, direct).await {
                    Ok(_) => prisma_pushed = true,
                    Err(e) => warnings.push(format!("Prisma db push warning (continuing deploy): {}", e)),
                }
            }
        }
    }

    // STEP 3: Vercel Project + Environment Variables
    emit_progress("vercel", 62, "Configuring Vercel project and environment variables...");
    let github_repo_full = format!("{}/{}", gh_username, repo_name);
    let vercel_proj_name = state
        .vercel_project_name
        .clone()
        .unwrap_or_else(|| repo_name.clone());

    let clean_custom_domain = req
        .custom_domain
        .as_deref()
        .map(|d| d.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_lowercase())
        .filter(|d| !d.is_empty());

    let nextauth_secret = state
        .nextauth_secret
        .clone()
        .unwrap_or_else(generate_nextauth_secret);
    state.nextauth_secret = Some(nextauth_secret.clone());

    let canonical_site_url = if let Some(ref dom) = clean_custom_domain {
        format!("https://{}", dom)
    } else {
        format!("https://{}.vercel.app", vercel_proj_name)
    };

    let mut env_vars: HashMap<String, String> = HashMap::new();
    if req.sync_local_env_keys {
        env_vars.extend(extract_safe_local_env_vars(&proj_dir));
    }
    if let Some(ref db_url) = pooled_db_url {
        env_vars.insert("DATABASE_URL".to_string(), db_url.clone());
    }
    if let Some(ref direct) = direct_db_url {
        env_vars.insert("DIRECT_URL".to_string(), direct.clone());
    }
    env_vars.insert("NEXTAUTH_SECRET".to_string(), nextauth_secret);
    env_vars.insert("NEXTAUTH_URL".to_string(), canonical_site_url);

    // Optional plug-and-play Redis / Upstash cache ENV overrides from Publish Modal
    if let Some(u) = req.upstash_redis_rest_url.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        env_vars.insert("UPSTASH_REDIS_REST_URL".to_string(), u.to_string());
    }
    if let Some(t) = req.upstash_redis_rest_token.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        env_vars.insert("UPSTASH_REDIS_REST_TOKEN".to_string(), t.to_string());
    }
    if let Some(r) = req.redis_url.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        env_vars.insert("REDIS_URL".to_string(), r.to_string());
    }

    let mut env_keys_synced: Vec<String> = env_vars.keys().cloned().collect();
    env_keys_synced.sort();

    let (vp_id, vp_name) = vercel_upsert_project_and_envs_at(
        &client,
        VERCEL_API_BASE,
        &vercel_token,
        &vercel_proj_name,
        &github_repo_full,
        req.vercel_team_id.as_deref(),
        &env_vars,
    )
    .await?;

    state.vercel_project_id = Some(vp_id.clone());
    state.vercel_project_name = Some(vp_name.clone());
    save_publish_state(&proj_dir, &state);

    // STEP 4: Trigger Vercel Production Deployment & Poll Build Status
    emit_progress("deploy", 74, "Triggering Vercel production deployment...");
    let team_qs = req
        .vercel_team_id
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .map(|t| format!("?teamId={}", t.trim()))
        .unwrap_or_default();

    let deploy_resp = client
        .post(format!("{}/v13/deployments{}", VERCEL_API_BASE, team_qs))
        .bearer_auth(&vercel_token)
        .json(&json!({
            "name": vp_name,
            "project": vp_id,
            "target": "production",
            "gitSource": {
                "type": "github",
                "repo": github_repo_full,
                "ref": "main"
            }
        }))
        .send()
        .await
        .map_err(|e| format!("Vercel deployment trigger failed: {}", e))?;

    let dep_status = deploy_resp.status();
    let dep_body: Value = deploy_resp.json().await.unwrap_or_else(|_| json!({}));
    if !dep_status.is_success() {
        let msg = dep_body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("Failed to trigger Vercel deployment");
        return Err(format!("Vercel deployment API {}: {}", dep_status, msg));
    }

    let deployment_id = dep_body
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut live_url = dep_body
        .get("url")
        .and_then(|v| v.as_str())
        .map(|u| if u.starts_with("http") { u.to_string() } else { format!("https://{}", u) });

    let mut ready_state = dep_body
        .get("readyState")
        .and_then(|v| v.as_str())
        .unwrap_or("QUEUED")
        .to_string();

    if !deployment_id.is_empty() {
        for attempt in 0..90u32 {
            if ready_state == "READY" || ready_state == "ERROR" || ready_state == "CANCELED" {
                break;
            }
            let pct = (76 + (attempt.min(16))).min(92) as u8;
            emit_progress("deploy", pct, &format!("Vercel build status: {}...", ready_state));
            tokio::time::sleep(Duration::from_secs(3)).await;

            let poll_url = format!("{}/v13/deployments/{}{}", VERCEL_API_BASE, deployment_id, team_qs);
            if let Ok(r) = client.get(&poll_url).bearer_auth(&vercel_token).send().await {
                if r.status().is_success() {
                    if let Ok(j) = r.json::<Value>().await {
                        if let Some(st) = j.get("readyState").and_then(|v| v.as_str()) {
                            ready_state = st.to_string();
                        }
                        if let Some(u) = j.get("url").and_then(|v| v.as_str()) {
                            live_url = Some(if u.starts_with("http") { u.to_string() } else { format!("https://{}", u) });
                        }
                    }
                }
            }
        }
    }

    if ready_state == "ERROR" || ready_state == "CANCELED" {
        return Err(format!(
            "Vercel production build ended with status '{}'. Inspect deployment logs in Vercel dashboard (deployment id: {}).",
            ready_state, deployment_id
        ));
    }

    let deploy_url = live_url.unwrap_or_else(|| format!("https://{}.vercel.app", vp_name));
    state.deploy_url = Some(deploy_url.clone());

    // STEP 5: Custom Domain & Optional Cloudflare Auto-DNS
    let mut domain_verified = false;
    let mut dns_records: Vec<DnsRecordInstruction> = Vec::new();
    let mut cloudflare_dns_configured = false;
    let mut custom_domain_url: Option<String> = None;

    if let Some(ref dom) = clean_custom_domain {
        emit_progress("domain", 94, &format!("Attaching custom domain {} on Vercel...", dom));
        match vercel_attach_domain_at(
            &client,
            VERCEL_API_BASE,
            &vercel_token,
            &vp_id,
            dom,
            req.vercel_team_id.as_deref(),
        )
        .await
        {
            Ok((verified, recs)) => {
                domain_verified = verified;
                dns_records = recs;
                custom_domain_url = Some(format!("https://{}", dom));
                state.custom_domain = Some(dom.clone());

                if !domain_verified && req.auto_cloudflare_dns {
                    if let Some(cf_token) = get_token("cloudflare-token".to_string())
                        .value
                        .filter(|s| !s.trim().is_empty())
                    {
                        emit_progress("domain", 97, "Configuring DNS records automatically via Cloudflare API...");
                        match cloudflare_configure_dns_at(&client, CLOUDFLARE_API_BASE, &cf_token, dom, &dns_records).await {
                            Ok(_) => {
                                cloudflare_dns_configured = true;
                                // Trigger immediate Vercel verification check
                                let verify_url = format!("{}/v9/projects/{}/domains/{}/verify{}", VERCEL_API_BASE, vp_id, dom, team_qs);
                                if let Ok(vr) = client.post(&verify_url).bearer_auth(&vercel_token).send().await {
                                    if let Ok(vj) = vr.json::<Value>().await {
                                        domain_verified = vj.get("verified").and_then(|v| v.as_bool()).unwrap_or(false);
                                    }
                                }
                            }
                            Err(e) => warnings.push(format!("Cloudflare auto-DNS warning: {}", e)),
                        }
                    } else {
                        warnings.push("Cloudflare auto-DNS was requested, but no cloudflare-token is saved in Settings.".to_string());
                    }
                }
                state.domain_verified = domain_verified;
            }
            Err(e) => warnings.push(format!("Custom domain warning: {}", e)),
        }
    }

    state.updated_at = Some(chrono::Utc::now().to_rfc3339());
    save_publish_state(&proj_dir, &state);
    update_projects_json_metadata(&req.project_id, &state, true);
    emit_progress("complete", 100, "Full-stack publish complete!");

    Ok(PublishResult {
        success: true,
        github_url: Some(github_url),
        deploy_url: Some(deploy_url),
        custom_domain_url,
        neon_project_id: state.neon_project_id,
        vercel_project_name: Some(vp_name),
        prisma_pushed,
        domain_verified,
        cloudflare_dns_configured,
        dns_records,
        env_keys_synced,
        warnings,
        error: None,
    })
}

#[tauri::command]
pub async fn publish_verify_domain(
    project_id: String,
    domain: String,
    vercel_team_id: Option<String>,
) -> Result<DomainVerifyResult, String> {
    let proj_dir = get_project_path(&project_id);
    let mut state = load_publish_state(&proj_dir).ok_or_else(|| {
        "No prior Vercel publish state found for this project. Publish to Vercel first.".to_string()
    })?;
    let vp_id = state
        .vercel_project_id
        .clone()
        .or_else(|| state.vercel_project_name.clone())
        .ok_or_else(|| "Vercel project ID not found in publish state.".to_string())?;
    let vercel_token = get_token("vercel-token".to_string())
        .value
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "Vercel token is missing in Settings.".to_string())?;

    let clean_dom = domain
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_lowercase();

    let team_qs = vercel_team_id
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .map(|t| format!("?teamId={}", t.trim()))
        .unwrap_or_default();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let verify_url = format!(
        "{}/v9/projects/{}/domains/{}/verify{}",
        VERCEL_API_BASE, vp_id, clean_dom, team_qs
    );
    let v_resp = client
        .post(&verify_url)
        .bearer_auth(&vercel_token)
        .send()
        .await
        .map_err(|e| format!("Vercel verify request failed: {}", e))?;
    let v_body: Value = v_resp.json().await.unwrap_or_else(|_| json!({}));

    let cfg_url = format!("{}/v6/domains/{}/config{}", VERCEL_API_BASE, clean_dom, team_qs);
    let cfg_body: Value = match client.get(&cfg_url).bearer_auth(&vercel_token).send().await {
        Ok(r) => r.json().await.unwrap_or_else(|_| json!({})),
        Err(_) => json!({}),
    };

    let verified = v_body.get("verified").and_then(|v| v.as_bool()).unwrap_or(false);
    let misconfigured = cfg_body
        .get("misconfigured")
        .and_then(|v| v.as_bool())
        .unwrap_or(!verified);
    let dns_records = build_dns_instructions(&clean_dom, &v_body);

    state.custom_domain = Some(clean_dom.clone());
    state.domain_verified = verified && !misconfigured;
    save_publish_state(&proj_dir, &state);
    update_projects_json_metadata(&project_id, &state, true);

    let message = if verified && !misconfigured {
        format!("Domain '{}' is verified and SSL is active on Vercel!", clean_dom)
    } else {
        format!(
            "DNS for '{}' is still propagating or needs the records shown below.",
            clean_dom
        )
    };

    Ok(DomainVerifyResult {
        success: true,
        domain: clean_dom,
        verified: verified && !misconfigured,
        misconfigured,
        dns_records,
        message,
    })
}

// ==================== UNIT TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_mock_http(responses: Vec<(u16, String)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let queue = Arc::new(Mutex::new(VecDeque::from(responses)));
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(p) => p,
                    Err(_) => break,
                };
                let next = { queue.lock().unwrap().pop_front() };
                let Some((code, body)) = next else { break };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    let _ = socket.read(&mut buf).await;
                    let reason = if code == 200 || code == 201 { "OK" } else { "Error" };
                    let http = format!(
                        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        code,
                        reason,
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(http.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        format!("http://{}", addr)
    }

    #[test]
    fn test_prepare_project_for_vercel_publish_creates_gitignore_prisma_scripts_and_force_dynamic() {
        let dir = std::env::temp_dir().join(format!("axiom-publish-prep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("prisma")).unwrap();
        fs::create_dir_all(dir.join("src/app/api/invoices")).unwrap();
        fs::write(dir.join("prisma/schema.prisma"), "datasource db { provider = \"postgresql\" url = env(\"DATABASE_URL\") }\n").unwrap();
        fs::write(
            dir.join("package.json"),
            r#"{"name":"demo","scripts":{"build":"next build","dev":"next dev"},"peerDependencies":{"react-query":"^3.0.0"}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("src/app/api/invoices/route.ts"),
            "import { NextResponse } from 'next/server';\nexport async function GET() { return NextResponse.json([]); }\n",
        )
        .unwrap();

        let notes = prepare_project_for_vercel_publish(&dir).unwrap();
        assert!(!notes.is_empty());

        let gi = fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert!(gi.contains("node_modules/") && gi.contains(".env.local") && gi.contains(".next/"));

        let pkg: Value = serde_json::from_str(&fs::read_to_string(dir.join("package.json")).unwrap()).unwrap();
        assert_eq!(pkg["scripts"]["postinstall"].as_str(), Some("prisma generate"));
        assert_eq!(pkg["scripts"]["build"].as_str(), Some("prisma generate && next build"));
        assert!(pkg.get("peerDependencies").is_none());

        let route = fs::read_to_string(dir.join("src/app/api/invoices/route.ts")).unwrap();
        assert!(route.starts_with("export const dynamic = 'force-dynamic';\n"));

        // Idempotent second run produces no extra notes
        let notes2 = prepare_project_for_vercel_publish(&dir).unwrap();
        assert!(notes2.is_empty(), "expected idempotent second run, got: {:?}", notes2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_extract_safe_local_env_vars_and_dns_instructions() {
        let dir = std::env::temp_dir().join(format!("axiom-publish-env-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(".env.local"),
            concat!(
                "DATABASE_URL=postgresql://postgres:pass@localhost:5432/dev\n",
                "NEXTAUTH_SECRET=local-dev-secret\n",
                "OPENAI_API_KEY=sk-live-realkey123\n",
                "STRIPE_SECRET_KEY=your_stripe_key_here\n",
                "RESEND_API_KEY=re_123456789\n"
            ),
        )
        .unwrap();

        let vars = extract_safe_local_env_vars(&dir);
        assert!(!vars.contains_key("DATABASE_URL"));
        assert!(!vars.contains_key("NEXTAUTH_SECRET"));
        assert!(!vars.contains_key("STRIPE_SECRET_KEY"));
        assert_eq!(vars.get("OPENAI_API_KEY").map(String::as_str), Some("sk-live-realkey123"));
        assert_eq!(vars.get("RESEND_API_KEY").map(String::as_str), Some("re_123456789"));

        let apex = build_dns_instructions(
            "clinicflow.io",
            &json!({ "verification": [{ "type": "TXT", "domain": "_vercel.clinicflow.io", "value": "vc-domain-verify=abc" }] }),
        );
        assert_eq!(apex[0].record_type, "A");
        assert_eq!(apex[0].name, "@");
        assert_eq!(apex[0].value, "76.76.21.21");
        assert_eq!(apex[1].record_type, "TXT");

        let sub = build_dns_instructions("app.clinicflow.io", &json!({}));
        assert_eq!(sub[0].record_type, "CNAME");
        assert_eq!(sub[0].name, "app");
        assert_eq!(sub[0].value, "cname.vercel-dns.com");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_neon_and_vercel_rest_orchestration_against_mock_servers() {
        let neon_base = spawn_mock_http(vec![
            (
                201,
                json!({
                    "project": { "id": "neon-proj-123" },
                    "connection_uris": [{ "connection_uri": "postgresql://owner:pw@ep-1.neon.tech/neondb?sslmode=require" }]
                })
                .to_string(),
            ),
            (
                200,
                json!({ "uri": "postgresql://owner:pw@ep-1-pooler.neon.tech/neondb?sslmode=require" }).to_string(),
            ),
        ])
        .await;

        let vercel_base = spawn_mock_http(vec![
            (200, json!({ "id": "prj_abc123", "name": "demo-app" }).to_string()),
            (200, json!({ "created": true }).to_string()),
            (200, json!({ "created": true }).to_string()),
            (
                200,
                json!({
                    "name": "app.demo.io",
                    "verified": false,
                    "verification": [{ "type": "TXT", "domain": "_vercel.demo.io", "value": "vc-verify=xyz" }]
                })
                .to_string(),
            ),
        ])
        .await;

        let client = reqwest::Client::new();
        let (nid, pooled, direct) = neon_provision_database_at(
            &client,
            &neon_base,
            "neon-secret",
            "demo-app",
            "aws-eu-central-1",
            None,
        )
        .await
        .unwrap();
        assert_eq!(nid, "neon-proj-123");
        assert!(pooled.contains("-pooler.neon.tech"));
        assert!(direct.contains("ep-1.neon.tech"));

        let mut envs = HashMap::new();
        envs.insert("DATABASE_URL".to_string(), pooled);
        envs.insert("NEXTAUTH_SECRET".to_string(), "sec".to_string());

        let (vpid, vpname) = vercel_upsert_project_and_envs_at(
            &client,
            &vercel_base,
            "vercel-secret",
            "demo-app",
            "user/demo-app",
            None,
            &envs,
        )
        .await
        .unwrap();
        assert_eq!(vpid, "prj_abc123");
        assert_eq!(vpname, "demo-app");

        let (verified, dns) = vercel_attach_domain_at(
            &client,
            &vercel_base,
            "vercel-secret",
            &vpid,
            "app.demo.io",
            None,
        )
        .await
        .unwrap();
        assert!(!verified);
        assert_eq!(dns[0].record_type, "CNAME");
        assert_eq!(dns[1].record_type, "TXT");
    }
}
