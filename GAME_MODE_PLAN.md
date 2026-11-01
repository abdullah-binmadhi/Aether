================================================================================
  WIFI-RUST  —  AUTOMATIC GAME MODE   (anti-cheat-safe, auto ping optimizer)
================================================================================

MY OPINION (read first)
-----------------------
The investigation confirmed your instinct: almost every "optimization" wifi-tui
has (kernel sysctl writes, SIGSTOP daemon/process freezing, MTU clamp, boost,
doctor, root) is exactly the kind of thing a kernel anti-cheat like Vanguard
flags — and those lingering changes also made your League CLIENT janky. So the
single most valuable thing Game Mode can do is NOT pile on more hacks; it is:
  1) auto-REVERT risky tweaks so the game runs on clean macOS defaults,
  2) apply only the few genuinely SAFE wins,
  3) HARD-LOCK every risky mutation while a game runs,
  4) do it all automatically, zero input, and restore everything on exit.

The one low-ping trick that is BOTH safe AND effective on macOS is disabling
AirDrop/AWDL (stops the Wi-Fi radio time-slicing to Continuity every few sec =
the #1 real jitter source). It's a user-defaults write, not tampering. That is
the "trick we didn't cleanly have" — I'll add a SAFE version (no daemon freeze).

--------------------------------------------------------------------------------
STATE MACHINE  (runs every refresh tick, fully automatic)
--------------------------------------------------------------------------------
  IDLE ──(game process detected)──▶ GAME MODE ACTIVE
     • revert any risky tweaks to macOS defaults (delayed_ack=3, buffers/mss,
       MTU 1500, SIGCONT any frozen procs)         → clean baseline
     • apply SAFE wins: AirDrop/AWDL off, prewarm CDN edges, flush DNS,
       keep CAM pacer flag on
     • LOCK: block every risky action (sysctl toggles, SIGSTOP, MTU, boost,
       doctor, root, zombie-reap, DHCP renew, auto-pilot) — ignored w/ a notice
  GAME MODE ACTIVE ──(game process gone)──▶ restore AirDrop on, UNLOCK ─▶ IDLE

Detection (anti-cheat SAFE, read-only): `pgrep -x` / `pgrep -f` every tick for:
  RiotClientServices, LeagueClient, "League of Legends", LeagueClientUx,
  VALORANT, valorant, vgc, vgk, vgtray  (+ a few common ones: csgo, cs2, Dota2,
  Overwatch, steam_osx game children). Extensible list.

SAFE actions auto-applied in Game Mode (NO anti-cheat risk, all existing-ish):
  • AirDrop/AWDL disable  — `defaults write com.apple.NetworkBrowser
    DisableAirDrop -bool YES` (revert: NO on exit). NEW safe-only fn, does NOT
    SIGSTOP daemons like the old toggle_awdl.
  • Prewarm CDN edges     — kernel_tuner::prewarm_workload_edges (outbound only).
  • Flush DNS cache       — DnsEngine::flush_cache (dscacheutil + HUP).
  • CAM pacer flag ON     — pure flag, safe.

RISKY actions HARD-BLOCKED while Game Mode is active (apply_action refuses them):
  Boost, Doctor, Hop2Repair, ZombieSync, Ipv6Guard, QuicFastpath, FixExclamation,
  SafeRenew, MtuClamp, RootAuth, ToggleWmm, ToggleNoDelay, PaletteToggleBuffers,
  Prewarm(manual), process pause (SIGSTOP via Confirm on Processes tab),
  AutoPilot. Each returns: "🎮 GAME MODE — mutation blocked (anti-cheat safe)".

AUTO-REVERT on game exit AND on wifi-tui quit (teardown already unfreezes procs):
  • AirDrop back ON.
  • Any kernel tweaks pushed back to macOS defaults via the toggle(Some(false))
    paths (delayed_ack=3, buffers/mss defaults, WMM off).
  • MTU: we snapshot `networksetup -getMTU` on entry; restore it on exit (and we
    never clamp MTU inside Game Mode anyway).

--------------------------------------------------------------------------------
UI
--------------------------------------------------------------------------------
  • Header badge when active:  "🎮 GAME MODE" (green) next to HEALTH.
  • Footer/status line on transitions: "🎮 GAME MODE ON — <game> detected,
    AirDrop off, risky tweaks locked & reverted to safe defaults."
  • Nav rail shows a small controller glyph when active.
  • A new line on the TUNE & FIX page explaining Game Mode is auto + what it does.

--------------------------------------------------------------------------------
IMPLEMENTATION NOTES
--------------------------------------------------------------------------------
  • New module src/core/game_mode.rs: GAME_PROCESSES list, detect() via pgrep,
    SafeTweaks apply/revert (AirDrop defaults-write + MTU snapshot/restore).
  • App gets `game_mode: GameMode` state (Idle/Active{game, mtu_snapshot}).
    game_mode_tick() called in refresh() after enrich(), every tick.
  • apply_action() guards risky arms with `if self.game_mode.active() { block }`.
  • RenderState carries `game_mode_active: bool` + `game_name: String` for the UI.
  • All detection/revert is read-only or user-defaults — anti-cheat SAFE. The
    one revert that needs sudo (sysctl defaults) only runs if a tweak was active;
    it's the SAME revert we ran by hand, restoring macOS defaults.
  • Tests: detection name-matching; risky actions are no-ops while active; safe
    set contains no SIGSTOP/sysctl-write call.

VERIFY: clippy · fmt · test · release · no-panic · commit · push · show in NVIM.
================================================================================
