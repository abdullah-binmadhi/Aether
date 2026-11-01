================================================================================
  WIFI-RUST  —  fill LIVE CONNECTIONS bottom + enlarge RF QUALITY / 4-HOP PATH
================================================================================

THREE tasks from the two screenshots:

--------------------------------------------------------------------------------
1. LIVE CONNECTIONS panel — ~65% empty below the socket list.
--------------------------------------------------------------------------------
ADD a 2nd must-have feature in the same panel's lower half:
  ★ TRAFFIC ENCRYPTION POSTURE  (derived from the socket counts we already have)
    • A big ENCRYPTED-vs-OTHER ratio: encrypted = HTTPS:443 + QUIC/UDP:443 +
      DNS(53/853); total established-ish = encrypted + other_ports.
    • Score meter + % + verdict ("X% of app traffic is encrypted").
    • A protocol MIX mini bar-chart: HTTPS / QUIC / DNS / other, each a labelled
      bar — a second angle on the same live sockets.
    • A one-line security read ("All app streams on TLS/QUIC — safe on public
      Wi-Fi" vs "N plaintext streams — avoid logins on open networks").
  This is the natural, genuinely-useful second lens on live connections, costs
  nothing new (same rs.socket_sample.sockets), and fills the empty bottom.

--------------------------------------------------------------------------------
2. RF QUALITY (Overview) — short gauges, tall empty panel.
--------------------------------------------------------------------------------
ENLARGE: keep the 5 labelled gauges, then add a tall RF time-series beneath them
that fills the panel:
  • A multi-row vertical block graph of signal_dbm over time (socket_sample
    .signal_history) with a dBm Y-axis — the "are you in a fading spot" view.
  • A SNR sparkline + Retry sparkline row with current values.
  Graph height computed from the panel's real inner height minus the gauge rows,
  so it always fits exactly (no overflow, nothing clipped).

--------------------------------------------------------------------------------
3. 4-HOP PATH (Overview) — short bars, tall empty panel.
--------------------------------------------------------------------------------
ENLARGE: keep the per-hop bars + wrapped verdict, then add a tall RTT history
time-series beneath:
  • Multi-row vertical block graph of scope_rtt_history (the latency-over-time
    view) sized to the leftover panel height.
  • Keep the VERDICT wrapped at the bottom.
  So the panel shows per-hop bars AND how total latency has behaved over time.

--------------------------------------------------------------------------------
NO-CONCAT / FIT GUARANTEE
--------------------------------------------------------------------------------
  • Every added graph's row count = inner.height − (fixed rows already drawn),
    clamped ≥2, so it can never overflow or push content out of the panel.
  • Bars/graphs sized from live inner width; all free text wrapped; clear = space.
  • Short-panel fallback: if leftover height <2 rows, the time-series is simply
    omitted (gauges/bars alone), never drawn as garbage.
  • Tests already cover vertical-graph row counts at many heights.

VERIFY: clippy · fmt · test · release · no-panic (Overview + Processes) ·
commit · push · show in NVIM.
================================================================================


================================================================================
  ✅ BUILT & PUSHED  (commit 9851820)
================================================================================
1 ✓ LIVE CONNECTIONS lower half → ★ TRAFFIC ENCRYPTION POSTURE: encrypted %
    meter (HTTPS+QUIC+DoT vs plaintext) + protocol-mix bars + a plain-English
    safety read ("All app streams on TLS/QUIC — safe on public Wi-Fi" vs
    "N plaintext stream(s) — avoid logins on open networks"). Red if plaintext>0.
2 ✓ RF QUALITY: 5 gauges + a tall SIGNAL-OVER-TIME vertical graph + SNR/Retry
    sparklines fill the panel.
3 ✓ 4-HOP PATH: per-hop bars + a tall LATENCY-OVER-TIME graph + wrapped verdict.

FIT GUARANTEE: every added graph's row count = panel inner height − fixed rows,
clamped ≥2, so it fits exactly; omitted entirely when the panel is too short.
Nothing concatenates or disappears.

VERIFIED: clippy clean · fmt clean · 22 tests pass · release builds · no panic
cycling Overview + Processes.
================================================================================
