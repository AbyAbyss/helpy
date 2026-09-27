import { describe, expect, it } from "vitest";
import { linkify } from "./links";

describe("linkify", () => {
  it("wraps bare web addresses, leaving trailing punctuation outside", () => {
    expect(linkify("See https://example.com/a?b=1.")).toBe("See <https://example.com/a?b=1>.");
    expect(linkify("http://x.org, then")).toBe("<http://x.org>, then");
  });

  it("leaves links that are already links alone", () => {
    for (const t of ["[docs](https://example.com)", "<https://example.com>", "not a link: example.com"]) {
      expect(linkify(t)).toBe(t);
    }
  });
});
