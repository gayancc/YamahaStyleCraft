import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { InitialEffects, type EffectSelection } from "./InitialEffects";
import type { View } from "./contracts";
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
const view = { revision: 3 } as View,
  output = { name: "Genos", index: 1 };
const preview = {
  token: "2:3",
  packets: [
    [240, 247],
    [240, 247],
  ],
  header: { reverb: { lateChanges: true }, chorus: { lateChanges: false } },
};
beforeEach(() => mocks.invoke.mockReset().mockResolvedValue(preview));
afterEach(() => cleanup());
it("starts unchecked and only preflights on explicit selection, without sending MIDI", async () => {
  const changed = vi.fn();
  render(
    <InitialEffects
      view={view}
      output={output}
      disabled={false}
      selection={null}
      onChange={changed}
    />,
  );
  const box = screen.getByRole("checkbox");
  expect((box as HTMLInputElement).checked).toBe(false);
  expect(mocks.invoke).not.toHaveBeenCalled();
  fireEvent.click(box);
  await waitFor(() =>
    expect(changed).toHaveBeenCalledWith({
      view,
      output,
      token: "2:3",
      packetCount: 2,
      lateChanges: true,
    }),
  );
  expect(mocks.invoke.mock.calls).toEqual([
    ["preview_style_effects", { revision: 3 }],
  ]);
});
it("reports late changes and unchecks without native calls", () => {
  const changed = vi.fn();
  const selection: EffectSelection = {
    view,
    output,
    token: "2:3",
    packetCount: 2,
    lateChanges: true,
  };
  render(
    <InitialEffects
      view={view}
      output={output}
      disabled={false}
      selection={selection}
      onChange={changed}
    />,
  );
  expect(screen.getByText(/Later effect messages exist/)).toBeTruthy();
  fireEvent.click(screen.getByRole("checkbox"));
  expect(changed).toHaveBeenCalledWith(null);
  expect(mocks.invoke).not.toHaveBeenCalled();
});
it("remains unchecked after preflight failure and permits retry", async () => {
  mocks.invoke.mockRejectedValueOnce("Unverified header");
  const changed = vi.fn();
  render(
    <InitialEffects
      view={view}
      output={output}
      disabled={false}
      selection={null}
      onChange={changed}
    />,
  );
  fireEvent.click(screen.getByRole("checkbox"));
  await screen.findByRole("alert");
  expect(changed).not.toHaveBeenCalled();
  expect((screen.getByRole("checkbox") as HTMLInputElement).checked).toBe(
    false,
  );
  fireEvent.click(screen.getByRole("checkbox"));
  await waitFor(() => expect(changed).toHaveBeenCalledOnce());
});
it("discards a delayed preflight when the document or output changes", async () => {
  for (const replace of ["view", "output"]) {
    let resolve!: (v: typeof preview) => void;
    mocks.invoke.mockImplementationOnce(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    );
    const changed = vi.fn();
    const props = {
      view,
      output,
      disabled: false,
      selection: null,
      onChange: changed,
    };
    const ui = render(<InitialEffects {...props} />);
    fireEvent.click(screen.getByRole("checkbox"));
    ui.rerender(
      <InitialEffects
        {...props}
        view={replace === "view" ? { ...view } : view}
        output={replace === "output" ? { ...output } : output}
      />,
    );
    await act(async () => resolve(preview));
    expect(changed).not.toHaveBeenCalled();
    ui.unmount();
  }
});
it("discards preflight after close and disables selection without a document, output or while busy", async () => {
  let resolve!: (v: typeof preview) => void;
  mocks.invoke.mockImplementationOnce(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const changed = vi.fn();
  const ui = render(
    <InitialEffects
      view={view}
      output={output}
      disabled={false}
      selection={null}
      onChange={changed}
    />,
  );
  fireEvent.click(screen.getByRole("checkbox"));
  ui.unmount();
  await act(async () => resolve(preview));
  expect(changed).not.toHaveBeenCalled();
  for (const props of [
    { view: null, output, disabled: false },
    { view, output: null, disabled: false },
    { view, output, disabled: true },
  ]) {
    const u = render(
      <InitialEffects {...props} selection={null} onChange={changed} />,
    );
    expect((screen.getByRole("checkbox") as HTMLInputElement).disabled).toBe(
      true,
    );
    u.unmount();
  }
});
