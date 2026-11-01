//! 5GHz / 2.4GHz channel occupancy analysis and clean-channel recommendation.
//!
//! Ported from the Python `radar.py` (`analyze_spectrum`), including the UNII /
//! DFS block grouping and the "recommend first empty block, else lowest floor" rule.

use super::model::{Band, Neighbor, Telemetry};

#[derive(Clone)]
pub struct Block {
    pub label: &'static str,
    pub channels: &'static [i64],
    pub count: u32,
    pub max_rssi: i64,
    pub mine: bool,
    pub colliding: bool,
}

const BLOCKS_5G: [(&str, &[i64]); 4] = [
    ("UNII-1 (36-48)", &[36, 40, 44, 48]),
    ("DFS-Low (52-64)", &[52, 56, 60, 64]),
    (
        "DFS-Mid (100-144)",
        &[100, 104, 108, 112, 116, 120, 124, 128, 132, 136, 140, 144],
    ),
    ("UNII-3 (149-161)", &[149, 153, 157, 161]),
];

#[derive(Clone)]
pub struct SpectrumReport {
    pub blocks_5g: Vec<Block>,
    pub collisions: Vec<Neighbor>,
    pub recommended_5g_channel: i64,
    pub recommendation_reason: String,
    pub count_24: u32,
    pub max_rssi_24: i64,
}

pub fn analyze(tel: &Telemetry) -> SpectrumReport {
    let curr_ch = tel.channel_num;

    let mut blocks: Vec<Block> = BLOCKS_5G
        .iter()
        .map(|(label, channels)| Block {
            label,
            channels,
            count: 0,
            max_rssi: -100,
            mine: channels.contains(&curr_ch),
            colliding: false,
        })
        .collect();

    let mut collisions = Vec::new();
    let mut count_24: u32 = 0;
    let mut max_rssi_24: i64 = -95;

    for net in &tel.neighbors {
        let ch = net.channel_num;
        let sig = net.signal_dbm;
        if net.band == Band::Band2_4GHz || (1..=14).contains(&ch) {
            count_24 += 1;
            max_rssi_24 = max_rssi_24.max(sig);
        } else if ch >= 36 {
            for b in &mut blocks {
                if b.channels.contains(&ch) {
                    b.count += 1;
                    b.max_rssi = b.max_rssi.max(sig);
                    if b.mine && sig >= -75 {
                        b.colliding = true;
                        collisions.push(net.clone());
                    }
                }
            }
        }
    }

    let unii1 = &blocks[0];
    let unii3 = &blocks[3];
    let (recommended_5g_channel, recommendation_reason) = if unii1.count == 0 {
        (
            36,
            "Channel 36 (80MHz UNII-1) is 100% empty in your airspace.".to_string(),
        )
    } else if unii3.count == 0 {
        (
            149,
            "Channel 149 (80MHz UNII-3) is 100% empty in your airspace.".to_string(),
        )
    } else if unii1.max_rssi <= unii3.max_rssi {
        (
            36,
            "Channel 36 has the lowest interference floor.".to_string(),
        )
    } else {
        (
            149,
            "Channel 149 has the lowest interference floor.".to_string(),
        )
    };

    SpectrumReport {
        blocks_5g: blocks,
        collisions,
        recommended_5g_channel,
        recommendation_reason,
        count_24,
        max_rssi_24,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neigh(ch: i64, band: Band, sig: i64) -> Neighbor {
        Neighbor {
            label: "n".into(),
            channel_num: ch,
            width_mhz: 80,
            band,
            signal_dbm: sig,
            noise_dbm: -95,
            phy_mode: "ac".into(),
            same_channel: false,
        }
    }

    #[test]
    fn detects_collision_and_recommends_clear_block() {
        let tel = Telemetry {
            band: Band::Band5GHz,
            channel_num: 132,
            neighbors: vec![
                neigh(132, Band::Band5GHz, -60), // collides with our Ch 132
                neigh(11, Band::Band2_4GHz, -44),
            ],
            ..Telemetry::default()
        };
        let r = analyze(&tel);
        assert_eq!(r.collisions.len(), 1);
        // UNII-1 is empty, so it should be recommended first.
        assert_eq!(r.recommended_5g_channel, 36);
    }
}
