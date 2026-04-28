import { WalletService } from '../services/wallet';

const walletService = new WalletService();

export async function handleCreateWallet(req: Request): Promise<Response> {
  const { userId } = req.body;
  const wallet = await walletService.createWallet(userId);
  return { status: 200, body: wallet };
}

export async function handleGetBalance(req: Request): Promise<Response> {
  const { walletId } = req.params;
  const balance = await walletService.getBalance(walletId);
  return { status: 200, body: { balance } };
}

interface Request {
  body: any;
  params: any;
}

interface Response {
  status: number;
  body: any;
}
