require_relative '../utils/validator'
require_relative '../utils/logger'

# WalletService handles wallet operations
class WalletService
  def initialize
    @logger = Logger.new
  end

  def create_wallet(user_id)
    @logger.info("Creating wallet")
    validate_address(user_id)
    persist_wallet(user_id)
  end

  def get_balance(wallet_id)
    @logger.info("Getting balance")
    fetch_balance(wallet_id)
  end

  private

  def persist_wallet(user_id)
    { id: "1", user_id: user_id, balance: 0 }
  end

  def fetch_balance(wallet_id)
    100.0
  end
end
