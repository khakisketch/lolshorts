/**
 * Utils Unit Tests
 *
 * Tests for className merging, error message extraction,
 * storage formatting, and duration formatting utilities.
 */

import { cn, getErrorMessage, formatStorage, formatDuration } from "../utils";

describe("cn", () => {
  it("returns a single class name unchanged", () => {
    expect(cn("foo")).toBe("foo");
  });

  it("merges multiple class names into one string", () => {
    expect(cn("foo", "bar")).toBe("foo bar");
  });

  it("deduplicates conflicting Tailwind classes, keeping the last one", () => {
    // twMerge resolves conflicts: p-4 wins over p-2
    expect(cn("p-2", "p-4")).toBe("p-4");
  });

  it("ignores falsy values (undefined, null, false)", () => {
    expect(cn("foo", undefined, null, false, "bar")).toBe("foo bar");
  });

  it("handles conditional object syntax from clsx", () => {
    expect(cn({ active: true, disabled: false })).toBe("active");
  });

  it("returns empty string when given no arguments", () => {
    expect(cn()).toBe("");
  });

  it("merges array inputs correctly", () => {
    expect(cn(["foo", "bar"], "baz")).toBe("foo bar baz");
  });
});

describe("getErrorMessage", () => {
  it("returns message property from an Error instance", () => {
    expect(getErrorMessage(new Error("something went wrong"))).toBe(
      "something went wrong",
    );
  });

  it("returns the string directly when error is a string", () => {
    expect(getErrorMessage("plain string error")).toBe("plain string error");
  });

  it("extracts message from a plain object with a message property", () => {
    expect(getErrorMessage({ message: "object error" })).toBe("object error");
  });

  it("falls back to String() for unknown error types like numbers", () => {
    expect(getErrorMessage(42)).toBe("42");
  });

  it("falls back to String() for null", () => {
    expect(getErrorMessage(null)).toBe("null");
  });

  it("falls back to String() for undefined", () => {
    expect(getErrorMessage(undefined)).toBe("undefined");
  });

  it("coerces a numeric message property to string", () => {
    expect(getErrorMessage({ message: 404 })).toBe("404");
  });
});

describe("formatStorage", () => {
  it('returns "0 B" for zero bytes', () => {
    expect(formatStorage(0)).toBe("0 B");
  });

  it("formats bytes correctly", () => {
    expect(formatStorage(512)).toBe("512.0 B");
  });

  it("formats kilobytes correctly", () => {
    expect(formatStorage(1024)).toBe("1.0 KB");
  });

  it("formats megabytes correctly", () => {
    expect(formatStorage(1024 * 1024)).toBe("1.0 MB");
  });

  it("formats gigabytes with one decimal place", () => {
    expect(formatStorage(1.5 * 1024 * 1024 * 1024)).toBe("1.5 GB");
  });

  it("formats terabytes correctly", () => {
    expect(formatStorage(1024 ** 4)).toBe("1.0 TB");
  });

  it("formats fractional megabytes with one decimal place", () => {
    expect(formatStorage(250 * 1024 * 1024)).toBe("250.0 MB");
  });
});

describe("formatDuration", () => {
  it("formats zero seconds as 0:00", () => {
    expect(formatDuration(0)).toBe("0:00");
  });

  it("formats seconds under one minute as M:SS", () => {
    expect(formatDuration(45)).toBe("0:45");
  });

  it("formats exactly one minute as 1:00", () => {
    expect(formatDuration(60)).toBe("1:00");
  });

  it("formats minutes and seconds as MM:SS", () => {
    expect(formatDuration(90)).toBe("1:30");
  });

  it("formats exactly one hour as H:MM:SS", () => {
    expect(formatDuration(3600)).toBe("1:00:00");
  });

  it("formats hours, minutes, and seconds as H:MM:SS", () => {
    expect(formatDuration(3661)).toBe("1:01:01");
  });

  it("pads seconds to two digits", () => {
    expect(formatDuration(65)).toBe("1:05");
  });

  it("floors fractional seconds", () => {
    expect(formatDuration(61.9)).toBe("1:01");
  });
});
