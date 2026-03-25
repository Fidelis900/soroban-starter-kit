/**
 * Error Handler
 * Manages error categorization, logging, and recovery
 */

import { loggingService } from '../../services/logging';

export type ErrorCategory = 'network' | 'validation' | 'auth' | 'permission' | 'server' | 'client' | 'unknown';
export type ErrorSeverity = 'low' | 'medium' | 'high' | 'critical';

export interface ErrorInfo {
  id: string;
  message: string;
  category: ErrorCategory;
  severity: ErrorSeverity;
  timestamp: number;
  stack?: string;
  context?: Record<string, any>;
  retryable: boolean;
  retryCount: number;
}

export interface ErrorReport {
  errors: ErrorInfo[];
  timestamp: number;
  userAgent: string;
  url: string;
}

class ErrorHandler {
  private errors: ErrorInfo[] = [];
  private maxErrors = 100;
  private listeners: Set<(error: ErrorInfo) => void> = new Set();
  private retryHandlers: Map<string, () => Promise<void>> = new Map();

  /**
   * Categorize error
   */
  private categorizeError(error: Error | string): ErrorCategory {
    const message = typeof error === 'string' ? error : error.message;

    if (message.includes('network') || message.includes('fetch') || message.includes('timeout')) {
      return 'network';
    }
    if (message.includes('validation') || message.includes('invalid')) {
      return 'validation';
    }
    if (message.includes('auth') || message.includes('unauthorized')) {
      return 'auth';
    }
    if (message.includes('permission') || message.includes('forbidden')) {
      return 'permission';
    }
    if (message.includes('500') || message.includes('server')) {
      return 'server';
    }
    if (message.includes('client') || message.includes('syntax')) {
      return 'client';
    }
    return 'unknown';
  }

  /**
   * Determine severity
   */
  private determineSeverity(category: ErrorCategory): ErrorSeverity {
    const severityMap: Record<ErrorCategory, ErrorSeverity> = {
      network: 'medium',
      validation: 'low',
      auth: 'high',
      permission: 'high',
      server: 'high',
      client: 'critical',
      unknown: 'medium',
    };
    return severityMap[category];
  }

  /**
   * Handle error
   */
  handleError(error: Error | string, context?: Record<string, any>): ErrorInfo {
    const message = typeof error === 'string' ? error : error.message;
    const category = this.categorizeError(error);
    const severity = this.determineSeverity(category);
    const retryable = category === 'network' || category === 'server';

    const errorInfo: ErrorInfo = {
      id: `error-${Date.now()}-${Math.random()}`,
      message,
      category,
      severity,
      timestamp: Date.now(),
      stack: error instanceof Error ? error.stack : undefined,
      context,
      retryable,
      retryCount: 0,
    };

    this.errors.push(errorInfo);
    if (this.errors.length > this.maxErrors) {
      this.errors.shift();
    }

    this.notifyListeners(errorInfo);
    this.logError(errorInfo);

    return errorInfo;
  }

  /**
   * Log error
   */
  private logError(error: ErrorInfo): void {
    // Log to console
    const logLevel = error.severity === 'critical' ? 'error' : 'warn';
    if (logLevel === 'error') {
      console.error(`[${error.category}] ${error.message}`, error);
    } else {
      console.warn(`[${error.category}] ${error.message}`, error);
    }
    
    // Also send to logging service
    loggingService.logError(error, 'ErrorHandler');
  }

  /**
   * Subscribe to errors
   */
  subscribe(listener: (error: ErrorInfo) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /**
   * Notify listeners
   */
  private notifyListeners(error: ErrorInfo): void {
    this.listeners.forEach(listener => {
      try {
        listener(error);
      } catch (e) {
        console.error('Error in error listener:', e);
      }
    });
  }

  /**
   * Register retry handler
   */
  registerRetryHandler(errorId: string, handler: () => Promise<void>): void {
    this.retryHandlers.set(errorId, handler);
  }

  /**
   * Retry error
   */
  async retry(errorId: string): Promise<boolean> {
    const error = this.errors.find(e => e.id === errorId);
    if (!error || !error.retryable) return false;

    const handler = this.retryHandlers.get(errorId);
    if (!handler) return false;

    try {
      error.retryCount++;
      await handler();
      return true;
    } catch (e) {
      this.handleError(e as Error, { retryAttempt: error.retryCount });
      return false;
    }
  }

  /**
   * Get errors
   */
  getErrors(): ErrorInfo[] {
    return [...this.errors];
  }

  /**
   * Get error by category
   */
  getErrorsByCategory(category: ErrorCategory): ErrorInfo[] {
    return this.errors.filter(e => e.category === category);
  }

  /**
   * Clear errors
   */
  clearErrors(): void {
    this.errors = [];
  }

  /**
   * Generate report
   */
  generateReport(): ErrorReport {
    return {
      errors: [...this.errors],
      timestamp: Date.now(),
      userAgent: navigator.userAgent,
      url: window.location.href,
    };
  }
}

export const errorHandler = new ErrorHandler();
export default errorHandler;
