import { cmd } from './client';

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

export const paymentApi = {
  confirmPayment: (_paymentKey: string, _orderId: string, _amount: number) =>
    Promise.reject(new Error('Payment confirmation is deferred until non-payment readiness gates pass.')),
    
  getSubscriptionDetails: () => 
    cmd<SubscriptionDetails>('get_subscription_details'),
    
  cancelSubscription: () => 
    cmd<void>('cancel_subscription'),
};
