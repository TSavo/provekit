type OrderState = "draft" | "submitted" | "approved" | "shipped" | "cancelled";

interface Order {
  id: string;
  state: OrderState;
  items: { sku: string; qty: number }[];
  approvedBy: string | null;
  trackingNumber: string | null;
}

const VALID_TRANSITIONS: Record<OrderState, OrderState[]> = {
  draft: ["submitted", "cancelled"],
  submitted: ["approved", "cancelled"],
  approved: ["shipped", "cancelled"],
  shipped: [],
  cancelled: [],
};

export function submitOrder(order: Order): void {
  order.state = "submitted";
  console.log(`Order ${order.id} submitted with ${order.items.length} items`);
  // BUG: no check that order.state === "draft" before transitioning
}

export function approveOrder(order: Order, approver: string): void {
  order.approvedBy = approver;
  order.state = "approved";
  console.log(`Order ${order.id} approved by ${approver}`);
  // BUG: can approve a cancelled or shipped order
}

export function shipOrder(order: Order, tracking: string): void {
  order.trackingNumber = tracking;
  order.state = "shipped";
  console.log(`Order ${order.id} shipped, tracking: ${tracking}`);
  // BUG: can ship a draft order, skipping submit and approve
}

export function cancelOrder(order: Order, reason: string): void {
  console.log(`Order ${order.id} cancelled: ${reason}, was in state ${order.state}`);
  order.state = "cancelled";
  // BUG: can cancel a shipped order
}
