import "@supabase/functions-js/edge-runtime.d.ts";

type JsonObject = Record<string, unknown>;
type BillingPeriod = "MONTHLY" | "YEARLY";

interface AuthUser {
  id: string;
  email?: string;
}

interface LicenseRecord {
  id: string;
  user_id: string;
  tier: string;
  status: string;
  expires_at: string | null;
}

interface PaymentRecord {
  id: string;
  user_id: string;
  subscription_id?: string | null;
  order_id: string;
  payment_key?: string | null;
  amount: number;
  status: string;
  method?: string | null;
  provider_data?: JsonObject | null;
}

interface SubscriptionRecord {
  id: string;
  user_id: string;
  license_id: string;
  period: BillingPeriod;
  amount: number;
  status: string;
  provider?: string | null;
  provider_subscription_id?: string | null;
  current_period_end?: string | null;
  cancel_at_period_end?: boolean | null;
  payment_method?: string | null;
  failure_reason?: string | null;
  metadata?: JsonObject | null;
}

interface EntitlementSnapshot {
  tier: "FREE" | "PRO";
  status: "active" | "inactive" | "expired" | "cancelled" | "past_due" | "none";
  expires_at: string | null;
}

const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers":
    "authorization, x-client-info, apikey, content-type, idempotency-key, x-lolshorts-webhook-secret",
  "Access-Control-Allow-Methods": "GET,POST,OPTIONS",
};

const PAYMENT_DISABLED_REASON =
  "Payment is disabled until non-payment E5 Field QA and paid QA gates are approved.";
const PAYMENT_DISABLED_NEXT_STEP =
  "Set LOLSHORTS_PAYMENT_ENABLED=true only after Toss sandbox/live payment QA, webhook replay, refund/cancel, and release approval pass.";

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") {
    return new Response("ok", { headers: CORS_HEADERS });
  }

  try {
    const url = new URL(req.url);
    const route = normalizeRoute(url.pathname);

    if (route === "/webhook/toss") {
      return await handleTossWebhook(req, url);
    }

    const user = await requireUser(req);

    if (route === "/subscription" && req.method === "GET") {
      return jsonResponse(await getSubscriptionResponse(user));
    }

    if (req.method !== "POST") {
      return errorResponse(405, "METHOD_NOT_ALLOWED", "Unsupported billing method.");
    }

    switch (route) {
      case "/checkout":
        return await createCheckout(user, await readJson(req));
      case "/confirm":
        return await confirmPayment(user, await readJson(req));
      case "/cancel":
        return await cancelSubscription(user);
      case "/sync-entitlement":
        return jsonResponse(await getSubscriptionResponse(user));
      default:
        return errorResponse(404, "NOT_FOUND", "Unknown billing route.");
    }
  } catch (error) {
    const message = error instanceof HttpError
      ? error.message
      : "Billing request failed before it could be completed.";
    const status = error instanceof HttpError ? error.status : 500;
    const code = error instanceof HttpError ? error.code : "BILLING_ERROR";
    return errorResponse(status, code, message);
  }
});

function normalizeRoute(pathname: string): string {
  const stripped = pathname
    .replace(/^\/functions\/v1\/billing/, "")
    .replace(/^\/billing/, "");
  return stripped === "" ? "/" : stripped;
}

