use crate::AppResult;
use std::fs;
use std::path::{Path, PathBuf};

const PROC_STAT: &str = "/proc/stat";
const PROC_MEMINFO: &str = "/proc/meminfo";
const THERMAL_DIR: &str = "/sys/class/thermal";

pub struct Metrics {
    pub cpu_percent: f32,
    pub ram_percent: f32,
    pub temp_celsius: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CpuTicks {
    total: u64,
    idle: u64,
}

pub struct Sampler {
    previous_cpu: CpuTicks,
    temp_path: Option<PathBuf>,
    preferred_types: Vec<String>,
}

impl Sampler {
    pub fn new_with_preference(preferred: &[String]) -> AppResult<Self> {
        let previous_cpu = parse_cpu_stat(&fs::read_to_string(PROC_STAT)?)?;
        Ok(Self {
            previous_cpu,
            temp_path: discover_temp_path(preferred),
            preferred_types: preferred.to_vec(),
        })
    }

    pub fn sample(&mut self) -> AppResult<Metrics> {
        let current_cpu = parse_cpu_stat(&fs::read_to_string(PROC_STAT)?)?;
        let ram_percent = parse_meminfo(&fs::read_to_string(PROC_MEMINFO)?)?;
        let cpu_percent = cpu_percent(self.previous_cpu, current_cpu);
        self.previous_cpu = current_cpu;

        let mut temp_celsius = self.temp_path.as_deref().and_then(read_temperature);
        if temp_celsius.is_none() {
            // Sensors can disappear after suspend or a driver reload.
            self.temp_path = discover_temp_path(&self.preferred_types);
            temp_celsius = self.temp_path.as_deref().and_then(read_temperature);
        }

        Ok(Metrics {
            cpu_percent,
            ram_percent,
            temp_celsius,
        })
    }
}

fn parse_cpu_stat(source: &str) -> AppResult<CpuTicks> {
    let line = source.lines().next().ok_or("/proc/stat is empty")?;
    let mut fields = line.split_ascii_whitespace();
    if fields.next() != Some("cpu") {
        return Err("/proc/stat has no aggregate CPU line".into());
    }

    // Linux includes guest time in user/nice, so do not add those fields again.
    let mut counters = [0_u64; 8];
    let mut count = 0;
    for (index, field) in fields.take(8).enumerate() {
        counters[index] = field.parse()?;
        count += 1;
    }
    if count < 4 {
        return Err("/proc/stat aggregate CPU line has too few counters".into());
    }

    let total = counters
        .iter()
        .try_fold(0_u64, |sum, value| sum.checked_add(*value))
        .ok_or("/proc/stat CPU counters overflowed")?;
    let idle = counters[3]
        .checked_add(counters[4])
        .ok_or("/proc/stat idle counters overflowed")?;
    Ok(CpuTicks { total, idle })
}

fn cpu_percent(previous: CpuTicks, current: CpuTicks) -> f32 {
    let total_delta = current.total.saturating_sub(previous.total);
    if total_delta == 0 {
        return 0.0;
    }
    let idle_delta = current.idle.saturating_sub(previous.idle).min(total_delta);
    (100.0 * (total_delta - idle_delta) as f64 / total_delta as f64).clamp(0.0, 100.0) as f32
}

fn parse_meminfo(source: &str) -> AppResult<f32> {
    let mut total = None;
    let mut available = None;
    for line in source.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name {
            "MemTotal" => {
                total = Some(
                    value
                        .split_ascii_whitespace()
                        .next()
                        .ok_or("MemTotal has no value")?
                        .parse::<u64>()?,
                )
            }
            "MemAvailable" => {
                available = Some(
                    value
                        .split_ascii_whitespace()
                        .next()
                        .ok_or("MemAvailable has no value")?
                        .parse::<u64>()?,
                )
            }
            _ => {}
        }
        if total.is_some() && available.is_some() {
            break;
        }
    }
    let total = total.ok_or("/proc/meminfo has no MemTotal")?;
    let available = available.ok_or("/proc/meminfo has no MemAvailable")?;
    if total == 0 {
        return Err("/proc/meminfo reports zero MemTotal".into());
    }
    Ok((100.0 * total.saturating_sub(available) as f64 / total as f64).clamp(0.0, 100.0) as f32)
}

