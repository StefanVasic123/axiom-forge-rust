import os from 'os';

// ─────────────────────────────────────────────────────────────
// MODEL CATALOG
// Curated list of code-focused Ollama-compatible models.
// Sources: Ollama library, unsloth.ai, VRAM benchmarks (May 2026)
// 
// Tiers:
//   nano  → < 4 GB RAM  (very old/basic machines, netbooks)
//   small → 4–8 GB RAM  (entry-level gaming PCs, old laptops)
//   mid   → 8–16 GB RAM (mainstream dev machines, M1/M2 Macs)
//   power → 16–24 GB   (M2 Pro/Max, RTX 3080/4070 workstations)
//   expert→ 24–40 GB   (RTX 3090/4090, M3 Max, dual-GPU rigs)
//   extreme→ 40 GB+    (multi-GPU servers, A100/H100)
// ─────────────────────────────────────────────────────────────
export class HardwareManager {
  static AVAILABLE_MODELS = [

    // ── NANO TIER (< 4 GB) ─────────────────────────────────
    {
      id: 'qwen2.5-coder:1.5b',
      name: 'Qwen 2.5 Coder 1.5B',
      tier: 'nano',
      sizeGB: 1.1,
      minRamGB: 2,
      tags: ['code', 'fast'],
      strengths: 'Ultra-fast responses, React/CSS snippets',
      weakness: 'No complex logic, poor for multi-file projects',
      description: 'Fastest model available. Perfect for auto-completing UI snippets on any machine.',
    },
    {
      id: 'llama3.2:1b',
      name: 'Llama 3.2 1B',
      tier: 'nano',
      sizeGB: 1.3,
      minRamGB: 2,
      tags: ['general', 'fast'],
      strengths: 'Reliable instructions, minimal resource usage',
      weakness: 'Not specialized for code generation',
      description: 'Meta\'s ultra-lightweight model. Good for instructions and config files.',
    },
    {
      id: 'gemma3:1b',
      name: 'Gemma 3 1B',
      tier: 'nano',
      sizeGB: 0.9,
      minRamGB: 2,
      tags: ['general', 'google', 'fast'],
      strengths: 'Google-trained, fast and clean output',
      weakness: 'Too small for complex architecture',
      description: 'Google\'s smallest Gemma 3. Ideal for config, env files, and simple utilities.',
    },
    {
      id: 'phi4-mini',
      name: 'Phi-4 Mini (3.8B)',
      tier: 'nano',
      sizeGB: 2.5,
      minRamGB: 4,
      tags: ['code', 'reasoning', 'microsoft'],
      strengths: 'Excellent STEM reasoning for its size, strong TypeScript',
      weakness: 'Small context window',
      description: 'Microsoft\'s efficient 3.8B model. Outstanding code quality per GB — best nano-tier for real projects.',
    },
    {
      id: 'gemma4:2b',
      name: 'Gemma 4 2B (Edge)',
      tier: 'nano',
      sizeGB: 1.8,
      minRamGB: 3,
      tags: ['general', 'google', 'fast', 'multimodal', 'edge'],
      strengths: 'Vision-capable, RAG tasks, document summarization, multilingual',
      weakness: 'Too small for complex multi-file code generation',
      description: 'Google\'s Gemma 4 edge model. Ideal for document summarization, local RAG search, and light code assistance. Runs on any modern phone or basic laptop with 8GB RAM.',
    },

    // ── SMALL TIER (4–8 GB) ─────────────────────────────────
    {
      id: 'gemma3:4b',
      name: 'Gemma 3 4B',
      tier: 'small',
      sizeGB: 3.1,
      minRamGB: 5,
      tags: ['general', 'google', 'balanced'],
      strengths: 'Multilingual, vision-capable, strong reasoning',
      weakness: 'Less specialized for code than Qwen',
      description: 'Google\'s Gemma 3 edge model. Strong general reasoning and good code quality for its size.',
    },
    {
      id: 'llama3.2:3b',
      name: 'Llama 3.2 3B',
      tier: 'small',
      sizeGB: 2.0,
      minRamGB: 4,
      tags: ['general', 'fast', 'meta'],
      strengths: 'Balanced speed/quality, great for instructions',
      weakness: 'Not specialized for code',
      description: 'Meta\'s Llama 3.2 3B. Well-rounded model for general tasks and instructions.',
    },
    {
      id: 'qwen2.5-coder:7b',
      name: 'Qwen 2.5 Coder 7B',
      tier: 'small',
      sizeGB: 4.7,
      minRamGB: 8,
      tags: ['code', 'recommended', 'balanced'],
      strengths: 'Near GPT-4 code quality, 128k context, fast',
      weakness: 'General reasoning weaker than specialized models',
      description: '⭐ Top pick for most users. Best code generation for the price/performance ratio. Handles full Next.js apps with ease.',
    },
    {
      id: 'deepseek-coder:6.7b',
      name: 'DeepSeek Coder 6.7B',
      tier: 'small',
      sizeGB: 3.8,
      minRamGB: 6,
      tags: ['code', 'fast'],
      strengths: 'Excellent code completion, trained on code-heavy corpus',
      weakness: 'Weaker on instructions vs Qwen',
      description: 'DeepSeek\'s 6.7B coding model. Reliable code generation for most web frameworks.',
    },
    {
      id: 'mistral:7b',
      name: 'Mistral 7B',
      tier: 'small',
      sizeGB: 4.1,
      minRamGB: 7,
      tags: ['general', 'fast', 'french'],
      strengths: 'Excellent reasoning, fast inference, well-rounded',
      weakness: 'Not optimized for code specifically',
      description: 'Mistral AI\'s flagship 7B. Known for clean and fast reasoning on a wide range of tasks.',
    },
    {
      id: 'gemma4:9b',
      name: 'Gemma 4 9B',
      tier: 'small',
      sizeGB: 5.8,
      minRamGB: 8,
      tags: ['general', 'google', 'multimodal', 'balanced'],
      strengths: 'Vision + text, local RAG, document Q&A, smooth on MacBook Air',
      weakness: 'Less code-specialized than Qwen Coder',
      description: 'Google\'s Gemma 4 mid-edge model. Excellent balance of speed and intelligence. Runs smoothly on MacBook Air 8GB or any 8GB gaming PC.',
    },

    // ── MID TIER (8–16 GB) ──────────────────────────────────
    {
      id: 'qwen2.5-coder:14b',
      name: 'Qwen 2.5 Coder 14B',
      tier: 'mid',
      sizeGB: 9.0,
      minRamGB: 12,
      tags: ['code', 'advanced'],
      strengths: 'Complex multi-file projects, superior architecture understanding',
      weakness: 'Slower on CPU-only setups',
      description: 'The serious dev\'s choice. Handles entire codebases, complex APIs, and database schemas with high accuracy.',
    },
    {
      id: 'phi4',
      name: 'Phi-4 (14B)',
      tier: 'mid',
      sizeGB: 9.1,
      minRamGB: 12,
      tags: ['code', 'reasoning', 'microsoft'],
      strengths: 'Top-tier STEM and coding reasoning, punches well above 14B',
      weakness: 'Context window smaller than Qwen',
      description: 'Microsoft\'s Phi-4 14B. Exceptional reasoning and math — ideal for algorithm-heavy projects.',
    },
    {
      id: 'gemma3:12b',
      name: 'Gemma 3 12B',
      tier: 'mid',
      sizeGB: 8.1,
      minRamGB: 11,
      tags: ['general', 'google', 'balanced'],
      strengths: 'Strong multilingual, vision tasks, solid coding',
      weakness: 'Less specialized for code than Qwen Coder',
      description: 'Google\'s Gemma 3 12B — a solid all-rounder for mixed code/content generation projects.',
    },
    {
      id: 'deepseek-coder-v2:16b',
      name: 'DeepSeek Coder V2 16B (Lite)',
      tier: 'mid',
      sizeGB: 9.0,
      minRamGB: 14,
      tags: ['code', 'moe', 'advanced'],
      strengths: 'MoE architecture — high quality with lower active parameters',
      weakness: 'Total param count is high, needs enough RAM',
      description: 'DeepSeek Coder V2 Lite. Mixture-of-Experts model delivering 16B-class code quality with smart efficiency.',
    },
    {
      id: 'llama3.3:latest',
      name: 'Llama 3.3 70B (Distilled)',
      tier: 'mid',
      sizeGB: 8.5,
      minRamGB: 12,
      tags: ['general', 'meta', 'distilled'],
      strengths: 'Distilled from 70B — high reasoning quality in small package',
      weakness: 'Larger download, not as specialized for code',
      description: 'Meta\'s Llama 3.3 distilled. Brings 70B-class reasoning into a 12GB-friendly format.',
    },
    {
      id: 'llama4:scout',
      name: 'Llama 4 Scout (17B MoE)',
      tier: 'mid',
      sizeGB: 10.0,
      minRamGB: 14,
      tags: ['general', 'meta', 'moe', 'multimodal', 'code'],
      strengths: 'Multi-step planning, full-app generation offline, vision-capable',
      weakness: 'MoE total params larger than active — needs enough RAM',
      description: 'Meta\'s Llama 4 Scout MoE. Excellent at multi-step task planning and writing full applications offline. Natively multimodal.',
    },
    {
      id: 'mistral-nemo:latest',
      name: 'Mistral NeMo (12B)',
      tier: 'mid',
      sizeGB: 7.1,
      minRamGB: 10,
      tags: ['general', 'mistral', 'multilingual', 'creative'],
      strengths: 'Best Slavic/Balkan language nuance, creative writing, less robotic tone',
      weakness: 'Not specialized for code generation',
      description: 'Mistral NeMo 12B — outstanding multilingual understanding including Serbian, Croatian, Bosnian. Much less robotic. Great for content-heavy and instruction-heavy apps.',
    },

    // ── POWER TIER (16–24 GB) ────────────────────────────────
    {
      id: 'qwen2.5-coder:32b',
      name: 'Qwen 2.5 Coder 32B',
      tier: 'power',
      sizeGB: 20.0,
      minRamGB: 24,
      tags: ['code', 'advanced', 'flagship'],
      strengths: 'State-of-art code gen, handles entire app architectures, Liquid, Shopify, Next.js expert',
      weakness: 'Requires 24GB RAM or dedicated GPU',
      description: 'Qwen 2.5 Coder 32B — the best open-source coding model for workstations. Specifically optimized for Liquid code, Shopify development, and complex Next.js architectures.',
    },
    {
      id: 'codestral:22b',
      name: 'Codestral 22B',
      tier: 'power',
      sizeGB: 14.0,
      minRamGB: 18,
      tags: ['code', 'mistral', 'specialized'],
      strengths: 'Mistral\'s dedicated code model, fill-in-the-middle tasks',
      weakness: 'Higher RAM requirement, large download',
      description: 'Mistral AI\'s code-specialized 22B model. Excellent for complex refactoring and FIM (fill-in-the-middle) tasks.',
    },
    {
      id: 'gemma4:26b',
      name: 'Gemma 4 26B (MoE)',
      tier: 'power',
      sizeGB: 16.0,
      minRamGB: 20,
      tags: ['code', 'google', 'moe', 'multimodal'],
      strengths: 'Google MoE — frontier-level coding, reasoning, and UI vision',
      weakness: 'Needs fast GPU; slow on CPU only',
      description: 'Google\'s Gemma 4 MoE 26B. Extraordinary speed via MoE architecture. Understands UI screenshots. Competitive with GPT-4 class coding. 24-32GB RAM needed.',
    },
    {
      id: 'llama4:maverick',
      name: 'Llama 4 Maverick (17B MoE)',
      tier: 'power',
      sizeGB: 14.0,
      minRamGB: 20,
      tags: ['general', 'meta', 'moe', 'multimodal', 'code'],
      strengths: 'Full app generation offline, deep multi-step planning, vision',
      weakness: 'Large total params, benefits strongly from GPU offload',
      description: 'Meta\'s Llama 4 Maverick — more capable sibling of Scout. Autonomous coding, business analysis, and complex multi-step tasks. Mac Studio M2/M3 sweet spot.',
    },
    {
      id: 'deepseek-v3:distill',
      name: 'DeepSeek V3 (Distilled)',
      tier: 'power',
      sizeGB: 14.0,
      minRamGB: 20,
      tags: ['code', 'reasoning', 'deepseek', 'science'],
      strengths: 'Math, science, and hardest coding benchmarks — efficiency champion per parameter',
      weakness: 'Requires fast GPU for reasonable inference speed',
      description: 'DeepSeek V3 distilled for local use. Current champion in efficiency per parameter. Excels at mathematical reasoning, scientific analysis, and complex Next.js / full-stack architecture.',
    },

    // ── EXPERT TIER (24–40 GB) ───────────────────────────────
    {
      id: 'deepseek-r1:32b',
      name: 'DeepSeek R1 32B',
      tier: 'expert',
      sizeGB: 20.0,
      minRamGB: 28,
      tags: ['reasoning', 'code', 'deepseek'],
      strengths: 'Chain-of-thought reasoning, complex logic, math',
      weakness: 'Slower due to reasoning steps, heavy RAM',
      description: 'DeepSeek\'s R1 with explicit reasoning. Perfect for debugging complex multi-file codebases and architectural decisions.',
    },
    {
      id: 'gemma4:31b',
      name: 'Gemma 4 31B (Dense)',
      tier: 'expert',
      sizeGB: 19.0,
      minRamGB: 28,
      tags: ['code', 'google', 'flagship', 'multimodal'],
      strengths: 'Frontier-level code quality, SWE-bench top performer',
      weakness: 'Very high VRAM requirement; slow without GPU',
      description: 'Google\'s largest Gemma 4 dense model. Top-tier SWE-bench scores — seriously competes with closed-source models.',
    },
    {
      id: 'mistral-large:latest',
      name: 'Mistral Large 2 (123B, Q4)',
      tier: 'expert',
      sizeGB: 34.0,
      minRamGB: 40,
      tags: ['general', 'reasoning', 'mistral', 'large'],
      strengths: 'Top-tier reasoning and code for Mistral family',
      weakness: 'Extreme hardware requirement, very slow on CPU',
      description: 'Mistral Large 2 quantized. Best-in-class reasoning for Mistral. Multi-GPU or 40GB+ unified memory required.',
    },
    {
      id: 'deepseek-v4:distill',
      name: 'DeepSeek V4 (Distilled)',
      tier: 'expert',
      sizeGB: 22.0,
      minRamGB: 32,
      tags: ['code', 'reasoning', 'deepseek', 'science', 'flagship'],
      strengths: 'Best-in-class math, science, and full-stack code generation — efficiency world champion',
      weakness: 'GPU cluster or Mac Ultra strongly recommended for speed',
      description: 'DeepSeek V4 distilled — current efficiency world champion. Scientific work, deepest architectural analysis, and full autonomous coding sessions. 2x RTX 3090/4090 or Mac Studio Ultra.',
    },

    // ── EXTREME TIER (40+ GB) ────────────────────────────────
    {
      id: 'llama4:70b',
      name: 'Llama 4 Muse/Spark (70B)',
      tier: 'extreme',
      sizeGB: 43.0,
      minRamGB: 64,
      tags: ['general', 'meta', 'reasoning', 'code', 'large', 'multimodal'],
      strengths: 'Entire-app planning, deep business audit, multi-step autonomous coding',
      weakness: '64-128GB RAM or 2x RTX 3090/4090 required',
      description: 'Meta Llama 4 at 70B scale. The "brain" for fully autonomous systems. Writes full apps offline, performs deep business analysis. Mac Studio M2/M3 Ultra 128GB is the sweet spot.',
    },
    {
      id: 'deepseek-r1:70b',
      name: 'DeepSeek R1 70B',
      tier: 'extreme',
      sizeGB: 43.0,
      minRamGB: 48,
      tags: ['reasoning', 'code', 'deepseek', 'large'],
      strengths: 'Best open-source reasoning at 70B scale',
      weakness: 'Requires enterprise-grade hardware',
      description: 'DeepSeek R1 at 70B — near-frontier reasoning for multi-server setups. Research-grade hardware required.',
    },
    {
      id: 'qwen2.5-coder:72b',
      name: 'Qwen 2.5 Coder 72B',
      tier: 'extreme',
      sizeGB: 47.0,
      minRamGB: 56,
      tags: ['code', 'extreme', 'flagship'],
      strengths: 'Best open-source code model at any scale',
      weakness: 'Only for high-end server GPU setups',
      description: 'Qwen 2.5 Coder at full 72B scale. Matches or beats GPT-4o for pure code generation. A100/H100 required.',
    },
  ];

