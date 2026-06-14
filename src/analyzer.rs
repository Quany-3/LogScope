pub const MODULE_NAME: &str = "analyzer";

use crate::model::LogLevel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Aggregated metrics shared by CLI output, reports, and the TUI summary panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub total_count: usize,
    pub level_counts: HashMap<LogLevel, usize>,
    pub source_counts: HashMap<String, usize>,
}

impl AnalysisResult {
    pub fn new(total_count: usize) -> Self {
        Self {
            total_count,
            level_counts: HashMap::new(),
            source_counts: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AnalysisResult;
    use crate::model::LogLevel;

    #[test]
    fn defines_analysis_result_models() {
        let mut result = AnalysisResult::new(3);
        result.level_counts.insert(LogLevel::Info, 2);
        result.level_counts.insert(LogLevel::Error, 1);
        result.source_counts.insert("api".to_string(), 2);

        assert_eq!(result.total_count, 3);
        assert_eq!(result.level_counts[&LogLevel::Info], 2);
        assert_eq!(result.source_counts["api"], 2);
    }
}
