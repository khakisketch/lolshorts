import { assertEquals } from "jsr:@std/assert@^1";
import {
  buildCheckResponse,
  currentMonth,
  FREE_MONTHLY_AUTO_EDIT_LIMIT,
  licenseNotExpired,
  parseAction,
  parseJobId,
  userIsProFromLicenses,
} from "./logic.ts";

Deno.test("parseAction accepts check/consume, rejects everything else", () => {
  assertEquals(parseAction("check"), "check");
  assertEquals(parseAction("consume"), "consume");
  assertEquals(parseAction("delete"), null);
  assertEquals(parseAction(""), null);
  assertEquals(parseAction(undefined), null);
  assertEquals(parseAction(null), null);
  assertEquals(parseAction(42), null);
});

Deno.test("parseJobId accepts stable bounded keys", () => {
  assertEquals(parseJobId("auto_edit_123"), "auto_edit_123");
  assertEquals(parseJobId("  job  "), "job");
  assertEquals(parseJobId(""), null);
  assertEquals(parseJobId("x".repeat(161)), null);
  assertEquals(parseJobId(undefined), null);
});

Deno.test("currentMonth formats YYYY-MM in UTC", () => {
  assertEquals(currentMonth(new Date("2026-07-09T12:00:00Z")), "2026-07");
  assertEquals(currentMonth(new Date("2026-01-31T23:59:59Z")), "2026-01");
  assertEquals(currentMonth(new Date("2026-12-01T00:00:00Z")), "2026-12");
});

Deno.test("buildCheckResponse allows under the limit, blocks at/over it", () => {
  assertEquals(buildCheckResponse(0, 5), { allowed: true, used: 0, limit: 5 });
  assertEquals(buildCheckResponse(4, 5), { allowed: true, used: 4, limit: 5 });
  assertEquals(buildCheckResponse(5, 5), { allowed: false, used: 5, limit: 5 });
  assertEquals(buildCheckResponse(6, 5), { allowed: false, used: 6, limit: 5 });
});

Deno.test("buildCheckResponse coerces bad `used` values to 0", () => {
  assertEquals(buildCheckResponse(NaN, 5), { allowed: true, used: 0, limit: 5 });
  assertEquals(buildCheckResponse(-3, 5), { allowed: true, used: 0, limit: 5 });
  assertEquals(buildCheckResponse(2.9, 5), { allowed: true, used: 2, limit: 5 });
});

Deno.test("licenseNotExpired: null never expires, future ok, past/invalid closed", () => {
  const now = Date.parse("2026-07-09T00:00:00Z");
  assertEquals(licenseNotExpired(null, now), true);
  assertEquals(licenseNotExpired("2026-08-01T00:00:00Z", now), true);
  assertEquals(licenseNotExpired("2026-06-01T00:00:00Z", now), false);
  assertEquals(licenseNotExpired("not-a-date", now), false);
});

Deno.test("userIsProFromLicenses requires an active, unexpired PRO license", () => {
  const now = Date.parse("2026-07-09T00:00:00Z");
  assertEquals(userIsProFromLicenses([], now), false);
  assertEquals(
    userIsProFromLicenses([{ tier: "FREE", status: "active", expires_at: null }], now),
    false,
  );
  assertEquals(
    userIsProFromLicenses([{ tier: "PRO", status: "active", expires_at: null }], now),
    true,
  );
  assertEquals(
    userIsProFromLicenses([{ tier: "PRO", status: "cancelled", expires_at: null }], now),
    false,
  );
  assertEquals(
    userIsProFromLicenses(
      [{ tier: "PRO", status: "active", expires_at: "2026-06-01T00:00:00Z" }],
      now,
    ),
    false,
  );
  // Mixed set: one active unexpired PRO among noise still unlocks.
  assertEquals(
    userIsProFromLicenses(
      [
        { tier: "FREE", status: "active", expires_at: null },
        { tier: "PRO", status: "expired", expires_at: "2026-06-01T00:00:00Z" },
        { tier: "PRO", status: "active", expires_at: "2026-08-01T00:00:00Z" },
      ],
      now,
    ),
    true,
  );
});

Deno.test("FREE monthly limit constant is 5", () => {
  assertEquals(FREE_MONTHLY_AUTO_EDIT_LIMIT, 5);
});
