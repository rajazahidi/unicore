use crate::config::HandoverConfig;

#[derive(Debug, Clone)]
pub struct PathMetrics {
    pub ewma_rtt_ms: f64,
    pub baseline_rtt_ms: f64,
    pub jitter_ms: f64,
    pub probes_sent: u64,
    pub probes_acked: u64,
    pub recent_loss_pct: f64,
    pub wifi_rssi_dbm: i32,
}

impl Default for PathMetrics {
    fn default() -> Self {
        Self {
            ewma_rtt_ms: 0.0,
            baseline_rtt_ms: 0.0,
            jitter_ms: 0.0,
            probes_sent: 0,
            probes_acked: 0,
            recent_loss_pct: 0.0,
            wifi_rssi_dbm: -55,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoverAdvice {
    Healthy,
    PrewarmStandby,
    CutoverNow(&'static str),
}

/// Evaluates link quality using EWMA RTT, jitter, packet loss, and Wi-Fi RSSI
/// to trigger predictive Make-Before-Break pre-warming and zero-drop cutover.
pub struct PathScorer {
    cfg: HandoverConfig,
    pub primary: PathMetrics,
    pub standby: PathMetrics,
    window_sent: u64,
    window_acked: u64,
}

impl PathScorer {
    pub fn new(cfg: HandoverConfig) -> Self {
        Self {
            cfg,
            primary: PathMetrics::default(),
            standby: PathMetrics::default(),
            window_sent: 0,
            window_acked: 0,
        }
    }

    pub fn record_probe_sent(&mut self, is_primary: bool) {
        if is_primary {
            self.primary.probes_sent += 1;
            self.window_sent += 1;
            self.recompute_loss();
        } else {
            self.standby.probes_sent += 1;
        }
    }

    pub fn record_rtt_sample(&mut self, is_primary: bool, sample_ms: f64) {
        let m = if is_primary {
            &mut self.primary
        } else {
            &mut self.standby
        };
        m.probes_acked += 1;
        if is_primary {
            self.window_acked += 1;
        }

        if m.ewma_rtt_ms == 0.0 {
            m.ewma_rtt_ms = sample_ms;
            m.baseline_rtt_ms = sample_ms;
            m.jitter_ms = 0.0;
        } else {
            let diff = (sample_ms - m.ewma_rtt_ms).abs();
            m.jitter_ms = 0.75 * m.jitter_ms + 0.25 * diff;
            m.ewma_rtt_ms = 0.75 * m.ewma_rtt_ms + 0.25 * sample_ms;
            if sample_ms < m.baseline_rtt_ms {
                m.baseline_rtt_ms = 0.9 * m.baseline_rtt_ms + 0.1 * sample_ms;
            }
        }
        if is_primary {
            self.recompute_loss();
        }
    }

    pub fn update_wifi_rssi(&mut self, rssi_dbm: i32) {
        self.primary.wifi_rssi_dbm = rssi_dbm;
    }

    fn recompute_loss(&mut self) {
        if self.window_sent >= 5 {
            let lost = self.window_sent.saturating_sub(self.window_acked);
            self.primary.recent_loss_pct = (lost as f64 / self.window_sent as f64) * 100.0;
            if self.window_sent >= 20 {
                self.window_sent /= 2;
                self.window_acked /= 2;
            }
        }
    }

    pub fn evaluate(&self, standby_ready: bool) -> HandoverAdvice {
        let loss_degraded = self.primary.recent_loss_pct > self.cfg.loss_threshold_pct;
        let rtt_spiked = self.primary.baseline_rtt_ms > 1.0
            && self.primary.ewma_rtt_ms
                > self.primary.baseline_rtt_ms * self.cfg.rtt_spike_multiplier;
        let rssi_weak = self.primary.wifi_rssi_dbm <= self.cfg.wifi_rssi_prewarm_dbm;

        if standby_ready {
            if loss_degraded {
                return HandoverAdvice::CutoverNow("Packet loss exceeded threshold");
            }
            if rtt_spiked && self.standby.ewma_rtt_ms > 0.0
                && self.standby.ewma_rtt_ms < self.primary.ewma_rtt_ms
            {
                return HandoverAdvice::CutoverNow("RTT spike with faster standby path");
            }
        }

        if rssi_weak || rtt_spiked || loss_degraded {
            return HandoverAdvice::PrewarmStandby;
        }
        HandoverAdvice::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scorer_triggers_prewarm_and_cutover_on_degradation() {
        let mut scorer = PathScorer::new(HandoverConfig::default());
        for _ in 0..6 {
            scorer.record_probe_sent(true);
            scorer.record_rtt_sample(true, 18.0);
        }
        assert_eq!(scorer.evaluate(false), HandoverAdvice::Healthy);

        // Weak Wi-Fi triggers prewarm even before packet loss
        scorer.update_wifi_rssi(-76);
        assert_eq!(scorer.evaluate(false), HandoverAdvice::PrewarmStandby);

        // Standby ready + packet loss triggers cutover
        for _ in 0..8 {
            scorer.record_probe_sent(true);
        }
        assert!(matches!(
            scorer.evaluate(true),
            HandoverAdvice::CutoverNow(_)
        ));
    }
}
