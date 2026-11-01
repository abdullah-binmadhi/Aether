================================================================================
  WIFI-RUST v3  —  WHAT CHANGED  (built this pass)
================================================================================

RUN IT:  wifi-rust        (in a terminal where the alias is loaded)

--------------------------------------------------------------------------------
1. TAB TRUNCATION ("[5:S") — FIXED
--------------------------------------------------------------------------------
The cramped top tab strip is gone. Navigation is now a LEFT NAV RAIL with full,
never-truncated labels and a live per-page status dot (● healthy / ! problem),
so you can see which area has an issue WITHOUT opening it.

--------------------------------------------------------------------------------
2. SPEEDTEST NOW ANIMATES — FIXED
--------------------------------------------------------------------------------
The test runs on its own background thread and the worker republishes ~10x/sec,
so the progress bar fills, the Mbps numbers climb, and the oscilloscope scrolls
in REAL TIME instead of jumping straight to the result. (Proven by a unit test
that samples live intermediate progress values.)

--------------------------------------------------------------------------------
3. CRYPTIC HOTKEYS -> TOGGLES & MENUS — DONE for TUNE & FIX
--------------------------------------------------------------------------------
The old "press D / G / O / Z / Y..." grid is replaced on the TUNE & FIX page by a
navigable list. Arrow ↑↓ to move, Space/Enter to flip a toggle or run an action.
Every row has a plain-English name, a one-line "what this does", and shows its
ON/OFF state. A preset RADIO row sits on top. (Letters still work as shortcuts
for power users, but nobody needs them now.)

--------------------------------------------------------------------------------
NEW LAYOUT (actual structure now)
--------------------------------------------------------------------------------

┌──────────┬──────────────────────────────────────────────────────────────────┐
│ WIFI     │ │ OVERVIEW │  ● Faisal.5G · 5GHz · 390 Mbps      HEALTH 65/100   │
│ RUST     │──────────────────────────────────────────────────────────────────│
│          │                                                                    │
│ ◉ OVERVIEW│   THE PIPE (RF)            THE FLOW (4-hop matrix)                │
│ 2 ◈ AIRSPACE ●│   SSID / band / signal    PING / jitter / loss + sparkline   │
│ 3 ◈ PROCESS ●│   SNR / link / MCS        [1] Mac  [2] GW  [3] ISP  [4] Cloud  │
│ 4 ⚙ TUNE&FIX !│                                                              │
│ 5 ⚡ SPEED  ● │   LIVE INTERPRETATION     VISUAL DECK (M / [ ] switches 4 modes)│
│          │                                                                    │
│ p palette│──────────────────────────────────────────────────────────────────│
│ q quit   │ › status message here            [Enter] palette  [R] rescan ...   │
└──────────┴──────────────────────────────────────────────────────────────────┘

TUNE & FIX page body:

  PRESET  ( ) Auto-Adapt  (●) FFXIV/LoL  ( ) 4K-STREAM  ( ) FORTRESS  ...
  ┌ TUNE & FIX ───────────────────────┐ ┌ WHAT THIS DOES ──────────────┐
  │ ── TOGGLES ──                      │ │ Zero-Lag Mode                │
  │ ▸ [ ● ON  ]  Zero-Lag Mode         │ │ Silence AWDL/AirDrop hopping │
  │   [ ○ OFF ]  WMM VIP Priority      │ │ & pause iCloud sync for 0ms  │
  │   [ ● ON  ]  25Hz CAM Pacer        │ │ Currently: ON                │
  │   [ ● ON  ]  Gaming TCP No-Delay   │ └──────────────────────────────┘
  │   ...                              │ ┌ WAN & DNS HEALTH ────────────┐
  │ ── ONE-SHOT ACTIONS ──             │ │ DNS     : 84.235.6.55 ...    │
  │   [  RUN  ]  God-Mode Boost        │ │ YouTube : LOCAL-GGC (83ms)   │
  │   [  RUN  ]  WAN & YouTube Doctor  │ │ IPv6    : Automatic          │
  │   [  RUN  ]  Export Ticket         │ │ Security: [OK] ...           │
  └────────────────────────────────────┘ └──────────────────────────────┘
      ↑↓ move   Space toggle / run

--------------------------------------------------------------------------------
STILL TO COME (your call — say the word and I'll build any of these)
--------------------------------------------------------------------------------
  • The global ACTION PALETTE overlay (press p, fuzzy-filter every action)
  • A semicircle HEALTH DIAL gauge on the Overview page (the "grandiose" piece)
  • Splitting Overview into separate CONNECTION / LATENCY pages
  • Double-line border on the active panel for extra visual hierarchy

Nav rail + header + footer, live speedtest animation, and the toggle-based
TUNE & FIX page are all LIVE now. Try `wifi-rust`, press 4 for TUNE & FIX, and
arrow through the toggles.
================================================================================
