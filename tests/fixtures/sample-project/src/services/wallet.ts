import { validateAddress } from '../utils/validator';
import { Logger } from '../utils/logger';

export class WalletService {
  private logger: Logger;

  constructor() {
    this.logger = new Logger();
  }

  async createWallet(userId: string): Promise<Wallet> {
    this.logger.info('Creating wallet');
    validateAddress(userId);
    return this.persistWallet(userId);
  }

  async getBalance(walletId: string): Promise<number> {
    this.logger.info('Getting balance');
    return this.fetchBalance(walletId);
  }

  private async persistWallet(userId: string): Promise<Wallet> {
    return { id: '1', userId, balance: 0 };
  }

  private async fetchBalance(walletId: string): Promise<number> {
    return 100;
  }
}

interface Wallet {
  id: string;
  userId: string;
  balance: number;
}
