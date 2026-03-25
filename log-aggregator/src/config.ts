import * as dotenv from 'dotenv';

dotenv.config();

export const config = {
  port: parseInt(process.env.PORT || '3001', 10),
  mongodbUri: process.env.MONGODB_URI || 'mongodb://localhost:27017/soroban-logs',
  logRetentionDays: parseInt(process.env.LOG_RETENTION_DAYS || '30', 10),
  logRetentionLevels: process.env.LOG_RETENTION_LEVELS ? 
    process.env.LOG_RETENTION_LEVELS.split(',') as Array<'debug' | 'info' | 'warn' | 'error'> : 
    ['debug', 'info', 'warn', 'error'],
  environment: process.env.NODE_ENV || 'development',
  sorobanRpcUrl: process.env.SOROBAN_RPC_URL || 'https://soroban-testnet.stellar.org',
  contractAddresses: process.env.CONTRACT_ADDRESSES || '', // Comma-separated list of contract addresses to monitor
  indexerPollingIntervalMs: parseInt(process.env.INDEXER_POLLING_INTERVAL_MS || '10000', 10), // 10 seconds default
  serverUrl: process.env.SERVER_URL || `http://localhost:${parseInt(process.env.PORT || '3001', 10)}`,
  winston: {
    level: process.env.WINSTON_LEVEL || 'info',
    console: {
      enabled: process.env.WINSTON_CONSOLE_ENABLED !== 'false'
    },
    file: {
      enabled: process.env.WINSTON_FILE_ENABLED !== 'false',
      filename: process.env.WINSTON_FILE_FILENAME || 'logs/application-%DATE%.log',
      datePattern: process.env.WINSTON_FILE_DATE_PATTERN || 'YYYY-MM-DD',
      zippedArchive: process.env.WINSTON_FILE_ZIPPED_ARCHIVE === 'true',
      maxSize: process.env.WINSTON_FILE_MAX_SIZE || '20m',
      maxFiles: process.env.WINSTON_FILE_MAX_FILES || '14d'
    }
  }
};