================================================================================
  WIFI-RUST  —  PROCESSES PAGE: fill the right-side empty space
================================================================================

PROBLEM (screenshot): the right ~30% of both PER-PROCESS BANDWIDTH FIREWALL and
TOP TALKERS is empty. We can shrink both to the left column and add a tall
right-side panel with a must-have, valuable feature.

MOST-RECOMMENDED FEATURE (my pick): "LIVE CONNECTIONS (KERNEL SOCKETS)"
WHY: the whole page is about where your network goes per process. The kernel
socket matrix (already sampled every tick into rs.socket_sample.sockets) answers
the next obvious question — WHAT KIND of connections are open right now and are
any stalled? It's the natural companion to the per-process table, it's already
computed (zero new cost), and it's actionable (flags stalled SYN/CLOSE_WAIT and
ties to the existing [Z] reap-zombie-sockets action).

--------------------------------------------------------------------------------
NEW PROCESSES LAYOUT
--------------------------------------------------------------------------------
┌───────────────────────────────────────────────┬──────────────────────────────┐
│ KPI STRIP (full width): ACTIVE · TOP HOG · UP · FROZEN                         │
├───────────────────────────────────────────────┼──────────────────────────────┤
│ PER-PROCESS BANDWIDTH FIREWALL (left ~68%)      │ LIVE CONNECTIONS             │
│ PID/PROCESS/DOWN/UP/SESS/LOAD/STATE             │ (KERNEL SOCKETS)   NEW, tall │
├───────────────────────────────────────────────┤  • connection-type breakdown │
│ TOP TALKERS — SESSION DATA (MB) (left ~68%)     │    bars (proportional)       │
│ horizontal bars                                 │  • totals + health verdict   │
├───────────────────────────────────────────────┤  • [Z] reap hint             │
│ SELECTED PROCESS            │ LAN DEVICE MAP     │ (spans both middle rows)     │
└─────────────────────────────┴────────────────────┴──────────────────────────────┘

--------------------------------------------------------------------------------
LIVE CONNECTIONS PANEL  (rs.socket_sample.sockets — all existing fields)
--------------------------------------------------------------------------------
  Header:  "ACTIVE SOCKETS: N"
  Stacked/labelled horizontal bars, each length ∝ count / total_active:
     ● ESTABLISHED TCP   ███████  12
     ● HTTPS / TLS :443  █████     8
     ● UDP / QUIC :443   ████      6
     ● DNS 53/853        ██        2
     ● TIME_WAIT cache   █         1
     ● SYN / CLOSE_WAIT  ▏         0     (red if elevated)
  Then:
     LISTEN sockets: n
     VERDICT: [OK]/[!] ...   (wrapped; the engine already flags stalls)
     Hint: "[Z] reap zombie sockets" when close_wait/syn high.
  Colour: green when clean, red when syn_sent>=4 or close_wait>=8.

This is a genuinely useful, always-live diagram of your machine's live network
connections — a must-have for a bandwidth-firewall page — with an interactive
tie-in ([Z] already reaps stale sockets).

--------------------------------------------------------------------------------
SAFETY / VERIFY
--------------------------------------------------------------------------------
  • Bars sized from live inner width; verdict wrapped; clear = empty not ░.
  • Add a test: socket bar counts never exceed total_active and the panel never
    divides by zero (total_active is clamped >=1 already).
  • clippy · fmt · test · release · no-panic run (open processes page) · commit ·
    push · show in NVIM.
================================================================================
