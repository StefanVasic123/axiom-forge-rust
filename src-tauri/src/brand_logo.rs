//! Hybrid Brand SVG Logo Engine (Parametric Vector Engine + Optional Local LLM Brand Brief + Pure-Rust PNG/ICO Export).
//!
//! Architecture:
//! 1. Brand Brief Extraction (`BrandBrief`):
//!    - Instant deterministic analyzer from project name, description, and existing CSS theme colors.
//!    - Optional local Ollama LLM brief generator (`format: "json"`, ~120 tokens — works great even on 7B models)
//!      with strict hex/motif sanitization and deterministic fallback.
//! 2. Parametric Vector SVG Engine:
//!    - 12 precision-crafted geometric motifs on a 128x128 grid (`pulse`, `prism`, `shield`, `spark`, `orbit`,
//!      `grid`, `wave`, `hexagon`, `bolt`, `leaf`, `layers`, `monogram`).
//!    - 4 distinct visual variants per brief (`gradient-badge`, `duotone-emblem`, `monogram-seal`, `minimal-float`).
//!    - Generates `mark_svg` (128x128), `wordmark_dark_svg` (360x96), `wordmark_light_svg` (360x96), `og_image_svg` (1200x630).
//! 3. Pure-Rust Rasterizer (`resvg` + `tiny-skia`):
//!    - Exports anti-aliased PNGs (`apple-touch-icon.png`, `icon-512.png`, `og-image.png`, Expo `icon.png`/`adaptive-icon.png`)
//!      and a genuine multi-resolution PNG-in-ICO `favicon.ico` (16x16, 32x32, 48x48).

use crate::project::{get_project_path, get_projects_file};
use resvg::{tiny_skia, usvg};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

pub const VALID_MOTIFS: &[&str] = &[
    "pulse", "prism", "shield", "spark", "orbit", "grid", "wave", "hexagon", "bolt", "leaf",
    "layers", "monogram",
];

pub const VALID_STYLES: &[&str] = &["modern-tech", "minimal", "geometric", "emblem", "playful"];

pub const VALID_INDUSTRIES: &[&str] = &[
    "fintech",
    "ai",
    "health",
    "developer",
    "commerce",
    "creative",
    "analytics",
    "security",
    "logistics",
    "energy",
    "industrial",
    "general",
];

