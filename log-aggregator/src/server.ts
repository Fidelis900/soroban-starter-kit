import express, { Request, Response, NextFunction } from 'express';
import cors from 'cors';
import helmet from 'helmet';
import mongoose from 'mongoose';
import dotenv from 'dotenv';
import winston from 'winston';
import DailyRotateFile from 'winston-daily-rotate-file';
import { z } from 'zod';
import { config } from './config';
import { retentionService } from './retention.service';
import { anomalyDetectionService } from './anomaly.detection';
import { indexerService } from './indexer.service';

// Load environment variables
dotenv.config();

// Define LogEntry type locally since we can't import from frontend
export interface LogEntry {
  timestamp: number;
  level: 'debug' | 'info' | 'warn' | 'error';
  message: string;
  source: string;
  context?: Record<string, any>;
  userId?: string;
  sessionId?: string;
  url?: string;
  userAgent?: string;
}

// Initialize Express app
const app = express();
const PORT = config.port;

// Middleware
app.use(helmet());
app.use(cors());
app.use(express.json({ limit: '10mb' }));
app.use(express.urlencoded({ extended: true, limit: '10mb' }));

// Winston logger configuration
const logger = winston.createLogger({
  level: config.winston.level,
  format: winston.format.combine(
    winston.format.timestamp(),
    winston.format.errors({ stack: true }),
    winston.format.splat(),
    winston.format.json()
  ),
  transports: [
    ...(config.winston.console.enabled ? [
      new winston.transports.Console({
        format: winston.format.combine(
          winston.format.colorize(),
          winston.format.simple()
        )
      })
    ] : []),
    ...(config.winston.file.enabled ? [
      new DailyRotateFile({
        filename: config.winston.file.filename,
        datePattern: config.winston.file.datePattern,
        zippedArchive: config.winston.file.zippedArchive,
        maxSize: config.winston.file.maxSize,
        maxFiles: config.winston.file.maxFiles
      })
    ] : [])
  ]
});

// MongoDB connection
mongoose.connect(config.mongodbUri)
  .then(() => {
    logger.info('Connected to MongoDB');
    // Start retention service after DB connection
    retentionService.start();
    // Start anomaly detection service after DB connection
    anomalyDetectionService.start();
    // Start indexer service after DB connection
    indexerService.start();
  })
  .catch((err) => logger.error('MongoDB connection error:', err));

// Log schema and model
const logEntrySchema = new mongoose.Schema({
  timestamp: { type: Number, required: true, index: true },
  level: { type: String, required: true, enum: ['debug', 'info', 'warn', 'error'] },
  message: { type: String, required: true },
  source: { type: String, required: true },
  context: { type: mongoose.Schema.Types.Mixed },
  userId: { type: String },
  sessionId: { type: String },
  url: { type: String },
  userAgent: { type: String },
  receivedAt: { type: Date, default: Date.now, index: true }
});

// Create indexes for better query performance
logEntrySchema.index({ timestamp: 1, level: 1 });
logEntrySchema.index({ source: 1, timestamp: -1 });
logEntrySchema.index({ userId: 1, timestamp: -1 });

const LogEntryModel = mongoose.model('LogEntry', logEntrySchema);

// Export model for use in other services
export { LogEntryModel };

// Validation schema for incoming logs
const logEntrySchemaZod = z.object({
  timestamp: z.number(),
  level: z.enum(['debug', 'info', 'warn', 'error']),
  message: z.string(),
  source: z.string(),
  context: z.record(z.any()).optional(),
  userId: z.string().optional(),
  sessionId: z.string().optional(),
  url: z.string().optional(),
  userAgent: z.string().optional()
});

// Health check endpoint
app.get('/health', (req: Request, res: Response) => {
  res.status(200).json({ status: 'OK', timestamp: Date.now() });
});

// Log ingestion endpoint
app.post('/api/logs', async (req: Request, res: Response, next: NextFunction) => {
  try {
    // Validate request body
    const { logs } = req.body;
    
    if (!Array.isArray(logs)) {
      return res.status(400).json({ error: 'Expected logs array' });
    }

    // Validate each log entry
    const validatedLogs = logs.map((log: any) => {
      const parsed = logEntrySchemaZod.safeParse(log);
      if (!parsed.success) {
        logger.warn('Invalid log entry received:', { error: parsed.error.format(), log });
        return null;
      }
      return parsed.data;
    }).filter((log): log is LogEntry => log !== null);

    if (validatedLogs.length === 0) {
      return res.status(400).json({ error: 'No valid log entries provided' });
    }

    // Store logs in MongoDB
    const insertedLogs = await LogEntryModel.insertMany(validatedLogs);
    
    // Log to Winston for monitoring
    logger.info(`Received and stored ${insertedLogs.length} log entries`, {
      count: insertedLogs.length,
      sources: [...new Set(insertedLogs.map(log => log.source))],
      levels: [...new Set(insertedLogs.map(log => log.level))]
    });

    res.status(200).json({ 
      status: 'success', 
      received: logs.length,
      stored: insertedLogs.length
    });
  } catch (error) {
    logger.error('Error processing logs:', error);
    next(error);
  }
});

