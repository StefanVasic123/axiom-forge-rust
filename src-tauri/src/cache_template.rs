//! Zero-Config Cache & Rate-Limit Adapter Generator (`src/lib/cache.ts`).
//!
//! Enables plug-and-play Redis/Upstash caching via environment variables with ZERO code changes:
//!   1. Upstash Redis REST (`UPSTASH_REDIS_REST_URL` + `UPSTASH_REDIS_REST_TOKEN`, or Vercel KV aliases
//!      `KV_REST_API_URL` + `KV_REST_API_TOKEN`) — pure `fetch()`, 0 npm dependencies, Edge & Serverless safe.
//!   2. Standard TCP Redis (`REDIS_URL`) — lazy dynamic `ioredis` loader (no build-time dependency; graceful
//!      fallback to memory if `ioredis` is not installed or unreachable).
//!   3. Automatic In-Memory LRU + TTL fallback — works out-of-the-box locally, during `next build`, and
//!      in production before Redis credentials are added.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CachePreflightSummary {
    pub has_cache_module: bool,
    /// "upstash" | "redis" | "memory"
    pub detected_provider: String,
    pub has_upstash_env: bool,
    pub has_redis_url_env: bool,
}

pub fn render_cache_ts() -> String {
    r##"/**
 * Zero-Config Cache & Rate-Limit Adapter (Upstash REST -> TCP Redis -> In-Memory LRU Fallback).
 *
 * Switch from local In-Memory cache to production Redis with ZERO code changes by setting
 * environment variables in `.env.local` or your Vercel Project Settings:
 *   - Upstash / Vercel KV (Recommended for Serverless & Edge — 0 npm packages):
 *       UPSTASH_REDIS_REST_URL="https://..."
 *       UPSTASH_REDIS_REST_TOKEN="..."
 *   - Standard TCP Redis (Redis Cloud / Railway / Docker — optional `ioredis` package):
 *       REDIS_URL="redis://default:password@host:6379"
 */

export type CacheProvider = "upstash" | "redis" | "memory";

export interface RateLimitResult {
  allowed: boolean;
  remaining: number;
  resetAt: number;
  provider: CacheProvider;
}

interface MemoryEntry {
  value: string;
  expiresAt: number;
}

const MAX_MEMORY_ENTRIES = 500;

const globalForCache = globalThis as unknown as {
  __axiomMemoryCache?: Map<string, MemoryEntry>;
  __axiomRedisClient?: any;
  __axiomRedisWarned?: boolean;
};

function getMemoryStore(): Map<string, MemoryEntry> {
  if (!globalForCache.__axiomMemoryCache) {
    globalForCache.__axiomMemoryCache = new Map<string, MemoryEntry>();
  }
  return globalForCache.__axiomMemoryCache;
}

function getUpstashConfig(): { url: string; token: string } | null {
  const url = (
    process.env.UPSTASH_REDIS_REST_URL ||
    process.env.KV_REST_API_URL ||
    ""
  ).trim();
  const token = (
    process.env.UPSTASH_REDIS_REST_TOKEN ||
    process.env.KV_REST_API_TOKEN ||
    ""
  ).trim();
  if (!url || !token) return null;
  return { url: url.replace(/\/+$/, ""), token };
}

function getRedisUrl(): string | null {
  const url = (process.env.REDIS_URL || "").trim();
  if (!url) return null;
  if (!url.startsWith("redis://") && !url.startsWith("rediss://")) return null;
  return url;
}

export function detectCacheProvider(): CacheProvider {
  if (getUpstashConfig()) return "upstash";
  if (getRedisUrl()) return "redis";
  return "memory";
}

async function upstashCommand<T = unknown>(args: (string | number)[]): Promise<T | null> {
  const cfg = getUpstashConfig();
  if (!cfg) return null;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 2500);
  try {
    const res = await fetch(cfg.url, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${cfg.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(args),
      cache: "no-store",
      signal: controller.signal,
    });
    if (!res.ok) return null;
    const data = (await res.json()) as { result?: T };
    return data?.result ?? null;
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

async function getTcpRedisClient(): Promise<any | null> {
  const url = getRedisUrl();
  if (!url) return null;
  if (globalForCache.__axiomRedisClient) {
    return globalForCache.__axiomRedisClient;
  }
  try {
    // Dynamic runtime-only import so builds succeed without `ioredis` installed
    const pkgName = "ioredis";
    const mod = await import(/* webpackIgnore: true */ pkgName);
    const RedisCtor = mod?.default ?? mod;
    if (typeof RedisCtor !== "function") return null;
    const client = new RedisCtor(url, {
      maxRetriesPerRequest: 1,
      connectTimeout: 2500,
      lazyConnect: true,
    });
    globalForCache.__axiomRedisClient = client;
    return client;
  } catch {
    if (!globalForCache.__axiomRedisWarned) {
      globalForCache.__axiomRedisWarned = true;
      console.warn(
        "[cache] REDIS_URL is set, but `ioredis` is not installed. Falling back to in-memory cache. Install `ioredis` or set UPSTASH_REDIS_REST_URL + UPSTASH_REDIS_REST_TOKEN."
      );
    }
    return null;
  }
}

function memoryGet(key: string): string | null {
  const store = getMemoryStore();
  const entry = store.get(key);
  if (!entry) return null;
  if (entry.expiresAt > 0 && Date.now() > entry.expiresAt) {
    store.delete(key);
    return null;
  }
  // Refresh LRU order
  store.delete(key);
  store.set(key, entry);
  return entry.value;
}

function memorySet(key: string, serialized: string, ttlSeconds: number): void {
  const store = getMemoryStore();
  if (store.size >= MAX_MEMORY_ENTRIES) {
    const oldestKey = store.keys().next().value;
    if (oldestKey !== undefined) store.delete(oldestKey);
  }
  const expiresAt = ttlSeconds > 0 ? Date.now() + ttlSeconds * 1000 : 0;
  store.set(key, { value: serialized, expiresAt });
}

function memoryDel(key: string): void {
  getMemoryStore().delete(key);
}

async function cacheGet<T = unknown>(key: string): Promise<T | null> {
  let raw: string | null = null;
  if (getUpstashConfig()) {
    const res = await upstashCommand<string | null>(["GET", key]);
    if (typeof res === "string") raw = res;
  } else if (getRedisUrl()) {
    const client = await getTcpRedisClient();
    if (client) {
      try {
        raw = await client.get(key);
      } catch {
        raw = memoryGet(key);
      }
    } else {
      raw = memoryGet(key);
    }
  } else {
    raw = memoryGet(key);
  }

  if (raw === null || raw === undefined) return null;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return raw as unknown as T;
  }
}

async function cacheSet<T = unknown>(key: string, value: T, ttlSeconds = 60): Promise<void> {
  const serialized = JSON.stringify(value);
  const safeTtl = Math.max(1, Math.floor(ttlSeconds));

  if (getUpstashConfig()) {
    const ok = await upstashCommand(["SET", key, serialized, "EX", safeTtl]);
    if (ok !== null) return;
  } else if (getRedisUrl()) {
    const client = await getTcpRedisClient();
    if (client) {
      try {
        await client.set(key, serialized, "EX", safeTtl);
        return;
      } catch {
        // Graceful fallback below
      }
    }
  }
  memorySet(key, serialized, safeTtl);
}

export const cache = {
  provider(): CacheProvider {
    return detectCacheProvider();
  },

  get: cacheGet,
  set: cacheSet,

  async del(key: string): Promise<void> {
    if (getUpstashConfig()) {
      await upstashCommand(["DEL", key]);
    } else if (getRedisUrl()) {
      const client = await getTcpRedisClient();
      if (client) {
        try {
          await client.del(key);
        } catch {}
      }
    }
    memoryDel(key);
  },

  async delMany(keys: string[]): Promise<void> {
    const clean = keys.filter(Boolean);
    if (clean.length === 0) return;
    if (getUpstashConfig()) {
      await upstashCommand(["DEL", ...clean]);
    } else if (getRedisUrl()) {
      const client = await getTcpRedisClient();
      if (client) {
        try {
          await client.del(...clean);
        } catch {}
      }
    }
    for (const k of clean) memoryDel(k);
  },

  async getOrSet<T>(
    key: string,
    ttlSeconds: number,
    fetcher: () => Promise<T>
  ): Promise<T> {
    const cached = await cacheGet<T>(key);
    if (cached !== null) return cached;
    const fresh = await fetcher();
    await cacheSet(key, fresh, ttlSeconds);
    return fresh;
  },

  async rateLimit(
    identifier: string,
    limit = 60,
    windowSeconds = 60
  ): Promise<RateLimitResult> {
    const safeWindow = Math.max(1, Math.floor(windowSeconds));
    const bucket = Math.floor(Date.now() / (safeWindow * 1000));
    const key = `rl:${identifier}:${bucket}`;
    const resetAt = (bucket + 1) * safeWindow * 1000;

    if (getUpstashConfig()) {
      const count = await upstashCommand<number>(["INCR", key]);
      if (typeof count === "number") {
        if (count === 1) {
          await upstashCommand(["EXPIRE", key, safeWindow]);
        }
        return {
          allowed: count <= limit,
          remaining: Math.max(0, limit - count),
          resetAt,
          provider: "upstash",
        };
      }
    } else if (getRedisUrl()) {
      const client = await getTcpRedisClient();
      if (client) {
        try {
          const count = Number(await client.incr(key));
          if (count === 1) {
            await client.expire(key, safeWindow);
          }
          return {
            allowed: count <= limit,
            remaining: Math.max(0, limit - count),
            resetAt,
            provider: "redis",
          };
        } catch {
          // Graceful fallback to memory below
        }
      }
    }

    const prevRaw = memoryGet(key);
    const nextCount = (prevRaw ? Number(prevRaw) || 0 : 0) + 1;
    memorySet(key, String(nextCount), safeTtlWindow(safeWindow));
    return {
      allowed: nextCount <= limit,
      remaining: Math.max(0, limit - nextCount),
      resetAt,
      provider: "memory",
    };
  },
};

function safeTtlWindow(sec: number): number {
  return Math.max(1, Math.floor(sec));
}

export default cache;
"##
    .to_string()
}

