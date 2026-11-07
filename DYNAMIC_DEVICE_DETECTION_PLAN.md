# Dynamic Device Hardware & Silicon Telemetry Architecture Plan

> **Target Version**: `wifi-tui v2.1.0`  
> **Objective**: Upgrade Tab [6] (*Hardware & Silicon Cockpit*) from static assumptions to a 100% dynamic, zero-overhead macOS hardware inspection engine that adapts to any Mac model, CPU core topology (Apple Silicon M1-M4, Pro, Max, Ultra, and Intel), GPU core counts, live GPU utilization, unified memory bandwidth, NVMe SSD controller, and Wi-Fi chipset.

---

## 1. Executive Summary & Problem Analysis

### Current Shortcomings
1. **Model Obfuscation**: The banner displays raw Darwin board identifiers like `MODEL: Mac14,2` instead of the human-readable marketing name `MacBook Air (M2, 2022)`.
2. **Missing GPU Intelligence**: There is zero GPU telemetry in the codebase—no GPU model, no core count (e.g. 10-core GPU), no live GPU utilization %, and no unified VRAM usage.
3. **Rigid Core Topology & Vertical Overflow**:
   - Hardcoded 4P + 4E fallback if sysctl fails.
   - Intel Macs have no E-cores, yet the UI shows 4 ghost E-cores at 0%.
   - High-core chips (M1/M2/M3/M4 Pro, Max, Ultra with 10, 12, 14, 16, or 24 cores) vertically overflow the UI tile because cores are rendered 1 per row.
4. **Opaque Storage & Memory**:
   - Shows generic APFS container gigabytes without detecting the physical SSD hardware model (`APPLE SSD AP0512Z`), PCIe/Apple Fabric bus, TRIM status, or S.M.A.R.T. health.
   - Unified memory shows simple usage without memory architecture, theoretical memory bandwidth (GB/s), or soldered status.
5. **Missing Wi-Fi Chipset**: Network tile displays interface `en0` without identifying the physical Wi-Fi chipset silicon (e.g. `Broadcom BCM4387 Wi-Fi 6`).

---

## 2. Dynamic Hardware Detection Specification (Zero-Overhead Darwin Probes)

All probes execute in sub-millisecond timeframes using native `sysctl` and `IODeviceTree` / `IOKit` registries, completely avoiding heavy CLI tools like `system_profiler`.

### 2.1 Host Model & Identity
* **Device Registry**: `ioreg -p IODeviceTree -k "product-name" -r` (or node `product` in `IODeviceTree`).
* **Attributes**:
  * `product-name`: `"MacBook Air (M2, 2022)"`
  * `product-soc-name`: `"Apple M2"`
  * `unique-model`: `"J413AP"`
  * `fdr-product-type`: `"Mac14,2"`
* **Fallback**: sysctl `hw.model` (`Mac14,2`) mapped against a lightweight offline lookup dictionary for legacy/virtualized environments.

### 2.2 CPU Architecture & Topology
* **Probes**:
  * Total logical CPUs: `hw.ncpu`
  * Physical cores: `hw.physicalcpu`
  * Performance Cores: `hw.perflevel0.logicalcpu` (P-Cores)
  * Efficiency Cores: `hw.perflevel1.logicalcpu` (E-Cores)
  * Architecture check: `hw.optional.arm64` (1 = Apple Silicon, 0 = Intel x86_64)
* **Dynamic Categorization**:
  * **Hybrid ARM Silicon (Apple Silicon)**:
    * `is_hybrid = true`
    * Dynamic P-Core count ($N_P$) + E-Core count ($N_E$).
    * Examples: M2 (4P + 4E), M3 Pro (6P + 6E), M3 Max (12P + 4E), M2 Ultra (16P + 8E).
  * **Symmetric x86_64 (Intel Mac)**:
    * `is_hybrid = false`
    * `e_cores = 0`, `p_cores = hw.physicalcpu`.
    * Labeled as: `Symmetric Core Matrix (N Cores / M Threads)`.

### 2.3 GPU Silicon & Live Utilization Engine
* **Registry Class**: `AGXAccelerator` (Apple Silicon) / `IOAccelerator` (Intel/AMD).
* **Attributes**:
  * GPU Model: `"model"` (e.g. `"Apple M2"`)
  * GPU Core Count: `"gpu-core-count"` (e.g. `10`)
  * Metal Generation: Derived from `"graphics-featureset-class"` (`"APPLE8"` = Metal 4).
* **Live Dynamic Telemetry (`PerformanceStatistics`)**:
  * Live GPU Load %: `"Device Utilization %"` (0 - 100%)
  * Active In-Use VRAM: `"In use system memory"` (e.g. 150 MB)
  * Total Allocated VRAM: `"Alloc system memory"` (e.g. 1.55 GB)
  * Tiler Utilization %: `"Tiler Utilization %"`
  * Renderer Utilization %: `"Renderer Utilization %"`