  /** Tier priority for recommendation (higher = better) */
  static TIER_ORDER = ['nano', 'small', 'mid', 'power', 'expert', 'extreme'];

  /**
   * Retrieves the system's total RAM in GB.
   */
  static getTotalRamGB() {
    return os.totalmem() / (1024 ** 3);
  }

  /**
   * Determines model compatibility and recommendation based on system RAM.
   * All models remain selectable (no hard disable) — incompatible ones
   * will show a warning in the UI instead of being blocked.
   */
  static getHardwareProfile() {
    const ramGB = this.getTotalRamGB();
    const cpus = os.cpus().length;
    const platform = os.platform();
    const cpuModel = os.cpus()[0]?.model || 'Unknown CPU';

    console.log(`[Hardware Benchmark] Total RAM: ${ramGB.toFixed(2)} GB | CPUs: ${cpus} | Platform: ${platform}`);

    // Pick the BEST model the system can comfortably run (leaves 20% overhead)
    const safeRam = ramGB * 0.80;
    const compatibleModels = this.AVAILABLE_MODELS.filter(m => safeRam >= m.minRamGB);
    
    // Pick highest tier compatible model, preferring 'code' tag
    const sorted = [...compatibleModels].sort((a, b) => {
      const tierDiff = this.TIER_ORDER.indexOf(b.tier) - this.TIER_ORDER.indexOf(a.tier);
      if (tierDiff !== 0) return tierDiff;
      // Within same tier, prefer code-specialized models
      const aCode = a.tags?.includes('code') ? 1 : 0;
      const bCode = b.tags?.includes('code') ? 1 : 0;
      return bCode - aCode;
    });

    const recommendedModelId = sorted[0]?.id ?? 'qwen2.5-coder:1.5b';

    const models = this.AVAILABLE_MODELS.map(model => {
      const isCompatible = ramGB >= model.minRamGB;
      const isMarginal = !isCompatible && ramGB >= model.minRamGB * 0.75; // within 75% = marginal

      return {
        ...model,
        isCompatible,
        isMarginal,      // can run but slow / might OOM
        isRecommended: model.id === recommendedModelId,
        // Never hard-disable — users can always override with a warning
        allowOverride: true,
      };
    });

    // Group models by tier for UI rendering
    const byTier = this.TIER_ORDER.reduce((acc, tier) => {
      acc[tier] = models.filter(m => m.tier === tier);
      return acc;
    }, {});

    return {
      platform,
      cpuModel,
      cpus,
      ramGB,
      models,
      byTier,
      recommendedModelId,
    };
  }
}

