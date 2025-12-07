//! Unit tests for health check and repair system
//!
//! Tests cover:
//! - Health status enum
//! - Batch report structure
//! - Basic health check logic (unit-level, not integration)
//!
//! NOTE: Full integration tests (commit → corrupt → repair) require setting up
//! the archive_directory properly and are better suited for end-to-end testing.

#[cfg(test)]
mod tests {
    use super::super::models::{BatchHealthReport, HealthStatus};

    #[test]
    fn test_health_status_equality() {
        assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
        assert_ne!(HealthStatus::Healthy, HealthStatus::Degraded);
        assert_ne!(HealthStatus::Recoverable, HealthStatus::Unrecoverable);
    }

    #[test]
    fn test_batch_health_report_initialization() {
        let report = BatchHealthReport {
            total_files: 10,
            healthy: 8,
            degraded: 1,
            recoverable: 1,
            unrecoverable: 0,
            reports: vec![],
        };

        assert_eq!(report.total_files, 10);
        assert_eq!(report.healthy, 8);
        assert_eq!(report.degraded, 1);
        assert_eq!(report.recoverable, 1);
        assert_eq!(report.unrecoverable, 0);
    }

    #[test]
    fn test_health_status_debug() {
        // Verify Debug trait works
        let status = HealthStatus::Degraded;
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("Degraded"));
    }
}
