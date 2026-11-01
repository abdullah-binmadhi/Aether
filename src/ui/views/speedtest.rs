//! Tab [5] Speed/bufferbloat/workload lab. Ported from speedtest.py.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::speedtest;
use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, bar, good, score_color, sparkline};
use crate::ui::{draw_kpi_strip, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let st = &rs.speedtest;
    let snap = &rs.speedtest_snapshot;

    let rows = Layout::vertical([
        Constraint::Length(4), // KPI strip
        Constraint::Length(3), // phase
        Constraint::Length(6), // gauges + scorecard
        Constraint::Min(6),    // oscilloscope (shrinks first on small terminals)
        Constraint::Length(8), // workloads + interpretation (room to wrap)
    ])
    .split(area);

    draw_kpi(f, rs, rows[0]);

    // Phase bar.
    let is_running = st.is_running;
    let phase_block =
        titled_block("SPEED, BUFFERBLOAT & WORKLOAD LAB ([Space] Full │ [V] 5s │ [N] Apple RPM)");
    let pi = phase_block.inner(rows[1]);
    f.render_widget(phase_block, rows[1]);
    let phase_txt = format!("{} {}", bar(st.progress_pct, 0.0, 100.0, 18), st.phase_name);
    let phase_style = if phase_txt.contains("[OK]") || is_running {
        good()
    } else {
        amber_bold()
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(phase_txt, phase_style))),
        pi,
    );

    // Gauges + scorecard.
    let mid =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[2]);
    render_gauges(f, rs, mid[0], snap);
    render_scorecard(f, rs, mid[1]);

    // Oscilloscope — a tall stacked graph that fills the panel instead of 3
    // lonely sparklines with a huge empty bottom.
    let scope_block = titled_block("REAL-TIME THROUGHPUT & LOADED PING OSCILLOSCOPE");
    let si = scope_block.inner(rows[3]);
    f.render_widget(scope_block, rows[3]);
    render_oscilloscope(f, st, si);

    // Workloads + interpretation.
    let bot =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[4]);
    render_workloads(f, bot[0], snap);
    render_interpretation(f, bot[1], snap);
}

fn render_oscilloscope(f: &mut Frame, st: &crate::core::speedtest::SpeedtestView, area: Rect) {
    use crate::ui::widgets::render_2d_vertical_graph;
    if area.height < 3 {
        return;
    }
    let w = area.width as usize;
    let h = area.height as usize;

    // Short-panel fallback: three compact sparklines (never garbage).
    if h < 8 {
        let spark_w = w.saturating_sub(18).max(12);
        let lines = vec![
            Line::from(Span::styled(
                format!(
                    "▼ DOWN {}  {:5.1}M",
                    sparkline(&st.down_history, spark_w, 1.0),
                    st.down_mbps
                ),
                good(),
            )),
            Line::from(Span::styled(
                format!(
                    "▲ UP   {}  {:5.1}M",
                    sparkline(&st.up_history, spark_w, 1.0),
                    st.up_mbps
                ),
                amber(),
            )),
            Line::from(Span::styled(
                format!(
                    "◆ RTT  {}  {:5.1}ms",
                    sparkline(&st.rtt_history, spark_w, 5.0),
                    st.loaded_down_rtt_ms
                ),
                amber(),
            )),
        ];
        f.render_widget(Paragraph::new(lines), area);
        return;
    }

    // Tall layout: DOWNLOAD graph (≈55%), UPLOAD graph (≈30%), RTT spark (rest).
    let rows = Layout::vertical([
        Constraint::Percentage(55),
        Constraint::Percentage(32),
        Constraint::Min(2),
    ])
    .split(area);

    // DOWNLOAD — big vertical block graph.
    let d = rows[0];
    let dh = (d.height as usize).saturating_sub(1).max(2);
    let mut dlines = vec![Line::from(Span::styled(
        format!(
            "▼ DOWNLOAD  {:.1} Mbps (peak {:.1})",
            st.down_mbps, st.peak_down_mbps
        ),
        good(),
    ))];
    for g in render_2d_vertical_graph(
        &st.down_history,
        dh,
        w,
        0.0,
        st.peak_down_mbps.max(10.0),
        "M",
    ) {
        dlines.push(Line::from(Span::styled(g, good())));
    }
    f.render_widget(Paragraph::new(dlines), d);

    // UPLOAD — shorter vertical block graph.
    let u = rows[1];
    let uh = (u.height as usize).saturating_sub(1).max(2);
    let mut ulines = vec![Line::from(Span::styled(
        format!(
            "▲ UPLOAD    {:.1} Mbps (peak {:.1})",
            st.up_mbps, st.peak_up_mbps
        ),
        amber_bold(),
    ))];
    for g in render_2d_vertical_graph(&st.up_history, uh, w, 0.0, st.peak_up_mbps.max(10.0), "M") {
        ulines.push(Line::from(Span::styled(g, amber())));
    }
    f.render_widget(Paragraph::new(ulines), u);

    // LOADED RTT — full-width sparkline anchor row.
    let r = rows[2];
    let spark_w = w.saturating_sub(22).max(12);
    let rlines = vec![Line::from(Span::styled(
        format!(
            "◆ LOAD RTT {}  {:.1}ms",
            sparkline(&st.rtt_history, spark_w, 5.0),
            st.loaded_down_rtt_ms
        ),
        amber(),
    ))];
    f.render_widget(Paragraph::new(rlines), r);
}

