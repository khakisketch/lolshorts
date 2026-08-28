import { cmd } from "./client";

export interface SubscriptionDetails {
  is_active: boolean;
  tier: string;
  status: string;
  expires_at: string | null;
  auto_renew: boolean;
  payment_method: string | null;
  payment_available: boolean;
  payment_message: string | null;
  reason: string;
  next_required_step: string;
  next_billing_date?: string | null;
  cancel_at_period_end?: boolean;
  provider?: string | null;
  checkout_url?: string | null;
  last_payment_error?: string | null;
}

export const paymentApi = {
  confirmPayment: (paymentKey: string, orderId: string, amount: number) =>
    cmd<SubscriptionDetails>("confirm_payment", {
      payment_key: paymentKey,
      order_id: orderId,
      amount,
    }),

  getSubscriptionDetails: () =>
    cmd<SubscriptionDetails>("get_subscription_details"),

  cancelSubscription: () => cmd<SubscriptionDetails>("cancel_subscription"),

  openPaymentPage: (period: "MONTHLY" | "YEARLY" = "MONTHLY") =>
    cmd<string>("open_payment_page", { period }),
};
