//! Hybrid Legal & Compliance Guard (`manifest.compliance` + Conservative Word-Boundary Fallback).
//!
//! Design goals:
//! - Web manifest `compliance` object is the authoritative source of truth.
//! - Conservative word-boundary detector acts ONLY as fallback when a field is omitted
//!   (and NEVER overrides explicit `false` flags from `manifest.compliance`).
//! - Renders 100% deterministic, zero-dependency, SSR-safe ("use client" + mounted guard)
//!   React/Tailwind components (`AgeGateModal.tsx`, `CookieConsentBanner.tsx`,
//!   `ComplianceDisclaimerBar.tsx`) so local 7B models never hallucinate broken compliance UI.
//! - Non-regulated standard apps remain completely untouched (zero regressions).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub const VALID_DISCLAIMERS: &[&str] = &["medical", "financial", "ai", "gambling", "alcohol"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgeGateConfig {
    pub enabled: bool,
    pub min_age: u8,
    /// "confirm" | "dob"
    pub mode: String,
    pub reason: String,
}

impl Default for AgeGateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_age: 18,
            mode: "confirm".to_string(),
            reason: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceConfig {
    pub age_gate: AgeGateConfig,
    pub cookie_consent: bool,
    pub register_consent_checkbox: bool,
    pub disclaimers: Vec<String>,
    pub jurisdiction: String,
    pub legal_pages: Vec<String>,
    pub contact_email: Option<String>,
    /// "manifest" | "auto_detected" | "none"
    pub source: String,
}