fn discover_temp_path(preferred: &[String]) -> Option<PathBuf> {
    let mut best: Option<(usize, PathBuf)> = None;
    for entry in fs::read_dir(THERMAL_DIR).ok()?.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("thermal_zone")
        {
            continue;
        }
        let base = entry.path();
        let kind = fs::read_to_string(base.join("type")).unwrap_or_default();
        let path = base.join("temp");
        if read_temperature(&path).is_none() {
            continue;
        }
        let rank = sensor_rank(kind.trim(), preferred);
        if best.as_ref().is_none_or(|(best_rank, _)| rank < *best_rank) {
            best = Some((rank, path));
        }
    }
    best.map(|(_, path)| path)
}

fn sensor_rank(kind: &str, preferred: &[String]) -> usize {
    if let Some(index) = preferred
        .iter()
        .position(|wanted| wanted.eq_ignore_ascii_case(kind))
    {
        return index;
    }
    let kind = kind.to_ascii_lowercase();
    preferred.len()
        + if kind == "tcpu" {
            0
        } else if kind == "x86_pkg_temp" {
            1
        } else if kind.contains("cpu") || kind.contains("package") || kind.contains("pkg") {
            2
        } else if kind.contains("soc") {
            3
        } else {
            4
        }
}

fn read_temperature(path: &Path) -> Option<f32> {
    fs::read_to_string(path)
        .ok()
        .and_then(|value| parse_temperature(&value))
}

fn parse_temperature(source: &str) -> Option<f32> {
    let raw = source.trim().parse::<f32>().ok()?;
    let celsius = if raw.abs() >= 1000.0 {
        raw / 1000.0
    } else {
        raw
    };
    if celsius.is_finite() && (-20.0..=150.0).contains(&celsius) {
        Some(celsius)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_ignores_guest_counters_and_uses_two_samples() {
        let before = parse_cpu_stat("cpu  100 20 30 400 10 5 5 0 99 99\n").unwrap();
        let after = parse_cpu_stat("cpu  130 20 40 450 10 5 5 0 99 99\n").unwrap();
        assert_eq!(
            before,
            CpuTicks {
                total: 570,
                idle: 410
            }
        );
        assert!((cpu_percent(before, after) - 44.444_443).abs() < 0.001);
    }

    #[test]
    fn cpu_handles_unchanged_or_reset_counters() {
        let before = CpuTicks {
            total: 100,
            idle: 80,
        };
        assert_eq!(cpu_percent(before, before), 0.0);
        assert_eq!(
            cpu_percent(
                before,
                CpuTicks {
                    total: 20,
                    idle: 10
                }
            ),
            0.0
        );
    }

    #[test]
    fn ram_uses_available_memory_and_clamps_bad_values() {
        assert_eq!(
            parse_meminfo("MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 300 kB\n").unwrap(),
            70.0
        );
        assert_eq!(
            parse_meminfo("MemTotal: 1000 kB\nMemAvailable: 1200 kB\n").unwrap(),
            0.0
        );
        assert!(parse_meminfo("MemTotal: 1000 kB\n").is_err());
    }

    #[test]
    fn temperature_accepts_millidegrees_and_degrees() {
        assert_eq!(parse_temperature("42000\n"), Some(42.0));
        assert_eq!(parse_temperature("42\n"), Some(42.0));
        assert_eq!(parse_temperature("invalid"), None);
        assert_eq!(parse_temperature("999999"), None);
    }

    #[test]
    fn cpu_sensor_has_priority() {
        assert!(sensor_rank("TCPU", &[]) < sensor_rank("x86_pkg_temp", &[]));
        assert!(sensor_rank("x86_pkg_temp", &[]) < sensor_rank("iwlwifi_1", &[]));
    }

    #[test]
    fn configured_sensor_order_takes_priority_over_defaults() {
        let preferred = vec!["x86_pkg_temp".to_string(), "TCPU".to_string()];
        assert!(sensor_rank("x86_pkg_temp", &preferred) < sensor_rank("TCPU", &preferred));
        assert!(sensor_rank("tcpu", &preferred) < sensor_rank("iwlwifi_1", &preferred));
    }
}