async function createCheckout(user: AuthUser, body: JsonObject): Promise<Response> {
  if (!paymentEnabled()) {
    return errorResponse(503, "PAYMENT_DISABLED", PAYMENT_DISABLED_REASON, PAYMENT_DISABLED_NEXT_STEP);
  }

  const period = normalizePeriod(body.period);
  const amount = amountForPeriod(period);
  const runtime = requiredPaymentRuntime();
  const orderId = `lolshorts_${crypto.randomUUID().replaceAll("-", "")}`;
  const orderName = period === "YEARLY" ? "LoLShorts PRO Yearly" : "LoLShorts PRO Monthly";

  const payment = await restInsert<PaymentRecord>("payments", {
    user_id: user.id,
    order_id: orderId,
    amount,
    currency: "KRW",
    status: "pending",
    provider: "toss",
    idempotency_key: orderId,
    metadata: { period, user_email: user.email ?? null },
    provider_data: { period, order_name: orderName },
  });

  const tossPayment = await tossFetch<JsonObject>("/v1/payments", {
    method: "POST",
    idempotencyKey: orderId,
    body: {
      method: "CARD",
      amount,
      currency: "KRW",
      orderId,
      orderName,
      successUrl: runtime.successUrl,
      failUrl: runtime.failUrl,
    },
  });

  const checkoutUrl = readCheckoutUrl(tossPayment);
  if (!checkoutUrl) {
    await markPaymentFailed(payment.order_id, "Toss did not return a checkout URL.", tossPayment);
    return errorResponse(
      502,
      "CHECKOUT_URL_MISSING",
      "Toss checkout was created without a checkout URL.",
      "Inspect Toss create-payment response and retry after correcting provider configuration.",
    );
  }

  await restPatch<PaymentRecord>(`payments?order_id=eq.${encodeURIComponent(orderId)}`, {
    payment_key: stringOrNull(tossPayment.paymentKey),
    provider_data: { ...tossPayment, period },
    updated_at: nowIso(),
  });

  const response = await getSubscriptionResponse(user);
  response.payment_available = true;
  response.checkout_url = checkoutUrl;
  response.reason = "Checkout was created by the server-side billing function.";
  response.next_required_step = "Open the returned Toss checkout URL, then confirm entitlement from Supabase after redirect.";
  return jsonResponse(response);
}

