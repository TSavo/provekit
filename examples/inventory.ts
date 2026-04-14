const db = {
  getStock: (productId: string): number => 0,
  getAvailable: (productId: string): number => 0,
  getReserved: (productId: string): number => 0,
  setAvailable: (productId: string, value: number): void => {},
  setReserved: (productId: string, value: number): void => {},
};

export function checkAvailability(productId: string): number {
  const available = db.getStock(productId);
  console.log(`Stock check for ${productId}: ${available} available`);
  return available;
}

export function reserveStock(productId: string, quantity: number): void {
  const available = db.getAvailable(productId);
  const reserved = db.getReserved(productId);
  console.log(`Reserving ${quantity} of ${productId} (${available} available, ${reserved} reserved)`);

  db.setAvailable(productId, available - quantity);
  db.setReserved(productId, reserved + quantity);

  const newAvailable = available - quantity;
  const newReserved = reserved + quantity;
  console.log(`Reservation complete: ${newAvailable} available, ${newReserved} reserved`);
}

export function releaseStock(productId: string, quantity: number): void {
  const available = db.getAvailable(productId);
  const reserved = db.getReserved(productId);
  console.log(`Releasing ${quantity} of ${productId} back to stock`);

  db.setAvailable(productId, available + quantity);
  db.setReserved(productId, reserved - quantity);

  console.log(`Release complete: ${available + quantity} available, ${reserved - quantity} reserved`);
}
