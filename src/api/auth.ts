import { cmd } from "./client";

export interface User {
  id: string;
  email: string;
  tier: "Free" | "Pro";
  expires_at: number;
}

export interface EntitlementInfo {
  tier: "FREE" | "PRO";
  status: "active" | "inactive" | "expired" | "cancelled" | "past_due" | "none";
  expires_at: string | null;
  source: "supabase";
  checked_at: string;
  payment_available: boolean;
}

export interface SessionSyncResponse {
  user: User;
  entitlement: EntitlementInfo;
}

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

export const authApi = {
  logout: () => cmd<void>("logout"),

  getCurrentEntitlement: (forceRefresh = false) =>
    cmd<EntitlementInfo>("get_current_entitlement", {
      force_refresh: forceRefresh,
    }),

  setSession: (
    accessToken: string,
    refreshToken: string,
    userId: string,
    email: string,
    expiresAt?: number | null,
  ) =>
    cmd<SessionSyncResponse>("set_session", {
      access_token: accessToken,
      refresh_token: refreshToken,
      user_id: userId,
      email,
      expires_at: expiresAt ?? null,
    }),

  // Subscription/Payment APIs
  getSubscriptionDetails: () =>
    cmd<SubscriptionDetails>("get_subscription_details"),

  openPaymentPage: (period: "MONTHLY" | "YEARLY" = "MONTHLY") =>
    cmd<string>("open_payment_page", { period }),

  cancelSubscription: () => cmd<SubscriptionDetails>("cancel_subscription"),

  confirmPayment: (paymentKey: string, orderId: string, amount: number) =>
    cmd<SubscriptionDetails>("confirm_payment", {
      payment_key: paymentKey,
      order_id: orderId,
      amount,
    }),
};