// ==================== TYPES ====================

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrandBrief {
    pub brand_name: String,
    pub tagline: String,
    pub industry: String,
    pub style: String,
    pub primary_color: String,
    pub secondary_color: String,
    pub accent_color: String,
    pub bg_color: String,
    pub motif: String,
    pub monogram: String,
    pub corner_radius: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LogoVariant {
    pub id: String,
    pub name: String,
    pub description: String,
    pub mark_svg: String,
    pub wordmark_dark_svg: String,
    pub wordmark_light_svg: String,
    pub og_image_svg: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrandLogoState {
    pub brief: BrandBrief,
    pub selected_variant_id: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrandLogoInitResponse {
    pub project_id: String,
    pub project_name: String,
    pub project_description: String,
    pub is_expo: bool,
    pub brief: BrandBrief,
    pub variants: Vec<LogoVariant>,
    pub selected_variant_id: String,
    pub has_existing_logo: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrandLogoGenerateRequest {
    pub project_id: String,
    pub brand_name: Option<String>,
    pub prompt: Option<String>,
    pub use_ai: bool,
    pub model_id: Option<String>,
    pub brief_override: Option<BrandBrief>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrandLogoGenerateResponse {
    pub brief: BrandBrief,
    pub variants: Vec<LogoVariant>,
    pub used_ai: bool,
    pub ai_note: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrandLogoApplyRequest {
    pub project_id: String,
    pub brief: BrandBrief,
    pub variant_id: String,
    #[serde(default = "default_true")]
    pub write_brand_component: bool,
    #[serde(default = "default_true")]
    pub patch_layout_metadata: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrandLogoApplyResponse {
    pub success: bool,
    pub files_written: Vec<String>,
    pub warnings: Vec<String>,
    pub variant_id: String,
}

// ==================== FONT CACHE FOR RESVG ====================

fn cached_fontdb() -> &'static Arc<usvg::fontdb::Database> {
    static FONTDB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTDB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
}

// ==================== SANITIZATION & DETERMINISTIC ANALYZER ====================

fn is_valid_hex(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7 && b[0] == b'#' && b[1..].iter().all(|c| c.is_ascii_hexdigit())
}

fn sanitize_hex(raw: Option<&str>, fallback: &str) -> String {
    if let Some(v) = raw {
        let trimmed = v.trim();
        let candidate = if trimmed.starts_with('#') {
            trimmed.to_string()
        } else {
            format!("#{}", trimmed)
        };
        if is_valid_hex(&candidate) {
            return candidate.to_uppercase();
        }
        // Expand 3-char hex like #0AF
        if candidate.len() == 4
            && candidate.as_bytes()[1..]
                .iter()
                .all(|c| c.is_ascii_hexdigit())
        {
            let chars: Vec<char> = candidate.chars().collect();
            return format!(
                "#{}{}{}{}{}{}",
                chars[1], chars[1], chars[2], chars[2], chars[3], chars[3]
            )
            .to_uppercase();
        }
    }
    fallback.to_uppercase()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn derive_monogram(brand_name: &str) -> String {
    let cleaned: String = brand_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c.is_whitespace() || c == '-' || c == '_' { c } else { ' ' })
        .collect();

    // First split by whitespace / hyphens
    let words: Vec<&str> = cleaned
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .collect();

    if words.len() >= 2 {
        let a = words[0].chars().next().unwrap_or('A').to_ascii_uppercase();
        let b = words[1].chars().next().unwrap_or('X').to_ascii_uppercase();
        return format!("{}{}", a, b);
    }

    if let Some(word) = words.first() {
        // Detect CamelCase like PulseLedger -> PL, ForgeCAD -> FC
        let uppers: Vec<char> = word.chars().filter(|c| c.is_ascii_uppercase()).collect();
        if uppers.len() >= 2 {
            return format!("{}{}", uppers[0], uppers[1]);
        }
        let chars: Vec<char> = word.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        if chars.len() >= 2 {
            return format!(
                "{}{}",
                chars[0].to_ascii_uppercase(),
                chars[1].to_ascii_uppercase()
            );
        } else if chars.len() == 1 {
            return chars[0].to_ascii_uppercase().to_string();
        }
    }

    "AF".to_string()
}

fn clean_display_brand_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "Axiom App".to_string();
    }
    // Strip common test prefixes like e2e-web_1-xxx or slug hyphens
    if !trimmed.contains(' ') && (trimmed.contains('-') || trimmed.contains('_')) {
        let parts: Vec<String> = trimmed
            .split(['-', '_'])
            .filter(|p| !p.is_empty())
            .map(|p| {
                let mut ch = p.chars();
                match ch.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + ch.as_str(),
                    None => String::new(),
                }
            })
            .collect();
        if !parts.is_empty() {
            return parts.join(" ");
        }
    }
    trimmed.to_string()
}

/// Splits text into lowercase whole-word tokens, also splitting CamelCase boundaries
/// (e.g. "GridWatch" -> ["gridwatch", "grid", "watch"], "ForgeCAD" -> ["forgecad", "forge", "cad"],
/// "SCADA" -> ["scada"] — never false-matching "cad" inside "scada" or "sec" inside "seconds").
fn extract_brand_tokens(text: &str) -> std::collections::HashSet<String> {
    let mut tokens = std::collections::HashSet::new();
    for raw_word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        if raw_word.is_empty() {
            continue;
        }
        tokens.insert(raw_word.to_ascii_lowercase());

        // Split CamelCase / acronym transitions: e.g. PulseLedger -> Pulse, Ledger; ForgeCAD -> Forge, CAD
        let chars: Vec<char> = raw_word.chars().collect();
        let mut start = 0usize;
        for i in 1..chars.len() {
            let prev = chars[i - 1];
            let cur = chars[i];
            let next_is_lower = chars.get(i + 1).map_or(false, |c| c.is_ascii_lowercase());
            let split_here = (prev.is_ascii_lowercase() && cur.is_ascii_uppercase())
                || (prev.is_ascii_uppercase() && cur.is_ascii_uppercase() && next_is_lower)
                || (prev.is_ascii_alphabetic() != cur.is_ascii_alphabetic());
            if split_here {
                let part: String = chars[start..i].iter().collect();
                if !part.is_empty() {
                    tokens.insert(part.to_ascii_lowercase());
                }
                start = i;
            }
        }
        if start > 0 && start < chars.len() {
            let tail: String = chars[start..].iter().collect();
            if !tail.is_empty() {
                tokens.insert(tail.to_ascii_lowercase());
            }
        }
    }
    tokens
}

fn has_any_token(tokens: &std::collections::HashSet<String>, words: &[&str]) -> bool {
    words.iter().any(|w| tokens.contains(*w))
}

/// Deterministic brand brief derived from project name, description, and prompt keywords.
pub fn derive_deterministic_brief(name: &str, description: &str, extra_prompt: Option<&str>) -> BrandBrief {
    let brand_name = clean_display_brand_name(name);
    let combined_raw = format!(
        "{} {} {}",
        brand_name,
        description,
        extra_prompt.unwrap_or("")
    );
    let tokens = extract_brand_tokens(&combined_raw);

    let (industry, motif, primary, secondary, accent, bg) = if has_any_token(
        &tokens,
        &[
            "freight", "fleet", "logistics", "shipment", "shipments", "shipping", "cargo",
            "warehouse", "dispatch", "delivery", "supply", "trucking", "courier",
        ],
    ) {
        ("logistics", "layers", "#0EA5E9", "#2563EB", "#38BDF8", "#071426")
    } else if has_any_token(
        &tokens,
        &[
            "scada", "substation", "substations", "power", "voltage", "energy", "electrical",
            "utility", "turbine", "solar", "microgrid", "industrial", "factory", "plant",
        ],
    ) {
        ("energy", "bolt", "#F59E0B", "#EA580C", "#FDE047", "#14100A")
    } else if has_any_token(
        &tokens,
        &[
            "ledger", "finance", "finops", "fintech", "budget", "budgets", "invoice", "invoices",
            "billing", "bank", "banking", "payment", "payments", "wallet", "expense", "expenses",
            "crypto", "trading", "accounting",
        ],
    ) {
        ("fintech", "pulse", "#10B981", "#0EA5E9", "#34D399", "#06151B")
    } else if has_any_token(
        &tokens,
        &[
            "health", "healthcare", "fitness", "fit", "clinic", "medical", "medicine", "patient",
            "patients", "workout", "workouts", "wellness", "doctor", "hospital", "therapy",
        ],
    ) {
        ("health", "pulse", "#06B6D4", "#10B981", "#67E8F9", "#071821")
    } else if has_any_token(
        &tokens,
        &[
            "sound", "audio", "music", "podcast", "beat", "beats", "synth", "daw", "mixer",
            "voice", "acoustic",
        ],
    ) {
        ("creative", "wave", "#8B5CF6", "#EC4899", "#F472B6", "#110B22")
    } else if has_any_token(
        &tokens,
        &[
            "cad", "3d", "mesh", "architect", "architecture", "design", "canvas", "studio",
            "render", "vector", "illustrator", "creative",
        ],
    ) {
        ("creative", "prism", "#F97316", "#6366F1", "#FBBF24", "#12101B")
    } else if has_any_token(
        &tokens,
        &[
            "security", "cyber", "cybersecurity", "auth", "vault", "guard", "shield", "trust",
            "firewall", "compliance", "audit", "identity",
        ],
    ) {
        ("security", "shield", "#3B82F6", "#14B8A6", "#60A5FA", "#081224")
    } else if has_any_token(
        &tokens,
        &[
            "ai", "cortex", "neural", "agent", "agents", "llm", "ml", "copilot", "automation",
            "cognitive", "intelligence",
        ],
    ) {
        ("ai", "orbit", "#6366F1", "#A855F7", "#38BDF8", "#0B0F1F")
    } else if has_any_token(
        &tokens,
        &[
            "eco", "carbon", "green", "sustainability", "sustainable", "climate", "solar",
            "forest", "farm", "agriculture",
        ],
    ) {
        ("general", "leaf", "#22C55E", "#14B8A6", "#86EFAC", "#071A12")
    } else if has_any_token(
        &tokens,
        &[
            "shop", "store", "commerce", "ecommerce", "marketplace", "market", "retail", "cart",
            "checkout", "merchant",
        ],
    ) {
        ("commerce", "layers", "#EC4899", "#F59E0B", "#FB7185", "#170D1C")
    } else if has_any_token(
        &tokens,
        &[
            "analytics", "telemetry", "metric", "metrics", "data", "bi", "chart", "charts",
            "inspection", "inspector", "monitor", "monitoring", "site", "field",
        ],
    ) {
        ("analytics", "grid", "#3B82F6", "#6366F1", "#38BDF8", "#0A1122")
    } else if has_any_token(
        &tokens,
        &[
            "dev", "developer", "code", "cloud", "api", "stack", "devops", "ops", "deploy",
            "infra", "kubernetes", "docker",
        ],
    ) {
        ("developer", "hexagon", "#6366F1", "#06B6D4", "#818CF8", "#0B1020")
    } else {
        ("general", "spark", "#6366F1", "#10B981", "#38BDF8", "#0B1120")
    };

    // Respect explicit color keywords in extra_prompt if user asked for specific hues
    let mut final_primary = primary.to_string();
    let mut final_secondary = secondary.to_string();
    if let Some(p) = extra_prompt {
        let p_tokens = extract_brand_tokens(p);
        if has_any_token(&p_tokens, &["emerald", "green", "zelena", "zeleni", "zeleno"]) {
            final_primary = "#10B981".to_string();
            final_secondary = "#059669".to_string();
        } else if has_any_token(&p_tokens, &["gold", "amber", "zlatna", "zlatni", "orange"]) {
            final_primary = "#F59E0B".to_string();
            final_secondary = "#EA580C".to_string();
        } else if has_any_token(&p_tokens, &["red", "crimson", "rose", "crvena", "crveni"]) {
            final_primary = "#F43F5E".to_string();
            final_secondary = "#E11D48".to_string();
        } else if has_any_token(&p_tokens, &["purple", "violet", "ljubicasta", "ljubicasti"]) {
            final_primary = "#8B5CF6".to_string();
            final_secondary = "#6366F1".to_string();
        } else if has_any_token(&p_tokens, &["blue", "cyan", "plava", "plavi"]) {
            final_primary = "#3B82F6".to_string();
            final_secondary = "#06B6D4".to_string();
        }
    }

    let tagline = description
        .split(['.', '\n', '—', '|'])
        .next()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && s.len() <= 54)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("Modern {} platform", industry));

    BrandBrief {
        monogram: derive_monogram(&brand_name),
        brand_name,
        tagline,
        industry: industry.to_string(),
        style: "modern-tech".to_string(),
        primary_color: final_primary,
        secondary_color: final_secondary,
        accent_color: accent.to_string(),
        bg_color: bg.to_string(),
        motif: motif.to_string(),
        corner_radius: 26,
    }
}

