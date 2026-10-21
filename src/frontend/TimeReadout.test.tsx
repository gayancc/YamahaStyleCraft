import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { TimeReadout, formatElapsed } from "./TimeReadout";

describe("TimeReadout", () => {
  afterEach(cleanup);

  it("formats minutes, seconds and milliseconds", () => {
    expect(formatElapsed(0)).toEqual({ minutes: "00", seconds: "00", millis: "000" });
    expect(formatElapsed(6632)).toEqual({ minutes: "00", seconds: "06", millis: "632" });
    expect(formatElapsed(125_007)).toEqual({ minutes: "02", seconds: "05", millis: "007" });
    expect(formatElapsed(-5).millis).toBe("000");
  });

  it("renders the reported time while stopped", () => {
    const { container } = render(<TimeReadout elapsedMs={6632} playing={false} />);
    expect(container.textContent).toBe("00:06.632");
  });

  it("keeps counting between reports while playing", async () => {
    const { container } = render(<TimeReadout elapsedMs={1000} playing />);
    await new Promise((resolve) => setTimeout(resolve, 120));
    const millis = Number(container.querySelector("span")?.textContent);
    expect(container.querySelectorAll("i.digit").length).toBe(7);
    expect(millis).toBeGreaterThan(0);
  });
});
