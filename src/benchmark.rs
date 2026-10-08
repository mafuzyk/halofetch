//! Timing of system information collection and of the full static render.

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::{Duration, Instant};

use color_eyre::Result;

use crate::config::{Config, Scene};
use crate::{info, render};

/// Columns used for the render measurement, a common terminal width.
const RENDER_WIDTH: usize = 120;

pub fn run(cfg: &Config, scene: Scene, iterations: u16) -> Result<()> {
    let runs = usize::from(iterations.max(1));
    // Warm the file system caches before measuring.
    black_box(info::collect());
    black_box(crate::render_static(cfg, scene, RENDER_WIDTH));

    let collection = measure(runs, || {
        black_box(info::collect());
    });
    let full = measure(runs, || {
        let lines = crate::render_static(cfg, scene, RENDER_WIDTH);
        black_box(render::to_ansi(&lines));
    });

    let mut out = format!(
        "HaloFetch benchmark: {runs} runs, scene {}, render width {RENDER_WIDTH}\n",
        scene.key()
    );
    write_stats(&mut out, "collection", &collection);
    write_stats(
        &mut out,
        "full render (collection, logo, scene, ANSI)",
        &full,
    );
    crate::emit(&out)
}

fn measure(runs: usize, mut work: impl FnMut()) -> Stats {
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        work();
        samples.push(started.elapsed());
    }
    summarize(&mut samples)
}

#[derive(Debug, Default, PartialEq)]
struct Stats {
    min: Duration,
    median: Duration,
    mean: Duration,
    max: Duration,
}

fn summarize(samples: &mut [Duration]) -> Stats {
    samples.sort_unstable();
    let (Some(&min), Some(&max)) = (samples.first(), samples.last()) else {
        return Stats::default();
    };
    let count = samples.len();
    let middle = count / 2;
    let median = if count % 2 == 1 {
        samples[middle]
    } else {
        (samples[middle - 1] + samples[middle]) / 2
    };
    let total: Duration = samples.iter().sum();
    let mean = total / u32::try_from(count).unwrap_or(u32::MAX);
    Stats {
        min,
        median,
        mean,
        max,
    }
}

fn write_stats(out: &mut String, title: &str, stats: &Stats) {
    let _ = writeln!(out, "{title}");
    let _ = writeln!(out, "  min:    {}", display(stats.min));
    let _ = writeln!(out, "  median: {}", display(stats.median));
    let _ = writeln!(out, "  mean:   {}", display(stats.mean));
    let _ = writeln!(out, "  max:    {}", display(stats.max));
}

fn display(duration: Duration) -> String {
    format!("{:.2} ms", duration.as_secs_f64() * 1_000.0)
}

#[cfg(test)]
mod tests {
    use super::{display, summarize, Stats};
    use std::time::Duration;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn duration_display_uses_milliseconds() {
        assert_eq!(display(Duration::from_micros(1_250)), "1.25 ms");
    }

    #[test]
    fn odd_sample_count_takes_the_middle_value() {
        let mut samples = vec![ms(9), ms(1), ms(5)];
        assert_eq!(
            summarize(&mut samples),
            Stats {
                min: ms(1),
                median: ms(5),
                mean: ms(5),
                max: ms(9),
            }
        );
    }

    #[test]
    fn even_sample_count_averages_the_middle_pair() {
        let mut samples = vec![ms(4), ms(2), ms(10), ms(8)];
        let stats = summarize(&mut samples);
        assert_eq!(stats.median, ms(6));
        assert_eq!(stats.mean, ms(6));
    }

    #[test]
    fn no_samples_gives_zeroes() {
        assert_eq!(summarize(&mut []), Stats::default());
    }
}