pub fn sanitize_brand_brief(mut raw: BrandBrief, fallback: &BrandBrief) -> BrandBrief {
    raw.brand_name = raw.brand_name.trim().to_string();
    if raw.brand_name.is_empty() {
        raw.brand_name = fallback.brand_name.clone();
    }
    if raw.brand_name.len() > 36 {
        raw.brand_name = raw.brand_name.chars().take(36).collect();
    }

    raw.tagline = raw.tagline.trim().to_string();
    if raw.tagline.is_empty() {
        raw.tagline = fallback.tagline.clone();
    }
    if raw.tagline.len() > 64 {
        raw.tagline = raw.tagline.chars().take(64).collect();
    }

    let ind_low = raw.industry.trim().to_lowercase();
    raw.industry = if VALID_INDUSTRIES.contains(&ind_low.as_str()) {
        ind_low
    } else {
        fallback.industry.clone()
    };

    let style_low = raw.style.trim().to_lowercase();
    raw.style = if VALID_STYLES.contains(&style_low.as_str()) {
        style_low
    } else {
        fallback.style.clone()
    };

    let motif_low = raw.motif.trim().to_lowercase();
    raw.motif = if VALID_MOTIFS.contains(&motif_low.as_str()) {
        motif_low
    } else {
        fallback.motif.clone()
    };

    raw.primary_color = sanitize_hex(Some(&raw.primary_color), &fallback.primary_color);
    raw.secondary_color = sanitize_hex(Some(&raw.secondary_color), &fallback.secondary_color);
    raw.accent_color = sanitize_hex(Some(&raw.accent_color), &fallback.accent_color);
    raw.bg_color = sanitize_hex(Some(&raw.bg_color), &fallback.bg_color);

    let mono: String = raw
        .monogram
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    raw.monogram = if mono.is_empty() {
        derive_monogram(&raw.brand_name)
    } else {
        mono
    };

    raw.corner_radius = raw.corner_radius.min(48);
    raw
}

// ==================== OPTIONAL LOCAL OLLAMA BRIEF GENERATOR ====================

async fn generate_brief_via_ollama(
    base_url: &str,
    model: &str,
    fallback: &BrandBrief,
    user_prompt: &str,
) -> Result<BrandBrief, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let system_prompt = format!(
        "You are a brand identity designer. Return ONLY valid JSON with these exact camelCase keys:\n\
        brandName (string, concise), tagline (string, max 6 words), industry (one of: {}), \
        style (one of: {}), primaryColor (#RRGGBB), secondaryColor (#RRGGBB), accentColor (#RRGGBB), \
        bgColor (#RRGGBB dark background), motif (one of: {}), monogram (1-2 uppercase letters), cornerRadius (integer 8-34).\n\
        Pick harmonious, high-contrast modern SaaS colors that match the brand prompt.",
        VALID_INDUSTRIES.join(", "),
        VALID_STYLES.join(", "),
        VALID_MOTIFS.join(", ")
    );

    let user_msg = format!(
        "Brand Name: {}\nProject Description: {}\nUser Creative Direction: {}\nReturn strictly JSON.",
        fallback.brand_name,
        fallback.tagline,
        if user_prompt.trim().is_empty() {
            "Clean modern high-contrast brand mark"
        } else {
            user_prompt.trim()
        }
    );

    let resp = client
        .post(format!("{}/api/chat", base_url.trim_end_matches('/')))
        .json(&json!({
            "model": model,
            "stream": false,
            "format": "json",
            "options": {
                "temperature": 0.4,
                "num_predict": 220
            },
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_msg }
            ]
        }))
        .send()
        .await
        .map_err(|e| format!("Ollama unreachable: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Ollama returned HTTP {}", resp.status()));
    }

    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    let content = body
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| "Empty response from Ollama".to_string())?;

    // Extract JSON block if wrapped in markdown fences
    let json_slice = if let (Some(start), Some(end)) = (content.find('{'), content.rfind('}')) {
        &content[start..=end]
    } else {
        content
    };

    let parsed: Value = serde_json::from_str(json_slice)
        .map_err(|e| format!("Could not parse AI BrandBrief JSON: {}", e))?;

    let candidate = BrandBrief {
        brand_name: parsed
            .get("brandName")
            .and_then(|v| v.as_str())
            .unwrap_or(&fallback.brand_name)
            .to_string(),
        tagline: parsed
            .get("tagline")
            .and_then(|v| v.as_str())
            .unwrap_or(&fallback.tagline)
            .to_string(),
        industry: parsed
            .get("industry")
            .and_then(|v| v.as_str())
            .unwrap_or(&fallback.industry)
            .to_string(),
        style: parsed
            .get("style")
            .and_then(|v| v.as_str())
            .unwrap_or(&fallback.style)
            .to_string(),
        primary_color: parsed
            .get("primaryColor")
            .and_then(|v| v.as_str())
            .unwrap_or(&fallback.primary_color)
            .to_string(),
        secondary_color: parsed
            .get("secondaryColor")
            .and_then(|v| v.as_str())
            .unwrap_or(&fallback.secondary_color)
            .to_string(),
        accent_color: parsed
            .get("accentColor")
            .and_then(|v| v.as_str())
            .unwrap_or(&fallback.accent_color)
            .to_string(),
        bg_color: parsed
            .get("bgColor")
            .and_then(|v| v.as_str())
            .unwrap_or(&fallback.bg_color)
            .to_string(),
        motif: parsed
            .get("motif")
            .and_then(|v| v.as_str())
            .unwrap_or(&fallback.motif)
            .to_string(),
        monogram: parsed
            .get("monogram")
            .and_then(|v| v.as_str())
            .unwrap_or(&fallback.monogram)
            .to_string(),
        corner_radius: parsed
            .get("cornerRadius")
            .and_then(|v| v.as_u64())
            .map(|n| n as u16)
            .unwrap_or(fallback.corner_radius),
    };

    Ok(sanitize_brand_brief(candidate, fallback))
}

// ==================== PARAMETRIC VECTOR SVG ENGINE ====================

