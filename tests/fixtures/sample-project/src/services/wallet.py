from utils.validator import validate_address
from utils.logger import Logger


class WalletService:
    """Service for wallet operations."""

    def __init__(self):
        self.logger = Logger()

    def create_wallet(self, user_id: str) -> dict:
        """Create a new wallet."""
        self.logger.info("Creating wallet")
        validate_address(user_id)
        return self._persist_wallet(user_id)

    def get_balance(self, wallet_id: str) -> float:
        self.logger.info("Getting balance")
        return self._fetch_balance(wallet_id)

    def _persist_wallet(self, user_id: str) -> dict:
        return {"id": "1", "user_id": user_id, "balance": 0}

    def _fetch_balance(self, wallet_id: str) -> float:
        return 100.0