### 2.4 Physical Storage & NVMe Controller
* **Registry Class**: `IOBlockStorageDevice`.
* **Attributes**:
  * Drive Model: `"Product Name"` = `"APPLE SSD AP0512Z"`
  * Medium: `"Medium Type"` = `"Solid State"`
  * Protocol / Interconnect: `"Physical Interconnect"` = `"Apple Fabric"` (Internal NVMe)
  * S.M.A.R.T. Health: `"Verified"`
  * TRIM: `true` (APFS native)

### 2.5 Unified Memory Architecture & Bandwidth
* **Capacity**: `sysctl -n hw.memsize` (e.g. 16.0 GB).
* **Architecture**: Unified LPDDR5 Memory (Soldered zero-copy memory pool shared between CPU, GPU, and ANE).
* **Theoretical Memory Bandwidth**:
  * M1 / M2 / M3 / M4 base: $100\text{ GB/s}$ ($68.25\text{ GB/s}$ on M1)
  * M1/M2/M3/M4 Pro: $150 - 200\text{ GB/s}$
  * M1/M2/M3/M4 Max: $300 - 400\text{ GB/s}$
  * M1/M2 Ultra: $800\text{ GB/s}$

### 2.6 Apple Neural Engine (ANE) & Wi-Fi Silicon
* **Neural Engine**: 16-Core ANE (15.8 TOPS on M2, 18 TOPS on M3, 38 TOPS on M4, 32-core on Ultra).
* **Wi-Fi Chipset**: `IODeviceTree/product` `"wifi-chipset"` = `"4387"` (Broadcom/Apple BCM4387 Wi-Fi 6 802.11ax 2x2 MIMO 160MHz).

---

## 3. Data Structure Extensions (`DeviceInfo`)

```rust
#[derive(Debug, Clone, Default)]
pub struct DeviceInfo {
    // Identity & Consumer Model
    pub model_name: String,        // "MacBook Air (M2, 2022)"
    pub model_identifier: String,  // "Mac14,2"
    pub chip_name: String,         // "Apple M2"
    pub soc_architecture: String,  // "ARM64" or "x86_64"
    pub is_hybrid_cpu: bool,       // true on Apple Silicon, false on Intel
    pub os_version: String,
    pub uptime_str: String,

    // CPU & Core Topology
    pub p_cores: usize,
    pub e_cores: usize,
    pub total_cores: usize,
    pub cpu_total_pct: f64,
    pub cpu_cores_pct: Vec<f64>,
    pub cpu_history: Vec<f64>,
    pub load_avg: (f64, f64, f64),
    pub thermal_state: String,

    // GPU Silicon (NEW)
    pub gpu_name: String,          // "Apple M2"
    pub gpu_cores: usize,          // 10
    pub gpu_load_pct: f64,         // Live utilization %
    pub gpu_vram_used_mb: f64,     // Active in-use VRAM
    pub gpu_vram_alloc_mb: f64,    // Allocated VRAM pool
    pub gpu_metal_feature: String, // "Metal 4 (APPLE8)"

    // Neural Engine & Display (NEW)
    pub ane_cores: usize,          // 16
    pub ane_tops: f64,             // 15.8 TOPS
    pub display_summary: String,   // "Built-in Liquid Retina (2560x1664)"

    // Memory Architecture
    pub mem_total_gb: f64,
    pub mem_used_gb: f64,
    pub mem_free_gb: f64,
    pub mem_app_gb: f64,
    pub mem_wired_gb: f64,
    pub mem_compressed_gb: f64,
    pub mem_cached_gb: f64,
    pub mem_swap_mb: f64,
    pub mem_bandwidth_gbs: f64,    // 100.0 GB/s
    pub mem_pressure_pct: f64,
    pub mem_pressure_label: String,

    // Storage & NVMe Hardware (NEW)
    pub disk_model: String,        // "APPLE SSD AP0512Z"
    pub disk_protocol: String,     // "Apple Fabric NVMe"
    pub disk_smart_status: String, // "Verified"
    pub disk_trim: bool,           // true
    pub disk_total_gb: f64,
    pub disk_used_gb: f64,
    pub disk_free_gb: f64,
    pub disk_read_mbs: f64,
    pub disk_write_mbs: f64,

    // Battery / Power
    pub battery_level_pct: f64,
    pub battery_health_pct: f64,
    pub battery_cycles: u32,
    pub battery_status: String,
    pub battery_power_source: String,
    pub battery_watts: f64,

    // Network Silicon (NEW)
    pub wifi_chipset: String,      // "Broadcom BCM4387 (Wi-Fi 6)"
    pub net_primary_iface: String,
    pub net_ip: String,
    pub net_mac_hardware: String,
    pub net_mac_is_private: bool,
    pub net_awdl_active: bool,
    pub net_tunnels: Vec<String>,

    // Top processes & companion
    pub top_processes: Vec<ProcessSummary>,
    pub mascot_visible: bool,
}
```

---

## 4. Adaptive Dynamic UI Layout Plan

