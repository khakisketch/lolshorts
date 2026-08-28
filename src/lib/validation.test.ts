import {
  getFieldError,
  validateSettings,
  validateYoutubeUpload,
} from "./validation";

describe("validation helpers", () => {
  it("accepts a complete YouTube upload", () => {
    const result = validateYoutubeUpload({
      title: "A clean highlight",
      description: "A short description",
      tags: "league,highlight",
      privacyStatus: "unlisted",
    });

    expect(result).toEqual({ valid: true, errors: [] });
  });

  it("reports all YouTube field constraints", () => {
    const result = validateYoutubeUpload({
      title: " ",
      description: "d".repeat(5001),
      tags: "t".repeat(501),
      privacyStatus: "invalid" as "public",
    });

    expect(result.valid).toBe(false);
    expect(result.errors).toEqual([
      { field: "title", message: "Title is required" },
      {
        field: "description",
        message: "Description must be 5000 characters or less",
      },
      { field: "tags", message: "Tags must be 500 characters or less" },
      {
        field: "privacyStatus",
        message: "Privacy status must be public, unlisted, or private",
      },
    ]);
    expect(getFieldError(result, "title")).toBe("Title is required");
    expect(getFieldError(result, "missing")).toBeUndefined();
  });

  it("rejects an overlong title", () => {
    const result = validateYoutubeUpload({
      title: "x".repeat(101),
      privacyStatus: "private",
    });

    expect(result.valid).toBe(false);
    expect(getFieldError(result, "title")).toBe(
      "Title must be 100 characters or less",
    );
  });

  it("accepts omitted optional upload fields", () => {
    expect(
      validateYoutubeUpload({ title: "Highlight", privacyStatus: "public" }),
    ).toEqual({ valid: true, errors: [] });
  });

  it("accepts an omitted summoner name", () => {
    expect(validateSettings({})).toEqual({ valid: true, errors: [] });
  });

  it("rejects blank and overlong summoner names", () => {
    expect(validateSettings({ summoner_name: "   " })).toEqual({
      valid: false,
      errors: [
        { field: "summoner_name", message: "Summoner name cannot be empty" },
      ],
    });
    expect(validateSettings({ summoner_name: "s".repeat(33) })).toEqual({
      valid: false,
      errors: [
        {
          field: "summoner_name",
          message: "Summoner name must be 32 characters or less",
        },
      ],
    });
  });
});
