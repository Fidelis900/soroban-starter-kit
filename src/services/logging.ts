import { ErrorInfo } from './error/errorHandler';

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface LogEntry {
  timestamp: number;
  level: LogLevel;
  message: string;
  source: string; // Component or service name
  context?: Record<string, any>;
  userId?: string;
  sessionId?: string;
  url?: string;
  userAgent?: string;
}

export interface LogConfig {
  endpoint: string;
  batchSize?: number;
  flushIntervalMs?: number;
  maxRetries?: number;
  enabled: boolean;
  minLevel: LogLevel;
}

class LoggingService {
  private config: LogConfig;
  private queue: LogEntry[] = [];
  private flushTimer: NodeJS.Timeout | null = null;
  private isFlushing = false;
  private readonly levelPriority: Record<LogLevel, number> = {
    debug: 0,
    info: 1,
    warn: 2,
    error: 3,
  };

  constructor(config: LogConfig) {
    this.config = config;
    this.startFlushInterval();
  }

  private startFlushInterval() {
    if (this.config.flushIntervalMs) {
      this.flushTimer = setInterval(() => {
        this.flush();
      }, this.config.flushIntervalMs);
    }
  }

  private stopFlushInterval() {
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = null;
    }
  }

  private shouldLog(level: LogLevel): boolean {
    return this.levelPriority[level] >= this.levelPriority[this.config.minLevel];
  }

   private createLogEntry(
     level: LogLevel,
     message: string,
     source: string,
     context?: Record<string, any>
   ): LogEntry {
     return {
       timestamp: Date.now(),
       level,
       message,
       source,
       context,
       userId: typeof localStorage !== 'undefined' ? localStorage.getItem('userId') ?? undefined : undefined,
       sessionId: typeof localStorage !== 'undefined' ? localStorage.getItem('sessionId') ?? undefined : undefined,
       url: typeof window !== 'undefined' ? window.location.href : undefined,
       userAgent: typeof navigator !== 'undefined' ? navigator.userAgent : undefined,
     };
   }

  private async sendLogs(logs: LogEntry[]): Promise<void> {
    if (!this.config.enabled || logs.length === 0) return;

    try {
      const response = await fetch(this.config.endpoint, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ logs }),
        credentials: 'include',
      });

      if (!response.ok) {
        throw new Error(`Failed to send logs: ${response.status}`);
      }
    } catch (error) {
      console.error('Failed to send logs to backend:', error);
      // Re-queue logs for retry
      this.queue.unshift(...logs);
      throw error;
    }
  }

  public async flush(): Promise<void> {
    if (this.isFlushing || this.queue.length === 0) return;

    this.isFlushing = true;
    const batchSize = this.config.batchSize ?? 50;
    const logsToSend = this.queue.splice(0, batchSize);

    try {
      await this.sendLogs(logsToSend);
    } finally {
      this.isFlushing = false;
      // If there are more logs, schedule another flush
      if (this.queue.length > 0 && this.flushTimer === null) {
        setTimeout(() => this.flush(), 0);
      }
    }
  }

  public debug(message: string, source: string, context?: Record<string, any>): void {
    if (!this.shouldLog('debug')) return;
    const entry = this.createLogEntry('debug', message, source, context);
    this.queue.push(entry);
    if (this.queue.length >= (this.config.batchSize ?? 50)) {
      this.flush();
    }
  }

  public info(message: string, source: string, context?: Record<string, any>): void {
    if (!this.shouldLog('info')) return;
    const entry = this.createLogEntry('info', message, source, context);
    this.queue.push(entry);
    if (this.queue.length >= (this.config.batchSize ?? 50)) {
      this.flush();
    }
  }

  public warn(message: string, source: string, context?: Record<string, any>): void {
    if (!this.shouldLog('warn')) return;
    const entry = this.createLogEntry('warn', message, source, context);
    this.queue.push(entry);
    if (this.queue.length >= (this.config.batchSize ?? 50)) {
      this.flush();
    }
  }

  public error(message: string, source: string, context?: Record<string, any>): void {
    if (!this.shouldLog('error')) return;
    const entry = this.createLogEntry('error', message, source, context);
    this.queue.push(entry);
    // Always flush errors immediately
    this.flush();
  }

  public logError(errorInfo: ErrorInfo, source: string): void {
    this.error(
      errorInfo.message,
      source,
      {
        ...errorInfo,
        stack: errorInfo.stack,
      }
    );
  }

  public setConfig(config: Partial<LogConfig>): void {
    this.config = { ...this.config, ...config };
    // Restart flush interval if changed
    if (config.flushIntervalMs !== undefined) {
      this.stopFlushInterval();
      this.startFlushInterval();
    }
  }

  public destroy(): void {
    this.stopFlushInterval();
    // Flush any remaining logs
    this.flush().catch(console.error);
  }
}

// Default configuration
const defaultConfig: LogConfig = {
  // In development, proxy to backend; in production, use relative path
  endpoint: 
    (typeof process !== 'undefined' && process.env.NODE_ENV === 'development') 
      ? 'http://localhost:3001/api/logs' 
      : '/api/logs',
  batchSize: 50,
  flushIntervalMs: 5000, // 5 seconds
  maxRetries: 3,
  enabled: !(typeof process !== 'undefined' && process.env.NODE_ENV === 'development'), // Disable in dev by default
  minLevel: (typeof process !== 'undefined' && process.env.NODE_ENV === 'development') ? 'debug' : 'info',
};

// Create and export singleton instance
export const loggingService = new LoggingService(defaultConfig);

export default loggingService;