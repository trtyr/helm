import { describe, expect, it } from "vitest";
import { newScanId } from "./MemScan";

describe("newScanId", () => {
  it("generates UUID-shaped unique ids (insecure-context safe)", () => {
    const uuidShape =
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
    const a = newScanId();
    const b = newScanId();
    expect(a).toMatch(uuidShape);
    expect(b).toMatch(uuidShape);
    expect(a).not.toBe(b);
  });
});
