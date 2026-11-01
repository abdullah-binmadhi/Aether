================================================================================
  WIFI-RUST  //  REDESIGN PROPOSAL v3  —  "GRAND COCKPIT"
================================================================================

This document covers three things:
  A. The three concrete fixes you asked for (tab truncation, speedtest
     animation, discoverability of key actions)
  B. The full redesign direction (layout, pages, interaction model, theme)
  C. ASCII sketches of every proposed screen

--------------------------------------------------------------------------------
PART A — THE THREE IMMEDIATE ISSUES
--------------------------------------------------------------------------------

1. WORD CONCATENATION / TRUNCATION AT TOP (your "[5:S" bug)
   Cause: the tab strip is given a fixed 58-column box, but the WIDE labels
   ([1:COCKPIT]...[5:SPEEDTEST]) need ~62 columns, so the last tab clips.
   Also the left SSID summary and the tabs share one row and fight for width.
   FIX: give the tab strip its own full-width row (its own line), measured
   exactly, and auto-shrink labels to MEDIUM/COMPACT before they ever clip.
   In the redesign the tabs become a dedicated top "ribbon" so this can't recur.

2. SPEEDTEST HAS NO ANIMATION (jumps straight to the result)
   Cause: the test runs as ONE blocking call on the worker thread; the UI only
   gets a new snapshot AFTER it finishes. The live progress %, the moving
   download/upload bars and the oscilloscope ARE being computed every 140ms —
   they're just never published mid-run.
   FIX: let the running test publish a fresh snapshot on every sample tick
   (shared Arc<Mutex<RenderState>> updated from inside the sample loop, or a
   lightweight "progress" channel the UI drains). Then you'll see the bars fill,
   the number climb, and the scope scroll in real time.

3. TOO MANY CRYPTIC HOTKEYS ([D] Doctor, [G] Boost, [O], [Z], [Y]...)
   Cause: 25+ single-letter actions, most invisible unless you read the deck.
   FIX: replace "press-a-letter-to-fire" with a MENU + TOGGLE model:
     - A single ACTION PALETTE (press Space or Enter) lists every action with a
       plain-English name, a one-line description, and its on/off state.
     - Persistent settings become TOGGLES you arrow to and flip, not fire-once
       letters.
     - The 7 presets become a visible RADIO-BUTTON row you pick from.
   Letters still work as power-user accelerators, but nobody NEEDS them.

--------------------------------------------------------------------------------
PART B — REDESIGN DIRECTION  ("grandiose but not complex")
--------------------------------------------------------------------------------

PRINCIPLES
  • One job per screen. Today Tab 1 alone shows RF + 4-hop + analysis + a
    4-mode visual deck. We split that into focused pages.
  • Every action is discoverable without a manual. Names, not letters.
  • State is always visible. You can SEE what's on/off, not guess.
  • Guided, not raw. Each page has a single "headline verdict" at the top in
    big type, then supporting detail below.
  • Keep the amber-CRT soul, but add structure: a left NAV RAIL with icons +
    labels instead of a cramped top tab strip, a persistent HEADER with the one
    number that matters (health score), and a CONTEXT BAR that changes per page.

NAVIGATION MODEL  (replaces the 5 cramped top tabs)
  A vertical NAV RAIL on the left, always visible:

        ┌──────────────┐
        │  ◉ OVERVIEW   │   <- health score, verdict, "what should I do"
        │  ◈ CONNECTION │   <- RF physics, signal, band, channel (was Pipe)
        │  ◈ LATENCY    │   <- 4-hop matrix + oscilloscope (was Flow)
        │  ◈ AIRSPACE   │   <- spectrum radar, neighbours (was Spectrum)
        │  ◈ PROCESSES  │   <- per-app firewall (was Firewall)
        │  ◈ SPEED TEST │   <- animated benchmark (was Tab 5)
        │  ⚙ TUNE & FIX │   <- toggles + action palette (was Tuning + hotkeys)
        └──────────────┘

  - Number keys 1-7 jump directly; Up/Down + Enter also works.
  - The rail shows a live mini-status glyph per page (● green / ! red) so you
    can see at a glance which area has a problem WITHOUT visiting it.

THE ACTION MODEL  (the big change)
  - "TUNE & FIX" page is a scrollable list of every capability as a ROW:
        [ ● ON  ]  Zero-Lag Mode (silence AWDL + sync daemons)      [space]
        [ ○ OFF ]  WMM VIP Airtime Priority                         [space]
        [  RUN  ]  WAN & YouTube Doctor (heals DNS/MTU/IPv6)         [enter]
        [  RUN  ]  God-Mode Boost (one-shot optimize everything)    [enter]
    Toggles show state; one-shot "RUN" actions show a spinner inline while busy.
  - A global ACTION PALETTE (press 'p' anywhere, fuzzy-filterable) for power
    users: type "doc" -> Doctor, "boo" -> Boost. VS-Code-command-palette style.

