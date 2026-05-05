import { cmd } from './client';

export interface User {
  id: string;
  email: string;
  tier: 'Free' | 'Pro';
  expires_at: number;
}

export interface EntitlementInfo {
  tier: 'FREE' | 'PRO';
  status: 'active' | 'inactive' | 'expired' | 'cancelled' | 'none';
  expires_at: string | null;
  source: 'supabase';
  checked_at: string;
  payment_available: boolean;
}

export interface SessionSyncResponse {
  user: User;
  entitlement: EntitlementInfo;
}

export interface LicenseInfo {
  tier: string;
  expires_at: string | null;
  is_active: boolean;
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
}

export const authApi = {
  login: (email: string, password: string) =>
    cmd<User>('login', { email, password }),

  signup: (email: string, password: string) =>
    cmd<User>('signup', { email, password }),

  logout: () =>
    cmd<void>('logout'),

  getUserStatus: () =>
    cmd<User | null>('get_user_status'),

  getLicenseInfo: () =>
    cmd<LicenseInfo | null>('get_license_info'),

  getUserLicense: () =>
    cmd<LicenseInfo>('get_user_license'),

  getCurrentEntitlement: (forceRefresh = false) =>
    cmd<EntitlementInfo>('get_current_entitlement', { force_refresh: forceRefresh }),

  refreshToken: () =>
    cmd<User>('refresh_token'),

  setSession: (accessToken: string, refreshToken: string, userId: string, email: string, expiresAt?: number | null) =>
    cmd<SessionSyncResponse>('set_session', { access_token: accessToken, refresh_token: refreshToken, user_id: userId, email, expires_at: expiresAt ?? null }),

  // Subscription/Payment APIs
  getSubscriptionDetails: () =>
    cmd<SubscriptionDetails>('get_subscription_details'),

  openPaymentPage: () =>
    cmd<string>('open_payment_page'),

  cancelSubscription: () =>
    cmd<void>('cancel_subscription'),
};
