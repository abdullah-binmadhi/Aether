//! Clean-channel recommendation. Ported from channel_advisor.py.

use super::model::{Band, Telemetry};

pub struct Advice {
    pub best_24g_channel: i64,
    pub best_5g_channel: i64,
    pub recommended_5g_width_mhz: i64,
    pub advice_line: String,
}

pub fn advise(tel: &Telemetry) -> Advice {
    let active_band = tel.band;
    let active_ch = if tel.channel_num > 0 {
        tel.channel_num
    } else {
        36
    };

    let mut scores_24: [(i64, i64); 3] = [(1, 0), (6, 0), (11, 0)];
    let mut scores_5: [(i64, i64); 4] = [(36, 0), (44, 0), (149, 0), (157, 0)];

    for net in &tel.neighbors {
        let n_ch = net.channel_num;
        let n_sig = net.signal_dbm;
        let weight = ((n_sig + 96) / 2).max(2);
        if net.band == Band::Band2_4GHz {
            for slot in scores_24.iter_mut() {
                let dist = (n_ch - slot.0).abs();
                if dist == 0 {
                    slot.1 += weight * 3;
                } else if dist <= 4 {
                    slot.1 += weight * (5 - dist);
                }
            }
        } else if net.band == Band::Band5GHz {
            for slot in scores_5.iter_mut() {
                if (n_ch - slot.0).abs() <= 12 {
                    slot.1 += weight * 2;
                }
            }
        }
    }

    let best_24 = scores_24
        .iter()
        .min_by_key(|s| s.1)
        .map(|s| s.0)
        .unwrap_or(1);
    let best_5_slot = scores_5
        .iter()
        .min_by_key(|s| s.1)
        .copied()
        .unwrap_or((36, 0));
    let best_5 = best_5_slot.0;
    let rec_width = if best_5_slot.1 < 35 { 80 } else { 40 };

    let advice_line = if active_band == Band::Band2_4GHz {
        format!("Router Advisor: 2.4GHz Ch{best_24} @ 20MHz is cleanest │ Best 5GHz: Ch{best_5} @ {rec_width}MHz (Non-DFS)")
    } else {
        let dfs_warn = if (52..=144).contains(&active_ch) {
            " (Avoids Radar DFS Drops)"
        } else {
            ""
        };
        format!("Router Advisor: Set 5GHz to Ch{best_5} @ {rec_width}MHz{dfs_warn} │ Best 2.4GHz: Ch{best_24} @ 20MHz")
    };

    Advice {
        best_24g_channel: best_24,
        best_5g_channel: best_5,
        recommended_5g_width_mhz: rec_width,
        advice_line,
    }
}