/// Renders one of the 12 curated precision geometric motifs centered in a 128x128 coordinate space.
fn render_motif_geometry(motif: &str, fg: &str, accent: &str, monogram: &str) -> String {
    match motif {
        "pulse" => format!(
            r#"<path d="M26 68 H44 L54 40 L67 88 L79 54 L88 68 H102" fill="none" stroke="{fg}" stroke-width="9" stroke-linecap="round" stroke-linejoin="round"/>
  <circle cx="102" cy="68" r="6" fill="{accent}"/>"#,
            fg = fg,
            accent = accent
        ),
        "prism" => format!(
            r#"<polygon points="64,24 102,94 26,94" fill="none" stroke="{fg}" stroke-width="8" stroke-linejoin="round"/>
  <path d="M64 24 L64 94 M64 60 L102 94 M64 60 L26 94" fill="none" stroke="{accent}" stroke-width="5.5" stroke-linecap="round" stroke-linejoin="round" opacity="0.9"/>"#,
            fg = fg,
            accent = accent
        ),
        "shield" => format!(
            r#"<path d="M64 24 L98 38 V64 C98 85 83 100 64 108 C45 100 30 85 30 64 V38 Z" fill="none" stroke="{fg}" stroke-width="8" stroke-linejoin="round"/>
  <path d="M49 65 L59 75 L80 52" fill="none" stroke="{accent}" stroke-width="8" stroke-linecap="round" stroke-linejoin="round"/>"#,
            fg = fg,
            accent = accent
        ),
        "spark" => format!(
            r#"<path d="M64 22 C67 47 81 61 106 64 C81 67 67 81 64 106 C61 81 47 67 22 64 C47 61 61 47 64 22 Z" fill="{fg}"/>
  <circle cx="94" cy="34" r="6" fill="{accent}"/>"#,
            fg = fg,
            accent = accent
        ),
        "orbit" => format!(
            r#"<circle cx="64" cy="64" r="14" fill="{fg}"/>
  <ellipse cx="64" cy="64" rx="40" ry="18" transform="rotate(-32 64 64)" fill="none" stroke="{fg}" stroke-width="6.5"/>
  <ellipse cx="64" cy="64" rx="40" ry="18" transform="rotate(32 64 64)" fill="none" stroke="{accent}" stroke-width="5.5" opacity="0.9"/>
  <circle cx="95" cy="43" r="6.5" fill="{accent}"/>"#,
            fg = fg,
            accent = accent
        ),
        "grid" => format!(
            r#"<rect x="28" y="28" width="30" height="30" rx="8" fill="{fg}"/>
  <rect x="70" y="28" width="30" height="30" rx="8" fill="{accent}"/>
  <rect x="28" y="70" width="30" height="30" rx="8" fill="{fg}" opacity="0.72"/>
  <rect x="70" y="70" width="30" height="30" rx="8" fill="{fg}"/>"#,
            fg = fg,
            accent = accent
        ),
        "wave" => format!(
            r#"<path d="M26 52 Q45 30 64 52 T102 52" fill="none" stroke="{fg}" stroke-width="9" stroke-linecap="round"/>
  <path d="M26 78 Q45 56 64 78 T102 78" fill="none" stroke="{accent}" stroke-width="9" stroke-linecap="round"/>"#,
            fg = fg,
            accent = accent
        ),
        "hexagon" => format!(
            r#"<polygon points="64,22 100,43 100,85 64,106 28,85 28,43" fill="none" stroke="{fg}" stroke-width="8" stroke-linejoin="round"/>
  <polygon points="64,44 82,54 82,74 64,84 46,74 46,54" fill="{accent}"/>"#,
            fg = fg,
            accent = accent
        ),
        "bolt" => format!(
            r#"<polygon points="72,20 34,68 62,68 54,108 94,58 66,58" fill="{fg}" stroke="{fg}" stroke-width="3" stroke-linejoin="round"/>
  <circle cx="92" cy="30" r="6" fill="{accent}"/>"#,
            fg = fg,
            accent = accent
        ),
        "leaf" => format!(
            r#"<path d="M32 94 C32 48 56 26 98 26 C98 68 76 94 32 94 Z" fill="none" stroke="{fg}" stroke-width="8" stroke-linejoin="round"/>
  <path d="M34 94 C54 74 70 58 88 38" fill="none" stroke="{accent}" stroke-width="7" stroke-linecap="round"/>"#,
            fg = fg,
            accent = accent
        ),
        "layers" => format!(
            r#"<polygon points="64,24 102,44 64,64 26,44" fill="{fg}"/>
  <polyline points="26,64 64,84 102,64" fill="none" stroke="{accent}" stroke-width="8" stroke-linecap="round" stroke-linejoin="round"/>
  <polyline points="26,84 64,104 102,84" fill="none" stroke="{fg}" stroke-width="8" stroke-linecap="round" stroke-linejoin="round" opacity="0.82"/>"#,
            fg = fg,
            accent = accent
        ),
        _ => {
            // "monogram"
            let escaped_mono = xml_escape(monogram);
            format!(
                r#"<text x="64" y="80" text-anchor="middle" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="48" font-weight="800" letter-spacing="-1" fill="{fg}">{mono}</text>
  <circle cx="98" cy="30" r="6" fill="{accent}"/>"#,
                fg = fg,
                accent = accent,
                mono = escaped_mono
            )
        }
    }
}

fn render_mark_for_variant(brief: &BrandBrief, variant_id: &str) -> String {
    let r = brief.corner_radius;
    let p = &brief.primary_color;
    let s = &brief.secondary_color;
    let a = &brief.accent_color;
    let bg = &brief.bg_color;
    let mono = xml_escape(&brief.monogram);

    match variant_id {
        "gradient-badge" => {
            let inner = render_motif_geometry(&brief.motif, "#FFFFFF", a, &brief.monogram);
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" width="128" height="128" role="img" aria-label="{name}">
  <defs>
    <linearGradient id="bgGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="{p}"/>
      <stop offset="100%" stop-color="{s}"/>
    </linearGradient>
    <linearGradient id="glass" x1="0%" y1="0%" x2="0%" y2="100%">
      <stop offset="0%" stop-color="#FFFFFF" stop-opacity="0.28"/>
      <stop offset="100%" stop-color="#FFFFFF" stop-opacity="0.0"/>
    </linearGradient>
  </defs>
  <rect x="6" y="6" width="116" height="116" rx="{r}" fill="url(#bgGrad)"/>
  <rect x="8" y="8" width="112" height="112" rx="{r_in}" fill="none" stroke="url(#glass)" stroke-width="3"/>
  {inner}
</svg>"##,
                name = xml_escape(&brief.brand_name),
                p = p,
                s = s,
                r = r,
                r_in = r.saturating_sub(2),
                inner = inner
            )
        }
        "duotone-emblem" => {
            let inner = render_motif_geometry(&brief.motif, "url(#strokeGrad)", a, &brief.monogram);
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" width="128" height="128" role="img" aria-label="{name}">
  <defs>
    <linearGradient id="strokeGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="{p}"/>
      <stop offset="100%" stop-color="{s}"/>
    </linearGradient>
  </defs>
  <rect x="6" y="6" width="116" height="116" rx="{r}" fill="{bg}" stroke="url(#strokeGrad)" stroke-width="5"/>
  {inner}
</svg>"##,
                name = xml_escape(&brief.brand_name),
                p = p,
                s = s,
                bg = bg,
                r = r,
                inner = inner
            )
        }
        "monogram-seal" => {
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" width="128" height="128" role="img" aria-label="{name}">
  <defs>
    <linearGradient id="sealGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="{p}"/>
      <stop offset="100%" stop-color="{s}"/>
    </linearGradient>
  </defs>
  <polygon points="64,6 114,35 114,93 64,122 14,93 14,35" fill="{bg}" stroke="url(#sealGrad)" stroke-width="6" stroke-linejoin="round"/>
  <polygon points="64,16 105,40 105,88 64,112 23,88 23,40" fill="url(#sealGrad)" opacity="0.14"/>
  <text x="64" y="80" text-anchor="middle" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="46" font-weight="800" letter-spacing="-1" fill="#FFFFFF">{mono}</text>
  <circle cx="64" cy="26" r="5" fill="{a}"/>
</svg>"##,
                name = xml_escape(&brief.brand_name),
                p = p,
                s = s,
                a = a,
                bg = bg,
                mono = mono
            )
        }
        _ => {
            // "minimal-float"
            let inner = render_motif_geometry(&brief.motif, "url(#floatGrad)", a, &brief.monogram);
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" width="128" height="128" role="img" aria-label="{name}">
  <defs>
    <linearGradient id="floatGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="{p}"/>
      <stop offset="100%" stop-color="{s}"/>
    </linearGradient>
  </defs>
  {inner}
</svg>"##,
                name = xml_escape(&brief.brand_name),
                p = p,
                s = s,
                inner = inner
            )
        }
    }
}

fn strip_outer_svg_wrapper(mark_svg: &str) -> String {
    let start = mark_svg.find('>').map(|i| i + 1).unwrap_or(0);
    let end = mark_svg.rfind("</svg>").unwrap_or(mark_svg.len());
    mark_svg[start..end].to_string()
}

fn render_wordmark_svg(brief: &BrandBrief, mark_svg: &str, dark_mode: bool) -> String {
    let inner_mark = strip_outer_svg_wrapper(mark_svg);
    let title_color = if dark_mode { "#F8FAFC" } else { "#0F172A" };
    let sub_color = if dark_mode { "#94A3B8" } else { "#475569" };
    let name_esc = xml_escape(&brief.brand_name);
    let tag_short: String = brief.tagline.chars().take(34).collect();
    let tag_esc = xml_escape(&tag_short);

    let name_chars = brief.brand_name.chars().count();
    let font_size = if name_chars > 24 {
        20
    } else if name_chars > 18 {
        23
    } else if name_chars > 14 {
        27
    } else if name_chars > 11 {
        31
    } else {
        34
    };

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 420 96" width="420" height="96" role="img" aria-label="{name}">
  <g transform="translate(8, 8) scale(0.625)">
    {inner}
  </g>
  <text x="104" y="52" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="{fs}" font-weight="800" letter-spacing="-0.6" fill="{tc}">{name}</text>
  <text x="105" y="75" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="500" fill="{sc}">{tag}</text>
</svg>"##,
        name = name_esc,
        inner = inner_mark,
        fs = font_size,
        tc = title_color,
        sc = sub_color,
        tag = tag_esc
    )
}

