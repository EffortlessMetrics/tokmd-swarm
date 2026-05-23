# tokmd-sensor

Sensor contract and substrate builder for tokmd.

## Problem
Sensors should share one scan-and-diff substrate instead of each re-running the expensive parts.

## What it gives you
- The `EffortlessSensor` trait.
- `substrate_builder::build_substrate(...) -> Result<RepoSubstrate>`.

## API / usage notes
- Implement `EffortlessSensor` for a sensor that consumes `RepoSubstrate` and returns `SensorReport`.
- `build_substrate` runs the scan once, normalizes diff membership, and builds the shared substrate.
- Keep sensor implementations in their own crates; this crate is the contract layer.

## Example

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokmd_envelope::{SensorReport, ToolMeta, Verdict};
use tokmd_sensor::{EffortlessSensor, RepoSubstrate};

struct LineBudgetSensor;

#[derive(Serialize, Deserialize)]
struct LineBudgetSettings {
    max_code_lines: usize,
}

impl EffortlessSensor for LineBudgetSensor {
    type Settings = LineBudgetSettings;

    fn name(&self) -> &str {
        "line-budget"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn run(
        &self,
        settings: &LineBudgetSettings,
        substrate: &RepoSubstrate,
    ) -> Result<SensorReport> {
        let verdict = if substrate.total_code_lines > settings.max_code_lines {
            Verdict::Warn
        } else {
            Verdict::Pass
        };

        Ok(SensorReport::new(
            ToolMeta::new(self.name(), self.version(), "check"),
            "2024-01-01T00:00:00Z".to_string(),
            verdict,
            format!(
                "{} code lines checked against budget {}",
                substrate.total_code_lines,
                settings.max_code_lines
            ),
        ))
    }
}
```

## Go deeper
- Tutorial: [tokmd README](../../README.md)
- How-to: [tokmd-envelope](../tokmd-envelope/README.md)
- Reference: [Architecture](../../docs/architecture.md)
- Explanation: [Design](../../docs/design.md)
