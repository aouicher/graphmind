package services

import (
	"fmt"
	"myapp/utils"
)

type Wallet struct {
	ID      string
	UserID  string
	Balance float64
}

type WalletService struct {
	logger *utils.Logger
}

// NewWalletService creates a new wallet service
func NewWalletService() *WalletService {
	return &WalletService{logger: utils.NewLogger()}
}

func (s *WalletService) CreateWallet(userID string) (*Wallet, error) {
	s.logger.Info("Creating wallet")
	utils.ValidateAddress(userID)
	return s.persistWallet(userID)
}

func (s *WalletService) GetBalance(walletID string) (float64, error) {
	s.logger.Info("Getting balance")
	return s.fetchBalance(walletID)
}

func (s *WalletService) persistWallet(userID string) (*Wallet, error) {
	return &Wallet{ID: "1", UserID: userID, Balance: 0}, nil
}

func (s *WalletService) fetchBalance(walletID string) (float64, error) {
	fmt.Println("fetching")
	return 100.0, nil
}