VISUAL LANGUAGE
  - Keep amber (#ffb000) as primary, add a 3-tone ramp: dim amber (chrome),
    bright amber (data), white (the single headline number).
  - Green = healthy/ON, Red = alert/problem, used sparingly.
  - Double-line borders (╔══╗) for the active panel, single (┌──┐) for others,
    so your eye is always drawn to where you are.
  - A real "gauge" widget (semicircle dial) for the health score on Overview,
    not just a number — that's the "grandiose" centerpiece.

--------------------------------------------------------------------------------
PART C — SCREEN SKETCHES
--------------------------------------------------------------------------------

── SCREEN 1: OVERVIEW (the new home page) ──────────────────────────────────────

╔═ WIFI-RUST ═══════════════════════════ Faisal.5G · 5GHz · 390 Mbps ═══════════╗
║            │                                                                   ║
║ ◉ OVERVIEW │        ╭──────────────────╮     NETWORK HEALTH                    ║
║ ◈ CONNECT  │        │       ╱───╲       │                                      ║
║ ◈ LATENCY  │        │     ╱   65  ╲     │     ●  Signal      -63 dBm  GOOD      ║
║ ◈ AIRSPACE │        │    │  /100   │    │     ●  Latency     5.9 ms   GOOD      ║
║ ◈ PROCESS  │        │     ╲ DEGRD ╱     │     !  Band        2.4→5G   FIX IT    ║
║ ◈ SPEED    │        │       ╲───╱       │     ●  Security    WPA2     OK        ║
║ ⚙ TUNE/FIX │        ╰──────────────────╯                                       ║
║            │                                                                   ║
║            │   WHAT TO DO NOW                                                  ║
║            │   ▸ Co-channel collision on Ch 132. Press ENTER to auto-fix,      ║
║            │     or open TUNE/FIX to pick a channel manually.                  ║
║            │                                                                   ║
║            │   [ Enter ] Apply recommended fix     [ p ] Action palette        ║
╚═══════════════════════════════════════════════════════════════════════════════╝

── SCREEN 2: SPEED TEST (animated — fixes your issue) ──────────────────────────

╔═ WIFI-RUST ═ SPEED TEST ══════════════════════════════════════════════════════╗
║ ◈ ...       │  RUNNING  ·  Phase 2/4: Download saturation                      ║
║ ◈ SPEED  ◀  │  ████████████████████░░░░░░░░░░░░░░░░  52%   ⠹ (live spinner)    ║
║             │                                                                  ║
║             │   ▼ DOWNLOAD   ███████████████████▌            246.5 Mbps  ↑     ║  <- bar GROWS live
║             │   ▲ UPLOAD     ████████▏                        59.2 Mbps        ║
║             │                                                                  ║
║             │   LIVE OSCILLOSCOPE          (scrolls left in real time)         ║
║             │   260┤            ▂▄▆█▇▆▅▄▃▂▁▂▄▆█    <- animates every 140ms      ║
║             │     0┤▁▁▂▃▄▅▆▇████████████████████                               ║
║             │                                                                  ║
║             │   Grade so far:  A (EXCELLENT)   ·   Loaded ping +12.7ms         ║
╚═══════════════════════════════════════════════════════════════════════════════╝

── SCREEN 3: TUNE & FIX (toggles + run actions, no cryptic letters) ────────────

╔═ WIFI-RUST ═ TUNE & FIX ══════════════════════════════════════════════════════╗
║ ⚙ TUNE  ◀   │  PRESET:  ( ) Auto-Adapt  (●) Gaming  ( ) Stream  ( ) Fortress   ║
║             │  ─────────────────────────────────────────────────────────────  ║
║             │  TOGGLES                                      STATE              ║
║             │  ▸ Zero-Lag Mode (mute AWDL + sync)          [ ● ON  ]           ║
║             │    WMM VIP Airtime Priority                  [ ○ OFF ]           ║
║             │    TCP No-Delay (gaming)                     [ ● ON  ]           ║
║             │    32MB High-BDP Buffers                     [ ● ON  ]           ║
║             │    IPv6 Link-Local Guard                     [ ○ OFF ]           ║
║             │  ─────────────────────────────────────────────────────────────  ║
║             │  ONE-SHOT ACTIONS                                                ║
║             │    ▸ Run WAN & YouTube Doctor                [ RUN ]  ⠹ busy...  ║
║             │      God-Mode Boost (optimize everything)    [ RUN ]             ║
║             │      Export ISP Diagnostic Ticket            [ RUN ]             ║
║             │                                                                  ║
║             │   ↑↓ move   Space toggle   Enter run   p palette                 ║
╚═══════════════════════════════════════════════════════════════════════════════╝

── THE ACTION PALETTE (press p anywhere) ───────────────────────────────────────

        ╔════════════════════════════════════════════════╗
        ║  ACTION PALETTE            type to filter: doc_  ║
        ║  ────────────────────────────────────────────── ║
        ║  ▸ WAN & YouTube Doctor      heal DNS/MTU/IPv6   ║
        ║    Fix macOS "!" mark        restore DHCP DNS    ║
        ║    Export Diagnostic Ticket  write .md report    ║
        ║  ────────────────────────────────────────────── ║
        ║   Enter run    Esc close                         ║
        ╚════════════════════════════════════════════════╝

--------------------------------------------------------------------------------
PART D — WHICH "AI MODEL FOR TUI DESIGN"?
--------------------------------------------------------------------------------
There isn't a separate specialized model I can switch to for TUI design — I do
the design and implementation directly here. What I CAN do is build it on proven
TUI design patterns:
  • ratatui's own widget idioms (Gauge, Tabs, List with state, Sparkline)
  • the "command palette" pattern (VS Code / Helix / lazygit)
  • a persistent nav rail + context bar (k9s, lazydocker, btop)
btop and k9s are the gold standard for "dense but legible" TUIs — the redesign
borrows their nav-rail + headline-number + context-footer structure.

--------------------------------------------------------------------------------
PROPOSED BUILD ORDER (so you can approve/stop at each step)
--------------------------------------------------------------------------------
  Step 1 (quick wins, ~now):  fix tab truncation + make speedtest animate live.
  Step 2:  add the left NAV RAIL + per-page status glyphs; split Tab 1 into
           Overview / Connection / Latency.
  Step 3:  build TUNE & FIX page with real toggles + RUN rows.
  Step 4:  add the Action Palette (press p, fuzzy filter).
  Step 5:  the health-score dial gauge on Overview + double-border active panel.

Tell me to proceed and I'll start with Step 1, then check in before each later
step so the direction stays yours.
================================================================================
