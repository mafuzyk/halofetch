//! Lightweight, dependency-free performance measurement.

use color_eyre::Result;
use std::time::{Duration, Instant};

pub fn run(iterations: u16) -> Result<()> {
    // Warm filesystem caches and command lookup before measuring.
    let _ = crate::info::collect()?;

    let mut samples = Vec::with_capacity(iterations as usize);
    for _ in 0..iterations {
        let started = Instant::now();
        let _ = crate::info::collect()?;
        samples.push(started.elapsed());
    }

    samples.sort_unstable();
    let total: Duration = samples.iter().copied().sum();
    let average = total / u32::from(iterations);
    let median = samples[samples.len() / 2];

    println!("AtlasFetch collection benchmark ({iterations} iterations)");
    println!("  min:    {}", display(samples[0]));
    println!("  median: {}", display(median));
    println!("  mean:   {}", display(average));
    println!(
        "  max:    {}",
        display(*samples.last().expect("at least one sample"))
    );
    Ok(())
}

fn display(duration: Duration) -> String {
    format!("{:.2} ms", duration.as_secs_f64() * 1_000.0)
}

#[cfg(test)]
mod tests {
    use super::display;
    use std::time::Duration;

    #[test]
    fn duration_display_uses_milliseconds() {
        assert_eq!(display(Duration::from_micros(1_250)), "1.25 ms");
    }
}