/// Splits a long title into 2 balanced lines at word boundaries so it fits cleanly inside the OG card.
fn split_og_title_lines(title: &str) -> (String, Option<String>) {
    let char_len = title.chars().count();
    if char_len <= 18 {
        return (title.to_string(), None);
    }
    let words: Vec<&str> = title.split_whitespace().collect();
    if words.len() < 2 {
        return (title.to_string(), None);
    }
    let mut best_split = 1usize;
    let mut best_diff = usize::MAX;
    for i in 1..words.len() {
        let l1 = words[..i].join(" ");
        let l2 = words[i..].join(" ");
        let diff = l1.chars().count().abs_diff(l2.chars().count());
        if diff < best_diff {
            best_diff = diff;
            best_split = i;
        }
    }
    (words[..best_split].join(" "), Some(words[best_split..].join(" ")))
}

fn render_og_image_svg(brief: &BrandBrief, mark_svg: &str) -> String {
    let inner_mark = strip_outer_svg_wrapper(mark_svg);
    let name_esc = xml_escape(&brief.brand_name);
    let tag_esc = xml_escape(&brief.tagline);
    let ind_upper = brief.industry.to_uppercase();
    let ind_esc = xml_escape(&ind_upper);

    // Dynamic industry pill width so longer labels like LOGISTICS / INDUSTRIAL never crowd the pill
    let pill_w = (ind_upper.chars().count() * 11 + 52).clamp(132, 240);
    let pill_cx = 380 + (pill_w / 2);

    // Dynamic title typography + balanced 2-line wrap so titles NEVER clip past x=1140 (760px available)
    let (line1, line2_opt) = split_og_title_lines(&brief.brand_name);
    let (title_svg, tag_y, bar_y) = if let Some(line2) = line2_opt {
        let max_line = line1.chars().count().max(line2.chars().count());
        let fs = if max_line <= 14 {
            58
        } else if max_line <= 18 {
            50
        } else if max_line <= 24 {
            42
        } else {
            34
        };
        let l1_esc = xml_escape(&line1);
        let l2_esc = xml_escape(&line2);
        let y1 = 296;
        let y2 = y1 + fs + 10;
        let svg_block = format!(
            r##"<text x="380" y="{y1}" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="{fs}" font-weight="800" letter-spacing="-1.2" fill="#F8FAFC">{l1}</text>
  <text x="380" y="{y2}" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="{fs}" font-weight="800" letter-spacing="-1.2" fill="#F8FAFC">{l2}</text>"##,
            y1 = y1,
            y2 = y2,
            fs = fs,
            l1 = l1_esc,
            l2 = l2_esc
        );
        (svg_block, y2 + 52, y2 + 94)
    } else {
        let len = line1.chars().count();
        let fs = if len <= 13 {
            72
        } else if len <= 17 {
            58
        } else if len <= 22 {
            46
        } else {
            ((700.0 / (len as f32 * 0.62)).round() as usize).clamp(28, 44)
        };
        let l1_esc = xml_escape(&line1);
        let svg_block = format!(
            r##"<text x="380" y="314" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="{fs}" font-weight="800" letter-spacing="-1.5" fill="#F8FAFC">{l1}</text>"##,
            fs = fs,
            l1 = l1_esc
        );
        (svg_block, 376, 422)
    };

    let tag_len = brief.tagline.chars().count();
    let tag_fs = if tag_len <= 38 {
        28
    } else if tag_len <= 52 {
        23
    } else {
        19
    };

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 630" width="1200" height="630" role="img" aria-label="{name}">
  <defs>
    <linearGradient id="ogBg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="{bg}"/>
      <stop offset="100%" stop-color="#050814"/>
    </linearGradient>
    <radialGradient id="glowA" cx="18%" cy="22%" r="55%">
      <stop offset="0%" stop-color="{p}" stop-opacity="0.32"/>
      <stop offset="100%" stop-color="{p}" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="glowB" cx="85%" cy="80%" r="50%">
      <stop offset="0%" stop-color="{s}" stop-opacity="0.28"/>
      <stop offset="100%" stop-color="{s}" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="1200" height="630" fill="url(#ogBg)"/>
  <rect width="1200" height="630" fill="url(#glowA)"/>
  <rect width="1200" height="630" fill="url(#glowB)"/>
  <rect x="44" y="44" width="1112" height="542" rx="32" fill="none" stroke="#FFFFFF" stroke-opacity="0.10" stroke-width="2"/>
  <g transform="translate(104, 195) scale(1.85)">
    {inner}
  </g>
  <rect x="380" y="188" width="{pill_w}" height="34" rx="17" fill="{p}" fill-opacity="0.18" stroke="{p}" stroke-opacity="0.45"/>
  <text x="{pill_cx}" y="210" text-anchor="middle" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="700" letter-spacing="1.6" fill="{a}">{ind}</text>
  {title_svg}
  <text x="382" y="{tag_y}" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="{tag_fs}" font-weight="400" fill="#CBD5E1">{tag}</text>
  <rect x="380" y="{bar_y}" width="120" height="6" rx="3" fill="{p}"/>
  <rect x="510" y="{bar_y}" width="48" height="6" rx="3" fill="{s}"/>
</svg>"##,
        name = name_esc,
        tag = tag_esc,
        ind = ind_esc,
        pill_w = pill_w,
        pill_cx = pill_cx,
        title_svg = title_svg,
        tag_y = tag_y,
        tag_fs = tag_fs,
        bar_y = bar_y,
        bg = brief.bg_color,
        p = brief.primary_color,
        s = brief.secondary_color,
        a = brief.accent_color,
        inner = inner_mark
    )
}

pub fn render_logo_variants(brief: &BrandBrief) -> Vec<LogoVariant> {
    let specs = [
        (
            "gradient-badge",
            "Gradient Glass Badge",
            "Modern SaaS squircle with diagonal gradient and glass rim",
        ),
        (
            "duotone-emblem",
            "Dark Duotone Emblem",
            "Deep dark surface with gradient outline and crisp geometric mark",
        ),
        (
            "monogram-seal",
            "Hexagonal Monogram Seal",
            "Bold typographic initials framed in a precision hexagon",
        ),
        (
            "minimal-float",
            "Minimal Floating Mark",
            "Pure background-free vector symbol for clean modern headers",
        ),
    ];

    specs
        .iter()
        .map(|(id, name, desc)| {
            let mark_svg = render_mark_for_variant(brief, id);
            let wordmark_dark_svg = render_wordmark_svg(brief, &mark_svg, true);
            let wordmark_light_svg = render_wordmark_svg(brief, &mark_svg, false);
            let og_image_svg = render_og_image_svg(brief, &mark_svg);
            LogoVariant {
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                mark_svg,
                wordmark_dark_svg,
                wordmark_light_svg,
                og_image_svg,
            }
        })
        .collect()
}

// ==================== PURE-RUST SVG -> PNG & MULTI-SIZE ICO ====================

pub fn rasterize_svg_to_png(svg_str: &str, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut opt = usvg::Options::default();
    opt.fontdb = Arc::clone(cached_fontdb());
    let tree = usvg::Tree::from_str(svg_str, &opt)
        .map_err(|e| format!("SVG parse error: {}", e))?;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| format!("Could not allocate {}x{} pixmap", width, height))?;

    let src_w = tree.size().width().max(1.0);
    let src_h = tree.size().height().max(1.0);
    let sx = width as f32 / src_w;
    let sy = height as f32 / src_h;
    let transform = tiny_skia::Transform::from_scale(sx, sy);

    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap
        .encode_png()
        .map_err(|e| format!("PNG encoding failed: {}", e))
}

/// Packs multiple PNG byte buffers (e.g. 16x16, 32x32, 48x48) into a standard PNG-in-ICO binary.
pub fn build_multi_size_ico(images: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let count = images.len() as u16;
    let mut out = Vec::new();
    // ICONDIR header (6 bytes): reserved=0, type=1 (ICO), count
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());

    let mut offset: u32 = 6 + (16 * images.len() as u32);
    for (dim, png_bytes) in images {
        let len = png_bytes.len() as u32;
        out.push(*dim); // width (0 means 256)
        out.push(*dim); // height
        out.push(0); // palette count
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // color planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&len.to_le_bytes()); // image size
        out.extend_from_slice(&offset.to_le_bytes()); // image offset
        offset += len;
    }

    for (_, png_bytes) in images {
        out.extend_from_slice(png_bytes);
    }
    out
}

