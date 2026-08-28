export const logger = {
  error: jest.fn<void, unknown[]>(),
  warn: jest.fn<void, unknown[]>(),
  info: jest.fn<void, unknown[]>(),
};
