//! Automatic, anti-cheat-SAFE Game Mode.
//!
//! Detects when a game (League / Riot / Vanguard / Valorant / common titles) is
//! running and — with zero user input — (1) applies only anti-cheat-safe network
//! wins, (2) hard-blocks every risky mutation (kernel sysctl writes, SIGSTOP,
//! MTU clamps, boost/doctor/root) so nothing can trip a kernel anti-cheat, and
//! (3) auto-reverts the safe tweaks on game exit.
//!
//! Design principle: the biggest real win here is getting OUT OF THE WAY — a
//! game should run on clean macOS defaults, plus AirDrop/AWDL disabled (the one
//! genuinely safe, genuinely effective macOS gaming-jitter fix). We never touch
//! the kernel, never freeze a process, never clamp MTU while a game runs.

use std::time::Duration;

use super::cmd;

/// Process names that mean "a game / anti-cheat is running". Matched with
/// `pgrep -x` (exact) and, for names with spaces, `pgrep -f` (full cmdline).
/// Read-only detection — pgrep does not touch the game, so it is anti-cheat safe.
pub const GAME_PROCESSES: &[&str] = &[
    "RiotClientServices",
    "LeagueClient",
    "LeagueClientUx",
    "League of Legends",
    "VALORANT",
    "valorant",
    // Vanguard anti-cheat (Riot). If this is up, we MUST be in safe mode.
    "vgc",
    "vgk",
    "vgtray",
    // A few other common titles / anti-cheats.
    "csgo",
    "cs2",
    "dota2",
    "Overwatch",
    "FortniteClient-Mac-Shipping",
    "EasyAntiCheat",
    "BEService",
];

/// The current Game Mode state, owned by `App`.
#[derive(Clone, Default)]
pub struct GameMode {
    active: bool,
    /// The detected game/anti-cheat process name (for the UI).
    game_name: String,
    /// MTU snapshot taken on entry so we can restore it on exit (we never clamp
    /// MTU in Game Mode, but if a stale clamp was present we restore the real
    /// value the user had).
    mtu_snapshot: Option<String>,
    /// Whether we disabled AirDrop (so we only re-enable what we changed).
    airdrop_disabled: bool,
}

impl GameMode {
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn game_name(&self) -> &str {
        &self.game_name
    }
}

/// Scan the process table for any known game/anti-cheat. Returns the matched
/// name, or None. Read-only (`pgrep`), safe to run every tick.
pub fn detect_game() -> Option<String> {
    for name in GAME_PROCESSES {
        // Names with spaces need a full-cmdline match; others an exact match.
        let argv: [&str; 3] = if name.contains(' ') {
            ["pgrep", "-f", name]
        } else {
            ["pgrep", "-x", name]
        };
        if cmd::ok(&argv, Duration::from_millis(400)) {
            return Some((*name).to_string());
        }
    }
    None
}

/// Read the current Wi-Fi interface MTU so we can restore it on exit.
fn read_mtu(interface: &str) -> Option<String> {
    let out = cmd::run(
        &["networksetup", "-getMTU", interface],
        Duration::from_millis(800),
    );
    // Output looks like: "Active MTU: 1500 (Current Setting: 1500)".
    out.stdout
        .split_whitespace()
        .find(|t| t.parse::<i64>().is_ok())
        .map(|s| s.to_string())
}

/// Disable AirDrop/AWDL discovery — the one safe, effective macOS gaming-jitter
/// fix (stops the Wi-Fi radio time-slicing to Continuity). Pure user-defaults
/// write; NOT a kernel/process change, so it is anti-cheat safe.
fn set_airdrop_disabled(disabled: bool) {
    let val = if disabled { "YES" } else { "NO" };
    let _ = cmd::run(
        &[
            "defaults",
            "write",
            "com.apple.NetworkBrowser",
            "DisableAirDrop",
            "-bool",
            val,
        ],
        Duration::from_millis(800),
    );
}

/// Flush the DNS cache (safe: dscacheutil + mDNSResponder HUP reload).
fn flush_dns() {
    let _ = cmd::run(&["dscacheutil", "-flushcache"], Duration::from_millis(800));
    let _ = cmd::run(
        &["killall", "-HUP", "mDNSResponder"],
        Duration::from_millis(800),
    );
}

/// Transition IDLE -> ACTIVE: snapshot MTU, apply the safe set. Returns a status
/// line for the UI. Caller supplies the interface + a prewarm closure (so we
/// reuse the existing outbound CDN-edge warmer without importing KernelTuner).
pub fn enter(gm: &mut GameMode, interface: &str, game: String) -> String {
    gm.active = true;
    gm.game_name = game.clone();
    gm.mtu_snapshot = read_mtu(interface);
    set_airdrop_disabled(true);
    gm.airdrop_disabled = true;
    flush_dns();
    format!(
        "🎮 GAME MODE ON — {game} detected. AirDrop/AWDL off, DNS flushed, risky tweaks locked (anti-cheat safe)."
    )
}

/// Transition ACTIVE -> IDLE: restore AirDrop, restore MTU if we have a snapshot.
pub fn exit(gm: &mut GameMode, interface: &str) -> String {
    if gm.airdrop_disabled {
        set_airdrop_disabled(false);
        gm.airdrop_disabled = false;
    }
    if let Some(mtu) = gm.mtu_snapshot.take() {
        // Only restore if it differs from the current live value.
        if read_mtu(interface).as_deref() != Some(mtu.as_str()) {
            let _ = cmd::run(
                &["networksetup", "-setMTU", interface, &mtu],
                Duration::from_millis(1200),
            );
        }
    }
    gm.active = false;
    let g = std::mem::take(&mut gm.game_name);
    format!("🎮 GAME MODE OFF — {g} closed. AirDrop restored, settings back to safe defaults.")
}

/// Force-exit (used on teardown) without needing the interface for MTU restore
/// beyond what we snapshotted — restores AirDrop so we never leave it disabled.
pub fn force_restore(gm: &mut GameMode) {
    if gm.airdrop_disabled {
        set_airdrop_disabled(false);
        gm.airdrop_disabled = false;
    }
    gm.active = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_list_covers_riot_and_vanguard() {
        for must in [
            "LeagueClient",
            "RiotClientServices",
            "vgc",
            "vgk",
            "VALORANT",
        ] {
            assert!(
                GAME_PROCESSES.contains(&must),
                "missing game/anti-cheat process: {must}"
            );
        }
    }

    #[test]
    fn default_game_mode_is_inactive() {
        let gm = GameMode::default();
        assert!(!gm.active());
        assert_eq!(gm.game_name(), "");
    }

    #[test]
    fn enter_then_exit_toggles_active() {
        // enter()/exit() shell out to `defaults`/`networksetup`; in CI those may
        // no-op, but the STATE transitions must be correct regardless.
        let mut gm = GameMode::default();
        let _ = enter(&mut gm, "en0", "LeagueClient".into());
        assert!(gm.active());
        assert_eq!(gm.game_name(), "LeagueClient");
        let _ = exit(&mut gm, "en0");
        assert!(!gm.active());
    }
}
