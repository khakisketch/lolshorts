// The global jest.setup mock for "i18next" is not chainable; i18n.ts calls
// `.use(...).use(...).init(...)`, so provide a chainable stub here.
jest.mock("i18next", () => {
  const instance: Record<string, unknown> = {};
  instance.use = () => instance;
  instance.init = () => Promise.resolve();
  return { __esModule: true, default: instance };
});
jest.mock("i18next-browser-languagedetector", () => ({
  __esModule: true,
  default: {},
}));

import { languages } from "@/i18n";

describe("language picker options (G013)", () => {
  it("offers only Korean and English", () => {
    expect(languages.map((entry) => entry.code).sort()).toEqual(["en", "ko"]);
  });

  it("keeps the display metadata each option needs", () => {
    for (const entry of languages) {
      expect(typeof entry.nativeName).toBe("string");
      expect(entry.nativeName.length).toBeGreaterThan(0);
      expect(typeof entry.name).toBe("string");
      expect(Array.isArray(entry.regions)).toBe(true);
    }
  });
});
