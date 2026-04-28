use crate::utils::validator::validate_address;
use crate::utils::logger::Logger;

pub struct Wallet {
    pub id: String,
    pub user_id: String,
    pub balance: f64,
}

pub enum WalletError {
    NotFound,
    InvalidAddress,
}

/// Trait for wallet operations
pub trait WalletOps {
    fn create_wallet(&self, user_id: &str) -> Result<Wallet, WalletError>;
    fn get_balance(&self, wallet_id: &str) -> Result<f64, WalletError>;
}

pub struct WalletService {
    logger: Logger,
}

impl WalletService {
    pub fn new() -> Self {
        Self { logger: Logger::new() }
    }

    pub fn create_wallet(&self, user_id: &str) -> Result<Wallet, WalletError> {
        self.logger.info("Creating wallet");
        validate_address(user_id);
        self.persist_wallet(user_id)
    }

    pub fn get_balance(&self, wallet_id: &str) -> Result<f64, WalletError> {
        self.logger.info("Getting balance");
        self.fetch_balance(wallet_id)
    }

    fn persist_wallet(&self, user_id: &str) -> Result<Wallet, WalletError> {
        Ok(Wallet {
            id: "1".to_string(),
            user_id: user_id.to_string(),
            balance: 0.0,
        })
    }

    fn fetch_balance(&self, wallet_id: &str) -> Result<f64, WalletError> {
        Ok(100.0)
    }
}