const ENV_EXAMPLE_CACHE_BLOCK: &str = "\n# Optional Production Cache & Rate Limiting (automatic in-memory fallback if left empty)\n# Recommended for Vercel Serverless & Edge (0 extra npm packages):\nUPSTASH_REDIS_REST_URL=\"\"\nUPSTASH_REDIS_REST_TOKEN=\"\"\n# Or standard TCP Redis (install `ioredis` if using TCP):\n# REDIS_URL=\"redis://default:password@host:6379\"\n";

/// Ensures `src/lib/cache.ts` and optional Redis/Upstash `.env.example` documentation exist
/// in Next.js / Electron projects. Never overwrites existing user cache code and skips Expo.
pub fn ensure_cache_infrastructure(proj_dir: &Path) {
    if crate::templates_expo::is_expo_project_dir(proj_dir) {
        return;
    }

    let cache_path = if proj_dir.join("src").is_dir() {
        proj_dir.join("src").join("lib").join("cache.ts")
    } else {
        proj_dir.join("lib").join("cache.ts")
    };

    if !cache_path.is_file() {
        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&cache_path, render_cache_ts());
    }

    let env_example = proj_dir.join(".env.example");
    let existing = fs::read_to_string(&env_example).unwrap_or_default();
    if !existing.contains("UPSTASH_REDIS_REST_URL") && !existing.contains("REDIS_URL") {
        let updated = if existing.is_empty() {
            ENV_EXAMPLE_CACHE_BLOCK.trim_start().to_string()
        } else if existing.ends_with('\n') {
            format!("{}{}", existing, ENV_EXAMPLE_CACHE_BLOCK)
        } else {
            format!("{}\n{}", existing, ENV_EXAMPLE_CACHE_BLOCK)
        };
        let _ = fs::write(&env_example, updated);
    }
}

