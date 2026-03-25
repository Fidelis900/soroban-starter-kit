import mongoose from 'mongoose';
import winston from 'winston';
import { config } from './config';

const logger = winston.createLogger({
  level: 'info',
  format: winston.format.json(),
  transports: [
    new winston.transports.Console({
      format: winston.format.simple()
    })
  ]
});

/**
 * Anomaly Detection Service
 * Detects unusual patterns in log data that might indicate issues
 */
export class AnomalyDetectionService {
  private readonly detectionIntervalMs: number;
  private detectionTimer: any = null;
  private isRunning = false;
  
  // Thresholds for anomaly detection
  private readonly errorRateThreshold = 0.1; // 10% error rate
  private readonly suddenIncreaseThreshold = 5; // 5x normal volume
  private readonly unusualSourceThreshold = 3; // 3 standard deviations
  
  // Baseline metrics (updated periodically)
  private baselineMetrics: Map<string, {
    avgCount: number;
    stdDev: number;
    lastUpdate: number;
  }> = new Map();

  constructor() {
    // Default detection interval: every 5 minutes
    this.detectionIntervalMs = 5 * 60 * 1000;
  }

  /**
   * Start the anomaly detection service
   */
  public start(): void {
    if (this.isRunning) {
      logger.warn('Anomaly detection service is already running');
      return;
    }

    this.isRunning = true;
    logger.info('Starting anomaly detection service');
    
    // Run initial baseline calculation
    this.updateBaselineMetrics().catch(err => {
      logger.error('Error during initial baseline calculation:', err);
    });
    
    // Schedule periodic detection
    this.detectionTimer = setInterval(async () => {
      try {
        await this.detectAnomalies();
        await this.updateBaselineMetrics();
      } catch (error) {
        logger.error('Error during anomaly detection cycle:', error);
      }
    }, this.detectionIntervalMs);
  }

  /**
   * Stop the anomaly detection service
   */
  public stop(): void {
    if (!this.isRunning) {
      logger.warn('Anomaly detection service is not running');
      return;
    }

    this.isRunning = false;
    logger.info('Stopping anomaly detection service');
    
    if (this.detectionTimer) {
      clearInterval(this.detectionTimer);
      this.detectionTimer = null;
    }
  }

  /**
   * Get the LogEntry model
   */
  private getLogEntryModel() {
    return mongoose.model('LogEntry');
  }

  /**
   * Detect anomalies in log data
   */
  public async detectAnomalies(): Promise<void> {
    logger.info('Running anomaly detection');
    
    const now = Date.now();
    const oneHourAgo = now - (60 * 60 * 1000); // Last hour
    
    try {
      // Get log counts by level and source for the last hour
      const logs = await this.getLogEntryModel().aggregate([
        {
          $match: {
            timestamp: { $gte: oneHourAgo }
          }
        },
        {
          $group: {
            _id: {
              level: '$level',
              source: '$source'
            },
            count: { $sum: 1 }
          }
        }
      ]);
      
      // Check for anomalies
      for (const log of logs) {
        const { level, source } = log._id;
        const count = log.count;
        const key = `${level}:${source}`;
        
        // Get baseline for this level/source combination
        const baseline = this.baselineMetrics.get(key);
        
        if (baseline) {
          // Check for sudden increase
          if (count > baseline.avgCount + (baseline.stdDev * this.unusualSourceThreshold)) {
            await this.triggerAlert('sudden_increase', {
              level,
              source,
              currentCount: count,
              baselineAvg: baseline.avgCount,
              baselineStdDev: baseline.stdDev,
              threshold: baseline.avgCount + (baseline.stdDev * this.unusualSourceThreshold)
            });
          }
        }
      }
      
      // Check overall error rate
      await this.checkErrorRate(oneHourAgo);
      
    } catch (error) {
      logger.error('Error during anomaly detection:', error);
    }
  }

  /**
   * Check if error rate exceeds threshold
   */
  private async checkErrorRate(sinceTimestamp: number): Promise<void> {
    try {
      const [totalCount, errorCount] = await Promise.all([
        this.getLogEntryModel().countDocuments({ timestamp: { $gte: sinceTimestamp } }),
        this.getLogEntryModel().countDocuments({ 
          timestamp: { $gte: sinceTimestamp },
          level: 'error' 
        })
      ]);
      
      const errorRate = totalCount > 0 ? errorCount / totalCount : 0;
      
      if (errorRate > this.errorRateThreshold) {
        await this.triggerAlert('high_error_rate', {
          errorRate,
          threshold: this.errorRateThreshold,
          totalCount,
          errorCount,
          timeWindow: '1 hour'
        });
      }
    } catch (error) {
      logger.error('Error checking error rate:', error);
    }
  }

  /**
   * Update baseline metrics for anomaly detection
   */
  private async updateBaselineMetrics(): Promise<void> {
    logger.info('Updating baseline metrics for anomaly detection');
    
    const now = Date.now();
    const twentyFourHoursAgo = now - (24 * 60 * 60 * 1000); // Last 24 hours
    
    try {
      // Get log counts by level and source for the last 24 hours, grouped by hour
      const hourlyStats = await this.getLogEntryModel().aggregate([
        {
          $match: {
            timestamp: { $gte: twentyFourHoursAgo }
          }
        },
        {
          $group: {
            _id: {
              level: '$level',
              source: '$source',
              hour: {
                $floor: {
                  $divide: ['$timestamp', 3600000] // Convert to hours since epoch
                }
              }
            },
            count: { $sum: 1 }
          }
        },
        {
          $group: {
            _id: {
              level: '$_id.level',
              source: '$_id.source'
            },
            avgCount: { $avg: '$count' },
            stdDev: { $stdDevPop: '$count' },
            sampleCount: { $sum: 1 }
          }
        }
      ]);
      
      // Update baseline metrics
      for (const stat of hourlyStats) {
        const { level, source } = stat._id;
        const key = `${level}:${source}`;
        
        // Only update if we have enough samples
        if (stat.sampleCount >= 3) {
          this.baselineMetrics.set(key, {
            avgCount: stat.avgCount,
            stdDev: stat.stdDev || 0,
            lastUpdate: now
          });
        }
      }
      
      logger.info(`Updated baseline metrics for ${this.baselineMetrics.size} level/source combinations`);
    } catch (error) {
      logger.error('Error updating baseline metrics:', error);
    }
  }

  /**
   * Trigger an alert for detected anomaly
   */
  private async triggerAlert(type: string, details: any): Promise<void> {
    const alertId = `${type}-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    
    logger.warn(`Anomaly detected: ${type}`, {
      alertId,
      type,
      ...details,
      timestamp: Date.now()
    });
    
    // In a real implementation, this would send notifications via email, Slack, etc.
    // For now, we'll just log it and store it in a separate collection if needed
    
    // Example of what could be done:
    // await AlertModel.create({
    //   id: alertId,
    //   type,
    //   details,
    //   timestamp: Date.now(),
    //   acknowledged: false
    // });
  }

  /**
   * Get current baseline metrics
   */
  public getBaselineMetrics(): Map<string, {
    avgCount: number;
    stdDev: number;
    lastUpdate: number;
  }> {
    return new Map(this.baselineMetrics);
  }
}

// Create and export singleton instance
export const anomalyDetectionService = new AnomalyDetectionService();