// Log query endpoint
app.get('/api/logs', async (req: Request, res: Response, next: NextFunction) => {
  try {
    const {
      startTime,
      endTime,
      level,
      source,
      userId,
      sessionId,
      limit = 100,
      offset = 0,
      sort = 'desc'
    } = req.query;

    // Build query
    const query: any = {};

    if (startTime) {
      query.timestamp = { ...query.timestamp, ...{ $gte: Number(startTime) } };
    }
    if (endTime) {
      query.timestamp = { ...query.timestamp, ...{ $lte: Number(endTime) } };
    }
    if (level) {
      query.level = level;
    }
    if (source) {
      query.source = source;
    }
    if (userId) {
      query.userId = userId;
    }
    if (sessionId) {
      query.sessionId = sessionId;
    }

    // Execute query
    const logs = await LogEntryModel.find(query)
      .sort({ timestamp: sort === 'desc' ? -1 : 1 })
      .skip(Number(offset))
      .limit(Number(limit))
      .lean();

    const total = await LogEntryModel.countDocuments(query);

    res.status(200).json({
      logs,
      pagination: {
        total,
        limit: Number(limit),
        offset: Number(offset),
        hasMore: Number(offset) + Number(limit) < total
      }
    });
  } catch (error) {
    logger.error('Error querying logs:', error);
    next(error);
  }
});

// Log statistics endpoint
app.get('/api/logs/stats', async (req: Request, res: Response, next: NextFunction) => {
  try {
    const {
      startTime,
      endTime,
      source
    } = req.query;

    // Build match stage
    const matchStage: any = {};
    if (startTime) {
      matchStage.timestamp = { ...matchStage.timestamp, ...{ $gte: Number(startTime) } };
    }
    if (endTime) {
      matchStage.timestamp = { ...matchStage.timestamp, ...{ $lte: Number(endTime) } };
    }
    if (source) {
      matchStage.source = source;
    }

    // Aggregation pipeline
    const pipeline = [];
    if (Object.keys(matchStage).length > 0) {
      pipeline.push({ $match: matchStage });
    }
    
    pipeline.push(
      {
        $group: {
          _id: {
            level: '$level',
            source: '$source'
          },
          count: { $sum: 1 }
        }
      },
      {
        $group: {
          _id: '$_id.source',
          levels: {
            $push: {
              level: '$_id.level',
              count: '$count'
            }
          },
          total: { $sum: '$count' }
        }
      }
    );

    const stats = await LogEntryModel.aggregate(pipeline);
    
    res.status(200).json({ stats });
  } catch (error) {
    logger.error('Error getting log stats:', error);
    next(error);
  }
});

// Log retention policy endpoint (for manual cleanup)
app.delete('/api/logs/retention', async (req: Request, res: Response, next: NextFunction) => {
  try {
    const cutoffDate = Date.now() - (config.logRetentionDays * 24 * 60 * 60 * 1000);
    
    // Delete logs older than retention period
    const result = await LogEntryModel.deleteMany({
      timestamp: { $lt: cutoffDate }
    });
    
    logger.info(`Log retention cleanup completed`, {
      deletedCount: result.deletedCount,
      cutoffDate: new Date(cutoffDate).toISOString()
    });
    
    res.status(200).json({
      status: 'success',
      deletedCount: result.deletedCount,
      cutoffDate: new Date(cutoffDate).toISOString()
    });
  } catch (error) {
    logger.error('Error during log retention cleanup:', error);
    next(error);
  }
});

// Retention stats endpoint
app.get('/api/logs/retention/stats', async (req: Request, res: Response, next: NextFunction) => {
  try {
    const stats = await retentionService.getRetentionStats();
    res.status(200).json({ stats });
  } catch (error) {
    logger.error('Error getting retention stats:', error);
    next(error);
  }
});

// Anomaly detection stats endpoint
app.get('/api/logs/anomaly/stats', async (req: Request, res: Response, next: NextFunction) => {
  try {
    const stats = anomalyDetectionService.getBaselineMetrics();
    // Convert Map to serializable format
    const serializableStats: Record<string, { avgCount: number; stdDev: number; lastUpdate: number }> = {};
    stats.forEach((value, key) => {
      serializableStats[key] = value;
    });
    res.status(200).json({ stats: serializableStats });
  } catch (error) {
    logger.error('Error getting anomaly detection stats:', error);
    next(error);
  }
});

// Indexer status endpoint
app.get('/api/indexer/status', async (req: Request, res: Response, next: NextFunction) => {
  try {
    const status = indexerService.getStatus();
    res.status(200).json({ status });
  } catch (error) {
    logger.error('Error getting indexer status:', error);
    next(error);
  }
});

// Error handling middleware
app.use((err: Error, req: Request, res: Response, next: NextFunction) => {
  logger.error('Unhandled error:', err);
  res.status(500).json({ error: 'Internal server error' });
});

// 404 handler
app.use((req: Request, res: Response) => {
  res.status(404).json({ error: 'Not found' });
});

// Start server
const server = app.listen(PORT, () => {
  logger.info(`Log aggregator server running on port ${PORT}`);
});

// Graceful shutdown
process.on('SIGTERM', () => {
  logger.info('SIGTERM received, shutting down gracefully');
  server.close(async () => {
    logger.info('Process terminated');
    await mongoose.disconnect();
    // Stop retention service
    retentionService.stop();
    // Stop anomaly detection service
    anomalyDetectionService.stop();
    // Stop indexer service
    indexerService.stop();
    process.exit(0);
  });
});

process.on('SIGINT', () => {
  logger.info('SIGINT received, shutting down gracefully');
  server.close(async () => {
    logger.info('Process terminated');
    await mongoose.disconnect();
    // Stop retention service
    retentionService.stop();
    // Stop anomaly detection service
    anomalyDetectionService.stop();
    // Stop indexer service
    indexerService.stop();
    process.exit(0);
  });
});

export default app;