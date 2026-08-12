import { describe, expect, it } from "vitest";
import { formatCell, formatDateOnly, formatNumberString, formatTimestamp } from "./cellFormat";

describe("formatNumberString", () => {
  it("adds thousand separators without touching precision", () => {
    expect(formatNumberString("12500")).toBe("12.500");
    expect(formatNumberString("1000000")).toBe("1.000.000");
  });

  it("converts the decimal point to a comma (pt-BR)", () => {
    expect(formatNumberString("1234.5")).toBe("1.234,5");
  });

  it("preserves every digit of a value far beyond f64 precision", () => {
    expect(formatNumberString("123456789012345678901234567890.5")).toBe(
      "123.456.789.012.345.678.901.234.567.890,5",
    );
  });

  it("handles negative numbers", () => {
    expect(formatNumberString("-4200")).toBe("-4.200");
  });

  it("leaves small numbers (no grouping needed) unchanged apart from the comma", () => {
    expect(formatNumberString("90")).toBe("90");
    expect(formatNumberString("90.5")).toBe("90,5");
  });
});

describe("formatDateOnly", () => {
  it("converts ISO date to dd/mm/yyyy", () => {
    expect(formatDateOnly("2022-03-01")).toBe("01/03/2022");
  });

  it("returns the raw string unchanged when it doesn't look like an ISO date", () => {
    expect(formatDateOnly("not-a-date")).toBe("not-a-date");
  });
});

describe("formatTimestamp", () => {
  it("shows date and time when the time isn't midnight", () => {
    expect(formatTimestamp("2022-03-01T14:30:00.000000000")).toBe("01/03/2022 14:30");
  });

  it("omits the time when it's exactly midnight", () => {
    expect(formatTimestamp("2022-03-01T00:00:00.000000000")).toBe("01/03/2022");
  });

  it("ignores a trailing timezone offset when checking for midnight", () => {
    expect(formatTimestamp("2022-03-01T00:00:00+00:00")).toBe("01/03/2022");
    expect(formatTimestamp("2022-03-01T14:30:00+00:00")).toBe("01/03/2022 14:30");
  });
});

describe("formatCell", () => {
  it("marks Null distinctly from an empty string", () => {
    const result = formatCell({ type: "Null" });
    expect(result.isNull).toBe(true);
    expect(result.text).toBe("—");
  });

  it("never converts Decimal through Number()", () => {
    const result = formatCell({
      type: "Decimal",
      value: "123456789012345678901234567890.5",
    });
    expect(result.text).toBe("123.456.789.012.345.678.901.234.567.890,5");
    expect(result.raw).toBe("123456789012345678901234567890.5");
  });

  it("keeps the raw value available even when the display text is formatted", () => {
    const result = formatCell({ type: "Timestamp", value: "2022-03-01T14:30:00.000000000" });
    expect(result.text).toBe("01/03/2022 14:30");
    expect(result.raw).toBe("2022-03-01T14:30:00.000000000");
  });

  it("formats booleans as true/false, not 1/0", () => {
    expect(formatCell({ type: "Bool", value: true }).text).toBe("true");
    expect(formatCell({ type: "Bool", value: false }).text).toBe("false");
  });
});
