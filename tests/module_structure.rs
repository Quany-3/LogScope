use log_scope::{analyzer, cli, config, filter, model, parser, report, tui, utils};

#[test]
fn exposes_base_project_modules() {
    assert_eq!(model::MODULE_NAME, "model");
    assert_eq!(parser::MODULE_NAME, "parser");
    assert_eq!(config::MODULE_NAME, "config");
    assert_eq!(utils::MODULE_NAME, "utils");
    assert_eq!(analyzer::MODULE_NAME, "analyzer");
    assert_eq!(filter::MODULE_NAME, "filter");
    assert_eq!(report::MODULE_NAME, "report");
    assert_eq!(cli::MODULE_NAME, "cli");
    assert_eq!(tui::MODULE_NAME, "tui");
}