// ==================== PROJECT ASSET WRITER & METADATA HELPERS ====================

fn load_brand_logo_state(proj_dir: &Path) -> Option<BrandLogoState> {
    let p = proj_dir.join(".axiom").join("brand-logo.json");
    let raw = fs::read_to_string(p).ok()?;
    serde_json::from_str::<BrandLogoState>(&raw).ok()
}

fn save_brand_logo_state(proj_dir: &Path, state: &BrandLogoState) {
    let dir = proj_dir.join(".axiom");
    let _ = fs::create_dir_all(&dir);
    let p = dir.join("brand-logo.json");
    if let Ok(pretty) = serde_json::to_string_pretty(state) {
        let _ = fs::write(p, pretty);
    }
}

fn read_project_info(project_id: &str, proj_dir: &Path) -> (String, String) {
    let file_path = get_projects_file();
    if let Ok(raw) = fs::read_to_string(&file_path) {
        if let Ok(projects) = serde_json::from_str::<Vec<Value>>(&raw) {
            if let Some(proj) = projects
                .iter()
                .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(project_id))
            {
                let name = proj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(project_id)
                    .to_string();
                let desc = proj
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                return (name, desc);
            }
        }
    }

    if let Ok(pkg_raw) = fs::read_to_string(proj_dir.join("package.json")) {
        if let Ok(pkg) = serde_json::from_str::<Value>(&pkg_raw) {
            let name = pkg
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(project_id)
                .to_string();
            let desc = pkg
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            return (name, desc);
        }
    }

    (project_id.to_string(), String::new())
}

fn update_projects_json_brand_meta(project_id: &str, brief: &BrandBrief) {
    let file_path = get_projects_file();
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
            let meta = obj
                .entry("metadata".to_string())
                .or_insert_with(|| json!({}));
            if let Some(m) = meta.as_object_mut() {
                m.insert("brandPrimaryColor".to_string(), json!(brief.primary_color));
                m.insert("brandMotif".to_string(), json!(brief.motif));
                m.insert(
                    "brandLogoUpdatedAt".to_string(),
                    json!(chrono::Utc::now().to_rfc3339()),
                );
            }
            if let Ok(pretty) = serde_json::to_string_pretty(&projects) {
                let _ = fs::write(&file_path, pretty);
            }
        }
    }
}

fn render_brand_logo_tsx(brief: &BrandBrief) -> String {
    let name_json = serde_json::to_string(&brief.brand_name).unwrap_or_else(|_| "\"Brand\"".to_string());
    format!(
        r#"import React from "react";

export interface BrandLogoProps {{
  size?: number;
  showText?: boolean;
  className?: string;
}}

export function BrandLogo({{ size = 32, showText = true, className = "" }}: BrandLogoProps) {{
  return (
    <span className={{`inline-flex items-center gap-2.5 select-none ${{className}}`}}>
      <img
        src="/logo-mark.svg"
        alt={name}
        width={{size}}
        height={{size}}
        className="shrink-0 object-contain"
      />
      {{showText && (
        <span className="font-bold tracking-tight text-current">
          {brand_plain}
        </span>
      )}}
    </span>
  );
}}

export default BrandLogo;
"#,
        name = name_json,
        brand_plain = brief.brand_name.replace('{', "").replace('}', "")
    )
}

/// Safely injects `icons` and `openGraph` fields into Next.js `export const metadata: Metadata = { ... };`
/// only when not already present. Never breaks syntax.
fn patch_nextjs_layout_metadata(proj_dir: &Path) -> Option<String> {
    let candidates = [
        proj_dir.join("src/app/layout.tsx"),
        proj_dir.join("app/layout.tsx"),
    ];
    let layout_path = candidates.into_iter().find(|p| p.is_file())?;
    let content = fs::read_to_string(&layout_path).ok()?;
    if content.contains("apple-touch-icon.png") && content.contains("og-image.png") {
        return None;
    }

    // Locate `export const metadata` block
    let anchor = content.find("export const metadata")?;
    let brace_open = content[anchor..].find('{').map(|i| anchor + i)?;
    let mut depth = 0usize;
    let mut brace_close = None;
    for (idx, ch) in content[brace_open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    brace_close = Some(brace_open + idx);
                    break;
                }
            }
            _ => {}
        }
    }
    let close_idx = brace_close?;
    let block = &content[brace_open..close_idx];

    let mut additions = String::new();
    if !block.contains("metadataBase:") {
        additions.push_str(
            "\n  metadataBase: new URL(process.env.NEXT_PUBLIC_APP_URL || process.env.NEXTAUTH_URL || \"http://localhost:3000\"),",
        );
    }
    if !block.contains("icons:") {
        additions.push_str(
            "\n  icons: {\n    icon: [{ url: \"/favicon.ico\" }, { url: \"/favicon.svg\", type: \"image/svg+xml\" }],\n    apple: \"/apple-touch-icon.png\",\n  },",
        );
    }
    if !block.contains("openGraph:") {
        additions.push_str(
            "\n  openGraph: {\n    images: [{ url: \"/og-image.png\", width: 1200, height: 630 }],\n  },",
        );
    }
    if additions.is_empty() {
        return None;
    }

    let before = content[..close_idx].trim_end();
    let needs_comma = !before.ends_with(',') && !before.ends_with('{');
    let patched = format!(
        "{}{}{}\n{}",
        before,
        if needs_comma { "," } else { "" },
        additions,
        &content[close_idx..]
    );
    fs::write(&layout_path, patched).ok()?;
    Some(
        layout_path
            .strip_prefix(proj_dir)
            .unwrap_or(&layout_path)
            .to_string_lossy()
            .replace('\\', "/"),
    )
}

