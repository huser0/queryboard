import { describe, expect, it } from "vitest";
import { mergedParamNames, paramsFor } from "../params";
import type { QuerySummary } from "../../../ipc/types";

function query(slug: string, paramNames: string[]): QuerySummary {
  return {
    id: slug,
    slug,
    name: slug,
    connection_slug: "conn",
    sql: "SELECT 1",
    params: paramNames.map((name) => ({ name, type: "string", required: true })),
    created_at: "",
    updated_at: "",
  };
}

describe("mergedParamNames", () => {
  it("dedupes and sorts alphabetically across queries", () => {
    const a = query("a", ["offer_id", "store_id"]);
    const b = query("b", ["offer_id", "product_id"]);
    expect(mergedParamNames([a, b])).toEqual(["offer_id", "product_id", "store_id"]);
  });

  it("returns empty for queries without params", () => {
    expect(mergedParamNames([query("a", [])])).toEqual([]);
  });
});

describe("paramsFor", () => {
  it("returns only the subset of names the query declares, defaulting missing values", () => {
    const a = query("a", ["offer_id", "store_id"]);
    const values = { offer_id: "5002", product_id: "100" };
    expect(paramsFor(a, values)).toEqual({ offer_id: "5002", store_id: "" });
  });
});
