import { LogEntryModel } from './server';
import { config } from './config';
import winston from 'winston';

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
 * Retention Service
 * Handles automatic cleanup of logs based on retention policies
 */
export class RetentionService {
  private readonly retentionPolicies: Map<string, number>; // level -> days
  private readonly cleanupIntervalMs: number;
  private cleanupTimer: any = null;
  private isRunning = false;

  constructor() {
    // Initialize retention policies from config
    this.retentionPolicies = new Map();
    config.logRetentionLevels.forEach(level => {
      this.retentionPolicies.set(level, config.logRetentionDays);
    });
    
    // Default cleanup interval: every 6 hours
    this.cleanupIntervalMs = 6 * 60 * 60 * 1000;
  }

  /**
   * Start the retention service
   */
  public start(): void {
    if (this.isRunning) {
      logger.warn('Retention service is already running');
      return;
    }

    this.isRunning = true;
    logger.info('Starting log retention service');
    
    // Run initial cleanup
    this.cleanupOldLogs().catch(err => {
      logger.error('Error during initial log cleanup:', err);
    });
    
    // Schedule periodic cleanup
    this.cleanupTimer = setInterval(() => {
      this.cleanupOldLogs().catch(err => {
        logger.error('Error during scheduled log cleanup:', err);
      });
    }, this.cleanupIntervalMs);
  }

  /**
   * Stop the retention service
   */
  public stop(): void {
    if (!this.isRunning) {
      logger.warn('Retention service is not running');
      return;
    }

    this.isRunning = false;
    logger.info('Stopping log retention service');
    
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }
  }

  /**
   * Clean up logs older than their retention period
   */
  public async cleanupOldLogs(): Promise<void> {
    const now = Date.now();
    let totalDeleted = 0;

    logger.info('Starting log retention cleanup');

    // Process each log level with its specific retention policy
    for (const [level, days] of this.retentionPolicies.entries()) {
      const cutoffDate = now - (days * 24 * 60 * 60 * 1000);
      
      try {
        const result = await LogEntryModel.deleteMany({
          timestamp: { $lt: cutoffDate },
          level: level
        });
        
        if (result.deletedCount > 0) {
          logger.info(`Cleaned up ${result.deletedCount} ${level} logs older than ${days} days`);
          totalDeleted += result.deletedCount;
        }
      } catch (error) {
        logger.error(`Error cleaning up ${level} logs:`, error);
      }
    }

    // Also clean up logs without a valid level (shouldn't happen, but just in case)
    try {
      const result = await LogEntryModel.deleteMany({
        timestamp: { $lt: now - (config.logRetentionDays * 24 * 60 * 60 * 1000) },
        level: { $nin: [...this.retentionPolicies.keys()] }
      });
      
      if (result.deletedCount > 0) {
        logger.info(`Cleaned up ${result.deletedCount} logs with invalid/unknown level`);
        totalDeleted += result.deletedCount;
      }
    } catch (error) {
      logger.error('Error cleaning up logs with invalid level:', error);
    }

    logger.info(`Log retention cleanup completed. Total deleted: ${totalDeleted}`);
  }

  /**
   * Get retention statistics
   */
  public async getRetentionStats(): Promise<Array<{ level: string; count: number; oldest: Date | null; newest: Date | null }>> {
    const stats: Array<{ level: string; count: number; oldest: Date | null; newest: Date | null }> = [];
    
    // Get stats for each configured level
    for (const level of this.retentionPolicies.keys()) {
      try {
        const [countResult, oldestResult, newestResult] = await Promise.all([
          LogEntryModel.countDocuments({ level }),
          LogEntryModel.findOne({ level }).sort({ timestamp: 1 }).select('timestamp'),
          LogEntryModel.findOne({ level }).sort({ timestamp: -1 }).select('timestamp')
        ]);
        
        stats.push({
          level,
          count: countResult,
          oldest: oldestResult ? new Date(oldestResult.timestamp) : null,
          newest: newestResult ? new Date(newestResult.timestamp) : null
        });
      } catch (error) {
        logger.error(`Error getting retention stats for level ${level}:`, error);
        stats.push({
          level,
          count: 0,
          oldest: null,
          newest: null
        });
      }
    }
    
    return stats;
  }

  /**
   * Update retention policy for a specific log level
   */
  public updateRetentionPolicy(level: string, days: number): void {
    if (days < 1) {
      throw new Error('Retention days must be at least 1');
    }
    
    this.retentionPolicies.set(level, days);
    logger.info(`Updated retention policy for ${level} to ${days} days`);
  }

  /**
   * Get current retention policies
   */
  public getRetentionPolicies(): Map<string, number> {
    return new Map(this.retentionPolicies);
  }
}

// Create and export singleton instance
export const retentionService = new RetentionService();