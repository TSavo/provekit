import { checkAvailability, reserveStock, releaseStock } from "./inventory";

interface CartItem {
  productId: string;
  quantity: number;
  basePrice: number;
}

interface Order {
  id: string;
  items: CartItem[];
  status: string;
}

export function placeOrder(customerId: string, cart: CartItem[]): number {
  let orderTotal = 0;
  for (const item of cart) {
    const available = checkAvailability(item.productId);
    const lineTotal = item.basePrice * item.quantity;
    orderTotal += lineTotal;
  }
  console.log(`Order total for ${customerId}: $${orderTotal.toFixed(2)}`);

  for (const item of cart) {
    reserveStock(item.productId, item.quantity);
  }
  console.log(`Order placed for ${customerId}, ${cart.length} items reserved`);

  return orderTotal;
}

export function processRefund(order: Order): void {
  let refundAmount = 0;
  for (const item of order.items) {
    refundAmount += item.basePrice * item.quantity;
  }
  console.log(`Refund amount: $${refundAmount.toFixed(2)}`);

  for (const item of order.items) {
    releaseStock(item.productId, item.quantity);
  }
  order.status = "refunded";
  console.log(`Order ${order.id} refunded`);
}