pub fn apply_brand_assets_to_dir(
    proj_dir: &Path,
    brief: &BrandBrief,
    variant_id: &str,
    write_brand_component: bool,
    patch_layout_metadata: bool,
) -> Result<(Vec<String>, Vec<String>), String> {
    let variants = render_logo_variants(brief);
    let chosen = variants
        .iter()
        .find(|v| v.id == variant_id)
        .unwrap_or(&variants[0]);

    let is_expo = crate::templates_expo::is_expo_project_dir(proj_dir);
    let mut files_written: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    if is_expo {
        let assets_dir = proj_dir.join("assets");
        fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;

        fs::write(assets_dir.join("logo-mark.svg"), &chosen.mark_svg).map_err(|e| e.to_string())?;
        files_written.push("assets/logo-mark.svg".to_string());

        fs::write(assets_dir.join("logo.svg"), &chosen.wordmark_dark_svg)
            .map_err(|e| e.to_string())?;
        files_written.push("assets/logo.svg".to_string());

        match rasterize_svg_to_png(&chosen.mark_svg, 1024, 1024) {
            Ok(png1024) => {
                fs::write(assets_dir.join("icon.png"), &png1024).map_err(|e| e.to_string())?;
                files_written.push("assets/icon.png".to_string());
                fs::write(assets_dir.join("adaptive-icon.png"), &png1024)
                    .map_err(|e| e.to_string())?;
                files_written.push("assets/adaptive-icon.png".to_string());
            }
            Err(e) => warnings.push(format!("Expo 1024px icon rasterization warning: {}", e)),
        }

        match rasterize_svg_to_png(&chosen.mark_svg, 48, 48) {
            Ok(png48) => {
                fs::write(assets_dir.join("favicon.png"), &png48).map_err(|e| e.to_string())?;
                files_written.push("assets/favicon.png".to_string());
            }
            Err(e) => warnings.push(format!("Expo favicon.png warning: {}", e)),
        }
    } else {
        let public_dir = proj_dir.join("public");
        fs::create_dir_all(&public_dir).map_err(|e| e.to_string())?;

        fs::write(public_dir.join("logo.svg"), &chosen.wordmark_dark_svg)
            .map_err(|e| e.to_string())?;
        files_written.push("public/logo.svg".to_string());

        fs::write(public_dir.join("logo-light.svg"), &chosen.wordmark_light_svg)
            .map_err(|e| e.to_string())?;
        files_written.push("public/logo-light.svg".to_string());

        fs::write(public_dir.join("logo-mark.svg"), &chosen.mark_svg)
            .map_err(|e| e.to_string())?;
        files_written.push("public/logo-mark.svg".to_string());

        fs::write(public_dir.join("favicon.svg"), &chosen.mark_svg).map_err(|e| e.to_string())?;
        files_written.push("public/favicon.svg".to_string());

        fs::write(public_dir.join("og-image.svg"), &chosen.og_image_svg)
            .map_err(|e| e.to_string())?;
        files_written.push("public/og-image.svg".to_string());

        // Multi-size favicon.ico (16x16, 32x32, 48x48)
        let mut ico_entries: Vec<(u8, Vec<u8>)> = Vec::new();
        for dim in [16u8, 32u8, 48u8] {
            match rasterize_svg_to_png(&chosen.mark_svg, dim as u32, dim as u32) {
                Ok(bytes) => ico_entries.push((dim, bytes)),
                Err(e) => warnings.push(format!("favicon {}px raster warning: {}", dim, e)),
            }
        }
        if !ico_entries.is_empty() {
            let ico_bytes = build_multi_size_ico(&ico_entries);
            fs::write(public_dir.join("favicon.ico"), ico_bytes).map_err(|e| e.to_string())?;
            files_written.push("public/favicon.ico".to_string());
        }

        // apple-touch-icon.png (180x180)
        match rasterize_svg_to_png(&chosen.mark_svg, 180, 180) {
            Ok(bytes) => {
                fs::write(public_dir.join("apple-touch-icon.png"), bytes)
                    .map_err(|e| e.to_string())?;
                files_written.push("public/apple-touch-icon.png".to_string());
            }
            Err(e) => warnings.push(format!("apple-touch-icon warning: {}", e)),
        }

        // icon-512.png (512x512)
        match rasterize_svg_to_png(&chosen.mark_svg, 512, 512) {
            Ok(bytes) => {
                fs::write(public_dir.join("icon-512.png"), bytes).map_err(|e| e.to_string())?;
                files_written.push("public/icon-512.png".to_string());
            }
            Err(e) => warnings.push(format!("icon-512 warning: {}", e)),
        }

        // og-image.png (1200x630)
        match rasterize_svg_to_png(&chosen.og_image_svg, 1200, 630) {
            Ok(bytes) => {
                fs::write(public_dir.join("og-image.png"), bytes).map_err(|e| e.to_string())?;
                files_written.push("public/og-image.png".to_string());
            }
            Err(e) => warnings.push(format!("og-image.png warning: {}", e)),
        }

        if write_brand_component {
            let comp_dir = if proj_dir.join("src").is_dir() {
                proj_dir.join("src").join("components")
            } else {
                proj_dir.join("components")
            };
            let _ = fs::create_dir_all(&comp_dir);
            let comp_path = comp_dir.join("BrandLogo.tsx");
            fs::write(&comp_path, render_brand_logo_tsx(brief)).map_err(|e| e.to_string())?;
            files_written.push(
                comp_path
                    .strip_prefix(proj_dir)
                    .unwrap_or(&comp_path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }

        if patch_layout_metadata {
            if let Some(rel) = patch_nextjs_layout_metadata(proj_dir) {
                files_written.push(rel);
            }
        }
    }

    let state = BrandLogoState {
        brief: brief.clone(),
        selected_variant_id: chosen.id.clone(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    save_brand_logo_state(proj_dir, &state);

    Ok((files_written, warnings))
}

/// Fast, non-intrusive default brand asset seeder called at the end of project generation.
/// Only writes static assets (`public/*` or `assets/*`) when no logo exists yet — never touches TS/TSX files.
pub fn ensure_default_brand_assets(proj_dir: &Path, project_name: &str, description: &str) {
    let is_expo = crate::templates_expo::is_expo_project_dir(proj_dir);
    if is_expo {
        if proj_dir.join("assets").join("logo-mark.svg").is_file() {
            return;
        }
    } else if proj_dir.join("public").join("logo-mark.svg").is_file() {
        return;
    }

    let brief = derive_deterministic_brief(project_name, description, None);
    let _ = apply_brand_assets_to_dir(proj_dir, &brief, "gradient-badge", false, false);
}

// ==================== TAURI COMMANDS ====================

#[tauri::command]
pub fn brand_logo_get_init(project_id: String) -> Result<BrandLogoInitResponse, String> {
    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err(format!("Project folder not found for '{}'", project_id));
    }

    let (project_name, project_description) = read_project_info(&project_id, &proj_dir);
    let is_expo = crate::templates_expo::is_expo_project_dir(&proj_dir);
    let existing = load_brand_logo_state(&proj_dir);

    let (brief, selected_variant_id, has_existing_logo) = if let Some(st) = existing {
        (st.brief, st.selected_variant_id, true)
    } else {
        let det = derive_deterministic_brief(&project_name, &project_description, None);
        let exists = if is_expo {
            proj_dir.join("assets").join("logo-mark.svg").is_file()
        } else {
            proj_dir.join("public").join("logo-mark.svg").is_file()
        };
        (det, "gradient-badge".to_string(), exists)
    };

    let variants = render_logo_variants(&brief);

    Ok(BrandLogoInitResponse {
        project_id,
        project_name,
        project_description,
        is_expo,
        brief,
        variants,
        selected_variant_id,
        has_existing_logo,
    })
}

#[tauri::command]
pub async fn brand_logo_generate(
    req: BrandLogoGenerateRequest,
) -> Result<BrandLogoGenerateResponse, String> {
    let proj_dir = get_project_path(&req.project_id);
    if !proj_dir.exists() {
        return Err(format!("Project folder not found for '{}'", req.project_id));
    }

    let (proj_name, proj_desc) = read_project_info(&req.project_id, &proj_dir);
    let effective_name = req
        .brand_name
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&proj_name);

    let base_fallback = derive_deterministic_brief(effective_name, &proj_desc, req.prompt.as_deref());

    // Direct parametric update (user tweaked color / motif / radius / text in Studio)
    if let Some(override_brief) = req.brief_override {
        let sanitized = sanitize_brand_brief(override_brief, &base_fallback);
        let variants = render_logo_variants(&sanitized);
        return Ok(BrandLogoGenerateResponse {
            brief: sanitized,
            variants,
            used_ai: false,
            ai_note: None,
        });
    }

    if req.use_ai {
        let settings = crate::project::get_axiom_settings();
        let host = settings
            .get("ollamaHost")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("http://127.0.0.1:11434")
            .to_string();
        let model = req
            .model_id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.to_string())
            .or_else(|| {
                settings
                    .get("selectedModel")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "qwen2.5-coder:7b".to_string());

        match generate_brief_via_ollama(
            &host,
            &model,
            &base_fallback,
            req.prompt.as_deref().unwrap_or(""),
        )
        .await
        {
            Ok(ai_brief) => {
                let variants = render_logo_variants(&ai_brief);
                return Ok(BrandLogoGenerateResponse {
                    brief: ai_brief,
                    variants,
                    used_ai: true,
                    ai_note: Some(format!("AI Brand Brief generated via {}", model)),
                });
            }
            Err(err) => {
                let variants = render_logo_variants(&base_fallback);
                return Ok(BrandLogoGenerateResponse {
                    brief: base_fallback,
                    variants,
                    used_ai: false,
                    ai_note: Some(format!("Used instant parametric engine (AI fallback: {})", err)),
                });
            }
        }
    }

    let variants = render_logo_variants(&base_fallback);
    Ok(BrandLogoGenerateResponse {
        brief: base_fallback,
        variants,
        used_ai: false,
        ai_note: None,
    })
}

#[tauri::command]
pub fn brand_logo_apply(req: BrandLogoApplyRequest) -> Result<BrandLogoApplyResponse, String> {
    let proj_dir = get_project_path(&req.project_id);
    if !proj_dir.exists() {
        return Err(format!("Project folder not found for '{}'", req.project_id));
    }

    let (proj_name, proj_desc) = read_project_info(&req.project_id, &proj_dir);
    let fallback = derive_deterministic_brief(&proj_name, &proj_desc, None);
    let clean_brief = sanitize_brand_brief(req.brief, &fallback);

    let (files_written, warnings) = apply_brand_assets_to_dir(
        &proj_dir,
        &clean_brief,
        &req.variant_id,
        req.write_brand_component,
        req.patch_layout_metadata,
    )?;

    update_projects_json_brand_meta(&req.project_id, &clean_brief);

    Ok(BrandLogoApplyResponse {
        success: true,
        files_written,
        warnings,
        variant_id: req.variant_id,
    })
}

// ==================== UNIT TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_brief_and_sanitization() {
        let b = derive_deterministic_brief(
            "PulseLedger",
            "Personal finance and budget tracking SaaS.",
            Some("Make it sleek with emerald accents"),
        );
        assert_eq!(b.brand_name, "PulseLedger");
        assert_eq!(b.monogram, "PL");
        assert_eq!(b.industry, "fintech");
        assert_eq!(b.motif, "pulse");
        assert_eq!(b.primary_color, "#10B981");

        let dirty = BrandBrief {
            brand_name: "  ".to_string(),
            tagline: "".to_string(),
            industry: "invalid_domain".to_string(),
            style: "weird".to_string(),
            primary_color: "not-a-color".to_string(),
            secondary_color: "#0af".to_string(),
            accent_color: "#123456".to_string(),
            bg_color: "bad".to_string(),
            motif: "unknown_shape".to_string(),
            monogram: "abc!!!".to_string(),
            corner_radius: 999,
        };
        let clean = sanitize_brand_brief(dirty, &b);
        assert_eq!(clean.brand_name, "PulseLedger");
        assert_eq!(clean.industry, "fintech");
        assert_eq!(clean.motif, "pulse");
        assert_eq!(clean.primary_color, "#10B981");
        assert_eq!(clean.secondary_color, "#00AAFF");
        assert_eq!(clean.monogram, "AB");
        assert_eq!(clean.corner_radius, 48);
    }

    #[test]
    fn test_render_variants_and_rasterize_png_and_ico() {
        let brief = derive_deterministic_brief("ForgeCAD", "3D parametric CAD studio", None);
        let variants = render_logo_variants(&brief);
        assert_eq!(variants.len(), 4);

        for v in &variants {
            assert!(v.mark_svg.contains("<svg") && v.mark_svg.contains("</svg>"));
            assert!(v.wordmark_dark_svg.contains("ForgeCAD"));
            assert!(v.og_image_svg.contains("viewBox=\"0 0 1200 630\""));

            let png64 = rasterize_svg_to_png(&v.mark_svg, 64, 64).expect("PNG rasterize must succeed");
            // Standard PNG signature: 89 50 4E 47 0D 0A 1A 0A
            assert_eq!(&png64[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
            assert!(png64.len() > 200);
        }

        let p16 = rasterize_svg_to_png(&variants[0].mark_svg, 16, 16).unwrap();
        let p32 = rasterize_svg_to_png(&variants[0].mark_svg, 32, 32).unwrap();
        let ico = build_multi_size_ico(&[(16, p16), (32, p32)]);
        // ICO header: 00 00 01 00 02 00
        assert_eq!(&ico[0..6], &[0, 0, 1, 0, 2, 0]);
        assert!(ico.len() > 300);
    }

    #[test]
    fn test_apply_brand_assets_nextjs_and_expo() {
        let tmp = std::env::temp_dir().join(format!("axiom_brand_test_{}", uuid::Uuid::new_v4()));
        let web_dir = tmp.join("web");
        fs::create_dir_all(web_dir.join("src/app")).unwrap();
        fs::write(
            web_dir.join("src/app/layout.tsx"),
            r#"import type { Metadata } from "next";
export const metadata: Metadata = {
  title: "CortexOps",
  description: "AI Operations Platform",
};
export default function RootLayout({ children }: { children: React.ReactNode }) {
  return <html><body>{children}</body></html>;
}
"#,
        )
        .unwrap();

        let brief = derive_deterministic_brief("CortexOps", "AI Operations Platform", None);
        let (written, warnings) =
            apply_brand_assets_to_dir(&web_dir, &brief, "gradient-badge", true, true).unwrap();
        assert!(warnings.is_empty(), "Unexpected warnings: {:?}", warnings);
        assert!(web_dir.join("public/logo.svg").is_file());
        assert!(web_dir.join("public/favicon.ico").is_file());
        assert!(web_dir.join("public/apple-touch-icon.png").is_file());
        assert!(web_dir.join("public/og-image.png").is_file());
        assert!(web_dir.join("src/components/BrandLogo.tsx").is_file());
        assert!(written.iter().any(|f| f == "src/app/layout.tsx"));

        let layout_after = fs::read_to_string(web_dir.join("src/app/layout.tsx")).unwrap();
        assert!(layout_after.contains("apple-touch-icon.png"));
        assert!(layout_after.contains("og-image.png"));

        // Expo project export
        let expo_dir = tmp.join("expo");
        fs::create_dir_all(&expo_dir).unwrap();
        fs::write(
            expo_dir.join("package.json"),
            r#"{"name":"pulsefit","dependencies":{"expo":"~52.0.0","react-native":"0.76.0"}}"#,
        )
        .unwrap();
        let expo_brief = derive_deterministic_brief("PulseFit", "Mobile workout tracker", None);
        let (_expo_written, expo_warnings) =
            apply_brand_assets_to_dir(&expo_dir, &expo_brief, "duotone-emblem", false, false)
                .unwrap();
        assert!(expo_warnings.is_empty());
        assert!(expo_dir.join("assets/icon.png").is_file());
        assert!(expo_dir.join("assets/adaptive-icon.png").is_file());
        assert!(expo_dir.join("assets/favicon.png").is_file());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_word_boundary_industry_and_long_og_title_wrapping() {
        // "scada" must NOT match "cad" -> should classify as energy/bolt, NOT creative/prism
        let grid = derive_deterministic_brief(
            "GridWatch Substation OS",
            "Native Electron SCADA console for electrical substation monitoring.",
            None,
        );
        assert_eq!(grid.industry, "energy");
        assert_eq!(grid.motif, "bolt");

        // "seconds" must NOT match "sec" -> should classify as logistics/layers, NOT security/shield
        let freight = derive_deterministic_brief(
            "FreightGrid Fleet Logistics",
            "Real-time fleet maintenance and shipment tracking updated every 5 seconds.",
            None,
        );
        assert_eq!(freight.industry, "logistics");
        assert_eq!(freight.motif, "layers");

        // CamelCase "ForgeCAD" still detects "cad" -> creative/prism
        let cad = derive_deterministic_brief("ForgeCAD", "3D parametric modeling studio", None);
        assert_eq!(cad.industry, "creative");
        assert_eq!(cad.motif, "prism");

        // Long titles wrap cleanly into 2 lines inside OG image and never use clipped font-size="72"
        let vars = render_logo_variants(&freight);
        let og = &vars[0].og_image_svg;
        assert!(og.contains("FreightGrid Fleet") && og.contains("Logistics"));
        assert!(!og.contains("font-size=\"72\""), "Long title must not use 72px font:\n{}", og);
        // Must rasterize cleanly to 1200x630 PNG
        let png = rasterize_svg_to_png(og, 1200, 630).expect("wrapped OG PNG must rasterize");
        assert_eq!(&png[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }
}
