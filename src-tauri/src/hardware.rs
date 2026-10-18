use crate::*;


// ==================== HARDWARE API ====================

#[derive(Serialize, Clone)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub tier: String,
    #[serde(rename = "sizeGB")]
    pub size_gb: f64,
    #[serde(rename = "minRamGB")]
    pub min_ram_gb: f64,
    pub tags: Vec<String>,
    pub strengths: String,
    pub weakness: String,
    pub description: String,
    #[serde(rename = "isCompatible")]
    pub is_compatible: bool,
    #[serde(rename = "isMarginal")]
    pub is_marginal: bool,
    #[serde(rename = "isRecommended")]
    pub is_recommended: bool,
    #[serde(rename = "allowOverride")]
    pub allow_override: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct GpuInfo {
    pub name: String,
    #[serde(rename = "vramGB")]
    pub vram_gb: f64,
    pub brand: String, // "Nvidia", "AMD", "Intel", "Apple", "Unknown"
}

pub fn detect_gpu() -> Option<GpuInfo> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("powershell");
        cmd.arg("-NoProfile")
            .arg("-Command")
            .arg("$regMax = (Get-ItemProperty -Path 'HKLM:\\SYSTEM\\ControlSet001\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\0*' -ErrorAction SilentlyContinue | ForEach-Object { $_.'HardwareInformation.qwMemorySize' } | Measure-Object -Maximum).Maximum; Get-CimInstance Win32_VideoController | Select-Object Name, AdapterRAM, @{N='QwMem';E={$regMax}} | ConvertTo-Json -Compress")
            .creation_flags(0x08000000); // CREATE_NO_WINDOW

        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let trimmed = stdout.trim();
                
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    let parse_single_gpu = |obj: &serde_json::Value| -> Option<GpuInfo> {
                        let name = obj.get("Name")?.as_str()?.to_string();
                        let parse_bytes = |field: &str| -> f64 {
                            if let Some(n) = obj.get(field).and_then(|v| v.as_u64()) {
                                n as f64
                            } else if let Some(s) = obj.get(field).and_then(|v| v.as_str()) {
                                s.parse::<f64>().unwrap_or(0.0)
                            } else if let Some(n) = obj.get(field).and_then(|v| v.as_i64()) {
                                n as f64
                            } else {
                                0.0
                            }
                        };

                        let adapter_bytes = parse_bytes("AdapterRAM");
                        let qw_bytes = parse_bytes("QwMem");
                        let name_lower = name.to_lowercase();
                        let brand = if name_lower.contains("nvidia") || name_lower.contains("geforce") || name_lower.contains("rtx") || name_lower.contains("gtx") {
                            "Nvidia".to_string()
                        } else if name_lower.contains("amd") || name_lower.contains("radeon") {
                            "AMD".to_string()
                        } else if name_lower.contains("intel") {
                            "Intel".to_string()
                        } else {
                            "Unknown".to_string()
                        };

                        // Prefer 64-bit QwMem for dedicated GPUs to avoid the 32-bit 4GB AdapterRAM cap on Windows
                        let ram_bytes = if (brand == "Nvidia" || brand == "AMD") && qw_bytes > adapter_bytes {
                            qw_bytes
                        } else {
                            adapter_bytes
                        };

                        let vram_gb = (ram_bytes / (1024.0 * 1024.0 * 1024.0) * 100.0).round() / 100.0;

                        Some(GpuInfo {
                            name,
                            vram_gb,
                            brand,
                        })
                    };

                    if val.is_array() {
                        if let Some(arr) = val.as_array() {
                            let mut best_gpu: Option<GpuInfo> = None;
                            for item in arr {
                                if let Some(gpu) = parse_single_gpu(item) {
                                    if gpu.brand == "Nvidia" || gpu.brand == "AMD" {
                                        return Some(gpu);
                                    }
                                    if best_gpu.is_none() {
                                        best_gpu = Some(gpu);
                                    }
                                }
                            }
                            return best_gpu;
                        }
                    } else {
                        return parse_single_gpu(&val);
                    }
                }
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("system_profiler")
            .arg("SPDisplaysDataType")
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let mut chip_set = "Apple M-Series".to_string();
                let mut vram_gb = 8.0;

                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("Chipset Model:") {
                        chip_set = trimmed.replace("Chipset Model:", "").trim().to_string();
                    } else if trimmed.starts_with("VRAM (Total):") || trimmed.starts_with("VRAM (Dynamic, Max):") {
                        let vram_str = trimmed
                            .replace("VRAM (Total):", "")
                            .replace("VRAM (Dynamic, Max):", "")
                            .trim()
                            .to_string();
                        if vram_str.contains("GB") {
                            vram_gb = vram_str.replace("GB", "").trim().parse::<f64>().unwrap_or(8.0);
                        } else if vram_str.contains("MB") {
                            let mb = vram_str.replace("MB", "").trim().parse::<f64>().unwrap_or(8192.0);
                            vram_gb = mb / 1024.0;
                        }
                    }
                }

                return Some(GpuInfo {
                    name: chip_set,
                    vram_gb,
                    brand: "Apple".to_string(),
                });
            }
        }
        None
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("lspci | grep -i -E 'vga|3d'")
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let line = stdout.lines().next().unwrap_or("").trim().to_string();
                
                let brand = if line.to_lowercase().contains("nvidia") {
                    "Nvidia".to_string()
                } else if line.to_lowercase().contains("amd") || line.to_lowercase().contains("ati") {
                    "AMD".to_string()
                } else if line.to_lowercase().contains("intel") {
                    "Intel".to_string()
                } else {
                    "Unknown".to_string()
                };

                return Some(GpuInfo {
                    name: line,
                    vram_gb: 4.0, 
                    brand,
                });
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
pub struct CachedHardwareSpecs {
    pub cpu_model: String,
    pub logical_cpus: usize,
    pub physical_cores: usize,
    pub ram_gb: f64,
    pub gpu: Option<GpuInfo>,
}

