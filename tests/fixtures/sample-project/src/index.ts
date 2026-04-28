import { handleCreateWallet, handleGetBalance } from './routes/wallet';
import { Logger } from './utils/logger';

const logger = new Logger();

export function startServer(): void {
  logger.info('Server starting');
  registerRoutes();
}

function registerRoutes(): void {
  logger.info('Registering routes');
}