pub fn build_cache_preflight_summary(proj_dir: &Path) -> CachePreflightSummary {
    let has_cache_module = proj_dir.join("src/lib/cache.ts").is_file()
        || proj_dir.join("lib/cache.ts").is_file()
        || proj_dir.join("src/lib/redis.ts").is_file();

    let safe_vars = crate::publish::extract_safe_local_env_vars(proj_dir);
    let has_upstash_env = (safe_vars.contains_key("UPSTASH_REDIS_REST_URL")
        && safe_vars.contains_key("UPSTASH_REDIS_REST_TOKEN"))
        || (safe_vars.contains_key("KV_REST_API_URL")
            && safe_vars.contains_key("KV_REST_API_TOKEN"));
    let has_redis_url_env = safe_vars.contains_key("REDIS_URL");

    let detected_provider = if has_upstash_env {
        "upstash".to_string()
    } else if has_redis_url_env {
        "redis".to_string()
    } else {
        "memory".to_string()
    };

    CachePreflightSummary {
        has_cache_module,
        detected_provider,
        has_upstash_env,
        has_redis_url_env,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_cache_ts_contains_all_drivers_and_exports() {
        let code = render_cache_ts();
        assert!(code.contains("UPSTASH_REDIS_REST_URL"));
        assert!(code.contains("KV_REST_API_URL"));
        assert!(code.contains("REDIS_URL"));
        assert!(code.contains("__axiomMemoryCache"));
        assert!(code.contains("export const cache ="));
        assert!(code.contains("export default cache;"));
        assert!(code.contains("async getOrSet<T>"));
        assert!(code.contains("async rateLimit("));
    }

    #[test]
    fn test_ensure_cache_infrastructure_idempotent_and_preflight_detection() {
        let tmp = std::env::temp_dir().join(format!("axiom_cache_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(tmp.join("src/app")).unwrap();
        fs::write(tmp.join(".env.example"), "DATABASE_URL=\"\"\n").unwrap();

        ensure_cache_infrastructure(&tmp);
        assert!(tmp.join("src/lib/cache.ts").is_file());
        let env_ex = fs::read_to_string(tmp.join(".env.example")).unwrap();
        assert!(env_ex.contains("UPSTASH_REDIS_REST_URL"));
        assert!(env_ex.contains("REDIS_URL"));

        // Idempotent second call must not duplicate block
        ensure_cache_infrastructure(&tmp);
        let env_ex_2 = fs::read_to_string(tmp.join(".env.example")).unwrap();
        assert_eq!(env_ex_2.matches("UPSTASH_REDIS_REST_URL").count(), 1);

        // Preflight defaults to memory when no Redis keys are configured
        let s1 = build_cache_preflight_summary(&tmp);
        assert!(s1.has_cache_module);
        assert_eq!(s1.detected_provider, "memory");

        // Adding Upstash keys in .env.local switches preflight to upstash
        fs::write(
            tmp.join(".env.local"),
            "UPSTASH_REDIS_REST_URL=\"https://demo-123.upstash.io\"\nUPSTASH_REDIS_REST_TOKEN=\"AX_live_secret_token\"\n",
        )
        .unwrap();
        let s2 = build_cache_preflight_summary(&tmp);
        assert_eq!(s2.detected_provider, "upstash");
        assert!(s2.has_upstash_env);

        // Expo projects are skipped
        let expo_dir = tmp.join("expo");
        fs::create_dir_all(&expo_dir).unwrap();
        fs::write(
            expo_dir.join("package.json"),
            r#"{"name":"mob","dependencies":{"expo":"~52.0.0","react-native":"0.76.0"}}"#,
        )
        .unwrap();
        ensure_cache_infrastructure(&expo_dir);
        assert!(!expo_dir.join("src/lib/cache.ts").is_file());
        assert!(!expo_dir.join("lib/cache.ts").is_file());

        let _ = fs::remove_dir_all(&tmp);
    }
}
