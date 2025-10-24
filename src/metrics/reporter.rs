//! Metrics Reporter
//! 
//! Generates usage reports and connection insights

use super::{Metrics, HistoricalStats, ActivitySummary};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tracing::{info, debug};

/// Connection usage report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageReport {
    pub report_id: String,
    pub generated_at: u64, // Unix timestamp
    pub period_start: u64,
    pub period_end: u64,
    pub summary: ReportSummary,
    pub top_users: Vec<UserActivity>,
    pub top_destinations: Vec<DestinationActivity>,
    pub hourly_stats: Vec<HourlyStats>,
    pub connection_patterns: ConnectionPatterns,
}

/// Report summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub total_connections: u64,
    pub unique_users: u64,
    pub unique_destinations: u64,
    pub total_bytes_transferred: u64,
    pub average_connection_duration: f64, // seconds
    pub peak_concurrent_connections: u64,
    pub authentication_success_rate: f64,
    pub blocked_requests: u64,
}

/// User activity statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserActivity {
    pub user_id: String,
    pub connection_count: u64,
    pub bytes_transferred: u64,
    pub average_session_duration: f64,
    pub unique_destinations: u64,
}

/// Destination activity statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestinationActivity {
    pub destination: String,
    pub connection_count: u64,
    pub bytes_transferred: u64,
    pub unique_users: u64,
}

/// Hourly statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyStats {
    pub hour: u64, // Unix timestamp for the hour
    pub connections: u64,
    pub bytes_transferred: u64,
    pub unique_users: u64,
}

/// Connection patterns analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPatterns {
    pub peak_hours: Vec<u8>, // Hours of day (0-23)
    pub average_connections_per_hour: f64,
    pub connection_duration_distribution: DurationDistribution,
    pub geographic_distribution: Vec<CountryStats>,
}

/// Duration distribution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DurationDistribution {
    pub short_connections: u64,    // < 1 minute
    pub medium_connections: u64,   // 1-10 minutes
    pub long_connections: u64,     // > 10 minutes
    pub average_duration: f64,
    pub median_duration: f64,
}

/// Geographic statistics (if GeoIP is enabled)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryStats {
    pub country_code: String,
    pub country_name: String,
    pub connection_count: u64,
    pub bytes_transferred: u64,
}

/// Connection insights generator
pub struct ConnectionInsights {
    metrics: Arc<Metrics>,
}

impl ConnectionInsights {
    /// Create a new connection insights generator
    pub fn new(metrics: Arc<Metrics>) -> Self {
        Self { metrics }
    }
    
    /// Generate a usage report for the specified time period
    pub async fn generate_usage_report(
        &self,
        period_start: SystemTime,
        period_end: SystemTime,
    ) -> anyhow::Result<UsageReport> {
        let report_id = uuid::Uuid::new_v4().to_string();
        let generated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        
        let period_start_secs = period_start.duration_since(UNIX_EPOCH)?.as_secs();
        let period_end_secs = period_end.duration_since(UNIX_EPOCH)?.as_secs();
        
        info!(
            report_id = %report_id,
            period_start = period_start_secs,
            period_end = period_end_secs,
            "Generating usage report"
        );
        
        // Get historical stats and activity summary
        let historical_stats = self.metrics.get_historical_stats()?;
        let activity_summary = self.metrics.get_activity_summary()?;
        
        // Generate report components
        let summary = self.generate_report_summary(&historical_stats, &activity_summary).await?;
        let top_users = self.generate_user_activity(&historical_stats).await?;
        let top_destinations = self.generate_destination_activity(&historical_stats).await?;
        let hourly_stats = self.generate_hourly_stats(&historical_stats, period_start, period_end).await?;
        let connection_patterns = self.analyze_connection_patterns(&historical_stats).await?;
        
        let report = UsageReport {
            report_id,
            generated_at,
            period_start: period_start_secs,
            period_end: period_end_secs,
            summary,
            top_users,
            top_destinations,
            hourly_stats,
            connection_patterns,
        };
        
        info!(report_id = %report.report_id, "Usage report generated successfully");
        Ok(report)
    }
    
