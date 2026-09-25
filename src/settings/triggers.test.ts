import { describe as group, expect, it } from "vitest";
import { describe, fromCron, toCron } from "./TriggerControls";

group("trigger schedules", () => {
  it("reads common schedules as plain words", () => {
    expect(describe("45 8 * * 1-5")).toBe("Weekdays at 08:45");
    expect(describe("0 9 * * *")).toBe("Every day at 09:00");
    expect(describe("30 18 * * 1,3,5")).toBe("Every Mon, Wed, Fri at 18:30");
    expect(describe("0 9 * * 7")).toBe("Every Sun at 09:00");
    expect(describe("0 */2 * * *")).toBe("Every 2 hours");
    expect(describe("*/15 * * * *")).toBe("Every 15 minutes");
    expect(describe("0 9 1 * *")).toBe('Schedule "0 9 1 * *"');
  });

  it("round-trips through the editor", () => {
    for (const cron of ["45 8 * * 1-5", "0 9 * * *", "30 18 * * 1,3,5", "0 */2 * * *", "*/15 * * * *", "0 9 1 * *"]) {
      expect(toCron(fromCron(cron))).toBe(cron);
    }
  });
});
