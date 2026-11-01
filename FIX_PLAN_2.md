================================================================================
  WIFI-RUST  —  FIX PLAN #2   (dead space round 2 + m-key + concat)
================================================================================

From the 4 new screenshots:

--------------------------------------------------------------------------------
1. AIRSPACE — "DETECTED LOCAL WI-FI NETWORKS" ~60% empty; congestion bars носий
   dim-fill noise on 0-neighbour blocks.
--------------------------------------------------------------------------------
FIX:
  • DETECTED NETWORKS: there are only 3-4 APs, so a flat list can't fill a tall
    panel. Instead make each AP a TALLER 2-line card:
        ● Faisal.5G                    Ch132 5GHz 80MHz   -65 dBm  (YOU)
          [████████████░░░]  SNR 28dB · 1024-QAM-capable · co-channel: none
    i.e. name/clip line + a full-width RSSI bar line with extra RF detail.
    With 2 lines per AP + spacing it fills far more of the panel, and still
    degrades (1 line each) when there are many APs.
  • Add a compact "2.4GHz ░░░  5GHz ███" band-split summary bar at the bottom.
  • CONGESTION bars: drop the dim "░" trail on 0-count blocks — a clear block
    should read as empty (just the label + [OK] CLEAR), not a full dim bar.
    Only draw the solid portion; pad with spaces, not ░, so "clear" looks clear.
  • Give both panels a taller share (congestion ~10, networks = rest).

--------------------------------------------------------------------------------
2. OVERVIEW — pressing [M] does nothing visible; WHAT TO DO NOW gauge cramped
   + empty bottom.
--------------------------------------------------------------------------------
ROOT CAUSE: the redesign removed the Visual Telemetry Deck from Overview, but
[M] still just flips an invisible `visual_mode` and prints a status line — so it
looks dead.
FIX:
  • Repurpose [M] on OVERVIEW to switch the big LATENCY OSCILLOSCOPE between
    views the user can actually see:
        mode 1: RTT latency graph (current)
        mode 2: DOWNLOAD throughput graph
        mode 3: UPLOAD throughput graph
        mode 4: JITTER graph
    Reuse `visual_mode` (already toggled by M / [ / ]); the oscilloscope reads it
    and swaps which history it plots + its title/axis. Now [M] has an obvious
    effect on Overview, and the status line matches what changed.
  • WHAT TO DO NOW: tighten the big-number gauge so row-2 digits and the
    "65/100 DEGRADED" text don't crowd; put the meter on its own line above the
    digits, then the action text, and let it wrap to consume the empty bottom.

--------------------------------------------------------------------------------
3. PROCESSES — ~50% empty below the table; LOAD bar renders as a dim grey block.
--------------------------------------------------------------------------------
FIX:
  • The LOAD column: stop drawing the ░ remainder (near-zero loads look like one
    grey rectangle). Use a short solid bar that's empty when load≈0, so idle
    processes show (almost) nothing and a real hog stands out.
  • Cap the table to the actual process count (no trailing blank rows inside the
    block), and ADD a new tile row that fills the freed space:
        "TOP TALKERS" — a horizontal bar chart of the top-N processes by total
        session MB (or live KB/s), bar ∝ value, so the panel shows a real chart
        instead of empty space.
  • Keep the SELECTED PROCESS + LAN DEVICE MAP tiles, but make the middle
    TOP-TALKERS chart absorb the dead space (table shrinks to fit rows).

--------------------------------------------------------------------------------
4. SPEED TEST — "BOTTLENECK INTERPRETATION & VERDICT" concat/cut mid-word
   ("390 Mbps Wi", "capacity above W", "stream 4K w").
--------------------------------------------------------------------------------
FIX: word-wrap every interpretation line to the panel width (same wrap helper).
  Also word-wrap the WORKLOAD READINESS rows if they overflow. No hard cuts.

--------------------------------------------------------------------------------
SAFETY / VERIFY
--------------------------------------------------------------------------------
  • Every bar/graph sized from live inner width/height; all free text wrapped;
    clear = spaces not ░.
  • cargo clippy · fmt · test · build --release · no-panic run cycling pages +
    pressing M on Overview + a quick speed test · commit · push · show in NVIM.
================================================================================


================================================================================
  ✅ ALL FIXES BUILT & PUSHED  (commit 5bd9249)
================================================================================
1 ✓ AIRSPACE congestion: clear blocks now show an empty track (spaces), not a
    dim ░ bar — clear reads as clear. Detected networks → 2-line RSSI cards
    (name/channel + full-width signal bar + quality word) that fill the tall
    panel; collapse to 1 line each when crowded + summary footer.
2 ✓ [M] on OVERVIEW now visibly switches the oscilloscope: LATENCY → DOWNLOAD →
    UPLOAD → COMBINED. Status line renamed to match. WHAT TO DO NOW gauge
    re-laid-out (meter on its own line, big digits below, no crowding) and the
    analysis text wraps to fill the panel's empty bottom.
3 ✓ PROCESSES: LOAD bar is solid-only (idle rows ≈ empty, hogs stand out);
    table sized to the real process count; NEW "TOP TALKERS" session-MB bar
    chart fills the freed space; SELECTED PROCESS + LAN MAP tiles below.
4 ✓ SPEED TEST: BOTTLENECK INTERPRETATION + WORKLOAD rows word-wrapped — no
    more "390 Mbps Wi" / "capacity above W" / "stream 4K w" cut-offs; bottom
    row given more height for the wrapped text.

VERIFIED: clippy clean · fmt clean · 20 tests pass · release builds · no panic
cycling [M] oscilloscope views + all pages + a quick speed test.
================================================================================