    /// Generate daily usage report
    pub async fn generate_daily_report(&self) -> anyhow::Result<UsageReport> {
        let now = SystemTime::now();
        let day_start = now - Duration::from_secs(24 * 60 * 60);
        
        self.generate_usage_report(day_start, now).await
    }
    
    /// Generate weekly usage report
    pub async fn generate_weekly_report(&self) -> anyhow::Result<UsageReport> {
        let now = SystemTime::now();
        let week_start = now - Duration::from_secs(7 * 24 * 60 * 60);
        
        self.generate_usage_report(week_start, now).await
    }
    
    /// Generate monthly usage report
    pub async fn generate_monthly_report(&self) -> anyhow::Result<UsageReport> {
        let now = SystemTime::now();
        let month_start = now - Duration::from_secs(30 * 24 * 60 * 60);
        
        self.generate_usage_report(month_start, now).await
    }
    
    /// Get real-time connection statistics
    pub async fn get_realtime_stats(&self) -> anyhow::Result<ActivitySummary> {
        self.metrics.get_activity_summary()
    }
    
    /// Generate connection insights and recommendations
    pub async fn generate_insights(&self) -> anyhow::Result<Vec<String>> {
        let historical_stats = self.metrics.get_historical_stats()?;
        let activity_summary = self.metrics.get_activity_summary()?;
        
        let mut insights = Vec::new();
        
        // Analyze connection patterns
        if historical_stats.total_connections > 100 {
            let avg_duration = historical_stats.average_connection_duration.as_secs();
            
            if avg_duration < 60 {
                insights.push("Many connections are short-lived. Consider optimizing connection setup overhead.".to_string());
            } else if avg_duration > 3600 {
                insights.push("Connections tend to be long-lived. Monitor for potential connection leaks.".to_string());
            }
        }
        
        // Analyze user activity
        if activity_summary.top_users.len() > 0 {
            let top_user_connections = activity_summary.top_users[0].1;
            let total_connections = activity_summary.total_connections_today;
            
            if total_connections > 0 && (top_user_connections as f64 / total_connections as f64) > 0.5 {
                insights.push("Single user accounts for >50% of traffic. Consider load balancing or rate limiting.".to_string());
            }
        }
        
        // Analyze destination patterns
        if activity_summary.top_destinations.len() > 0 {
            let top_dest_connections = activity_summary.top_destinations[0].1;
            let total_connections = activity_summary.total_connections_today;
            
            if total_connections > 0 && (top_dest_connections as f64 / total_connections as f64) > 0.7 {
                insights.push("Most traffic goes to a single destination. Consider caching or direct routing.".to_string());
            }
        }
        
        // Resource utilization insights
        if activity_summary.active_connections > 800 {
            insights.push("High number of active connections. Monitor system resources and consider scaling.".to_string());
        }
        
        if insights.is_empty() {
            insights.push("Connection patterns appear normal. No specific recommendations at this time.".to_string());
        }
        
        debug!(insights_count = insights.len(), "Generated connection insights");
        Ok(insights)
    }
    
    async fn generate_report_summary(
        &self,
        historical_stats: &HistoricalStats,
        activity_summary: &ActivitySummary,
    ) -> anyhow::Result<ReportSummary> {
        let unique_users = historical_stats.user_activity.len() as u64;
        let unique_destinations = historical_stats.top_destinations.len() as u64;
        
        // Calculate authentication success rate (simplified)
        let auth_success_rate = if activity_summary.authentication_attempts_today > 0 {
            // This is a simplified calculation - in reality we'd track successes separately
            0.85 // Placeholder value
        } else {
            1.0
        };
        
        Ok(ReportSummary {
            total_connections: historical_stats.total_connections,
            unique_users,
            unique_destinations,
            total_bytes_transferred: historical_stats.total_bytes_transferred,
            average_connection_duration: historical_stats.average_connection_duration.as_secs_f64(),
            peak_concurrent_connections: activity_summary.active_connections as u64,
            authentication_success_rate: auth_success_rate,
            blocked_requests: activity_summary.blocked_requests_today,
        })
    }
    
