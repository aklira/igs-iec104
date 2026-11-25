// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The percentiles of a campaign's samples, by the nearest-rank method.

use std::time::Duration;

/// The percentiles of a set of samples, in milliseconds.
pub struct Percentiles {
    pub count: usize,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub p999: f64,
    pub max: f64,
}

/// The percentiles of `samples`, or `None` when there is none.
pub fn of(samples: &[Duration]) -> Option<Percentiles> {
    let mut sorted: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    sorted.sort_by(f64::total_cmp);
    let count = sorted.len();
    let max = *sorted.last()?;
    let at = |percent: f64| -> f64 {
        let rank = ((percent / 100.0) * count as f64).ceil() as usize;
        sorted.get(rank.saturating_sub(1)).copied().unwrap_or(max)
    };
    Some(Percentiles {
        count,
        p50: at(50.0),
        p95: at(95.0),
        p99: at(99.0),
        p999: at(99.9),
        max,
    })
}

/// A line of a report: the name of the samples and their percentiles.
pub fn line(name: &str, samples: &[Duration]) -> String {
    match of(samples) {
        None => format!("| {name} | 0 | - | - | - | - | - |"),
        Some(p) => format!(
            "| {name} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |",
            p.count, p.p50, p.p95, p.p99, p.p999, p.max
        ),
    }
}

/// The header of a table of percentiles.
pub const HEADER: &str =
    "| samples | n | p50 (ms) | p95 (ms) | p99 (ms) | p99.9 (ms) | max (ms) |\n\
| --- | --- | --- | --- | --- | --- | --- |";
