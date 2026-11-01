================================================================================
  WIFI-RUST  —  FIX PLAN  (word-concat + dead-space fill)   for approval/record
================================================================================

From your 4 screenshots I found 3 concrete bugs + 3 dead-space panels. Here is
exactly what I will change and why, data-accurate (every new visual binds to a
field the app already produces).

--------------------------------------------------------------------------------
A. WORD-CONCATENATION / TRUNCATION / WEIRD-GLYPH BUGS  (correctness first)
--------------------------------------------------------------------------------
A1. 4-HOP PATH — "Google Ed" / "Google Edge" cut off.
    CAUSE: hop label is hard-truncated to 9 chars (`.take(9)`), left over from
    when the panel was narrow. Now the panel is wide.
    FIX: widen the label field to 12 and size the bar from the REAL inner width
    (not a guessed constant), so "Google Edge" shows in full and the bar still
    fits. No truncation unless the terminal is genuinely tiny.

A2. 4-HOP PATH — "VERDICT: ... Zero Bufferbl" cut off at the right edge.
    CAUSE: the verdict string (`scope_fault`) is printed on one line with no
    wrap, so ratatui clips it.
    FIX: word-wrap the verdict across as many lines as needed (there's plenty of
    vertical room in that panel now). Same wrap helper the advice tile uses.

A3. WHAT TO DO NOW — the arc gauge renders as tiny broken ticks
    (``````·· 65/100).
    CAUSE: `arc_gauge` draws a "semicircle" with the ◝ glyph; at 2 rows it
    collapses into a couple of stray marks that read as garbage, not a gauge.
    FIX: replace that header with a CLEAN, legible gauge that can't look broken:
      • a bold bracketed progress meter  [█████████░░░░░░]  65/100  DEGRADED
      • rendered with the proven `bar()`/block glyphs, colour-graded
        (green ≥72 / amber ≥50 / red <50)
      • plus a 3-row BIG-DIGIT "65" using block numerals so it still feels like a
        dashboard gauge, but every cell is a solid block (never a stray tick).
    I will DROP the fragile ◝ arc entirely (it's the thing "looking weird").

A4. Speed-test phase line — "[!] Apple RPM finished ... (networkQuality
    unavailable)" is fine, but I'll make sure the phase bar + text can't collide
    by giving the text its own width budget. (Minor; verified while here.)

--------------------------------------------------------------------------------
B. SPEED TEST — "REAL-TIME THROUGHPUT & LOADED PING OSCILLOSCOPE" (huge dead space)
--------------------------------------------------------------------------------
Today: 3 one-line sparklines at the top, ~90% of the panel empty (screenshot 3).
DATA AVAILABLE: down_history / up_history / rtt_history each hold 56 samples.

FIX — turn it into a real stacked oscilloscope that fills the height:
  • TOP band  (≈55% height): DOWNLOAD throughput as a tall multi-row vertical
    block graph (reuse render_2d_vertical_graph) with a Mbps Y-axis. Big, bold.
  • MID band  (≈30% height): UPLOAD throughput as its own shorter vertical graph.
  • BOTTOM band(≈15%)       : LOADED-PING RTT as a full-width sparkline with a
    left "RTT" axis label + current value on the right.
  Each band keeps a header line (▼ DOWN 188M / ▲ UP 60M / ◆ RTT 6ms) so numbers
  stay readable. Result: the whole panel is live graph, no empty bottom.

--------------------------------------------------------------------------------
C. AIRSPACE — "5GHz & 2.4GHz SPECTRUM CONGESTION" (dead space)
--------------------------------------------------------------------------------
Today: a 6-row table, lots of empty space below (screenshot 4).
DATA: blocks_5g[] (per UNII/DFS block: count, max_rssi, mine, colliding),
count_24 / max_rssi_24, recommended channel + reason.

