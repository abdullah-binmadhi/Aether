================================================================================
  WIFI-TUI  —  TAB 6: HARDWARE & SYSTEM COCKPIT  (PLAN & VISUAL SKETCH)
================================================================================

MY OPINION & DESIGN VISION
--------------------------
The screenshot of `mo status` you shared has a fantastic, clean hacker aesthetic
that feels right at home in a terminal. But for `wifi-tui`, we can take it even
further:
  1. Match our signature Amber CRT / Phosphor Gold theme (`#FFB000` / `#FF8000`).
  2. Native Zero-Cost Collectors: Do NOT run slow Python or shell scripts. We can
     read CPU, Memory, Disk, and Battery directly via macOS Mach Kernel APIs,
     `sysctl`, and `IOKit` in < 1 millisecond with ~0.0% CPU overhead!
  3. Seamless Tab Integration: Becomes `[6:DEVICE]` on the top navigation bar.
     Pressing `6` switches directly into it.
  4. Expanded & More Useful: Your screenshot shows CPU, RAM, Disk, Power, Procs,
     and Network. We will include ALL of that, PLUS:
       • Apple Silicon Core Topology Breakdown (P-cores vs E-cores separately)
       • macOS Native Memory Pressure Gauge (Normal / Warning / Critical)
       • Battery Condition, Cycle Count & Wattage Draw (e.g. 8W ⚡, 79% Health)
       • Thermal State & Power Mode (Nominal / Fair / Low Power Mode)
       • Wi-Fi Chipset, Hardware MAC vs Random Private MAC, and VPN/Tunnel Status
       • Optional CRT Cat Mascot `/\_/\` toggleable with `[C]`!

--------------------------------------------------------------------------------
ASCII ART FULL-SCREEN MOCKUP (What it looks like in Neovim / Terminal)
--------------------------------------------------------------------------------
Assumes standard 120-col cockpit layout. On narrow screens, columns collapse vertically.

┌── NAV RAIL ──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ [1:COCKPIT]  [2:SPECTRUM]  [3:FIREWALL]  [4:TUNING]  [5:SPEEDTEST]  [6:DEVICE]                               │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
┌── SYSTEM SCORECARD ──────────────────────────────────────────────────────────────────────────────────────────┐
│ MODEL: MacBook Air 15" · Apple M2 (4P+4E, 10GPU) · 16GB Unified · macOS 27.0.1 · Uptime 8h 24m   /\_/\    [C]│
│ THERMAL: Nominal (Cool)  ·  POWER: 8.2W ⚡ On Charger (100%)  ·  MEM PRESSURE: Normal (12%)       \ o o /   cat│
└─────────────────────────────────────────────────────────────────────────────────────────────────=( Y )=──────┘

┌── ◉ CPU & SILICON TOPOLOGY ──────────────────┐┌── ⊞ UNIFIED MEMORY & PRESSURE ───────────────────────────────┐
│ Total Load  ████████░░░░░░░░░░░░░░░░   34.3% ││ Used RAM    ████████████████░░░░░░░░   10.3 GB / 16.0 GB  (64.5%)│
│ Load Avg    2.16 · 2.96 · 3.12  (1m/5m/15m)  ││   • App     ████████░░░░░░░░░░░░░░   5.4 GB                    │
│ History     ▅▃▂▅▇▃▂ ▂▃▅▃▂▂ ▂▃▅▇▃▂▂   30s avg ││   • Wired   ████░░░░░░░░░░░░░░░░░░   2.8 GB                    │
│                                              ││   • Compr   ██░░░░░░░░░░░░░░░░░░░░   2.1 GB  (Swap: 0 B)       │
│ Performance Cores (P0-P3):                   ││ Free Cache  █████░░░░░░░░░░░░░░░░░░   3.0 GB                    │
│  P0 ████████████░░░░ 55.0%   P1 ███████████░░░░░ 52.4%││ Memory Pressure: [====================] NORMAL (14%)       │
│  P2 ██████████░░░░░░ 47.6%   P3 █████████░░░░░░░ 41.2%│└─────────────────────────────────────────────────────────────┘
│ Efficiency Cores (E0-E3):                    │┌── ◪ BATTERY, POWER & HEALTH ─────────────────────────────────┐
│  E0 ██████░░░░░░░░░░ 28.1%   E1 █████░░░░░░░░░░░ 22.0%││ Charge Level  ████████████████████   100.0% [AC Connected ⚡] │
│  E2 ████░░░░░░░░░░░░ 18.5%   E3 ███░░░░░░░░░░░░░ 14.2%││ Maximum Cap   ██████████████░░░░░░   79.0%  (Service Recom) │
│ State: Clean (No Thermal Throttling detected)││ Health State  Service Recommended · 1,144 Cycles             │
└──────────────────────────────────────────────┘│ Power Flow    Consuming 8.2W · 35W Dual USB-C Adapter        │
┌── ▥ STORAGE & APFS VOLUMES ──────────────────┐└─────────────────────────────────────────────────────────────┘
│ Macintosh HD (APFS)                          │┌── ⇄ NETWORK ADAPTERS & TUNNELS ──────────────────────────────┐
│  Usage    ████████████████░░░░░░░░   75.2%   ││ en0 (Wi-Fi)     192.168.100.20/24  ·  AirPort BCM4387       │
│  Space    346.2 GB Used  ·  114.2 GB Free    ││                 MAC: F4:D4:88:xx:xx:xx (Hardware)            │
│  Total    460.4 GB Container Capacity        ││                 Private Wi-Fi Address: OFF (Static MAC)      │
│ Throughput:                                  ││ awdl0 (AirDrop) 02:2b:xx:xx:xx:xx  ·  Link Active (AWDL)     │
│  Read   ███░░░░░░░░░░░░░░   1.2 MB/s         ││ utun3 (VPN/TUN) 10.8.0.42          ·  WireGuard / Proxy TUN  │
│  Write  ██████░░░░░░░░░░░   4.8 MB/s         ││ Session Total   ▼ 1.84 GB RX  ·  ▲ 412 MB TX                 │
└──────────────────────────────────────────────┘└─────────────────────────────────────────────────────────────┘
┌── ✱ TOP PROCESS CONSUMERS ───────────────────────────────────────────────────────────────────────────────────┐
│   PID  NAME                 CPU %   CPU BAR             RAM MB   RAM BAR            USER       STATUS        │
│   248  WindowServer         36.0%   ████████░░░░░░░     1,420M   ███████░░░░░░░     _windowserver Normal     │
│ 59659  plugin-container     21.6%   █████░░░░░░░░░░       890M   ████░░░░░░░░░░     abdullah     Normal    │
│   412  coreaudiod           11.4%   ███░░░░░░░░░░░░       180M   █░░░░░░░░░░░░░     _coreaudio   High-Pri  │
│ 18320  wifi-tui              2.8%   █░░░░░░░░░░░░░░        35M   ░░░░░░░░░░░░░░     abdullah     Cockpit   │
│   512  Terminal / Kitty      2.1%   █░░░░░░░░░░░░░░       240M   █░░░░░░░░░░░░░     abdullah     Normal    │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
 [1..6] Select Tab  ·  [Tab] Cycle  ·  [C] Toggle Mascot  ·  [R] Refresh Hardware  ·  [Q] Quit

--------------------------------------------------------------------------------
PANEL-BY-PANEL SPECIFICATION
--------------------------------------------------------------------------------

### 1. TOP HEADER & SILICON BADGE
* **MacBook Model & Year**: Read via `sysctlbyname("hw.model")` (e.g. `Mac14,15` -> `MacBook Air (15-inch, M2, 2023)`).
* **Chip Specification**: `sysctlbyname("machdep.cpu.brand_string")` -> `Apple M2`.
* **Core Configuration**: Read via `hw.perflevel0.logicalcpu` (P-cores) & `hw.perflevel1.logicalcpu` (E-cores).
* **OS & Kernel Version**: `kern.osproductversion` (`macOS 27.0.1`) & `kern.osrelease`.
* **System Uptime**: Computed from `kern.boottime`.
* **ASCII Mascot**: Toggleable CRT Mascot (Cat / Apple Silicon chip logo).

### 2. ◉ CPU & SILICON TOPOLOGY PANEL
* **Total CPU Percentage**: Real-time measurement over 1.5s delta.
* **Per-Core Breakdown**:
  - Uses native Mach kernel call: `mach_port::host_processor_info(PROCESSOR_CPU_LOAD_INFO)`
  - Reads exact tick deltas (user, system, idle, nice) for EVERY individual core.
  - Groups them dynamically into Performance Cores (`P0..Pn`) and Efficiency Cores (`E0..En`).
* **Load Average**: `getloadavg()` (1 min, 5 min, 15 min).
* **Thermal Status**: Read via `IOPMgetThermalState` or `pmset -g therm` (Nominal / Fair / Serious).

### 3. ⊞ UNIFIED MEMORY & PRESSURE PANEL
* **Total Physical RAM**: `sysctlbyname("hw.memsize")`.
* **Breakdown via Mach `host_statistics64(HOST_VM_INFO64)`**:
  - `active_count`, `inactive_count`, `wire_count`, `compressor_page_count`, `purgeable_count`.
  - Computes:
    - **App Memory**: Active + Inactive.
    - **Wired Memory**: Frameworks and kernel allocations that cannot be paged out.
    - **Compressed Memory**: RAM compression engine savings.
    - **Cached Files**: Purgeable files cache.
    - **Free Memory**: Unallocated pages.
* **macOS Memory Pressure**: Computed from VM pageout/reclaim levels (Normal 🟢 / Warning 🟡 / Critical 🔴).

### 4. ◪ BATTERY, POWER & HEALTH PANEL (Apple Smart Battery)
* **Native Source**: `IOPowerSourcesCopyPowerSourcesInfo()` + CoreFoundation property dict.
* **Metrics**:
  - State of Charge (`CurrentCapacity / MaxCapacity` %).
  - State: `AC Connected ⚡` / `Battery Discharging` / `Fully Charged`.
  - Health Percentage: `MaxCapacity / DesignCapacity * 100` (e.g. 79%).
  - Health Verdict: `Normal` / `Service Recommended` / `Replace Soon`.
  - Cycle Count: Exact hardware cycle counter (e.g. `1,144 cycles`).
  - Real-time Wattage Draw: Instant Amperage * Voltage (e.g. `8.2W`).
  - Charger Adapter Info: Wattage (e.g. `35W Dual USB-C Port Compact Power Adapter`).

### 5. ▥ STORAGE & APFS VOLUMES
* **Root Volume**: Read via POSIX `libc::statvfs("/")`.
* **Container Metrics**: Total Capacity, Used Space, Available Space.
* **Filesystem Type**: APFS (Apple File System).
* **I/O Throughput**: Read and Write delta counters via `iostat` Mach counters or `getfsstat`.

### 6. ⇄ NETWORK ADAPTERS & TUNNELS
* **Active Adapters**:
  - Primary Wi-Fi (`en0`): IPv4 address, netmask, link state.
  - Hardware MAC vs Randomized Private MAC indicator (helps users know if MAC randomization is active).
  - Apple Wireless Direct Link (`awdl0`): AirDrop & AirPlay state.
  - Virtual Tunnels (`utun*`): WireGuard, Tailscale, Cloudflare WARP, or Proxy TUN detection.
* **Session Byte Counters**: Total RX / TX MB transferred on interface.

### 7. ✱ TOP PROCESS CONSUMERS
* **Top 5 CPU & RAM Consumers**:
  - Filtered from `procmon` / Mach task threads.
  - Displays PID, process name, CPU percentage with meter, memory usage, user, and priority.

--------------------------------------------------------------------------------
ARCHITECTURE & CODE INTEGRATION
--------------------------------------------------------------------------------

1. **New Core Engine**: `src/core/device_info.rs`
   - Zero-overhead collectors:
     - `collect_cpu_topology()` -> `CpuSnapshot` (total%, load avg, per-core P/E meters).
     - `collect_memory()` -> `MemorySnapshot` (total, app, wired, compressed, free, pressure).
     - `collect_power()` -> `PowerSnapshot` (charge%, health%, cycles, watts, charger info).
     - `collect_storage()` -> `StorageSnapshot` (APFS used/free, read/write MB/s).
     - `collect_hardware_id()` -> Cached static struct (Mac model, chip name, OS, uptime).

2. **Render State Extension**: `src/render.rs`
   - Add `pub device: DeviceRenderState` to `RenderState`.
   - Populated on Tab 6 view by worker thread.

3. **New UI View**: `src/ui/views/device.rs`
   - Responsive grid renderer following our Amber CRT design system:
     - `render_silicon_card()`
     - `render_cpu_topology()`
     - `render_memory_matrix()`
     - `render_battery_power()`
     - `render_storage_apfs()`
     - `render_network_interfaces()`
     - `render_top_processes()`
   - Seamlessly uses existing textured bars `tex_bar` and arc meters.

4. **Tab Enum Update**:
   - Add `Tab::Device = 6` to `src/app.rs`.
   - Update nav keys: `'6'` switches to Device tab.
   - Update tab list: `[1:COCKPIT] [2:SPECTRUM] [3:FIREWALL] [4:TUNING] [5:SPEEDTEST] [6:DEVICE]`.

--------------------------------------------------------------------------------
REVIEW IN NEOVIM
--------------------------------------------------------------------------------
You can open this plan in Neovim anytime to inspect the full visual layout:
  nvim DEVICE_PAGE_PLAN.md
================================================================================
