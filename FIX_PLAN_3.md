================================================================================
  WIFI-RUST  —  AIRSPACE PAGE ENHANCEMENT   (fill dead space, promote features)
================================================================================

PROBLEM (screenshot): the DETECTED NETWORKS panel is huge but only holds 2-3
APs, leaving ~55% of the page empty. The excellent Sweet-Spot Locator only lives
as a one-line status message when you press [S]. Waste of space + a great
feature buried.

GOAL: shrink the oversized networks panel, and fill the page with permanent,
always-live panels built around Sweet-Spot Locator + equally-useful RF features.
All data already exists in the engines (no invented data).

--------------------------------------------------------------------------------
NEW AIRSPACE LAYOUT  (5 panels + KPI strip)
--------------------------------------------------------------------------------
┌──────────────────────────────────────────────────────────────────────────────┐
│ KPI STRIP:  RF HEALTH · CHANNEL · WIDTH · NEIGHBORS            (unchanged)      │
├───────────────────────────────────────┬────────────────────────────────────────┤
│ SPECTRUM CONGESTION (occupancy bars)   │ ★ SWEET-SPOT LOCATOR  (NEW, always-on) │
│ per-UNII/DFS block + 2.4GHz + rec.     │  big score meter + rating + live       │
│                                        │  Signal/SNR/MCS/Retry gauges + click/s │
│                                        │  + "move laptop" guidance by trend     │
├───────────────────────────────────────┼────────────────────────────────────────┤
│ DETECTED NETWORKS (SHRUNK to fit APs)  │ RF INTERFERENCE SCANNER (NEW)          │
│ compact RSSI cards + footer            │  Microwave / Bluetooth / ACI / DFS     │
│                                        │  detection rows w/ confidence bars     │
├───────────────────────────────────────┴────────────────────────────────────────┤
│ RF SIGNAL CHRONOGRAPH (NEW, full width): Signal / SNR / Retry history graphs    │
│ tall vertical block graph of signal_dbm + SNR & retry sparklines over time      │
└──────────────────────────────────────────────────────────────────────────────┘

--------------------------------------------------------------------------------
PANEL DETAILS (every value = existing engine data)
--------------------------------------------------------------------------------
★ SWEET-SPOT LOCATOR  (acoustic_radar::calculate_quality_score)
    • Big block-digit score + wide ■-meter, colour by rating (PEAK/EXCELLENT/
      GOOD/FAIR/DEAD ZONE).
    • Live gauges: Signal dBm, SNR dB, MCS /11 (hbar_row), Retry %.
    • Geiger click-rate line (clicks/s) + interval, like the [S] locator but
      permanent and visual.
    • Movement hint: compare current signal to the recent signal_history trend
      → "↑ getting warmer (move this way)" / "↓ colder" / "• hold position".
    [S] still runs the one-shot locator action; the panel is always live.

RF INTERFERENCE SCANNER  (rf_inference: detect_microwave/bluetooth/aci/dfs)
    • One row per source: label + confidence bar (0-100%) + [DETECTED]/[clear].
    • Overall RF airspace score + the primary advice line (wrapped).
    This is the "equally useful" companion — tells you WHAT is polluting the air.

RF SIGNAL CHRONOGRAPH  (socket_sample.signal_history / snr_history / retry_history)
    • Tall vertical block graph of signal_dbm over time (dead-zone / fade view).
    • SNR sparkline + Retry sparkline beneath, with current values.
    • A one-line interpretation (stable vs fluctuating envelope).
    Fills the big empty bottom with a real time-series chart.

DETECTED NETWORKS (shrunk): fixed to ~6-7 rows (enough for the compact cards)
    so it no longer balloons; overflow scrolls by signal rank. Footer kept.

--------------------------------------------------------------------------------
SAFETY / VERIFY
--------------------------------------------------------------------------------
  • All bars/graphs sized from live inner width/height; text wrapped; short-
    terminal fallbacks; clear = spaces not ░.
  • New tests: quality-score meter proportions already covered; add a test that
    the interference scanner yields a confidence in 0..=100 per row.
  • clippy · fmt · test · release · no-panic run (cycle pages + [S]) · commit ·
    push · show in NVIM.
================================================================================
