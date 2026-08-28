import { waitFor } from "@testing-library/react";

describe("optional error telemetry", () => {
  const originalDsn = process.env.VITE_SENTRY_DSN;

  beforeEach(() => {
    jest.resetModules();
    process.env.VITE_SENTRY_DSN = "https://public@example.invalid/1";
  });

  afterEach(() => {
    if (originalDsn === undefined) {
      delete process.env.VITE_SENTRY_DSN;
    } else {
      process.env.VITE_SENTRY_DSN = originalDsn;
    }
    jest.dontMock("@sentry/react");
  });

  it("does not load Sentry while the user remains opted out", async () => {
    let moduleFactoryCalls = 0;
    jest.doMock("@sentry/react", () => {
      moduleFactoryCalls += 1;
      return {
        init: jest.fn(),
        close: jest.fn().mockResolvedValue(true),
        captureException: jest.fn(),
      };
    });

    const { configureErrorTelemetry } = await import("./telemetry");
    configureErrorTelemetry(false);
    await Promise.resolve();

    expect(moduleFactoryCalls).toBe(0);
  });

  it("loads on opt-in and closes the transport on opt-out", async () => {
    const init = jest.fn();
    const close = jest.fn().mockResolvedValue(true);
    jest.doMock("@sentry/react", () => ({
      init,
      close,
      captureException: jest.fn(),
    }));

    const { configureErrorTelemetry } = await import("./telemetry");
    configureErrorTelemetry(true);

    await waitFor(() => expect(init).toHaveBeenCalledTimes(1));

    configureErrorTelemetry(false);
    expect(close).toHaveBeenCalledTimes(1);
  });
});
