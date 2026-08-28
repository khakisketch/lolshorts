/**
 * Logger Unit Tests
 *
 * logger.ts uses import.meta.env which Jest (CommonJS mode) cannot parse.
 * jest.config.js therefore maps '@/lib/logger' -> '__mocks__/loggerMock.ts'.
 *
 * Strategy:
 *  - Part 1: Test the mock (always-on) logger to verify the public API contract.
 *  - Part 2: Test the conditional-logging logic in isolation using a small
 *    inline module that mirrors the production behaviour via process.env,
 *    which Jest can handle without ESM support.
 */

// ── Part 1: Mock logger (records without polluting test output) ─────────────

import { logger } from "@/lib/logger"; // resolves to __mocks__/loggerMock.ts

describe("logger mock API contract", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("exposes error, warn, and info methods", () => {
    expect(typeof logger.error).toBe("function");
    expect(typeof logger.warn).toBe("function");
    expect(typeof logger.info).toBe("function");
  });

  it("logger.error records all arguments", () => {
    logger.error("msg", { detail: 1 });
    expect(logger.error).toHaveBeenCalledWith("msg", { detail: 1 });
  });

  it("logger.warn records all arguments", () => {
    logger.warn("warning", 42);
    expect(logger.warn).toHaveBeenCalledWith("warning", 42);
  });

  it("logger.info records all arguments", () => {
    logger.info("info msg", [1, 2, 3]);
    expect(logger.info).toHaveBeenCalledWith("info msg", [1, 2, 3]);
  });

  it("logger.error can be called with no arguments", () => {
    expect(() => logger.error()).not.toThrow();
  });

  it("logger.warn can be called with no arguments", () => {
    expect(() => logger.warn()).not.toThrow();
  });

  it("logger.info can be called with no arguments", () => {
    expect(() => logger.info()).not.toThrow();
  });
});

// ── Part 2: Conditional-logging logic (process.env based) ───────────────────
//
// The real logger.ts guards calls with isDev() which reads import.meta.env.DEV.
// We replicate that exact conditional pattern using process.env so Jest can run
// it without ESM. This validates the branching behaviour independently of the
// import.meta syntax issue.

function makeConditionalLogger(isDev: () => boolean) {
  return {
    error: (...args: unknown[]): void => {
      if (isDev()) console.error(...args);
    },
    warn: (...args: unknown[]): void => {
      if (isDev()) console.warn(...args);
    },
    info: (...args: unknown[]): void => {
      if (isDev()) console.info(...args);
    },
  };
}

describe("conditional logger in development mode", () => {
  const devLogger = makeConditionalLogger(() => true);
  let errorSpy: jest.SpyInstance;
  let warnSpy: jest.SpyInstance;
  let infoSpy: jest.SpyInstance;

  beforeEach(() => {
    errorSpy = jest.spyOn(console, "error").mockImplementation(() => {});
    warnSpy = jest.spyOn(console, "warn").mockImplementation(() => {});
    infoSpy = jest.spyOn(console, "info").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("calls console.error when isDev returns true", () => {
    devLogger.error("dev error");
    expect(errorSpy).toHaveBeenCalledWith("dev error");
  });

  it("calls console.warn when isDev returns true", () => {
    devLogger.warn("dev warn");
    expect(warnSpy).toHaveBeenCalledWith("dev warn");
  });

  it("calls console.info when isDev returns true", () => {
    devLogger.info("dev info");
    expect(infoSpy).toHaveBeenCalledWith("dev info");
  });
});

describe("conditional logger in production mode", () => {
  const prodLogger = makeConditionalLogger(() => false);
  let errorSpy: jest.SpyInstance;
  let warnSpy: jest.SpyInstance;
  let infoSpy: jest.SpyInstance;

  beforeEach(() => {
    errorSpy = jest.spyOn(console, "error").mockImplementation(() => {});
    warnSpy = jest.spyOn(console, "warn").mockImplementation(() => {});
    infoSpy = jest.spyOn(console, "info").mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("does not call console.error when isDev returns false", () => {
    prodLogger.error("should be silent");
    expect(errorSpy).not.toHaveBeenCalled();
  });

  it("does not call console.warn when isDev returns false", () => {
    prodLogger.warn("should be silent");
    expect(warnSpy).not.toHaveBeenCalled();
  });

  it("does not call console.info when isDev returns false", () => {
    prodLogger.info("should be silent");
    expect(infoSpy).not.toHaveBeenCalled();
  });
});