async function confirmPayment(user: AuthUser, body: JsonObject): Promise<Response> {
  if (!paymentEnabled()) {
    return errorResponse(503, "PAYMENT_DISABLED", PAYMENT_DISABLED_REASON, PAYMENT_DISABLED_NEXT_STEP);
  }

  const paymentKey = requiredString(body.paymentKey ?? body.payment_key, "paymentKey");
  const orderId = requiredString(body.orderId ?? body.order_id, "orderId");
  const amount = requiredNumber(body.amount, "amount");
  const payment = await selectPaymentForUser(user.id, orderId);

  if (!payment) {
    return errorResponse(403, "ORDER_NOT_OWNED", "No pending payment order belongs to the authenticated user.");
  }

  if (payment.amount !== amount) {
    await markPaymentFailed(orderId, "Client amount did not match server order amount.");
    return errorResponse(400, "AMOUNT_MISMATCH", "Payment amount does not match the server-created order.");
  }

  let tossPayment: JsonObject;
  try {
    tossPayment = await tossFetch<JsonObject>("/v1/payments/confirm", {
      method: "POST",
      idempotencyKey: `confirm_${orderId}`,
      body: { paymentKey, orderId, amount },
    });
  } catch (error) {
    await markPaymentFailed(orderId, "Toss payment confirmation failed.", {
      message: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }

  await reconcileTossPayment(tossPayment, payment);
  return jsonResponse(await getSubscriptionResponse(user));
}

async function cancelSubscription(user: AuthUser): Promise<Response> {
  if (!paymentEnabled()) {
    return errorResponse(503, "PAYMENT_DISABLED", PAYMENT_DISABLED_REASON, PAYMENT_DISABLED_NEXT_STEP);
  }

  const subscription = await selectLatestSubscription(user.id);
  if (!subscription) {
    return jsonResponse(await getSubscriptionResponse(user));
  }

  await restPatch<SubscriptionRecord>(`subscriptions?id=eq.${encodeURIComponent(subscription.id)}`, {
    status: "cancelled",
    cancel_at_period_end: true,
    cancelled_at: nowIso(),
    updated_at: nowIso(),
    metadata: {
      ...(subscription.metadata ?? {}),
      cancellation_source: "server_api",
      cancellation_policy: "entitlement remains active until user_licenses.expires_at",
    },
  });

  return jsonResponse(await getSubscriptionResponse(user));
}

async function handleTossWebhook(req: Request, url: URL): Promise<Response> {
  if (!paymentEnabled()) {
    return errorResponse(503, "PAYMENT_DISABLED", PAYMENT_DISABLED_REASON, PAYMENT_DISABLED_NEXT_STEP);
  }

  verifyWebhookSecret(req, url);

  const payload = await readJson(req);
  const data = readPaymentObjectFromWebhook(payload);
  const eventId = eventIdForWebhook(req, payload, data);
  const eventType = String(payload.eventType ?? payload.type ?? "UNKNOWN");
  const orderId = stringOrNull(data.orderId);
  const paymentKey = stringOrNull(data.paymentKey);

  let eventCreated = true;
  try {
    await restInsert("billing_events", {
      provider: "toss",
      event_id: eventId,
      event_type: eventType,
      order_id: orderId,
      payment_key: paymentKey,
      status: "received",
      payload,
    });
  } catch (error) {
    if (error instanceof HttpError && error.status === 409) {
      eventCreated = false;
    } else {
      throw error;
    }
  }

  if (!eventCreated) {
    return jsonResponse({ received: true, duplicate: true });
  }

  const localPayment = orderId ? await selectPaymentByOrderId(orderId) : null;
  if (!localPayment) {
    await updateBillingEvent(eventId, {
      status: "ignored",
      error_message: "No local payment record matched the webhook orderId.",
      processed_at: nowIso(),
    });
    return jsonResponse({ received: true, ignored: true });
  }

  const authoritativePayment = paymentKey
    ? await tossFetch<JsonObject>(`/v1/payments/${encodeURIComponent(paymentKey)}`, { method: "GET" })
    : data;

  await reconcileTossPayment(authoritativePayment, localPayment);
  await updateBillingEvent(eventId, {
    user_id: localPayment.user_id,
    payment_id: localPayment.id,
    status: "processed",
    processed_at: nowIso(),
  });

  return jsonResponse({ received: true, processed: true });
}

async function reconcileTossPayment(tossPayment: JsonObject, payment: PaymentRecord): Promise<void> {
  const status = String(tossPayment.status ?? "").toUpperCase();

  if (status === "DONE") {
    await grantPaidEntitlement(payment, tossPayment);
    return;
  }

  if (status === "CANCELED" || status === "PARTIAL_CANCELED") {
    await revokePaidEntitlement(payment, tossPayment, status === "PARTIAL_CANCELED");
    return;
  }

  if (status === "ABORTED" || status === "EXPIRED") {
    await markPaymentFailed(payment.order_id, `Toss payment status is ${status}.`, tossPayment);
    return;
  }

  await restPatch<PaymentRecord>(`payments?order_id=eq.${encodeURIComponent(payment.order_id)}`, {
    status: "pending",
    payment_key: stringOrNull(tossPayment.paymentKey) ?? payment.payment_key ?? null,
    provider_data: tossPayment,
    updated_at: nowIso(),
  });
}

async function grantPaidEntitlement(payment: PaymentRecord, tossPayment: JsonObject): Promise<void> {
  const period = periodFromPayment(payment);
  const periodStart = new Date();
  const periodEnd = addBillingPeriod(periodStart, period);
  const method = paymentMethodSummary(tossPayment);

  const updatedPayment = await restPatch<PaymentRecord>(
    `payments?order_id=eq.${encodeURIComponent(payment.order_id)}`,
    {
      status: "completed",
      payment_key: stringOrNull(tossPayment.paymentKey) ?? payment.payment_key ?? null,
      method,
      provider_data: tossPayment,
      completed_at: nowIso(),
      failure_reason: null,
      updated_at: nowIso(),
    },
  );
  const completedPayment = updatedPayment[0] ?? payment;

  const existingLicense = await selectCurrentLicense(payment.user_id);
  const licensePayload = {
    user_id: payment.user_id,
    tier: "PRO",
    status: "active",
    started_at: periodStart.toISOString(),
    expires_at: periodEnd.toISOString(),
    updated_at: nowIso(),
    metadata: {
      source: "toss",
      order_id: payment.order_id,
      payment_id: completedPayment.id,
      period,
    },
  };
  const license = existingLicense
    ? (await restPatch<LicenseRecord>(`user_licenses?id=eq.${encodeURIComponent(existingLicense.id)}`, licensePayload))[0]
    : await restInsert<LicenseRecord>("user_licenses", { ...licensePayload, created_at: nowIso() });

  const existingSubscription = await selectLatestSubscription(payment.user_id);
  const subscriptionPayload = {
    user_id: payment.user_id,
    license_id: license.id,
    period,
    amount: payment.amount,
    status: "active",
    provider: "toss",
    provider_order_id: payment.order_id,
    current_period_start: periodStart.toISOString(),
    current_period_end: periodEnd.toISOString(),
    next_billing_date: periodEnd.toISOString().slice(0, 10),
    cancel_at_period_end: false,
    last_payment_id: completedPayment.id,
    failure_reason: null,
    updated_at: nowIso(),
    metadata: {
      payment_method_summary: method,
      latest_toss_status: tossPayment.status ?? null,
    },
  };

  const subscription = existingSubscription
    ? (await restPatch<SubscriptionRecord>(
      `subscriptions?id=eq.${encodeURIComponent(existingSubscription.id)}`,
      subscriptionPayload,
    ))[0]
    : await restInsert<SubscriptionRecord>("subscriptions", { ...subscriptionPayload, created_at: nowIso() });

  await restPatch<PaymentRecord>(`payments?id=eq.${encodeURIComponent(completedPayment.id)}`, {
    subscription_id: subscription.id,
    updated_at: nowIso(),
  });
}

async function revokePaidEntitlement(
  payment: PaymentRecord,
  tossPayment: JsonObject,
  partial: boolean,
): Promise<void> {
  await restPatch<PaymentRecord>(`payments?order_id=eq.${encodeURIComponent(payment.order_id)}`, {
    status: partial ? "partially_refunded" : "refunded",
    provider_data: tossPayment,
    refunded_at: nowIso(),
    updated_at: nowIso(),
  });
  await restPatch<SubscriptionRecord>(
    `subscriptions?user_id=eq.${encodeURIComponent(payment.user_id)}&status=in.(active,past_due,pending)`,
    {
      status: "cancelled",
      cancel_at_period_end: false,
      cancelled_at: nowIso(),
      updated_at: nowIso(),
      failure_reason: partial ? "Partial refund processed by Toss." : "Refund or cancellation processed by Toss.",
    },
  );
  await restPatch<LicenseRecord>(
    `user_licenses?user_id=eq.${encodeURIComponent(payment.user_id)}&tier=eq.PRO&status=eq.active`,
    {
      status: "cancelled",
      cancelled_at: nowIso(),
      updated_at: nowIso(),
    },
  );
}

async function getSubscriptionResponse(user: AuthUser): Promise<JsonObject> {
  const license = await selectCurrentLicense(user.id);
  const subscription = await selectLatestSubscription(user.id);
  const entitlement = entitlementFromLicense(license);
  const isActive = entitlement.tier === "PRO" && entitlement.status === "active";
  const methodSummary = stringOrNull(subscription?.metadata?.payment_method_summary) ??
    stringOrNull(subscription?.payment_method);

  return {
    is_active: isActive,
    tier: entitlement.tier,
    status: entitlement.status,
    expires_at: entitlement.expires_at,
    auto_renew: Boolean(subscription && subscription.status === "active" && !subscription.cancel_at_period_end),
    payment_method: methodSummary,
    payment_available: paymentEnabled(),
    payment_message: paymentEnabled() ? null : PAYMENT_DISABLED_REASON,
    reason: paymentEnabled() ? "Server-side billing is available." : PAYMENT_DISABLED_REASON,
    next_required_step: paymentEnabled()
      ? "Use /billing/checkout for payment and refresh entitlement from Supabase after confirmation."
      : PAYMENT_DISABLED_NEXT_STEP,
    next_billing_date: subscription?.current_period_end ?? null,
    cancel_at_period_end: Boolean(subscription?.cancel_at_period_end),
    provider: subscription?.provider ?? "toss",
    checkout_url: null,
    last_payment_error: subscription?.failure_reason ?? null,
  };
}

function entitlementFromLicense(license: LicenseRecord | null): EntitlementSnapshot {
  if (!license) {
    return { tier: "FREE", status: "none", expires_at: null };
  }

  if (license.status === "active" && license.tier === "PRO" && licenseNotExpired(license.expires_at)) {
    return { tier: "PRO", status: "active", expires_at: license.expires_at };
  }

  if (license.status === "active" && !licenseNotExpired(license.expires_at)) {
    return { tier: "FREE", status: "expired", expires_at: license.expires_at };
  }

  if (["inactive", "expired", "cancelled", "past_due"].includes(license.status)) {
    return {
      tier: "FREE",
      status: license.status as EntitlementSnapshot["status"],
      expires_at: license.expires_at,
    };
  }

  return { tier: "FREE", status: "none", expires_at: license.expires_at };
}

async function selectCurrentLicense(userId: string): Promise<LicenseRecord | null> {
  const licenses = await restArray<LicenseRecord>(
    `user_licenses?user_id=eq.${encodeURIComponent(userId)}&select=*&order=created_at.desc`,
  );
  return licenses.find((license) =>
    license.tier === "PRO" && license.status === "active" && licenseNotExpired(license.expires_at)
  ) ??
    licenses.find((license) => license.status === "active" && licenseNotExpired(license.expires_at)) ??
    licenses[0] ??
    null;
}

async function selectLatestSubscription(userId: string): Promise<SubscriptionRecord | null> {
  const subscriptions = await restArray<SubscriptionRecord>(
    `subscriptions?user_id=eq.${encodeURIComponent(userId)}&select=*&order=created_at.desc&limit=1`,
  );
  return subscriptions[0] ?? null;
}

async function selectPaymentForUser(userId: string, orderId: string): Promise<PaymentRecord | null> {
  const payments = await restArray<PaymentRecord>(
    `payments?user_id=eq.${encodeURIComponent(userId)}&order_id=eq.${encodeURIComponent(orderId)}&select=*&limit=1`,
  );
  return payments[0] ?? null;
}

async function selectPaymentByOrderId(orderId: string): Promise<PaymentRecord | null> {
  const payments = await restArray<PaymentRecord>(
    `payments?order_id=eq.${encodeURIComponent(orderId)}&select=*&limit=1`,
  );
  return payments[0] ?? null;
}

async function markPaymentFailed(orderId: string, reason: string, providerData: JsonObject = {}): Promise<void> {
  await restPatch<PaymentRecord>(`payments?order_id=eq.${encodeURIComponent(orderId)}`, {
    status: "failed",
    provider_data: providerData,
    failed_at: nowIso(),
    failure_reason: reason,
    updated_at: nowIso(),
  });
}

async function updateBillingEvent(eventId: string, payload: JsonObject): Promise<void> {
  await restPatch(`billing_events?provider=eq.toss&event_id=eq.${encodeURIComponent(eventId)}`, payload);
}

async function requireUser(req: Request): Promise<AuthUser> {
  const token = bearerToken(req);
  if (!token) {
    throw new HttpError(401, "AUTH_REQUIRED", "A Supabase access token is required.");
  }

  const response = await fetch(`${requiredEnv("SUPABASE_URL")}/auth/v1/user`, {
    headers: {
      apikey: requiredEnv("SUPABASE_ANON_KEY"),
      authorization: `Bearer ${token}`,
    },
  });

  if (!response.ok) {
    throw new HttpError(401, "AUTH_INVALID", "Supabase access token could not be verified.");
  }

  return await response.json() as AuthUser;
}

async function restArray<T>(pathAndQuery: string): Promise<T[]> {
  const response = await supabaseRest(pathAndQuery, { method: "GET" });
  return await response.json() as T[];
}

async function restInsert<T>(table: string, payload: JsonObject): Promise<T> {
  const response = await supabaseRest(table, {
    method: "POST",
    headers: { Prefer: "return=representation" },
    body: JSON.stringify(payload),
  });
  const rows = await response.json() as T[];
  return rows[0];
}

async function restPatch<T>(pathAndQuery: string, payload: JsonObject): Promise<T[]> {
  const response = await supabaseRest(pathAndQuery, {
    method: "PATCH",
    headers: { Prefer: "return=representation" },
    body: JSON.stringify(payload),
  });
  const text = await response.text();
  return text ? JSON.parse(text) as T[] : [];
}

async function supabaseRest(pathAndQuery: string, init: RequestInit): Promise<Response> {
  const base = requiredEnv("SUPABASE_URL").replace(/\/$/, "");
  const serviceRoleKey = requiredEnv("SUPABASE_SERVICE_ROLE_KEY");
  const response = await fetch(`${base}/rest/v1/${pathAndQuery}`, {
    ...init,
    headers: {
      apikey: serviceRoleKey,
      authorization: `Bearer ${serviceRoleKey}`,
      "content-type": "application/json",
      ...(init.headers ?? {}),
    },
  });

  if (!response.ok) {
    const body = await response.text();
    throw new HttpError(response.status, "SUPABASE_REST_ERROR", body || "Supabase REST request failed.");
  }

  return response;
}

async function tossFetch<T>(
  path: string,
  options: { method: "GET" | "POST"; idempotencyKey?: string; body?: JsonObject },
): Promise<T> {
  const response = await fetch(`https://api.tosspayments.com${path}`, {
    method: options.method,
    headers: {
      authorization: `Basic ${btoa(`${requiredEnv("TOSS_SECRET_KEY")}:`)}`,
      "content-type": "application/json",
      ...(options.idempotencyKey ? { "Idempotency-Key": options.idempotencyKey } : {}),
    },
    body: options.body ? JSON.stringify(options.body) : undefined,
  });

  const text = await response.text();
  const body = text ? JSON.parse(text) as JsonObject : {};
  if (!response.ok) {
    const message = stringOrNull(body.message) ?? "Toss request failed.";
    throw new HttpError(response.status, "TOSS_API_ERROR", message);
  }

  return body as T;
}

function requiredPaymentRuntime(): { successUrl: string; failUrl: string } {
  return {
    successUrl: requiredEnv("LOLSHORTS_PAYMENT_SUCCESS_URL"),
    failUrl: requiredEnv("LOLSHORTS_PAYMENT_FAIL_URL"),
  };
}

function verifyWebhookSecret(req: Request, url: URL): void {
  const expected = requiredEnv("TOSS_WEBHOOK_SECRET");
  const provided = req.headers.get("x-lolshorts-webhook-secret") ?? url.searchParams.get("webhook_secret");
  if (!provided || provided !== expected) {
    throw new HttpError(401, "WEBHOOK_SECRET_INVALID", "Webhook secret is missing or invalid.");
  }
}

function readPaymentObjectFromWebhook(payload: JsonObject): JsonObject {
  return asObject(payload.data) ?? asObject(payload.entityBody) ?? payload;
}

function eventIdForWebhook(req: Request, payload: JsonObject, data: JsonObject): string {
  return req.headers.get("x-toss-webhook-id") ??
    stringOrNull(payload.eventId) ??
    stringOrNull(payload.id) ??
    [
      stringOrNull(payload.eventType) ?? "UNKNOWN",
      stringOrNull(data.orderId) ?? "NO_ORDER",
      stringOrNull(data.paymentKey) ?? "NO_PAYMENT_KEY",
      stringOrNull(data.status) ?? "NO_STATUS",
      stringOrNull(payload.createdAt) ?? nowIso(),
    ].join(":");
}

function normalizePeriod(period: unknown): BillingPeriod {
  if (period === "YEARLY") {
    return "YEARLY";
  }
  if (period === "MONTHLY" || period === undefined || period === null) {
    return "MONTHLY";
  }
  throw new HttpError(400, "INVALID_PERIOD", "Billing period must be MONTHLY or YEARLY.");
}

function amountForPeriod(period: BillingPeriod): number {
  const envName = period === "YEARLY" ? "LOLSHORTS_PRO_YEARLY_KRW" : "LOLSHORTS_PRO_MONTHLY_KRW";
  const configured = Deno.env.get(envName);
  if (!configured) {
    return period === "YEARLY" ? 99000 : 9900;
  }
  const parsed = Number(configured);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new HttpError(500, "PAYMENT_CONFIG_INVALID", `${envName} must be a positive integer.`);
  }
  return parsed;
}

function periodFromPayment(payment: PaymentRecord): BillingPeriod {
  return normalizePeriod(payment.provider_data?.period ?? payment.provider_data?.["period"]);
}

function addBillingPeriod(base: Date, period: BillingPeriod): Date {
  const result = new Date(base);
  if (period === "YEARLY") {
    result.setFullYear(result.getFullYear() + 1);
  } else {
    result.setMonth(result.getMonth() + 1);
  }
  return result;
}

function paymentMethodSummary(payment: JsonObject): string | null {
  const card = asObject(payment.card);
  if (card) {
    const company = stringOrNull(card.company) ?? "card";
    const number = stringOrNull(card.number);
    return number ? `${company} ${number}` : company;
  }
  return stringOrNull(payment.method);
}

function readCheckoutUrl(payment: JsonObject): string | null {
  const checkout = asObject(payment.checkout);
  return stringOrNull(checkout?.url);
}

function paymentEnabled(): boolean {
  return Deno.env.get("LOLSHORTS_PAYMENT_ENABLED")?.toLowerCase() === "true";
}

function requiredEnv(name: string): string {
  const value = Deno.env.get(name);
  if (!value) {
    throw new HttpError(500, "PAYMENT_CONFIG_MISSING", `${name} is not configured.`);
  }
  return value;
}

function bearerToken(req: Request): string | null {
  const header = req.headers.get("authorization");
  if (!header?.toLowerCase().startsWith("bearer ")) {
    return null;
  }
  return header.slice("bearer ".length).trim();
}

async function readJson(req: Request): Promise<JsonObject> {
  const text = await req.text();
  if (!text) {
    return {};
  }
  const parsed = JSON.parse(text);
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new HttpError(400, "INVALID_JSON", "Request body must be a JSON object.");
  }
  return parsed as JsonObject;
}

function requiredString(value: unknown, name: string): string {
  if (typeof value !== "string" || value.trim() === "") {
    throw new HttpError(400, "INVALID_INPUT", `${name} is required.`);
  }
  return value;
}

function requiredNumber(value: unknown, name: string): number {
  const parsed = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new HttpError(400, "INVALID_INPUT", `${name} must be a positive number.`);
  }
  return parsed;
}

function stringOrNull(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function asObject(value: unknown): JsonObject | null {
  return value && typeof value === "object" && !Array.isArray(value) ? value as JsonObject : null;
}

function licenseNotExpired(expiresAt: string | null): boolean {
  if (!expiresAt) {
    return true;
  }
  const expires = Date.parse(expiresAt);
  return Number.isFinite(expires) && expires > Date.now();
}

function nowIso(): string {
  return new Date().toISOString();
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { ...CORS_HEADERS, "content-type": "application/json" },
  });
}

function errorResponse(status: number, code: string, message: string, nextRequiredStep?: string): Response {
  return jsonResponse({
    error: code,
    message,
    payment_available: false,
    reason: message,
    next_required_step: nextRequiredStep ?? "Resolve the billing error and retry from the app.",
  }, status);
}

class HttpError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
  ) {
    super(message);
  }
}