FIX — add a CHANNEL-OCCUPANCY BAR CHART beneath the table that fills the room:
  • One horizontal bar PER 5GHz block (UNII-1, DFS-Low, DFS-Mid, UNII-3) + a
    2.4GHz row, bar length ∝ neighbour count, colour = clear(green) /
    occupied(amber) / YOUR-collision(red), with the dBm floor on the right.
  • A "◆ RECOMMENDED: Ch NN — reason" banner line, bold green, as the panel's
    bottom anchor.
  This visually answers "which block is congested + where should I move" at a
  glance and consumes the dead space with a chart, not filler.

--------------------------------------------------------------------------------
D. AIRSPACE — "DETECTED LOCAL WI-FI NETWORKS & RF SIGNATURES" (dead space)
--------------------------------------------------------------------------------
Today: a short list (active AP + a few neighbours), empty below.
DATA: telemetry.neighbors[] (label, channel, band, signal_dbm, same_channel).

FIX — make it a SIGNAL-STRENGTH RANK CHART that fills the panel:
  • Sort neighbours by signal (strongest first), one row each:
      ● SSID.............  Ch132 5GHz  [███████████░░░]  -63 dBm
    bar ∝ RSSI (-95..-40), YOUR AP in bold, co-channel APs in red.
  • Show as many rows as the panel height allows (was capped low).
  • A one-line footer: "N networks · M on your channel · strongest -63 dBm".
  Fills the space with a readable chart and makes co-channel clashes obvious.

--------------------------------------------------------------------------------
E. SAFETY RAILS (so nothing concatenates / disappears / looks weird again)
--------------------------------------------------------------------------------
  • Every new bar/graph is sized from the panel's ACTUAL inner width/height at
    draw time (never a magic constant), and clamped to a safe minimum, so
    resizing can't clip or overlap text.
  • Every free-text line that can be long (verdict, recommendation, interp) is
    word-wrapped, never hard-cut mid-word.
  • A short-terminal fallback: if a panel is too short for the tall graph, it
    degrades to the compact sparkline version instead of drawing garbage.
  • Add unit tests: vertical graph returns the exact requested row count at
    several heights; neighbour sort is descending; no label is emitted longer
    than its column.

--------------------------------------------------------------------------------
VERIFY / SHIP
--------------------------------------------------------------------------------
  cargo clippy --all-targets · cargo fmt · cargo test · cargo build --release ·
  run binary + no-panic check · commit · push · show this file in NVIM.

Say the word if you want B/C/D laid out differently; otherwise I'm implementing
A→E now.
================================================================================


================================================================================
  ✅ ALL FIXES BUILT & PUSHED  (commit 8e3362c)
================================================================================
A1 ✓ 4-HOP PATH label widened to 12 — "Google Edge" shows in full; bar sized
     from the real panel width so it never clips.
A2 ✓ VERDICT line is word-wrapped — no more "Zero Bufferbl" cut-off.
A3 ✓ The broken ```` ·· arc gauge is GONE. Replaced with big_number (3-row solid
     block digits) + score_meter (solid bracketed meter). Every cell is a block
     or a space — it can never render as stray ticks again.
B  ✓ SPEED TEST oscilloscope now fills the panel: tall DOWNLOAD vertical graph
     (~55%), UPLOAD vertical graph (~32%), full-width LOADED-RTT sparkline.
     (<8 rows → compact 3-sparkline fallback, never garbage.)
C  ✓ AIRSPACE congestion: per-block occupancy BAR CHART (bar ∝ neighbour count,
     colour = clear/occupied/collision) + wrapped recommendation banner.
D  ✓ AIRSPACE detected networks: signal-strength RANK CHART, strongest first,
     RSSI bars, your AP bold, co-channel red, + summary footer; uses every row.
E  ✓ Safety rails: all bars/graphs sized from live inner width/height; long
     text wrapped; short-panel fallbacks; new tests for digit shape, meter
     proportions, and vertical-graph row counts at several heights.

VERIFIED: clippy clean · fmt clean · 20 tests pass · release builds · no panic
cycling all pages + a quick speed test.
================================================================================