impl ComplianceConfig {
    pub fn has_any_active(&self) -> bool {
        self.age_gate.enabled
            || self.cookie_consent
            || self.register_consent_checkbox
            || !self.disclaimers.is_empty()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CompliancePreflightSummary {
    pub active: bool,
    pub source: String,
    pub age_gate_enabled: bool,
    pub age_gate_min_age: u8,
    pub cookie_consent: bool,
    pub register_consent_checkbox: bool,
    pub disclaimers: Vec<String>,
    pub has_privacy_page: bool,
    pub has_terms_page: bool,
    pub notices: Vec<String>,
}

// ==================== WORD-BOUNDARY TOKENIZER ====================

fn extract_tokens(text: &str) -> HashSet<String> {
    let mut tokens = HashSet::new();
    for raw in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        if raw.is_empty() {
            continue;
        }
        tokens.insert(raw.to_ascii_lowercase());
        let chars: Vec<char> = raw.chars().collect();
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

fn has_any(tokens: &HashSet<String>, words: &[&str]) -> bool {
    words.iter().any(|w| tokens.contains(*w))
}

// ==================== HYBRID RESOLVER ====================

/// Resolves `ComplianceConfig` from `manifest.compliance` (authoritative) with conservative
/// word-boundary fallback for regulated verticals when omitted.
pub fn resolve_compliance_config(
    manifest: &Value,
    project_name: &str,
    description: &str,
) -> ComplianceConfig {
    let m_comp = manifest
        .get("compliance")
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("compliance")))
        .and_then(|v| v.as_object());

    let tokens = extract_tokens(&format!("{} {}", project_name, description));

    let jurisdiction = m_comp
        .and_then(|o| o.get("jurisdiction"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "GLOBAL".to_string());

    // 1. Age Gate resolution (supports boolean, integer age, or full object in manifest.compliance.ageGate)
    let mut age_gate = AgeGateConfig::default();
    let explicit_age_gate = m_comp.and_then(|o| o.get("ageGate"));
    if let Some(ag_val) = explicit_age_gate {
        if let Some(b) = ag_val.as_bool() {
            age_gate.enabled = b;
            age_gate.min_age = if jurisdiction == "US" { 21 } else { 18 };
            if b {
                age_gate.reason = "Age-restricted content".to_string();
            }
        } else if let Some(n) = ag_val.as_u64() {
            age_gate.enabled = n >= 13;
            age_gate.min_age = (n as u8).clamp(13, 25);
            age_gate.reason = "Age-restricted content".to_string();
        } else if let Some(obj) = ag_val.as_object() {
            age_gate.enabled = obj.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            age_gate.min_age = obj
                .get("minAge")
                .and_then(|v| v.as_u64())
                .map(|n| (n as u8).clamp(13, 25))
                .unwrap_or(18);
            let mode_raw = obj
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("confirm")
                .to_lowercase();
            age_gate.mode = if mode_raw == "dob" {
                "dob".to_string()
            } else {
                "confirm".to_string()
            };
            age_gate.reason = obj
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("Age-restricted platform")
                .to_string();
        }
    } else {
        // Conservative word-boundary auto-detection ONLY for unambiguously age-restricted industries
        let is_gambling = has_any(
            &tokens,
            &["casino", "betting", "sportsbook", "gambling", "poker", "slots", "wagering"],
        );
        let is_alcohol = has_any(
            &tokens,
            &[
                "alcohol", "brewery", "winery", "wineries", "liquor", "spirits", "wine", "beer",
                "whiskey", "vodka", "cocktail", "cocktails", "distillery",
            ],
        );
        let is_cannabis_tobacco = has_any(
            &tokens,
            &["cannabis", "dispensary", "weed", "cbd", "tobacco", "vape", "vaping", "nicotine"],
        );
        let is_adult_dating = has_any(&tokens, &["adult", "nsfw", "escort", "dating"]);

        if is_gambling || is_alcohol || is_cannabis_tobacco || is_adult_dating {
            age_gate.enabled = true;
            age_gate.min_age = if jurisdiction == "US" && (is_alcohol || is_cannabis_tobacco) {
                21
            } else {
                18
            };
            age_gate.mode = "confirm".to_string();
            age_gate.reason = if is_gambling {
                "Responsible gaming & age-restricted wagering".to_string()
            } else if is_alcohol {
                "Age-restricted alcoholic beverages".to_string()
            } else if is_cannabis_tobacco {
                "Age-restricted cannabis or tobacco products".to_string()
            } else {
                "Age-restricted 18+ community".to_string()
            };
        }
    }

    // 2. Disclaimers resolution
    let mut disclaimers: Vec<String> = Vec::new();
    if let Some(arr) = m_comp
        .and_then(|o| o.get("disclaimers"))
        .and_then(|v| v.as_array())
    {
        for item in arr {
            if let Some(s) = item.as_str() {
                let norm = s.trim().to_lowercase();
                if VALID_DISCLAIMERS.contains(&norm.as_str()) && !disclaimers.contains(&norm) {
                    disclaimers.push(norm);
                }
            }
        }
    } else {
        if has_any(
            &tokens,
            &[
                "clinic", "medical", "medicine", "patient", "patients", "doctor", "hospital",
                "diagnosis", "symptom", "symptoms", "prescription", "telehealth", "telemedicine",
                "therapy", "psychiatry", "pharmacy", "clinical", "hipaa",
            ],
        ) {
            disclaimers.push("medical".to_string());
        }
        if has_any(
            &tokens,
            &[
                "trading", "brokerage", "forex", "crypto", "cryptocurrency", "defi", "derivatives",
                "investing", "investment", "lending", "loan", "loans", "mortgage", "wealth",
                "roboadvisor",
            ],
        ) {
            disclaimers.push("financial".to_string());
        }
        if has_any(
            &tokens,
            &["casino", "betting", "sportsbook", "gambling", "poker", "wagering"],
        ) {
            disclaimers.push("gambling".to_string());
        }
        if has_any(
            &tokens,
            &["alcohol", "brewery", "winery", "liquor", "spirits", "distillery"],
        ) {
            disclaimers.push("alcohol".to_string());
        }
    }

    // 3. Cookie Consent (explicit in manifest OR EU/GDPR jurisdiction)
    let cookie_consent = if let Some(b) = m_comp
        .and_then(|o| o.get("cookieConsent"))
        .and_then(|v| v.as_bool())
    {
        b
    } else {
        jurisdiction == "EU" || jurisdiction == "EEA" || jurisdiction == "UK"
    };

    // 4. Register Consent Checkbox
    let register_consent_checkbox = if let Some(b) = m_comp
        .and_then(|o| o.get("registerConsentCheckbox"))
        .and_then(|v| v.as_bool())
    {
        b
    } else {
        age_gate.enabled || cookie_consent || !disclaimers.is_empty()
    };

    // 5. Legal Pages & Contact Email
    let legal_pages: Vec<String> = m_comp
        .and_then(|o| o.get("legalPages"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_lowercase()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let contact_email = m_comp
        .and_then(|o| o.get("contactEmail"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let mut cfg = ComplianceConfig {
        age_gate,
        cookie_consent,
        register_consent_checkbox,
        disclaimers,
        jurisdiction,
        legal_pages,
        contact_email,
        source: "none".to_string(),
    };
    cfg.source = if m_comp.is_some() {
        "manifest".to_string()
    } else if cfg.has_any_active() {
        "auto_detected".to_string()
    } else {
        "none".to_string()
    };

    cfg
}

pub fn save_compliance_config(proj_dir: &Path, cfg: &ComplianceConfig) {
    let dir = proj_dir.join(".axiom");
    let _ = fs::create_dir_all(&dir);
    let p = dir.join("compliance.json");
    if let Ok(pretty) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(p, pretty);
    }
}

pub fn load_compliance_config(proj_dir: &Path) -> Option<ComplianceConfig> {
    let p = proj_dir.join(".axiom").join("compliance.json");
    let raw = fs::read_to_string(p).ok()?;
    serde_json::from_str::<ComplianceConfig>(&raw).ok()
}

pub fn resolve_and_save_compliance(
    proj_dir: &Path,
    manifest: &Value,
    project_name: &str,
    description: &str,
) -> ComplianceConfig {
    let cfg = resolve_compliance_config(manifest, project_name, description);
    if cfg.has_any_active() || cfg.source == "manifest" {
        save_compliance_config(proj_dir, &cfg);
    }
    cfg
}

// ==================== DETERMINISTIC UI COMPONENT TEMPLATES ====================

pub fn render_age_gate_modal_tsx(cfg: &AgeGateConfig) -> String {
    let min_age = cfg.min_age.max(13);
    let is_dob = cfg.mode == "dob";
    let reason_json = serde_json::to_string(if cfg.reason.trim().is_empty() {
        "This platform contains age-restricted content."
    } else {
        cfg.reason.trim()
    })
    .unwrap_or_else(|_| "\"This platform contains age-restricted content.\"".to_string());

    if is_dob {
        format!(
            r##""use client";

import React, {{ useEffect, useState }} from "react";

const STORAGE_KEY = "axiom_age_verified_{min_age}";

export function AgeGateModal() {{
  const [mounted, setMounted] = useState(false);
  const [verified, setVerified] = useState(true);
  const [dob, setDob] = useState("");
  const [error, setError] = useState("");
  const [blocked, setBlocked] = useState(false);

  useEffect(() => {{
    setMounted(true);
    try {{
      const saved = window.localStorage.getItem(STORAGE_KEY);
      if (saved === "true") {{
        setVerified(true);
      }} else {{
        setVerified(false);
      }}
    }} catch {{
      setVerified(false);
    }}
  }}, []);

  if (!mounted || verified) return null;

  const handleVerify = (e: React.FormEvent) => {{
    e.preventDefault();
    setError("");
    if (!dob) {{
      setError("Please enter your date of birth.");
      return;
    }}
    const birth = new Date(dob);
    if (Number.isNaN(birth.getTime())) {{
      setError("Please enter a valid date.");
      return;
    }}
    const today = new Date();
    let age = today.getFullYear() - birth.getFullYear();
    const m = today.getMonth() - birth.getMonth();
    if (m < 0 || (m === 0 && today.getDate() < birth.getDate())) {{
      age -= 1;
    }}
    if (age < {min_age}) {{
      setBlocked(true);
      setError("You must be at least {min_age} years old to access this platform.");
      return;
    }}
    try {{
      window.localStorage.setItem(STORAGE_KEY, "true");
    }} catch {{}}
    setVerified(true);
  }};

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="age-gate-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/90 backdrop-blur-md p-4"
    >
      <div className="w-full max-w-md rounded-2xl border border-slate-700 bg-slate-900 p-6 text-slate-100 shadow-2xl space-y-5">
        <div className="space-y-2 text-center">
          <span className="inline-block rounded-full bg-amber-500/15 border border-amber-500/30 px-3 py-1 text-xs font-semibold text-amber-300">
            {min_age}+ Age Verification Required
          </span>
          <h2 id="age-gate-title" className="text-xl font-bold text-white">
            Confirm Your Date of Birth
          </h2>
          <p className="text-sm text-slate-400">
            {{{reason}}}
          </p>
        </div>

        {{error && (
          <div className="rounded-xl border border-rose-500/30 bg-rose-500/10 px-4 py-3 text-sm text-rose-300">
            {{error}}
          </div>
        )}}

        {{!blocked ? (
          <form onSubmit={{handleVerify}} className="space-y-4">
            <div>
              <label htmlFor="age-gate-dob" className="block text-xs font-medium text-slate-300 mb-1.5">
                Date of Birth
              </label>
              <input
                id="age-gate-dob"
                type="date"
                required
                value={{dob}}
                onChange={{(e) => setDob(e.target.value)}}
                className="w-full rounded-xl border border-slate-700 bg-slate-950 px-3.5 py-2.5 text-sm text-white focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
            </div>
            <button
              type="submit"
              className="w-full rounded-xl bg-indigo-600 px-4 py-2.5 text-sm font-semibold text-white hover:bg-indigo-500 transition"
            >
              Verify Age ({min_age}+)
            </button>
          </form>
        ) : (
          <p className="text-center text-xs text-slate-400">
            Access has been restricted based on the date of birth entered.
          </p>
        )}}
      </div>
    </div>
  );
}}

export default AgeGateModal;
"##,
            min_age = min_age,
            reason = reason_json
        )
    } else {
        format!(
            r##""use client";

import React, {{ useEffect, useState }} from "react";

const STORAGE_KEY = "axiom_age_verified_{min_age}";

export function AgeGateModal() {{
  const [mounted, setMounted] = useState(false);
  const [verified, setVerified] = useState(true);
  const [declined, setDeclined] = useState(false);

  useEffect(() => {{
    setMounted(true);
    try {{
      const saved = window.localStorage.getItem(STORAGE_KEY);
      setVerified(saved === "true");
    }} catch {{
      setVerified(false);
    }}
  }}, []);

  if (!mounted || verified) return null;

  const handleConfirm = () => {{
    try {{
      window.localStorage.setItem(STORAGE_KEY, "true");
    }} catch {{}}
    setVerified(true);
  }};

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="age-gate-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/90 backdrop-blur-md p-4"
    >
      <div className="w-full max-w-md rounded-2xl border border-slate-700 bg-slate-900 p-6 text-slate-100 shadow-2xl space-y-5">
        <div className="space-y-2 text-center">
          <span className="inline-block rounded-full bg-amber-500/15 border border-amber-500/30 px-3 py-1 text-xs font-semibold text-amber-300">
            {min_age}+ Age Verification Required
          </span>
          <h2 id="age-gate-title" className="text-xl font-bold text-white">
            Are you at least {min_age} years old?
          </h2>
          <p className="text-sm text-slate-400">
            {{{reason}}}
          </p>
        </div>

        {{declined ? (
          <div className="rounded-xl border border-rose-500/30 bg-rose-500/10 px-4 py-3 text-center text-sm text-rose-300">
            You must be {min_age} or older to enter this site.
          </div>
        ) : (
          <div className="flex flex-col sm:flex-row gap-3">
            <button
              type="button"
              onClick={{handleConfirm}}
              className="flex-1 rounded-xl bg-indigo-600 px-4 py-2.5 text-sm font-semibold text-white hover:bg-indigo-500 transition"
            >
              Yes, I am {min_age}+
            </button>
            <button
              type="button"
              onClick={{() => setDeclined(true)}}
              className="flex-1 rounded-xl border border-slate-700 bg-slate-800 px-4 py-2.5 text-sm font-medium text-slate-300 hover:bg-slate-700 transition"
            >
              No, I am under {min_age}
            </button>
          </div>
        )}}
      </div>
    </div>
  );
}}

export default AgeGateModal;
"##,
            min_age = min_age,
            reason = reason_json
        )
    }
}

pub fn render_cookie_consent_banner_tsx() -> String {
    r##""use client";

import React, { useEffect, useState } from "react";
import Link from "next/link";

const STORAGE_KEY = "axiom_cookie_consent";

export function CookieConsentBanner() {
  const [mounted, setMounted] = useState(false);
  const [dismissed, setDismissed] = useState(true);

  useEffect(() => {
    setMounted(true);
    try {
      const saved = window.localStorage.getItem(STORAGE_KEY);
      setDismissed(Boolean(saved));
    } catch {
      setDismissed(false);
    }
  }, []);

  if (!mounted || dismissed) return null;

  const choose = (level: "all" | "essential") => {
    try {
      window.localStorage.setItem(STORAGE_KEY, level);
    } catch {}
    setDismissed(true);
  };

  return (
    <div
      role="region"
      aria-label="Cookie consent"
      className="fixed bottom-4 left-4 right-4 z-40 mx-auto max-w-3xl rounded-2xl border border-slate-700/80 bg-slate-900/95 p-4 text-slate-200 shadow-2xl backdrop-blur-md"
    >
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <p className="text-xs leading-relaxed text-slate-300">
          We use essential cookies to keep this service secure and functional, and optional analytics cookies to improve your experience. See our{" "}
          <Link href="/legal/privacy" prefetch={false} className="underline text-indigo-400 hover:text-indigo-300">
            Privacy Policy
          </Link>
          .
        </p>
        <div className="flex items-center gap-2 shrink-0">
          <button
            type="button"
            onClick={() => choose("essential")}
            className="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1.5 text-xs font-medium text-slate-200 hover:bg-slate-700 transition"
          >
            Essential Only
          </button>
          <button
            type="button"
            onClick={() => choose("all")}
            className="rounded-lg bg-indigo-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-indigo-500 transition"
          >
            Accept All
          </button>
        </div>
      </div>
    </div>
  );
}

export default CookieConsentBanner;
"##
    .to_string()
}

pub fn render_compliance_disclaimer_bar_tsx(cfg: &ComplianceConfig) -> String {
    let min_age = cfg.age_gate.min_age.max(18);
    let mut texts: Vec<String> = Vec::new();
    for d in &cfg.disclaimers {
        match d.as_str() {
            "medical" => texts.push("Medical Disclaimer: Information on this platform is for informational purposes only and is not a substitute for professional medical advice, diagnosis, or treatment.".to_string()),
            "financial" => texts.push("Financial Disclaimer: Content and tools on this platform are for informational purposes only and do not constitute financial, investment, or tax advice.".to_string()),
            "ai" => texts.push("AI Disclosure: Certain outputs or recommendations on this platform are generated by artificial intelligence and should be independently verified.".to_string()),
            "gambling" => texts.push(format!("Responsible Gaming ({}+): Please participate responsibly. Seek support if gaming becomes a concern.", min_age)),
            "alcohol" => texts.push(format!("Age-Restricted Notice ({}+): Intended strictly for adults of legal drinking age. Please enjoy responsibly.", min_age)),
            _ => {}
        }
    }
    let items_json = serde_json::to_string(&texts).unwrap_or_else(|_| "[]".to_string());

    format!(
        r##""use client";

import React from "react";

const DISCLAIMERS: string[] = {items_json};

export function ComplianceDisclaimerBar() {{
  if (DISCLAIMERS.length === 0) return null;
  return (
    <aside
      aria-label="Legal disclaimers"
      className="w-full border-t border-slate-800 bg-slate-950/90 px-4 py-3 text-slate-400"
    >
      <div className="mx-auto max-w-6xl space-y-1 text-center text-xs leading-relaxed">
        {{DISCLAIMERS.map((text, idx) => (
          <p key={{idx}}>{{text}}</p>
        ))}}
      </div>
    </aside>
  );
}}

export default ComplianceDisclaimerBar;
"##,
        items_json = items_json
    )
}

// ==================== PRE-PUBLISH COMPLIANCE SUMMARY ====================

pub fn build_compliance_preflight_summary(proj_dir: &Path) -> Option<CompliancePreflightSummary> {
    let cfg = load_compliance_config(proj_dir)?;
    if !cfg.has_any_active() {
        return None;
    }

    let has_privacy_page = proj_dir.join("src/app/legal/privacy/page.tsx").is_file()
        || proj_dir.join("app/legal/privacy/page.tsx").is_file()
        || proj_dir.join("src/app/privacy/page.tsx").is_file();
    let has_terms_page = proj_dir.join("src/app/legal/terms/page.tsx").is_file()
        || proj_dir.join("app/legal/terms/page.tsx").is_file()
        || proj_dir.join("src/app/terms/page.tsx").is_file();

    let mut notices: Vec<String> = Vec::new();
    if cfg.age_gate.enabled {
        notices.push(format!(
            "{}+ Age Gate modal is active ({})",
            cfg.age_gate.min_age, cfg.age_gate.mode
        ));
    }
    if cfg.cookie_consent {
        notices.push("GDPR Cookie Consent banner is active".to_string());
    }
    if !cfg.disclaimers.is_empty() {
        notices.push(format!(
            "Disclaimers active: {}",
            cfg.disclaimers.join(", ")
        ));
    }
    if !has_privacy_page || !has_terms_page {
        notices.push(
            "Review legal pages before production launch (/legal/privacy & /legal/terms)".to_string(),
        );
    }

    Some(CompliancePreflightSummary {
        active: true,
        source: cfg.source,
        age_gate_enabled: cfg.age_gate.enabled,
        age_gate_min_age: cfg.age_gate.min_age,
        cookie_consent: cfg.cookie_consent,
        register_consent_checkbox: cfg.register_consent_checkbox,
        disclaimers: cfg.disclaimers,
        has_privacy_page,
        has_terms_page,
        notices,
    })
}

// ==================== UNIT TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_manifest_compliance_is_authoritative_and_never_overridden() {
        // Explicit manifest.compliance with ageGate: false must stay false even if text mentions "wine"
        let manifest = json!({
            "name": "Wine Cellar Inventory",
            "compliance": {
                "ageGate": { "enabled": false, "minAge": 18, "mode": "confirm" },
                "cookieConsent": true,
                "disclaimers": ["ai"],
                "jurisdiction": "EU"
            }
        });
        let cfg = resolve_compliance_config(&manifest, "Wine Cellar", "Manage private wine notes");
        assert_eq!(cfg.source, "manifest");
        assert!(!cfg.age_gate.enabled, "explicit ageGate.enabled=false must be respected");
        assert!(cfg.cookie_consent);
        assert_eq!(cfg.disclaimers, vec!["ai".to_string()]);
        assert!(cfg.register_consent_checkbox);

        // Explicit DOB 21+ age gate from manifest
        let m2 = json!({
            "compliance": {
                "ageGate": { "enabled": true, "minAge": 21, "mode": "dob" },
                "disclaimers": ["alcohol"]
            }
        });
        let cfg2 = resolve_compliance_config(&m2, "CraftBrew", "Direct-to-consumer brewery");
        assert!(cfg2.age_gate.enabled);
        assert_eq!(cfg2.age_gate.min_age, 21);
        assert_eq!(cfg2.age_gate.mode, "dob");
        let dob_tsx = render_age_gate_modal_tsx(&cfg2.age_gate);
        assert!(dob_tsx.contains("type=\"date\""));
        assert!(dob_tsx.contains("21+ Age Verification Required"));
    }

    #[test]
    fn test_conservative_fallback_avoids_false_positives_and_catches_regulated_domains() {
        // Standard non-regulated app must stay completely clean (zero regressions)
        let plain_manifest = json!({ "name": "TaskBoard" });
        let plain = resolve_compliance_config(
            &plain_manifest,
            "TaskBoard Sidebar Studio",
            "Simple kanban board with a collapsible sidebar and fast search.",
        );
        assert!(!plain.has_any_active(), "sidebar must NOT trigger alcohol/bar age gate: {:?}", plain);
        assert_eq!(plain.source, "none");

        // Casino / betting triggers 18+ age gate + gambling disclaimer
        let casino = resolve_compliance_config(
            &json!({}),
            "RoyalSlots",
            "Online casino and sportsbook wagering platform.",
        );
        assert_eq!(casino.source, "auto_detected");
        assert!(casino.age_gate.enabled);
        assert_eq!(casino.age_gate.min_age, 18);
        assert!(casino.disclaimers.contains(&"gambling".to_string()));
        assert!(casino.register_consent_checkbox);

        // Medical clinic triggers medical disclaimer (no age gate)
        let clinic = resolve_compliance_config(
            &json!({}),
            "MedFlow Clinic",
            "Patient appointment scheduler and telehealth symptom intake.",
        );
        assert!(!clinic.age_gate.enabled);
        assert!(clinic.disclaimers.contains(&"medical".to_string()));
    }

    #[test]
    fn test_render_compliance_components_and_preflight_summary() {
        let tmp = std::env::temp_dir().join(format!("axiom_comp_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(tmp.join("src/app/legal/privacy")).unwrap();
        fs::write(tmp.join("src/app/legal/privacy/page.tsx"), "export default function P(){return null;}").unwrap();

        let manifest = json!({
            "compliance": {
                "ageGate": true,
                "cookieConsent": true,
                "disclaimers": ["financial", "ai"]
            }
        });
        let cfg = resolve_and_save_compliance(&tmp, &manifest, "AlphaTrade", "Crypto trading platform");
        assert!(cfg.age_gate.enabled);

        let modal_tsx = render_age_gate_modal_tsx(&cfg.age_gate);
        assert!(modal_tsx.contains("\"use client\""));
        assert!(modal_tsx.contains("Yes, I am 18+"));

        let cookie_tsx = render_cookie_consent_banner_tsx();
        assert!(cookie_tsx.contains("Essential Only"));
        assert!(cookie_tsx.contains("Accept All"));

        let bar_tsx = render_compliance_disclaimer_bar_tsx(&cfg);
        assert!(bar_tsx.contains("Financial Disclaimer"));
        assert!(bar_tsx.contains("AI Disclosure"));

        let summary = build_compliance_preflight_summary(&tmp).expect("summary should exist");
        assert!(summary.age_gate_enabled);
        assert!(summary.cookie_consent);
        assert!(summary.has_privacy_page);
        assert!(!summary.has_terms_page);

        let _ = fs::remove_dir_all(&tmp);
    }
}
