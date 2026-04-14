interface Order {
  id: string;
  items: { productId: string; unitPrice: number; quantity: number }[];
  subtotal: number;
  couponDiscount: number;
  amountCharged: number;
  status: string;
}

const db = {
  getOrder: (orderId: string): Order => ({} as Order),
  saveOrder: (order: Order): void => {},
  getPaymentRecord: (orderId: string): { amount: number; method: string } => ({ amount: 0, method: "" }),
};

const paymentGateway = {
  refund: (amount: number, method: string): { success: boolean; txId: string } =>
    ({ success: false, txId: "" }),
};

export function processRefund(orderId: string): void {
  const order = db.getOrder(orderId);
  console.log(`Processing refund for order ${orderId}`);

  // Calculate refund from item prices — ignores coupon discount
  let refundAmount = 0;
  for (const item of order.items) {
    refundAmount += item.unitPrice * item.quantity;
  }
  console.log(`Refund amount calculated: $${refundAmount.toFixed(2)}`);

  const payment = db.getPaymentRecord(orderId);
  const result = paymentGateway.refund(refundAmount, payment.method);
  console.log(`Refund ${result.success ? "succeeded" : "failed"}: txId=${result.txId}`);

  order.status = "refunded";
  db.saveOrder(order);
  console.log(`Order ${orderId} marked as refunded`);
}