static HARDWARE_SPECS_CACHE: std::sync::OnceLock<CachedHardwareSpecs> = std::sync::OnceLock::new();

pub fn get_cached_hardware_specs() -> &'static CachedHardwareSpecs {
    HARDWARE_SPECS_CACHE.get_or_init(|| {
        let mut sys = System::new_all();
        sys.refresh_all();
        let total_ram_bytes = sys.total_memory();
        let ram_gb = total_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let logical_cpus = sys.cpus().len().max(1);
        let physical_cores = sys.physical_core_count().unwrap_or_else(|| (logical_cpus / 2).max(1));
        let cpu_model = sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_else(|| "Unknown CPU".to_string());
        let gpu = detect_gpu();
        CachedHardwareSpecs {
            cpu_model,
            logical_cpus,
            physical_cores,
            ram_gb,
            gpu,
        }
    })
}

#[derive(Serialize, Clone, Debug)]
pub struct OllamaRuntimeConfig {
    pub profile_tier: String,
    #[serde(rename = "numCtx")]
    pub num_ctx: u32,
    #[serde(rename = "defaultNumPredict")]
    pub default_num_predict: u32,
    #[serde(rename = "manifestNumPredict")]
    pub manifest_num_predict: u32,
    #[serde(rename = "numBatch")]
    pub num_batch: u32,
    #[serde(rename = "numThread")]
    pub num_thread: Option<usize>,
}

pub fn estimate_model_size_gb(model_id: &str) -> f64 {
    let lower = model_id.to_lowercase();
    for m in get_all_models() {
        if m.id.to_lowercase() == lower {
            return m.size_gb;
        }
    }
    if lower.contains("32b") || lower.contains("31b") || lower.contains("34b") {
        19.0
    } else if lower.contains("26b") || lower.contains("27b") {
        16.0
    } else if lower.contains("14b") || lower.contains("13b") {
        9.0
    } else if lower.contains("12b") {
        7.2
    } else if lower.contains("7b") || lower.contains("8b") || lower.contains("6.7b") {
        4.7
    } else if lower.contains("4b") {
        2.8
    } else if lower.contains("3b") {
        2.0
    } else if lower.contains("1.5b") || lower.contains("2b") || lower.contains("1b") {
        1.2
    } else {
        4.7
    }
}

