const isDev = (): boolean => {
  try {
    return import.meta.env?.DEV ?? false;
  } catch {
    return process.env.NODE_ENV !== "production";
  }
};

export const logger = {
  error: (...args: unknown[]): void => {
    console.error(...args);
  },
  warn: (...args: unknown[]): void => {
    if (isDev()) {
      console.warn(...args);
    }
  },
  info: (...args: unknown[]): void => {
    if (isDev()) {
      // eslint-disable-next-line no-console
      console.info(...args);
    }
  },
};
