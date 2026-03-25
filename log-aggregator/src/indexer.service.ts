import { Server } from '@stellar/stellar-sdk/rpc';
import { Networks } from '@stellar/stellar-sdk';
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
 * Indexer Service
 * Polls Soroban network for contract events and indexes them
 */
export class IndexerService {
  private readonly server: Server;
  private readonly contractAddresses: string[];
  private readonly pollingIntervalMs: number;
  private pollingTimer: any = null;
  private isRunning = false;
  private lastLedger: number = 0;

  constructor() {
     // Initialize Soroban RPC server
     this.server = new Server(config.sorobanRpcUrl || 'https://soroban-testnet.stellar.org');
    
    // Contract addresses to monitor (from environment or default)
    this.contractAddresses = config.contractAddresses ? 
      config.contractAddresses.split(',') : 
      []; // Empty array means monitor all contracts (not recommended for production)
    
    // Default polling interval: 10 seconds
    this.pollingIntervalMs = config.indexerPollingIntervalMs || 10 * 1000;
  }

  /**
   * Start the indexer service
   */
  public async start(): Promise<void> {
    if (this.isRunning) {
      logger.warn('Indexer service is already running');
      return;
    }

    this.isRunning = true;
    logger.info('Starting Soroban event indexer service');
    
    try {
      // Get current ledger to start from
      const latest = await this.server.getLatestLedger();
      this.lastLedger = latest.sequence;
      logger.info(`Starting indexer from ledger ${this.lastLedger}`);
    } catch (error) {
      logger.error('Failed to get initial ledger:', error);
      // Start from ledger 1 if we can't get the latest
      this.lastLedger = 1;
    }
    
    // Start polling for events
    this.pollingTimer = setInterval(async () => {
      try {
        await this.pollForEvents();
      } catch (error) {
        logger.error('Error during event polling:', error);
      }
    }, this.pollingIntervalMs);
    
    // Do an initial poll
    await this.pollForEvents();
  }

  /**
   * Stop the indexer service
   */
  public stop(): void {
    if (!this.isRunning) {
      logger.warn('Indexer service is not running');
      return;
    }

    this.isRunning = false;
    logger.info('Stopping Soroban event indexer service');
    
    if (this.pollingTimer) {
      clearInterval(this.pollingTimer);
      this.pollingTimer = null;
    }
  }

    /**
    * Poll for new events since last ledger
    */
    private async pollForEvents(): Promise<void> {
      try {
        // Get events since last ledger
        const eventsResponse = await this.server.getEvents({
          // For now, we'll get all events and filter by contract
          // In a production system, you'd want to use more specific filters
          cursor: this.lastLedger.toString(),
          limit: 1000,
          filters: [] // Empty filters means no filtering
        });

        // The Stellar SDK returns events in the events property
        const events = eventsResponse.events;
        if (events.length > 0) {
          logger.info(`Found ${events.length} new events`);
          
          // Process each event
          for (const event of events) {
            await this.processEvent(event);
          }
          
          // Update last ledger
          if (events.length > 0) {
            const latestEventLedger = Math.max(...events.map((e: any) => e.ledger));
            this.lastLedger = latestEventLedger;
          }
        }
      } catch (error) {
        logger.error('Error polling for events:', error);
        // Don't update lastLedger on error to avoid skipping events
      }
    }

  /**
   * Process a single event and send it to the log aggregator
   */
  private async processEvent(event: any): Promise<void> {
    try {
      // Check if we should monitor this contract
      if (this.contractAddresses.length > 0 && 
          !this.contractAddresses.includes(event.contract_id)) {
        return; // Skip events from contracts we're not monitoring
      }

      // Convert event to log entry format
      const logEntry: any = {
        timestamp: event.ledger_close_time ? new Date(event.ledger_close_time).getTime() : Date.now(),
        level: 'info',
        message: `Contract event: ${event.type} from ${event.contract_id}`,
        source: 'SorobanIndexer',
        context: {
          contractId: event.contract_id,
          eventType: event.type,
          eventId: event.id,
          ledger: event.ledger,
          pagingToken: event.paging_token,
          // Include the event value if it's not too large
          ...(event.value && typeof event.value === 'object' && JSON.stringify(event.value).length < 1000 
            ? { eventValue: event.value } 
            : { eventValuePresent: !!event.value })
        }
      };

      // Send to our log aggregator via HTTP (since we're in the same service)
      // In a production setup, you might use a message queue or direct database write
      await this.sendLogToAggregator(logEntry);
      
      logger.debug(`Processed event from contract ${event.contract_id}`, {
        eventId: event.id,
        ledger: event.ledger
      });
    } catch (error) {
      logger.error(`Error processing event ${event.id}:`, error);
    }
  }

  /**
   * Send log entry to our log aggregator service
   */
  private async sendLogToAggregator(logEntry: any): Promise<void> {
    try {
      // Since we're in the same service, we could directly store it
      // But to keep the architecture clean, we'll send it via HTTP to our own endpoint
      const response = await fetch(`${config.serverUrl || `http://localhost:${config.port}`}/api/logs`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ logs: [logEntry] }),
      });

      if (!response.ok) {
        throw new Error(`Failed to send log to aggregator: ${response.status}`);
      }
    } catch (error) {
      logger.error('Error sending log to aggregator:', error);
      // Fallback: log locally using our frontend logging service (if available)
      try {
        // This is a bit of a hack since we're in backend but importing frontend service
        // In a real system, you'd have a shared logging library or direct DB access
        console.warn('Fallback logging due to aggregator send failure:', logEntry);
      } catch (fallbackError) {
        logger.error('Fallback logging also failed:', fallbackError);
      }
    }
  }

  /**
   * Get current status
   */
  public getStatus(): {
    running: boolean;
    lastLedger: number;
    contractAddresses: string[];
    pollingIntervalMs: number;
  } {
    return {
      running: this.isRunning,
      lastLedger: this.lastLedger,
      contractAddresses: this.contractAddresses,
      pollingIntervalMs: this.pollingIntervalMs
    };
  }
}

// Create and export singleton instance
export const indexerService = new IndexerService();