/**
 * ErrorMapper Unit Tests
 *
 * Tests for getErrorKey, getErrorKeyFromCode, isNetworkError, and isAuthError.
 *
 * jest.setup.js globally mocks errorMapper, so we must unmock it here
 * to test the real implementation (same pattern as auth.test.ts).
 */

// jest.setup.js mocks errorMapper globally. Unmock it here so we test
// the real implementation. Jest resolves this via the @/ moduleNameMapper alias.
jest.unmock("@/lib/errorMapper");

import {
  getErrorKey,
  getErrorKeyFromCode,
  isNetworkError,
  isAuthError,
} from "../errorMapper";

describe("getErrorKey", () => {
  describe("null / undefined input", () => {
    it("returns errors.generic for null", () => {
      expect(getErrorKey(null)).toBe("errors.generic");
    });

    it("returns errors.generic for undefined", () => {
      expect(getErrorKey(undefined)).toBe("errors.generic");
    });
  });

  describe("string input", () => {
    it("maps known exact message to correct i18n key", () => {
      expect(getErrorKey("Invalid login credentials")).toBe(
        "errors.invalidCredentials",
      );
    });

    it('maps "User already registered" to emailAlreadyInUse', () => {
      expect(getErrorKey("User already registered")).toBe(
        "errors.emailAlreadyInUse",
      );
    });

    it('maps "Email rate limit exceeded" to tooManyRequests', () => {
      expect(getErrorKey("Email rate limit exceeded")).toBe(
        "errors.tooManyRequests",
      );
    });

    it("returns errors.generic for an unknown string", () => {
      expect(getErrorKey("totally unknown error")).toBe("errors.generic");
    });

    it("maps message matching network pattern via regex", () => {
      expect(getErrorKey("fetch failed")).toBe("errors.networkError");
    });

    it("maps message matching rate limit pattern via regex", () => {
      expect(getErrorKey("rate limit hit")).toBe("errors.tooManyRequests");
    });
  });

  describe("Error instance input", () => {
    it("maps error code invalid_credentials to invalidCredentials", () => {
      const err = Object.assign(new Error("bad"), {
        code: "invalid_credentials",
      });
      expect(getErrorKey(err)).toBe("errors.invalidCredentials");
    });

    it("maps error code weak_password to weakPassword", () => {
      const err = Object.assign(new Error("weak"), { code: "weak_password" });
      expect(getErrorKey(err)).toBe("errors.weakPassword");
    });

    it("maps error code session_expired to sessionExpired", () => {
      const err = Object.assign(new Error("expired"), {
        code: "session_expired",
      });
      expect(getErrorKey(err)).toBe("errors.sessionExpired");
    });

    it("maps structured backend network errors before message fallback", () => {
      const err = Object.assign(new Error("request failed"), {
        code: "NETWORK_ERROR",
      });
      expect(getErrorKey(err)).toBe("errors.networkError");
    });

    it("falls back to message matching when code is unknown", () => {
      const err = Object.assign(new Error("Invalid login credentials"), {
        code: "unknown_code",
      });
      expect(getErrorKey(err)).toBe("errors.invalidCredentials");
    });

    it("returns errors.generic when both code and message are unknown", () => {
      const err = new Error("completely unknown");
      expect(getErrorKey(err)).toBe("errors.generic");
    });
  });

  describe("plain object input", () => {
    it("maps object with known code to correct key", () => {
      expect(getErrorKey({ code: "invalid_email", message: "bad email" })).toBe(
        "errors.invalidEmail",
      );
    });

    it("falls back to message matching when code is absent", () => {
      expect(getErrorKey({ message: "User already registered" })).toBe(
        "errors.emailAlreadyInUse",
      );
    });

    it("returns errors.generic when object has no code or message", () => {
      expect(getErrorKey({})).toBe("errors.generic");
    });
  });
});

describe("getErrorKeyFromCode", () => {
  it("returns the mapped key for a known code", () => {
    expect(getErrorKeyFromCode("invalid_credentials")).toBe(
      "errors.invalidCredentials",
    );
  });

  it("returns the mapped key for rate_limit_exceeded", () => {
    expect(getErrorKeyFromCode("rate_limit_exceeded")).toBe(
      "errors.tooManyRequests",
    );
  });

  it("returns the mapped key for a structured backend code", () => {
    expect(getErrorKeyFromCode("PROCESS_TIMEOUT")).toBe(
      "errors.processTimeout",
    );
  });

  it("returns errors.generic for an unknown code", () => {
    expect(getErrorKeyFromCode("totally_unknown_code")).toBe("errors.generic");
  });

  it("returns errors.generic for an empty string", () => {
    expect(getErrorKeyFromCode("")).toBe("errors.generic");
  });
});

describe("isNetworkError", () => {
  it('returns true for an Error with "network" in the message', () => {
    expect(isNetworkError(new Error("network timeout"))).toBe(true);
  });

  it('returns true for an Error with "fetch" in the message', () => {
    expect(isNetworkError(new Error("fetch failed"))).toBe(true);
  });

  it("returns true for ECONNREFUSED errors", () => {
    expect(
      isNetworkError(new Error("connect ECONNREFUSED 127.0.0.1:3000")),
    ).toBe(true);
  });

  it("returns true for a structured backend network error code", () => {
    const err = Object.assign(new Error("request failed"), {
      code: "NETWORK_ERROR",
    });
    expect(isNetworkError(err)).toBe(true);
  });

  it("returns false for a generic Error without network keywords", () => {
    expect(isNetworkError(new Error("Invalid credentials"))).toBe(false);
  });

  it("returns false for non-Error values", () => {
    expect(isNetworkError("network error string")).toBe(false);
    expect(isNetworkError(null)).toBe(false);
  });
});

describe("isAuthError", () => {
  it("returns true for an Error with a known auth error code", () => {
    const err = Object.assign(new Error("bad creds"), {
      code: "invalid_credentials",
    });
    expect(isAuthError(err)).toBe(true);
  });

  it("returns true for user_banned error code", () => {
    const err = Object.assign(new Error("banned"), { code: "user_banned" });
    expect(isAuthError(err)).toBe(true);
  });

  it("returns false for an Error with no code property", () => {
    expect(isAuthError(new Error("no code here"))).toBe(false);
  });

  it("returns false for an Error with an unknown code", () => {
    const err = Object.assign(new Error("unknown"), {
      code: "totally_unknown",
    });
    expect(isAuthError(err)).toBe(false);
  });

  it("returns false for non-Error values", () => {
    expect(isAuthError(null)).toBe(false);
    expect(isAuthError({ code: "invalid_credentials" })).toBe(false);
  });
});
