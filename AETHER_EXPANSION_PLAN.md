# ⚡ AETHER EXPANSION PLAN: TABS [8], [9], AND [10]
## Architecture, Backend Probes, and ASCII Cockpit UI Sketches

---

## 1. Executive Overview

Aether has evolved from a Wi-Fi cockpit into a full **Host Silicon, Hardware, Network, and Maintenance Cockpit**. To complete its vision as the ultimate command center, this plan details the architecture, native platform probes, and exact amber CRT visual sketches for the **Top 3 Recommended Systems**:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            AETHER GRAND COCKPIT                             │
├───────────────┬─────────────────────────────┬───────────────────────────────┤
│ EXISTING TABS │  Tab [1] Cockpit / RF       │  Tab [5] SpeedTest / RPM      │
│               │  Tab [2] Spectrum Airspace  │  Tab [6] Silicon / Hardware   │
│               │  Tab [3] Process Firewall   │  Tab [7] Clean & Optimize     │
│               │  Tab [4] Kernel Tuning Lab  │                               │
├───────────────┼─────────────────────────────┴───────────────────────────────┤
│  NEW EXPANSION│  Tab [8] ⌖ LAN Radar & Subnet Device Map                    │
│    MODULES    │  Tab [9] ⚔ Gaming & Game Server Ping Radar                  │
│               │  Tab [0] 🛡 Privacy Fortress & DNS Racer                    │
└───────────────┴─────────────────────────────────────────────────────────────┘
```

---

## 2. Tab [8] — LAN Radar & Subnet Device Map

### 2.1 Core Purpose
Discovers, profiles, and tracks every physical host on the local network (Wi-Fi and Ethernet). Identifies hardware vendor silicon via IEEE OUI MAC translation, alerts on rogue intruders or ARP spoofing attacks, and probes open ports.

### 2.2 Native Backend Probes (Zero Hardcoding)
- **Host Discovery**:
  - Broadcast unprivileged ICMP echo sweep over `/24` subnet.
  - Native ARP cache inspection (`/proc/net/arp` on Linux, `sysctl net.link.ether` on macOS).
  - mDNS / Bonjour reverse query via UDP 5353 (discovering names like `Faisal-iPhone.local`, `LivingRoom-AppleTV.local`, `PS5-Console.local`).
- **Silicon Identification (OUI Engine)**:
  - In-memory embedded Trie of top IEEE OUIs: `Apple`, `Sony Interactive`, `Espressif` (Smart Home), `Raspberry Pi`, `Nintendo`, `Intel`, `Samsung`, `LG Electronics`, `Amazon Tech`.
- **Security Watcher**:
  - Detects duplicate MACs (ARP cache poisoning / Man-In-The-Middle attacks).
  - Flags unknown MAC addresses joining the subnet.

### 2.3 ASCII Cockpit Layout Sketch (Tab 8)

```text
╔═ AETHER ═ LAN RADAR & SUBNET DEVICE MAP ══════════════════════ 192.168.100.0/24 ═╗
║ [ ACTIVE HOSTS ]   [ GATEWAY RTT ]   [ SECURITY POSTURE ]   [ LOCAL BROADCAST ]  ║
║   14 Hosts Online     1.8 ms (CLEAN)   [SECURE] Zero Spoof    2.4% Subnet Load   ║
╠═════════════════════════════════════════════╤════════════════════════════════════╣
║ ⌖ LOCAL SUBNET HOSTS (14 Detected)          │ ◉ HOST INSPECTOR: 192.168.100.14   ║
║ IP ADDRESS     HOSTNAME           HARDWARE  │ Name    : Apple TV 4K (Living Room)║
║─────────────────────────────────────────────│ MAC     : a4:83:e7:29:bb:12        ║
║ 192.168.100.1  router.home [GW]   Huawei GW │ Vendor  : Apple Inc. (A15 Bionic)  ║
║ 192.168.100.2  aether-mac  [YOU]  Apple M2  │ Signal  : -52 dBm (5GHz Ch 36)     ║
║ 192.168.100.4  Faisal-iPhone.local Apple A17│ Ping    : 3.2 ms (Jitter 0.4ms)    ║
║ 192.168.100.7  PS5-LivingRoom     Sony Ent. │ Status  : Streaming (AirPlay/HomeKit║
║ 192.168.100.9  Desk-Lamp.local    Espressif │────────────────────────────────────║
║ 192.168.100.12 Work-ThinkPad      Intel Corp│ OPEN SERVICES & PORTS              ║
║▶192.168.100.14 AppleTV-Living     Apple TV  │  • Port 80    : HTTP AirPlay Server║
║ 192.168.100.18 LG-OLED-TV         LG WebOS  │  • Port 443   : TLS Secure Stream  ║
║ 192.168.100.22 HomePod-Mini       Apple S5  │  • Port 5353  : mDNS Discovery     ║
║ 192.168.100.25 Switch-OLED        Nintendo  │  • Port 7000  : AirPlay Mirroring  ║
║ 192.168.100.31 Ring-Doorbell      Amazon Lab│────────────────────────────────────║
║ 192.168.100.44 Dyson-Purifier     Espressif │ ◪ SUBNET INTEGRITY & ROGUE LOG     ║
║─────────────────────────────────────────────│  [OK] Gateway ARP baseline verified║
║ [↑/↓] Select host   [P] Port scan   [R] Rescan│  [OK] No duplicate IP collisions   ║
║ [Space] Mark trusted   [Enter] Ping traceroute│  [OK] Zero rogue DHCP offers       ║
╚═════════════════════════════════════════════╧════════════════════════════════════╝
```

---

## 3. Tab [9] — Gaming & Game Server Ping Radar

### 3.1 Core Purpose
Provides zero-lag routing intelligence for competitive online gamers. Shows real-time millisecond latency, packet jitter, and hop loss to game server clusters (*Valorant, CS2, Final Fantasy XIV, League of Legends, Apex Legends, Fortnite*) before starting a match. Locks out background macOS AWDL ping spikes and enforces Anti-Cheat-safe QoS prioritization.

### 3.2 Native Backend Probes
- **Game Server Cluster Probes**:
  - Live UDP and ICMP socket probing to real regional datacenters:
    - **Bahrain / Middle East**: AWS `me-south-1` (Valorant, Apex, CS2)
    - **Frankfurt / Europe**: AWS `eu-central-1` / Riot EU Central (Valorant, LoL, CS2)
    - **London / Europe**: AWS `eu-west-2` (FFXIV Chaos / Light Datacenters)
    - **Tokyo / Asia**: AWS `ap-northeast-1` (FFXIV Mana, Apex Tokyo)
    - **US-East / North America**: AWS `us-east-1` (FFXIV Aether, CoD, Fortnite)
- **AWDL Anti-Stutter Shield**:
  - Actively mutes Apple Wireless Direct Link (AWDL) frequency hopping while gaming mode is active. This permanently eliminates the infamous periodic 100ms lag spikes caused by AirDrop background scanning.
- **Vanguard & Anti-Cheat Safe QoS**:
  - Zero DLL injection or memory tampering. Uses standard POSIX socket TOS / IP_TOS flags (`0x60` WMM Video / `0x80` WMM Voice) and local UDP port prioritizing.

### 3.3 ASCII Cockpit Layout Sketch (Tab 9)

```text
╔═ AETHER ═ GAMING & GAME SERVER PING RADAR ═══════════════ ESPORTS TELEMETRY ═╗
║ [ BEST REGION ]    [ JITTER STABILITY ]   [ AWDL LAG SHIELD ]   [ ANTI-CHEAT SAFE ]║
║  Bahrain (12 ms)    ±0.3 ms (ROCK SOLID)   [LOCKED] 0Hz AWDL     [ACTIVE] Riot Safe║
╠═════════════════════════════════════════════╤════════════════════════════════════╣
║ ⚔ REGIONAL GAME CLUSTER PING MATRIX         │ ◉ ACTIVE GAME OPTIMIZER & PACING   ║
║ CLUSTER / REGION     PING   JITTER  LOSS    │ Profile : Riot Games & FFXIV Mode  ║
║─────────────────────────────────────────────│ Engine  : UDP High-Priority (VIP)  ║
║ ▼ VALORANT / RIOT                           │ WMM     : 0x80 Voice VIP Queue     ║
║  • Bahrain (ME)      12 ms   ±0.2ms   0.0%  │ AWDL    : Fully Suspended (0 spikes)║
║  • Dubai (UAE)       18 ms   ±0.4ms   0.0%  │ TCP Pkt : NODELAY + Fast-ACK Active║
║  • Frankfurt (EU)    68 ms   ±0.6ms   0.0%  │────────────────────────────────────║
║  • London (UK)       74 ms   ±0.9ms   0.0%  │ ▦ 60-SECOND JITTER SPARKLINE       ║
║  • Tokyo (JP)       142 ms   ±1.8ms   0.0%  │ 15ms ─┐                            ║
║ ▼ FINAL FANTASY XIV                         │ 12ms ─┼────────────────────────────║
║  • Chaos (EU - Fra)  69 ms   ±0.5ms   0.0%  │  9ms ─┤ (Dead flat - Zero Spikes)  ║
║  • Light (EU - Gbr)  75 ms   ±0.8ms   0.0%  │  6ms ─┤                            ║
║  • Aether (NA - Sac)178 ms   ±2.1ms   0.0%  │  3ms ─┘                            ║
║ ▼ COUNTER-STRIKE 2                          │────────────────────────────────────║
║  • Dubai Server      16 ms   ±0.3ms   0.0%  │ ◪ ROUTE HOP ANALYSIS (Hop 1-4)     ║
║  • Vienna Central    64 ms   ±0.7ms   0.0%  │  Hop 1 [Local Mac]  : 0.1ms (Clean)║
║ ▼ APEX LEGENDS / EA                         │  Hop 2 [Gateway]    : 1.8ms (Clean)║
║  • Bahrain GCE       14 ms   ±0.4ms   0.0%  │  Hop 3 [ISP Fiber]  : 5.4ms (Clean)║
║  • Frankfurt GCE     67 ms   ±0.6ms   0.0%  │  Hop 4 [Game Edge]  : 12.1ms (VIP) ║
║─────────────────────────────────────────────│  Verdict: [S-TIER] OPTIMAL FOR COMP║
║ [Space] Toggle Game Shield   [G] Filter Game│ Anti-Cheat: Riot Vanguard Verified ║
╚═════════════════════════════════════════════╧════════════════════════════════════╝
```

---

## 4. Tab [0] (10) — Privacy Fortress & DNS Racer

### 4.1 Core Purpose
Provides deep visibility into outbound telemetry and tracking calls made by background apps, benchmarks all major public DNS providers in 1 keypress, applies the fastest resolver instantly, and monitors VPN/WireGuard encrypted tunnels.

### 4.2 Native Backend Probes
- **1-Key DNS Racer Engine**:
  - Concurrently queries root and domain records against:
    - `Cloudflare`: `1.1.1.1` & `1.0.0.1` (DNSSEC + DoH)
    - `Google`: `8.8.8.8` & `8.8.4.4` (DNSSEC)
    - `Quad9`: `9.9.9.9` & `149.112.112.112` (Malware Block + Privacy)
    - `NextDNS`: Custom encrypted endpoint
    - `AdGuard`: `94.140.14.14` (Tracker & Ad blocking)
    - `ISP Gateway`: Default local DHCP resolver
  - Measures 5-query average RTT, standard deviation, and ECS privacy leakage.
- **Background App Telemetry Sniffer**:
  - Reads active socket endpoints via `lsof` / native Mach kernel socket tables.
  - Identifies which app PIDs are connecting to tracking domains (e.g., Adobe telemetry, Microsoft telemetry, Google Analytics, telemetry daemons).
- **Encrypted DNS & Tunnel Watcher**:
  - Detects active WireGuard / OpenVPN (`utun*`) interfaces.
  - Verifies if DNS is leaking outside the encrypted tunnel.

### 4.3 ASCII Cockpit Layout Sketch (Tab 10 / [0])

```text
╔═ AETHER ═ PRIVACY FORTRESS & DNS RACER ══════════════════ ENCRYPTED SECURITY ═╗
║ [ ACTIVE RESOLVER ]   [ DOH ENCRYPTION ]   [ TRACKERS BLOCKED ]  [ VPN TUNNEL ]   ║
║  Cloudflare (3.8ms)    [ENCRYPTED] DoH      42 Trackers Halted    utun3 [WireGuard]║
╠═════════════════════════════════════════════╤════════════════════════════════════╣
║ ⚡ 1-KEY DNS RACER & RESOLVER MATRIX        │ ◉ APP TELEMETRY & CALL-HOME SNIFFER║
║ RESOLVER       IP / ENDPOINT    RTT   GRADE │ APP / PROCESS   PID   DESTINATION  ║
║─────────────────────────────────────────────│────────────────────────────────────║
║▶Cloudflare     1.1.1.1          3.8ms [A+]  │ Spotify.app     4821  telemetry.spo║
║ Google DNS     8.8.8.8          7.2ms [A]   │ Chrome.app      1204  google-analyt║
║ Quad9 Secure   9.9.9.9          8.1ms [A]   │ Adobe Creative  8912  adobesc.data.║
║ NextDNS        45.90.28.0      11.4ms [A-]  │ macOS launchd    1   metrics.apple║
║ AdGuard DNS    94.140.14.14    14.2ms [B+]  │ Discord.app     5120  discord-gatew║
║ ISP Default    192.168.100.1   18.6ms [C]   │ Slack.app       3310  slack-edge.co║
║─────────────────────────────────────────────│────────────────────────────────────║
║ Features: DoH Encrypted · DNSSEC · No-Logs  │ ◪ ENCRYPTED TUNNEL & LEAK SHIELD   ║
║ [Space] Apply Cloudflare 1.1.1.1 as System  │  • Tunnel   : utun3 (WireGuard)    ║
║ [R] Re-race resolvers   [F] Flush local DNS │  • Assigned : 10.64.0.2/32         ║
║─────────────────────────────────────────────│  • DNS Leak : [PROTECTED] Zero Leak║
║ Security: DNS Hijack Protection ACTIVE      │  • MTU Check: 1420 Bytes (Optimal) ║
║ Privacy Score: 96/100 (Hardened Profile)    │  • Sessions : RX 840MB · TX 192MB  ║
╚═════════════════════════════════════════════╧════════════════════════════════════╝
```

---

## 5. Navigation & Keyboard Layout

With 10 tabs total, the keyboard mapping is intuitive and uses standard number keys:

| Key | Tab ID | Nav Rail Display | Module Description |
|:---:|:---:|:---|:---|
| `[1]` | 1 | `◉ DASH` | Wi-Fi RF telemetry, signal, 4-hop matrix, link health |
| `[2]` | 2 | `◈ AIR` | 2.4/5/6 GHz spectrum radar, interference, sweet spot |
| `[3]` | 3 | `▤ PROC` | Live per-process bandwidth firewall & QoS shaping |
| `[4]` | 4 | `⚙ TUNE` | Kernel optimizer lab, WMM VIP, TCP no-delay |
| `[5]` | 5 | `▲ TEST` | Bufferbloat multi-stream benchmark, Apple RPM |
| `[6]` | 6 | `◆ DEVC` | Dynamic host hardware cockpit (M1/M2/M3/M4/PC) |
| `[7]` | 7 | `🧹 OPT` | RAM purge, SSD snapshot cleanup, app uninstaller |
| **`[8]`** | **8** | **`⌖ LAN`** | **Subnet device map, OUI silicon, rogue intruder alert** |
| **`[9]`** | **9** | **`⚔ GAME`**| **Esports ping radar, AWDL stutter shield, Vanguard safe**|
| **`[0]`** | **10**| **`🛡 PRIV`**| **Privacy fortress, 1-key DNS racer, DoH, app sniffer** |

---

## 6. Implementation Roadmap

### Phase 1: Tab [8] LAN Radar & Device Map
1. **Engine**: Implement `src/core/lan_radar.rs` with ARP cache scanner, ICMP subnet ping, and mDNS responder querying.
2. **OUI Database**: Embed lightweight vendor prefix table (Apple, Sony, Intel, Espressif, Samsung, LG).
3. **UI**: Create `src/ui/views/lan_radar.rs` rendering the 2-column radar table and host inspector.

### Phase 2: Tab [9] Gaming & Game Server Ping Radar
1. **Engine**: Implement `src/core/gaming_radar.rs` with regional game cluster endpoints (AWS ME, EU, NA, JP, Riot, FFXIV, Valve, EA).
2. **AWDL Pacer**: Wire `antilag.toggle_zero_lag(true)` to automatically engage when entering Game Mode.
3. **UI**: Create `src/ui/views/gaming.rs` with live ping matrix, jitter sparkline, and Vanguard safe status indicator.

### Phase 3: Tab [0] Privacy Fortress & DNS Racer
1. **Engine**: Implement `src/core/dns_racer.rs` with concurrent asynchronous resolver benchmarking and socket sniffer.
2. **System Resolver**: Connect `networksetup -setdnsservers` for instant 1-key DNS switching.
3. **UI**: Create `src/ui/views/privacy.rs` with DNS racer scorecard, live app tracker log, and WireGuard tunnel telemetry.