fn draw_kpi(f: &mut Frame, rs: &RenderState, area: Rect) {
    let st = &rs.speedtest;
    // Grade -> pseudo-score for colour accent.
    let g = st.bufferbloat_grade.chars().next().unwrap_or('-');
    let grade_accent = match g {
        'A' | 'B' => good(),
        'C' => amber_bold(),
        '-' => amber_bold(),
        _ => alert(),
    };
    let cards = [
        Kpi {
            title: "▼ DOWN",
            value: format!("{:.0}", st.down_mbps),
            unit: "Mbps",
            history: &st.down_history,
            floor: 0.0,
            accent: Some(good()),
        },
        Kpi {
            title: "▲ UP",
            value: format!("{:.0}", st.up_mbps),
            unit: "Mbps",
            history: &st.up_history,
            floor: 0.0,
            accent: None,
        },
        Kpi {
            title: "BLOAT",
            value: st.bufferbloat_grade.to_string(),
            unit: "grade",
            history: &st.rtt_history,
            floor: 0.0,
            accent: Some(grade_accent),
        },
        Kpi {
            title: "RPM",
            value: format!("{}", st.rpm_score),
            unit: "score",
            history: &[st.rpm_score as f64],
            floor: 0.0,
            accent: Some(score_color((st.rpm_score / 10).clamp(0, 100))),
        },
    ];
    draw_kpi_strip(f, area, &cards);
}

fn render_gauges(f: &mut Frame, rs: &RenderState, area: Rect, snap: &speedtest::Snapshot) {
    let st = &rs.speedtest;
    let block = titled_block("THROUGHPUT GAUGES & PHY EFFICIENCY");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let link = snap.link_rate_mbps.max(1);
    let bw = ((area.width as usize).saturating_sub(28) / 2).max(8);
    let lines = vec![
        Line::from(Span::styled(
            format!(
                "▼ DOWNLOAD : {:6.1} Mbps  {}  (Peak: {:.1}M)",
                st.down_mbps,
                bar(st.down_mbps, 0.0, link.max(100) as f64, bw),
                st.peak_down_mbps
            ),
            if st.down_mbps > 0.0 {
                good()
            } else {
                amber_bold()
            },
        )),
        Line::from(Span::styled(
            format!(
                "▲ UPLOAD   : {:6.1} Mbps  {}  (Peak: {:.1}M)",
                st.up_mbps,
                bar(st.up_mbps, 0.0, (link as f64 * 0.5).max(50.0), bw),
                st.peak_up_mbps
            ),
            amber(),
        )),
        Line::from(Span::styled(
            format!(
                "◆ Wi-Fi PHY Link : {link} Mbps ({} MCS {})",
                rs.telemetry.band.label(),
                rs.telemetry.mcs_index
            ),
            amber(),
        )),
        Line::from(Span::styled(
            format!(
                "◆ WAN/PHY Ratio  : {}% of airlink capacity utilized",
                snap.phy_eff_pct
            ),
            amber(),
        )),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_scorecard(f: &mut Frame, rs: &RenderState, area: Rect) {
    let st = &rs.speedtest;
    let block = titled_block("BUFFERBLOAT & RESPONSIVENESS SCORECARD");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let g = &st.bufferbloat_grade;
    let g_style = if g.starts_with('A') || g.starts_with('B') {
        good()
    } else if g == "—" {
        amber_bold()
    } else {
        alert()
    };
    let worst = st.loaded_down_rtt_ms.max(st.loaded_up_rtt_ms) - st.unloaded_rtt_ms;
    let lines = vec![
        Line::from(Span::styled(
            format!("BUFFERBLOAT GRADE : [ {g} ]"),
            g_style,
        )),
        Line::from(Span::styled(
            format!(
                "Responsiveness    : {} RPM  │  Loaded Jitter: {:.1} ms",
                st.rpm_score, st.loaded_jitter_ms
            ),
            amber_bold(),
        )),
        Line::from(Span::styled(
            format!(
                "Unloaded Ping     : {:5.1} ms  (Baseline Idle RTT)",
                st.unloaded_rtt_ms
            ),
            amber(),
        )),
        Line::from(Span::styled(
            format!(
                "Loaded ▼ / ▲ Ping : {:.1}ms │ {:.1}ms",
                st.loaded_down_rtt_ms, st.loaded_up_rtt_ms
            ),
            if worst <= 25.0 { good() } else { alert() },
        )),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_workloads(f: &mut Frame, area: Rect, snap: &speedtest::Snapshot) {
    let block = titled_block("WORKLOAD READINESS MATRIX (LoL/FFXIV/4K/AI)");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;
    let mut lines = Vec::new();
    for (name, verdict, ok) in &snap.workloads {
        let st = if *ok { good() } else { alert() };
        let full = format!("▸ {:<20}: {}", name, verdict);
        for (i, wl) in wrap_text(&full, w).into_iter().enumerate() {
            // Indent continuation lines so wrapped verdicts stay readable.
            let text = if i == 0 { wl } else { format!("    {wl}") };
            lines.push(Line::from(Span::styled(text, st)));
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_interpretation(f: &mut Frame, area: Rect, snap: &speedtest::Snapshot) {
    let block = titled_block("BOTTLENECK INTERPRETATION & VERDICT");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;
    let mut lines = Vec::new();
    for s in &snap.interpretation {
        let lc = s.to_lowercase();
        let style = if lc.contains("safe") || lc.contains("headroom") || lc.contains("optimal") {
            good()
        } else {
            amber()
        };
        for (i, wl) in wrap_text(s, w).into_iter().enumerate() {
            let text = if i == 0 { wl } else { format!("   {wl}") };
            lines.push(Line::from(Span::styled(text, style)));
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Minimal greedy word-wrap for a single paragraph.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(12);
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let cand = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if cand.chars().count() <= width {
            cur = cand;
        } else {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