pub fn get_ollama_runtime_config(model_id: &str) -> OllamaRuntimeConfig {
    let specs = get_cached_hardware_specs();
    let model_size_gb = estimate_model_size_gb(model_id);

    let (has_dedicated_gpu, vram_gb) = match &specs.gpu {
        Some(gpu) if gpu.brand == "Nvidia" || gpu.brand == "AMD" || gpu.brand == "Apple" => (true, gpu.vram_gb),
        _ => (false, 0.0),
    };

    let vram_headroom = vram_gb - model_size_gb;

    if has_dedicated_gpu && vram_headroom >= 3.0 {
        // Tier 1: High-End GPU (Full model + large KV cache fit comfortably in VRAM)
        OllamaRuntimeConfig {
            profile_tier: "high-end-gpu".to_string(),
            num_ctx: 12288,
            default_num_predict: 4096,
            manifest_num_predict: 8192,
            num_batch: 512,
            num_thread: None,
        }
    } else if has_dedicated_gpu && vram_headroom >= -1.0 {
        // Tier 2: Hybrid GPU+CPU (e.g., 6GB VRAM with 7B model: compact 6144 context keeps layers in VRAM + protects CPU)
        let safe_threads = if specs.physical_cores > 2 {
            specs.physical_cores - 1
        } else {
            specs.physical_cores.max(1)
        };
        OllamaRuntimeConfig {
            profile_tier: "hybrid-gpu-cpu".to_string(),
            num_ctx: 6144,
            default_num_predict: 4096,
            manifest_num_predict: 6144,
            num_batch: 256,
            num_thread: Some(safe_threads),
        }
    } else {
        // Tier 3: Low-Spec / CPU-Heavy (Integrated GPU or model significantly larger than VRAM)
        let safe_threads = ((specs.physical_cores * 3) / 4).max(1);
        OllamaRuntimeConfig {
            profile_tier: "low-spec-cpu".to_string(),
            num_ctx: 6144,
            default_num_predict: 4096,
            manifest_num_predict: 6144,
            num_batch: 128,
            num_thread: Some(safe_threads),
        }
    }
}

pub fn build_ollama_options(model_id: &str, temperature: f64, num_predict_override: Option<u32>) -> serde_json::Value {
    let cfg = get_ollama_runtime_config(model_id);
    let num_predict = num_predict_override.unwrap_or(cfg.default_num_predict);
    let mut opts = serde_json::json!({
        "temperature": temperature,
        "num_ctx": cfg.num_ctx,
        "num_predict": num_predict,
        "num_batch": cfg.num_batch,
    });
    if let Some(threads) = cfg.num_thread {
        if let Some(obj) = opts.as_object_mut() {
            obj.insert("num_thread".to_string(), serde_json::json!(threads));
        }
    }
    opts
}

#[derive(Serialize)]
pub struct HardwareProfile {
    pub platform: String,
    #[serde(rename = "cpuModel")]
    pub cpu_model: String,
    pub cpus: usize,
    #[serde(rename = "ramGB")]
    pub ram_gb: f64,
    pub models: Vec<Model>,
    #[serde(rename = "byTier")]
    pub by_tier: HashMap<String, Vec<Model>>,
    #[serde(rename = "recommendedModelId")]
    pub recommended_model_id: String,
    pub gpu: Option<GpuInfo>,
    #[serde(rename = "runtimeOptimization")]
    pub runtime_optimization: OllamaRuntimeConfig,
}