```
┌────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ [CPU LOAD: 8.2%]         │ [GPU LOAD: 4.0% · 10C]   │ [RAM: 8.4/16.0 GB · 100GB/s] │ [BATT: 78% · 4.2W]│
├────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ SYSTEM SPECIFICATION & HOST IDENTITY                                                                   │
│  /\_/\   MODEL: MacBook Air (M2, 2022) [Mac14,2]   CHIP: Apple M2 (8 CPU Cores: 4P + 4E)             │
│ ( o.o )  GPU: Apple M2 (10 Cores, Metal 4)          ANE: 16-Core Neural Engine (15.8 TOPS)             │
│  > ^ <   STORAGE: APPLE SSD AP0512Z (512GB NVMe)   WIFI: Broadcom BCM4387 Wi-Fi 6                     │
├────────────────────────────────────────────────────┬───────────────────────────────────────────────────┤
│ ◉ CPU & SILICON TOPOLOGY (Dynamic 1 or 2 Columns)  │ ⊞ UNIFIED MEMORY & PRESSURE                       │
│  Total Load: 8.2% [========            ] Load: 1.8 │  Used: 8.4 GB / 16.0 GB (52%) [======    ] 100GB/s │
│  Performance Cores (4 P-Cores):                    │   • App Memory: 5.2 GB    • Wired Memory: 1.8 GB  │
│   P0 [====        ] 14.2%   P1 [==          ]  8.1%│   • Compressed: 1.4 GB    • Swap Used: 0.0 MB     │
│   P2 [======      ] 22.0%   P3 [=           ]  4.5%│   • Memory Pressure: NORMAL (18% used headroom)   │
│  Efficiency Cores (4 E-Cores):                     ├───────────────────────────────────────────────────┤
│   E0 [=           ]  3.1%   E1 [=           ]  2.0%│ ⊞ GPU ACCELERATOR & METAL ENGINE                  │
│   E2 [            ]  0.8%   E3 [            ]  0.5%│  Live GPU Load:  4.0% [==                       ] │
│  Thermal: [NOMINAL] · Frequency Clocks: Dynamic    │  Active VRAM:   150 MB / 1.55 GB Allocated Pool   │
├────────────────────────────────────────────────────┤  Metal API:      Metal 4 (APPLE8 Feature Class)   │
│ ▥ PHYSICAL NVMe STORAGE & APFS CONTAINER           ├───────────────────────────────────────────────────┤
│  APPLE SSD AP0512Z · Apple Fabric PCIe NVMe        │ ⚡ BATTERY, POWER & THERMALS                       │
│  Usage: 110.7 GB Free / 494.4 GB Total (77.6% Used)│  Level: 78% [BATT] · Health: 98% (142 Cycles)     │
│  Throughput: Read 0.12 MB/s  ·  Write 1.45 MB/s    │  Discharging at 4.8W · Condition: Normal          │
└────────────────────────────────────────────────────┴───────────────────────────────────────────────────┘
```

### Layout Enhancements
1. **High-Core Density Grid**:
   * If total cores $\le 8$: Single-column core listing.
   * If total cores $> 8$ (M-Pro, M-Max, M-Ultra): Automatically lays out cores in a **compact dual-column matrix** (`P0 ... P1` / `P2 ... P3`), completely eliminating vertical scrolling or clipped elements.
2. **Dedicated GPU & Metal Box**:
   * Displays live GPU device utilization, active in-use VRAM, and total allocated system memory pool.
3. **Physical NVMe Details**:
   * Shows the exact Apple SSD model name, protocol, and TRIM state in the storage block.
4. **Network Silicon Chipset**:
   * Displays the physical Broadcom/Apple Wi-Fi generation directly alongside the primary interface and MAC privacy state.

---

## 5. Implementation Milestones

| Step | Component | Changes Required |
| :--- | :--- | :--- |
| **1** | `src/core/device_info.rs` | Add IODeviceTree `product-name` & `product-soc-name` query for exact marketing name. |
| **2** | `src/core/device_info.rs` | Add `AGXAccelerator` / `IOAccelerator` parser for GPU cores, live GPU utilization %, and VRAM. |
| **3** | `src/core/device_info.rs` | Add `IOBlockStorageDevice` parser for physical SSD controller model and NVMe protocol. |
| **4** | `src/core/device_info.rs` | Add memory bandwidth calculator and Intel vs Apple Silicon core topology detector. |
| **5** | `src/ui/views/device.rs` | Implement adaptive 2-column core grid for high-core systems ($>8$ cores). |
| **6** | `src/ui/views/device.rs` | Add dedicated GPU tile and update banner with exact model, chip, GPU, and Wi-Fi silicon. |
| **7** | Unit & Live Tests | Add automated unit tests covering M1-M4 and Intel topology edge cases. |

---

## 6. Execution Command

To open this plan in Neovim within Kitty:
```bash
/Applications/kitty.app/Contents/MacOS/kitty @ launch --type=tab --tab-title="Dynamic Device Plan" /opt/homebrew/bin/nvim /Users/abdullahbinmadhi/Projects/WIFI-TUI/DYNAMIC_DEVICE_DETECTION_PLAN.md
```
