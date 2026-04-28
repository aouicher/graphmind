export function validateAddress(address: string): boolean {
  if (!address || address.length < 3) {
    throw new Error('Invalid address');
  }
  return true;
}

export function sanitizeInput(input: string): string {
  return input.trim().toLowerCase();
}