pub fn get_all_models() -> Vec<Model> {
    vec![
        Model {
            id: "qwen2.5-coder:1.5b".to_string(),
            name: "Qwen 2.5 Coder 1.5B".to_string(),
            tier: "nano".to_string(),
            size_gb: 0.98,
            min_ram_gb: 2.0,
            tags: vec!["code".to_string(), "fast".to_string()],
            strengths: "Ultra-lightweight, minimal resource usage".to_string(),
            weakness: "Limited reasoning for highly complex apps".to_string(),
            description: "Perfect for legacy hardware or systems with limited RAM. Extremely fast response times.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "gemma4:e2b".to_string(),
            name: "Gemma 4 Edge 2B".to_string(),
            tier: "nano".to_string(),
            size_gb: 1.5,
            min_ram_gb: 3.0,
            tags: vec!["code".to_string(), "fast".to_string(), "multimodal".to_string()],
            strengths: "Very fast local generation, audio/image support".to_string(),
            weakness: "Basic code planning".to_string(),
            description: "Super lightweight, ideal for laptops and low-spec machines. Trimodal inputs (text, image, audio).".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "qwen2.5-coder:3b".to_string(),
            name: "Qwen 2.5 Coder 3B".to_string(),
            tier: "nano".to_string(),
            size_gb: 2.0,
            min_ram_gb: 4.0,
            tags: vec!["code".to_string(), "fast".to_string(), "balanced".to_string()],
            strengths: "Excellent coding for its size, strong TypeScript".to_string(),
            weakness: "Smaller context capacity than 7B".to_string(),
            description: "Outstanding code quality for entry-level machines. Recommended for 4GB-8GB RAM setups.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "gemma4:e4b".to_string(),
            name: "Gemma 4 Edge 4B".to_string(),
            tier: "small".to_string(),
            size_gb: 2.8,
            min_ram_gb: 6.0,
            tags: vec!["code".to_string(), "fast".to_string(), "multimodal".to_string()],
            strengths: "Trimodal support, excellent speed-to-quality ratio".to_string(),
            weakness: "Limited context compared to 31B".to_string(),
            description: "Balanced edge model with native audio/image inputs. Great for consumer laptops with 8GB RAM.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "deepseek-coder:6.7b".to_string(),
            name: "DeepSeek Coder 6.7B".to_string(),
            tier: "small".to_string(),
            size_gb: 3.8,
            min_ram_gb: 8.0,
            tags: vec!["code".to_string(), "balanced".to_string()],
            strengths: "Highly reliable, excellent repository understanding".to_string(),
            weakness: "Slightly older architecture".to_string(),
            description: "Highly robust and proven coder model. Excellent for balanced performance.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "qwen2.5-coder:7b".to_string(),
            name: "Qwen 2.5 Coder 7B".to_string(),
            tier: "small".to_string(),
            size_gb: 4.7,
            min_ram_gb: 8.0,
            tags: vec!["code".to_string(), "recommended".to_string(), "balanced".to_string()],
            strengths: "Near GPT-4 code quality, 128k context, fast".to_string(),
            weakness: "General non-code reasoning is average".to_string(),
            description: "Top pick for most developers. Best code generation for the price/performance ratio. Handles full Next.js apps with ease.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "qwen2.5-coder:14b".to_string(),
            name: "Qwen 2.5 Coder 14B".to_string(),
            tier: "mid".to_string(),
            size_gb: 9.0,
            min_ram_gb: 12.0,
            tags: vec!["code".to_string(), "advanced".to_string()],
            strengths: "Complex multi-file projects, superior architecture understanding".to_string(),
            weakness: "Requires more VRAM/RAM for fast performance".to_string(),
            description: "The professional choice. Handles entire codebases, complex APIs, and database schemas with extreme accuracy. Perfect for 16GB+ RAM setups.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "gemma4:12b".to_string(),
            name: "Gemma 4 12B".to_string(),
            tier: "mid".to_string(),
            size_gb: 7.2,
            min_ram_gb: 16.0,
            tags: vec!["code".to_string(), "advanced".to_string(), "multimodal".to_string()],
            strengths: "Strong reasoning, excellent structured JSON, native audio".to_string(),
            weakness: "Higher VRAM requirements".to_string(),
            description: "Highly capable model for intermediate configurations. Near-frontier reasoning on workstations with 16GB+ RAM.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "gemma4:26b".to_string(),
            name: "Gemma 4 26B (MoE)".to_string(),
            tier: "power".to_string(),
            size_gb: 16.0,
            min_ram_gb: 24.0,
            tags: vec!["code".to_string(), "advanced".to_string(), "fast".to_string()],
            strengths: "Fast MoE inference, excellent programming logic".to_string(),
            weakness: "Large download size, high VRAM usage".to_string(),
            description: "Google's Mixture-of-Experts powerhouse. Extremely fast coding generation for systems with 24GB+ RAM / 16GB+ VRAM.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "gemma4:31b".to_string(),
            name: "Gemma 4 31B".to_string(),
            tier: "expert".to_string(),
            size_gb: 18.0,
            min_ram_gb: 32.0,
            tags: vec!["code".to_string(), "expert".to_string()],
            strengths: "Frontier reasoning, ranked #3 on Arena, complex database design".to_string(),
            weakness: "Requires substantial system memory, slow on entry-level GPUs".to_string(),
            description: "Google's flagship open-weight model. Outstanding logic, code layout, and architecture understanding. Requires 32GB+ RAM / 24GB+ VRAM.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "qwen2.5-coder:32b".to_string(),
            name: "Qwen 2.5 Coder 32B".to_string(),
            tier: "expert".to_string(),
            size_gb: 19.0,
            min_ram_gb: 32.0,
            tags: vec!["code".to_string(), "expert".to_string()],
            strengths: "Expert coder, near GPT-4 coding capabilities".to_string(),
            weakness: "Requires massive system resources".to_string(),
            description: "The absolute benchmark for local code generation. Outstanding repository understanding and bug fixing. Requires 32GB+ RAM.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        }
    ]
}

#[tauri::command]
pub fn get_hardware_profile() -> HardwareProfile {
    let specs = get_cached_hardware_specs();
    let ram_gb = specs.ram_gb;
    let cpus = specs.logical_cpus;
    let cpu_model = specs.cpu_model.clone();
    let gpu_info = specs.gpu.clone();
    
    let raw_models = get_all_models();
    
    // Determine recommended model based on GPU first, fall back to RAM
    let mut best_model_id = "qwen2.5-coder:7b".to_string(); // default fallback
    let mut recommendation_set = false;

    if let Some(ref gpu) = gpu_info {
        if gpu.brand == "Nvidia" || gpu.brand == "Apple" || gpu.brand == "AMD" {
            if gpu.vram_gb >= 10.0 {
                best_model_id = "qwen2.5-coder:14b".to_string();
                recommendation_set = true;
            } else if gpu.vram_gb >= 3.0 {
                best_model_id = "qwen2.5-coder:7b".to_string();
                recommendation_set = true;
            }
        }
    }

    if !recommendation_set {
        let safe_ram = ram_gb * 0.80;
        let mut best_tier_idx = -1;
        let mut best_is_code = false;
        
        let tier_order = vec!["nano", "small", "mid", "power", "expert", "extreme"];
        
        for model in &raw_models {
            // Apply 0.5 GB RAM tolerance to safe RAM check
            if (safe_ram + 0.5) >= model.min_ram_gb {
                let tier_idx = tier_order.iter().position(|&t| t == model.tier).unwrap_or(0) as i32;
                let is_code = model.tags.contains(&"code".to_string());
                
                // Higher tier preferred. If same tier, prefer 'code'
                if tier_idx > best_tier_idx || (tier_idx == best_tier_idx && is_code && !best_is_code) {
                    best_model_id = model.id.clone();
                    best_tier_idx = tier_idx;
                    best_is_code = is_code;
                }
            }
        }
    }
    
    let runtime_optimization = get_ollama_runtime_config(&best_model_id);
    
    let mut models = Vec::new();
    let mut by_tier: HashMap<String, Vec<Model>> = HashMap::new();
    let tolerance = 0.5;
    
    for mut model in raw_models {
        // Apply 0.5 GB RAM tolerance to compatibility checks
        model.is_compatible = (ram_gb + tolerance) >= model.min_ram_gb;
        model.is_marginal = !model.is_compatible && (ram_gb + tolerance) >= model.min_ram_gb * 0.75;
        model.is_recommended = model.id == best_model_id;
        
        models.push(model.clone());
        by_tier.entry(model.tier.clone()).or_insert_with(Vec::new).push(model);
    }
    
    HardwareProfile {
        platform: std::env::consts::OS.to_string(),
        cpu_model,
        cpus,
        ram_gb,
        models,
        by_tier,
        recommended_model_id: best_model_id,
        gpu: gpu_info,
        runtime_optimization,
    }
}

#[derive(Serialize)]
pub struct LocalToolsProfile {
    pub node: Option<String>,
    pub npm: Option<String>,
    pub rust: Option<String>,
    pub cargo: Option<String>,
}

#[tauri::command]
pub async fn get_local_tools_profile() -> LocalToolsProfile {
    let node_ver = get_tool_version("node", &["-v"]).await;
    
    #[cfg(target_os = "windows")]
    let npm_ver = get_tool_version("cmd", &["/C", "npm", "-v"]).await;
    #[cfg(not(target_os = "windows"))]
    let npm_ver = get_tool_version("npm", &["-v"]).await;
    
    let rust_ver = get_tool_version("rustc", &["--version"]).await;
    let cargo_ver = get_tool_version("cargo", &["--version"]).await;
    
    LocalToolsProfile {
        node: node_ver,
        npm: npm_ver,
        rust: rust_ver,
        cargo: cargo_ver,
    }
}

pub async fn get_tool_version(cmd: &str, args: &[&str]) -> Option<String> {
    let mut command = tokio::process::Command::new(cmd);
    for arg in args {
        command.arg(arg);
    }
    
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    
    if let Ok(output) = command.output().await {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            let cleaned = raw.trim()
                .replace("rustc ", "")
                .replace("cargo ", "");
            let first_word = cleaned.split_whitespace().next().unwrap_or("").to_string();
            if !first_word.is_empty() {
                return Some(first_word);
            }
            return Some(cleaned.to_string());
        }
    }
    None
}

#[tauri::command]
pub async fn ollama_unload_model(model: String) -> Result<(), String> {
    let client = reqwest::Client::new();
    let req_body = serde_json::json!({
        "model": model,
        "prompt": "",
        "keep_alive": 0
    });
    
    let res = client.post("http://127.0.0.1:11434/api/generate")
        .json(&req_body)
        .send()
        .await;
        
    match res {
        Ok(resp) => {
            if resp.status().is_success() {
                println!("[Ollama] Model '{}' unloaded successfully", model);
                Ok(())
            } else {
                Err(format!("Failed to unload model, HTTP status: {}", resp.status()))
            }
        }
        Err(e) => Err(format!("Failed to connect to Ollama: {}", e.to_string()))
    }
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

