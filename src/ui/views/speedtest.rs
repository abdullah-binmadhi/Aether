//! Tab [5] Speed/bufferbloat/workload lab. Ported from speedtest.py.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::speedtest;
use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, bar, good, sparkline};
use crate::ui::titled_block;

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let st = &rs.speedtest;
    let snap = &rs.speedtest_snapshot;

    let rows = Layout::vertical([
        Constraint::Length(3), // phase
        Constraint::Length(7), // gauges + scorecard
        Constraint::Min(4),    // oscilloscope
        Constraint::Length(7), // workloads + interpretation
    ])
    .split(area);

    // Phase bar.
    let is_running = st.is_running;
    let phase_block =
        titled_block("SPEED, BUFFERBLOAT & WORKLOAD LAB ([Space] Full │ [V] 5s │ [N] Apple RPM)");
    let pi = phase_block.inner(rows[0]);
    f.render_widget(phase_block, rows[0]);
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
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);
    render_gauges(f, rs, mid[0], snap);
    render_scorecard(f, rs, mid[1]);

    // Oscilloscope.
    let scope_block = titled_block("REAL-TIME THROUGHPUT & LOADED PING OSCILLOSCOPE");
    let si = scope_block.inner(rows[2]);
    f.render_widget(scope_block, rows[2]);
    let spark_w = (si.width as usize).saturating_sub(18).max(12);
    let scope_lines = vec![
        Line::from(Span::styled(
            format!(
                "▼ DOWN Mbps  {}  {:5.1}M",
                sparkline(&st.down_history, spark_w, 1.0),
                st.down_mbps
            ),
            good(),
        )),
        Line::from(Span::styled(
            format!(
                "▲ UP   Mbps  {}  {:5.1}M",
                sparkline(&st.up_history, spark_w, 1.0),
                st.up_mbps
            ),
            amber(),
        )),
        Line::from(Span::styled(
            format!(
                "◆ LOAD RTT   {}  {:5.1}ms",
                sparkline(&st.rtt_history, spark_w, 5.0),
                st.loaded_down_rtt_ms
            ),
            amber(),
        )),
    ];
    f.render_widget(Paragraph::new(scope_lines), si);

    // Workloads + interpretation.
    let bot =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[3]);
    render_workloads(f, bot[0], snap);
    render_interpretation(f, bot[1], snap);
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
    let mut lines = Vec::new();
    for (name, verdict, ok) in &snap.workloads {
        lines.push(Line::from(Span::styled(
            format!("▸ {:<20}: {}", name, verdict),
            if *ok { good() } else { alert() },
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_interpretation(f: &mut Frame, area: Rect, snap: &speedtest::Snapshot) {
    let block = titled_block("BOTTLENECK INTERPRETATION & VERDICT");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines: Vec<Line> = snap
        .interpretation
        .iter()
        .map(|s| {
            let lc = s.to_lowercase();
            let style = if lc.contains("safe") || lc.contains("headroom") || lc.contains("optimal")
            {
                good()
            } else {
                amber()
            };
            Line::from(Span::styled(s.clone(), style))
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}