    async fn generate_user_activity(&self, historical_stats: &HistoricalStats) -> anyhow::Result<Vec<UserActivity>> {
        let mut user_activities = Vec::new();
        
        for (user_id, connection_count) in &historical_stats.user_activity {
            // This is simplified - in a real implementation we'd track more detailed per-user stats
            user_activities.push(UserActivity {
                user_id: user_id.clone(),
                connection_count: *connection_count,
                bytes_transferred: 0, // Would need to track this separately
                average_session_duration: historical_stats.average_connection_duration.as_secs_f64(),
                unique_destinations: 0, // Would need to track this separately
            });
        }
        
        // Sort by connection count
        user_activities.sort_by(|a, b| b.connection_count.cmp(&a.connection_count));
        user_activities.truncate(10);
        
        Ok(user_activities)
    }
    
    async fn generate_destination_activity(&self, historical_stats: &HistoricalStats) -> anyhow::Result<Vec<DestinationActivity>> {
        let mut destination_activities = Vec::new();
        
        for (destination, connection_count) in &historical_stats.top_destinations {
            destination_activities.push(DestinationActivity {
                destination: destination.clone(),
                connection_count: *connection_count,
                bytes_transferred: 0, // Would need to track this separately
                unique_users: 0, // Would need to track this separately
            });
        }
        
        // Already sorted from historical_stats
        destination_activities.truncate(10);
        
        Ok(destination_activities)
    }
    
    async fn generate_hourly_stats(
        &self,
        _historical_stats: &HistoricalStats,
        period_start: SystemTime,
        period_end: SystemTime,
    ) -> anyhow::Result<Vec<HourlyStats>> {
        let mut hourly_stats = Vec::new();
        
        let period_hours = period_end.duration_since(period_start)?.as_secs() / 3600;
        let start_timestamp = period_start.duration_since(UNIX_EPOCH)?.as_secs();
        
        // Generate hourly buckets (simplified implementation)
        for hour in 0..period_hours {
            let hour_timestamp = start_timestamp + (hour * 3600);
            
            hourly_stats.push(HourlyStats {
                hour: hour_timestamp,
                connections: 0, // Would need to aggregate from historical data
                bytes_transferred: 0,
                unique_users: 0,
            });
        }
        
        Ok(hourly_stats)
    }
    
    async fn analyze_connection_patterns(&self, historical_stats: &HistoricalStats) -> anyhow::Result<ConnectionPatterns> {
        // Simplified pattern analysis
        let duration_distribution = DurationDistribution {
            short_connections: 0,  // Would analyze actual durations
            medium_connections: 0,
            long_connections: 0,
            average_duration: historical_stats.average_connection_duration.as_secs_f64(),
            median_duration: historical_stats.average_connection_duration.as_secs_f64(),
        };
        
        Ok(ConnectionPatterns {
            peak_hours: vec![9, 10, 11, 14, 15, 16], // Example peak hours
            average_connections_per_hour: historical_stats.total_connections as f64 / 24.0,
            connection_duration_distribution: duration_distribution,
            geographic_distribution: Vec::new(), // Would require GeoIP integration
        })
    }
}

/// Export usage report to JSON format
pub fn export_report_json(report: &UsageReport) -> anyhow::Result<String> {
    serde_json::to_string_pretty(report)
        .map_err(|e| anyhow::anyhow!("Failed to serialize report to JSON: {}", e))
}

/// Export usage report to CSV format (simplified)
pub fn export_report_csv(report: &UsageReport) -> anyhow::Result<String> {
    let mut csv = String::new();
    
    // Header
    csv.push_str("Report ID,Generated At,Period Start,Period End,Total Connections,Total Bytes\n");
    
    // Data
    csv.push_str(&format!(
        "{},{},{},{},{},{}\n",
        report.report_id,
        report.generated_at,
        report.period_start,
        report.period_end,
        report.summary.total_connections,
        report.summary.total_bytes_transferred
    ));
    
    Ok(csv)
}