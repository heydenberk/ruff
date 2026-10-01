//! Settings for the `mccabe` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// The complexity metric that `C901` measures.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ComplexityMetric {
    /// McCabe cyclomatic complexity, which counts the independent paths through a function.
    #[default]
    Cyclomatic,
    /// SonarSource cognitive complexity, which weighs nested control flow more heavily than
    /// flat control flow.
    Cognitive,
}

impl ComplexityMetric {
    /// The threshold used when `max-complexity` is not set.
    pub const fn default_max_complexity(self) -> usize {
        match self {
            Self::Cyclomatic => DEFAULT_MAX_COMPLEXITY,
            Self::Cognitive => DEFAULT_MAX_COGNITIVE_COMPLEXITY,
        }
    }
}

impl Display for ComplexityMetric {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cyclomatic => f.write_str("cyclomatic"),
            Self::Cognitive => f.write_str("cognitive"),
        }
    }
}

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub max_complexity: usize,
    pub metric: ComplexityMetric,
}

const DEFAULT_MAX_COMPLEXITY: usize = 10;

/// SonarSource's recommended threshold for cognitive complexity.
const DEFAULT_MAX_COGNITIVE_COMPLEXITY: usize = 15;

impl Default for Settings {
    fn default() -> Self {
        let metric = ComplexityMetric::default();
        Self {
            max_complexity: metric.default_max_complexity(),
            metric,
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.mccabe",
            fields = [
                self.max_complexity,
                self.metric
            ]
        }
        Ok(())
    }
}
